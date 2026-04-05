/**
 * Focuser Extension — Background Script
 *
 * Blocking approach:
 * 1. onBeforeNavigate: inject CSS to immediately hide page (no flash)
 * 2. onCommitted: inject content script to replace page with block page
 * URL bar stays as the original blocked domain — clean!
 *
 * Communication:
 * - Primary: Native Messaging via com.focuser.native host binary
 * - Fallback: HTTP polling to localhost:17549 (for development/when native host not installed)
 */

var NATIVE_HOST_NAME = 'com.focuser.native';
var API_BASE = 'http://127.0.0.1:17549';
var POLL_INTERVAL_MS = 2000;
var RECONNECT_DELAY_MS = 5000;

var currentRules = null;
var blockedDomains = new Set();
var blockedKeywords = [];
var blockedWildcards = [];
var blockedUrlPaths = [];
var blockEntireInternet = false;
var allowedDomains = new Set();

var nativePort = null;
var useNativeMessaging = true;


// ─── Native Messaging ──────────────────────────────────────────────

function connectNative() {
  try {
    nativePort = chrome.runtime.connectNative(NATIVE_HOST_NAME);

    nativePort.onMessage.addListener(function(msg) {
      if (msg.msg_type === 'RuleUpdate' && msg.payload) {
        applyRules(msg.payload);
      } else if (msg.msg_type === 'Pong') {
        // Heartbeat response
      }
    });

    nativePort.onDisconnect.addListener(function() {
      var err = chrome.runtime.lastError;
      if (err) {
        console.log('Focuser: Native host disconnected:', err.message);
      }
      nativePort = null;

      // If native messaging fails, fall back to HTTP polling
      if (useNativeMessaging) {
        useNativeMessaging = false;
        console.log('Focuser: Falling back to HTTP polling');
        startHttpPolling();
      }

      // Try to reconnect after delay
      setTimeout(function() {
        useNativeMessaging = true;
        connectNative();
      }, RECONNECT_DELAY_MS);
    });

    // Request current rules
    nativePort.postMessage({
      msg_type: 'RuleUpdate',
      payload: null
    });

    // Send connected event
    nativePort.postMessage({
      msg_type: 'Event',
      payload: {
        Connected: {
          browser: detectBrowser(),
          extension_version: chrome.runtime.getManifest().version
        }
      }
    });

    useNativeMessaging = true;
    updateBadge(true);
    console.log('Focuser: Connected to native messaging host');

  } catch (e) {
    console.log('Focuser: Native messaging not available:', e.message);
    useNativeMessaging = false;
    startHttpPolling();
  }
}

function detectBrowser() {
  var ua = navigator.userAgent;
  if (ua.indexOf('Edg/') !== -1) return 'Edge';
  if (ua.indexOf('Brave') !== -1) return 'Brave';
  if (ua.indexOf('OPR/') !== -1 || ua.indexOf('Opera') !== -1) return 'Opera';
  if (ua.indexOf('Firefox/') !== -1) return 'Firefox';
  if (ua.indexOf('Chrome/') !== -1) return 'Chrome';
  return { Other: 'unknown' };
}


// ─── HTTP Polling Fallback ─────────────────────────────────────────

var httpPollTimer = null;

function startHttpPolling() {
  if (httpPollTimer) return;
  fetchRulesHttp();
  httpPollTimer = setInterval(fetchRulesHttp, POLL_INTERVAL_MS);
}

function stopHttpPolling() {
  if (httpPollTimer) {
    clearInterval(httpPollTimer);
    httpPollTimer = null;
  }
}

async function fetchRulesHttp() {
  try {
    var browser = detectBrowser();
    var browserName = typeof browser === 'string' ? browser : 'Other';
    var resp = await fetch(API_BASE + '/api/rules', {
      headers: { 'X-Focuser-Browser': browserName }
    });
    if (!resp.ok) { updateBadge(false); return; }
    var rules = await resp.json();
    applyRules(rules);
  } catch (e) {
    updateBadge(false);
  }
}


// ─── Rule Application ──────────────────────────────────────────────

function applyRules(rules) {
  if (!rules) { updateBadge(false); return; }

  var rulesJson = JSON.stringify(rules);
  if (rulesJson === JSON.stringify(currentRules)) return;

  currentRules = rules;
  blockedDomains = new Set((rules.blocked_domains || []).map(function(d) { return d.toLowerCase(); }));
  blockedKeywords = (rules.blocked_keywords || []).map(function(k) { return k.toLowerCase(); });
  blockedWildcards = rules.blocked_wildcards || [];
  blockedUrlPaths = (rules.blocked_url_paths || []).map(function(p) { return p.toLowerCase(); });
  blockEntireInternet = rules.block_entire_internet || false;
  allowedDomains = new Set((rules.allowed_domains || []).map(function(d) { return d.toLowerCase(); }));

  updateBadge(true);
  enforceOnAllTabs();
}


// ─── Domain Matching ────────────────────────────────────────────────

function isDomainBlocked(hostname, url) {
  hostname = (hostname || '').toLowerCase();
  url = (url || '').toLowerCase();

  if (isAllowed(hostname)) return false;
  if (blockEntireInternet) return true;

  if (blockedDomains.has(hostname)) return true;
  var parts = hostname.split('.');
  for (var i = 1; i < parts.length; i++) {
    if (blockedDomains.has(parts.slice(i).join('.'))) return true;
  }

  for (var k = 0; k < blockedKeywords.length; k++) {
    if (url.indexOf(blockedKeywords[k]) !== -1) return true;
  }

  for (var p = 0; p < blockedUrlPaths.length; p++) {
    if (url.indexOf(blockedUrlPaths[p]) !== -1) return true;
  }

  return false;
}

function isAllowed(hostname) {
  if (allowedDomains.has(hostname)) return true;
  var parts = hostname.split('.');
  for (var i = 1; i < parts.length; i++) {
    if (allowedDomains.has(parts.slice(i).join('.'))) return true;
  }
  return false;
}

function isInternalUrl(protocol) {
  return protocol === 'chrome:' || protocol === 'chrome-extension:' ||
    protocol === 'about:' || protocol === 'moz-extension:' ||
    protocol === 'edge:' || protocol === 'data:';
}

// ─── Blocking ───────────────────────────────────────────────────────

// content-early.js (registered in manifest at document_start) handles hiding.
// Once page commits, inject our full block page content script:
chrome.webNavigation.onCommitted.addListener(function(details) {
  if (details.frameId !== 0) return;
  try {
    var url = new URL(details.url);
    if (isInternalUrl(url.protocol)) return;
    if (isDomainBlocked(url.hostname, details.url)) {
chrome.scripting.executeScript({
        target: { tabId: details.tabId },
        files: ['content-block.js']
      }).catch(function() {});

      // Report blocked event via native messaging
      reportBlocked(details.url, url.hostname);
    }
  } catch (e) {}
});

// Step 3: Safety net — if page somehow loaded, catch it on complete
chrome.webNavigation.onCompleted.addListener(function(details) {
  if (details.frameId !== 0) return;
  try {
    var url = new URL(details.url);
    if (isInternalUrl(url.protocol)) return;
    if (isDomainBlocked(url.hostname, details.url)) {
      chrome.scripting.executeScript({
        target: { tabId: details.tabId },
        files: ['content-block.js']
      }).catch(function() {});
    }
  } catch (e) {}
});

// Enforce on all currently open tabs (when rules change)
function enforceOnAllTabs() {
  chrome.tabs.query({}, function(tabs) {
    tabs.forEach(function(tab) {
      if (!tab.url) return;
      try {
        var url = new URL(tab.url);
        if (isInternalUrl(url.protocol)) return;
        if (isDomainBlocked(url.hostname, tab.url)) {
          chrome.scripting.executeScript({
            target: { tabId: tab.id },
            files: ['content-block.js']
          }).catch(function() {});
        }
      } catch (e) {}
    });
  });
}


// ─── Event Reporting ───────────────────────────────────────────────

var _recentlyReported = {};

function reportBlocked(url, hostname) {
  // Deduplicate: don't report the same domain more than once per 5 seconds
  var now = Date.now();
  if (_recentlyReported[hostname] && now - _recentlyReported[hostname] < 5000) return;
  _recentlyReported[hostname] = now;

  // Report via native messaging if connected
  if (nativePort) {
    nativePort.postMessage({
      msg_type: 'Event',
      payload: {
        Blocked: {
          url: url,
          matched_rule: { Domain: hostname },
          timestamp: new Date().toISOString()
        }
      }
    });
  }
  // Also report via HTTP API so the desktop app records the event
  fetch(API_BASE + '/api/blocked', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ domain: hostname })
  }).catch(function() {});
}


// ─── Badge ──────────────────────────────────────────────────────────

function updateBadge(connected) {
  if (!connected) {
    chrome.action.setBadgeText({ text: '!' });
    chrome.action.setBadgeBackgroundColor({ color: '#ef4444' });
    chrome.action.setTitle({ title: 'Focuser — App not running' });
    return;
  }

  var uniqueDomains = 0;
  blockedDomains.forEach(function(d) {
    if (!d.startsWith('www.') || !blockedDomains.has(d.substring(4))) uniqueDomains++;
  });
  var count = uniqueDomains + blockedKeywords.length + blockedUrlPaths.length;
  if (blockEntireInternet) count = '∞';

  chrome.action.setBadgeText({ text: count > 0 ? String(count) : '' });
  chrome.action.setBadgeBackgroundColor({ color: '#8b5cf6' });
  chrome.action.setTitle({ title: 'Focuser — ' + count + ' sites blocked' });
}


// ─── Startup ───────────────────────────────────────────────────────

// Try native messaging first, fall back to HTTP polling
connectNative();


// ─── Message Handler ────────────────────────────────────────────────

chrome.runtime.onMessage.addListener(function(msg, sender, sendResponse) {
  if (msg.type === 'get-rules') {
    sendResponse({ rules: currentRules, connected: currentRules !== null });
    return;
  }
  if (msg.type === 'refresh') {
    if (nativePort) {
      nativePort.postMessage({ msg_type: 'RuleUpdate', payload: null });
      sendResponse({ ok: true });
    } else {
      fetchRulesHttp().then(function() { sendResponse({ ok: true }); });
      return true;
    }
    return;
  }
  if (msg.type === 'report-blocked') {
    reportBlocked(msg.url || '', msg.hostname);
    return;
  }
  if (msg.type === 'check-domain') {
    var isBlocked = isDomainBlocked(msg.hostname, msg.url);
    if (isBlocked) {
      reportBlocked(msg.url || '', msg.hostname);
    }
    sendResponse({ blocked: isBlocked });
    return;
  }
});

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
var MAX_RECONNECT_DELAY_MS = 300000;
var NATIVE_RETRY_LIMIT = 3;
var nativeReconnectDelay = RECONNECT_DELAY_MS;
var nativeConnectedAt = 0;
var nativeFailCount = 0;

var currentRules = null;
var blockedDomains = new Set();
var blockedKeywords = [];
var blockedWildcards = [];
var blockedUrlPaths = [];
var blockEntireInternet = false;
var allowedDomains = new Set();
var domainCategories = {};

var nativePort = null;
var useNativeMessaging = true;


// ─── Native Messaging ──────────────────────────────────────────────

function connectNative() {
  try {
    nativePort = chrome.runtime.connectNative(NATIVE_HOST_NAME);

    nativePort.onMessage.addListener(function(msg) {
      if (nativeFailCount > 0) {
        nativeFailCount = 0;
        nativeReconnectDelay = RECONNECT_DELAY_MS;
      }
      console.log('Focuser: Native messaging active');
      if (msg.msg_type === 'RuleUpdate' && msg.payload) {
        applyRules(msg.payload);
      }
    });

    nativePort.onDisconnect.addListener(function() {
      var err = chrome.runtime.lastError;
      if (err) {
        console.log('Focuser: Native host disconnected:', err.message);
      }
      nativePort = null;

      var wasStable = (Date.now() - nativeConnectedAt) > 10000;

      if (useNativeMessaging) {
        useNativeMessaging = false;
        startHttpPolling();
      }

      if (wasStable) {
        nativeFailCount = 0;
        nativeReconnectDelay = RECONNECT_DELAY_MS;
      } else {
        nativeFailCount++;
        nativeReconnectDelay = Math.min(nativeReconnectDelay * 2, MAX_RECONNECT_DELAY_MS);
      }

      if (nativeFailCount >= NATIVE_RETRY_LIMIT) {
        console.log('Focuser: Native messaging unavailable, using HTTP polling');
        return;
      }

      console.log('Focuser: Native host disconnected, retry in ' + Math.round(nativeReconnectDelay / 1000) + 's');
      setTimeout(function() {
        useNativeMessaging = true;
        connectNative();
      }, nativeReconnectDelay);
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
    nativeConnectedAt = Date.now();
    updateBadge(true);

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
  // Send immediate heartbeat so the app knows we're here
  sendHeartbeat();
  fetchRulesHttp();
  httpPollTimer = setInterval(fetchRulesHttp, POLL_INTERVAL_MS);
}

function sendHeartbeat() {
  var browser = detectBrowser();
  var browserName = typeof browser === 'string' ? browser : 'Other';
  // Use dedicated heartbeat endpoint (URL-based, no header parsing needed)
  fetch(API_BASE + '/api/heartbeat?browser=' + encodeURIComponent(browserName))
    .catch(function() {});
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
  domainCategories = rules.domain_categories || {};

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

// ─── Category + Visit Count Tracking ──────────────────────────────

// ─── Category Intelligence System ─────────────────────────────────
// Builds a smart category index from premade-lists.json + API data.
// For keyword blocking, searches premade domains to find which category
// the keyword belongs to (e.g., "porn" matches "pornhub.com" → adult).

var premadeIndex = {};
var premadeCategoryMap = {
  'porn': 'adult', 'dating': 'dating', 'gambling': 'gambling',
  'games': 'gaming', 'news': 'news', 'shopping': 'shopping',
  'social_media': 'social_media', 'videos': 'video',
  'distractions': 'social_media', 'email': 'default',
  'proxies': 'default', 'search_engines': 'default',
  'productivity': 'default'
};

function loadPremadeIndex() {
  try {
    var url = chrome.runtime.getURL('premade-lists.json');
    fetch(url).then(function(r) { return r.json(); }).then(function(data) {
      if (!data || !data.categories) return;
      premadeIndex = {};
      Object.keys(data.categories).forEach(function(key) {
        var cat = data.categories[key];
        var msgCategory = premadeCategoryMap[key] || 'default';
        (cat.domains || []).forEach(function(d) {
          premadeIndex[d.toLowerCase()] = msgCategory;
        });
        (cat.wildcards || []).forEach(function(w) {
          premadeIndex['wc:' + w.toLowerCase()] = msgCategory;
        });
      });
    }).catch(function() {});
  } catch (e) {}
}

function getCategoryForDomain(hostname) {
  hostname = hostname.toLowerCase();
  if (domainCategories[hostname]) return domainCategories[hostname];
  var parts = hostname.split('.');
  for (var i = 1; i < parts.length; i++) {
    var parent = parts.slice(i).join('.');
    if (domainCategories[parent]) return domainCategories[parent];
  }
  if (premadeIndex[hostname]) return premadeIndex[hostname];
  for (var i = 1; i < parts.length; i++) {
    var parent = parts.slice(i).join('.');
    if (premadeIndex[parent]) return premadeIndex[parent];
  }
  return 'default';
}

function getMatchedKeyword(url) {
  url = (url || '').toLowerCase();
  for (var k = 0; k < blockedKeywords.length; k++) {
    if (url.indexOf(blockedKeywords[k]) !== -1) return blockedKeywords[k];
  }
  return null;
}

function getCategoryForKeyword(keyword) {
  var kw = keyword.toLowerCase();
  var domains = Object.keys(premadeIndex);
  for (var i = 0; i < domains.length; i++) {
    var d = domains[i];
    if (d.startsWith('wc:')) continue;
    if (d.indexOf(kw) !== -1) return premadeIndex[d];
  }
  for (var i = 0; i < domains.length; i++) {
    var d = domains[i];
    if (!d.startsWith('wc:')) continue;
    if (d.indexOf(kw) !== -1) return premadeIndex[d];
  }
  if (domainCategories) {
    var dcKeys = Object.keys(domainCategories);
    for (var i = 0; i < dcKeys.length; i++) {
      if (dcKeys[i].indexOf(kw) !== -1) return domainCategories[dcKeys[i]];
    }
  }
  return 'default';
}

// Single source of truth: the app's /api/blocked endpoint.
// POSTs the block event and returns the current count for the tracking key.
// No local storage — the app's statistics database is authoritative.
function reportBlockedAndGetCount(domain, trackingKey, callback) {
  fetch(API_BASE + '/api/blocked', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ domain: domain, tracking_key: trackingKey })
  })
    .then(function(r) { return r.json(); })
    .then(function(data) { callback((data && data.count) || 1); })
    .catch(function() { callback(1); });
}

// Per-tab deduplication: prevents double-counting from
// onCommitted + onCompleted + enforceOnAllTabs firing for the same navigation.
var _recentInjections = {};
var INJECTION_DEDUP_MS = 3000;

function shouldInject(tabId, trackingKey) {
  var now = Date.now();
  var entryKey = tabId + ':' + trackingKey;
  var last = _recentInjections[entryKey];
  if (last && (now - last) < INJECTION_DEDUP_MS) return false;
  _recentInjections[entryKey] = now;
  // Periodic cleanup
  if (Object.keys(_recentInjections).length > 100) {
    Object.keys(_recentInjections).forEach(function(k) {
      if ((now - _recentInjections[k]) > INJECTION_DEDUP_MS * 2) {
        delete _recentInjections[k];
      }
    });
  }
  return true;
}

// Clear dedup entries for a tab when it navigates to a new URL
chrome.tabs.onRemoved.addListener(function(tabId) {
  Object.keys(_recentInjections).forEach(function(k) {
    if (k.indexOf(tabId + ':') === 0) delete _recentInjections[k];
  });
});

function injectBlockPage(tabId, hostname, fullUrl) {
  var matchedKeyword = getMatchedKeyword(fullUrl);
  var category, trackingKey, blockedTarget, blockReason;

  if (matchedKeyword && !blockedDomains.has(hostname.toLowerCase())) {
    category = getCategoryForKeyword(matchedKeyword);
    trackingKey = 'kw:' + matchedKeyword;
    blockedTarget = matchedKeyword;
    blockReason = 'keyword';
  } else {
    category = getCategoryForDomain(hostname);
    trackingKey = hostname.toLowerCase();
    blockedTarget = hostname;
    blockReason = 'domain';
  }

  if (!shouldInject(tabId, trackingKey)) return;

  reportBlockedAndGetCount(hostname, trackingKey, function(count) {
    chrome.scripting.executeScript({
      target: { tabId: tabId },
      func: function(cat, cnt, target, reason, dom) {
        window.__focuserCategory = cat;
        window.__focuserCount = cnt;
        window.__focuserTarget = target;
        window.__focuserReason = reason;
        window.__focuserDomain = dom;
      },
      args: [category, count, blockedTarget, blockReason, hostname]
    }).then(function() {
      chrome.scripting.executeScript({
        target: { tabId: tabId },
        files: ['content-block.js']
      }).catch(function() {});
    }).catch(function() {
      chrome.scripting.executeScript({
        target: { tabId: tabId },
        files: ['content-block.js']
      }).catch(function() {});
    });
  });
}


// ─── Blocking ───────────────────────────────────────────────────────

chrome.webNavigation.onCommitted.addListener(function(details) {
  if (details.frameId !== 0) return;
  try {
    var url = new URL(details.url);
    if (isInternalUrl(url.protocol)) return;
    if (isDomainBlocked(url.hostname, details.url)) {
      injectBlockPage(details.tabId, url.hostname, details.url);
    }
  } catch (e) {}
});

chrome.webNavigation.onCompleted.addListener(function(details) {
  if (details.frameId !== 0) return;
  try {
    var url = new URL(details.url);
    if (isInternalUrl(url.protocol)) return;
    if (!isDomainBlocked(url.hostname, details.url)) return;

    // Only inject if the page wasn't already blocked by onCommitted.
    // Avoids double-counting when both events fire for the same navigation.
    chrome.scripting.executeScript({
      target: { tabId: details.tabId },
      func: function() { return !!document.getElementById('focuser-blocked'); }
    }).then(function(results) {
      var alreadyBlocked = results && results[0] && results[0].result === true;
      if (alreadyBlocked) return;
      injectBlockPage(details.tabId, url.hostname, details.url);
    }).catch(function() {});
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

        var nowBlocked = isDomainBlocked(url.hostname, tab.url);

        chrome.scripting.executeScript({
          target: { tabId: tab.id },
          func: function() { return !!document.getElementById('focuser-blocked'); }
        }).then(function(results) {
          var hasBlockPage = results && results[0] && results[0].result === true;

          if (nowBlocked && !hasBlockPage) {
            // Should be blocked but isn't yet — inject
            injectBlockPage(tab.id, url.hostname, tab.url);
          } else if (!nowBlocked && hasBlockPage) {
            // Was blocked but no longer is — reload to restore original content
            chrome.tabs.reload(tab.id);
          }
        }).catch(function() {});
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

// Load premade category index for smart keyword→category matching
loadPremadeIndex();

// Send immediate heartbeat so the Focuser app knows this browser has the extension
sendHeartbeat();

// ─── Reliable heartbeat via chrome.alarms ─────────────────────────
// setInterval doesn't survive service worker suspends in MV3.
// chrome.alarms wakes up the background script reliably. Chrome's
// minimum alarm period is 30 seconds. When the alarm fires, the SW
// wakes up and the setInterval below resumes too — so when the SW is
// awake we get rapid heartbeats, and when it's asleep the alarm
// guarantees we wake up at least every 30 seconds.
chrome.alarms.create('focuser-heartbeat', { periodInMinutes: 0.5 });

chrome.alarms.onAlarm.addListener(function(alarm) {
  if (alarm.name === 'focuser-heartbeat') {
    sendHeartbeat();
    if (!nativePort) fetchRulesHttp();
  }
});

// Fast heartbeat when the service worker is awake.
// This is best-effort and gets suspended along with the SW, but the
// chrome.alarms above guarantees we wake up at least every 30s.
setInterval(sendHeartbeat, 3000);

// Belt-and-suspenders: fire heartbeat on common browser events too,
// so even if alarms misbehave we still get caught.
chrome.runtime.onStartup.addListener(function() { sendHeartbeat(); });
chrome.runtime.onInstalled.addListener(function() { sendHeartbeat(); });
if (chrome.tabs && chrome.tabs.onActivated) {
  chrome.tabs.onActivated.addListener(function() { sendHeartbeat(); });
}
if (chrome.windows && chrome.windows.onFocusChanged) {
  chrome.windows.onFocusChanged.addListener(function() { sendHeartbeat(); });
}

// Start HTTP polling (native messaging requires focuser-service running separately)
startHttpPolling();


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
  if (msg.type === 'get-category') {
    var cat = getCategoryForDomain(msg.hostname);
    getCategoryCount(cat, function(cnt) {
      sendResponse({ category: cat, count: cnt });
    });
    return true;
  }
});

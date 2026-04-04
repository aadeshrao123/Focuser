/**
 * Focuser Extension — Background Script
 *
 * Blocking approach:
 * 1. onBeforeNavigate: inject CSS to immediately hide page (no flash)
 * 2. onCommitted: inject content script to replace page with block page
 * URL bar stays as the original blocked domain — clean!
 */

var API_BASE = 'http://127.0.0.1:17549';
var POLL_INTERVAL_MS = 2000;

var currentRules = null;
var blockedDomains = new Set();
var blockedKeywords = [];
var blockedWildcards = [];
var blockedUrlPaths = [];
var blockEntireInternet = false;
var allowedDomains = new Set();


// ─── API ────────────────────────────────────────────────────────────

async function apiGet(path) {
  try {
    var resp = await fetch(API_BASE + path);
    return resp.ok ? await resp.json() : null;
  } catch (e) { return null; }
}

async function apiPost(path, body) {
  try {
    var resp = await fetch(API_BASE + path, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(body),
    });
    return resp.ok ? await resp.json() : null;
  } catch (e) { return null; }
}

// ─── Rule Fetching ──────────────────────────────────────────────────

async function fetchRules() {
  var rules = await apiGet('/api/rules');
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

// ─── Polling ────────────────────────────────────────────────────────

setInterval(fetchRules, POLL_INTERVAL_MS);
fetchRules();

// ─── Message Handler ────────────────────────────────────────────────

chrome.runtime.onMessage.addListener(function(msg, sender, sendResponse) {
  if (msg.type === 'get-rules') {
    sendResponse({ rules: currentRules, connected: currentRules !== null });
    return;
  }
  if (msg.type === 'refresh') {
    fetchRules().then(function() { sendResponse({ ok: true }); });
    return true;
  }
  if (msg.type === 'check-domain') {
    sendResponse({ blocked: isDomainBlocked(msg.hostname, msg.url) });
    return;
  }
});

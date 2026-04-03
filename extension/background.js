/**
 * Focuser Extension — Background Service Worker
 *
 * Polls the Focuser app API for blocking rules, then:
 * - Redirects blocked tabs to the block page
 * - Updates the badge with blocked site count
 * - Instantly blocks/unblocks when rules change
 */

const API_BASE = 'http://127.0.0.1:17549';
const POLL_INTERVAL_MS = 2000;

let currentRules = null;
let blockedDomains = new Set();
let blockedKeywords = [];
let blockedWildcards = [];
let blockedUrlPaths = [];
let blockEntireInternet = false;
let allowedDomains = new Set();

// ─── API Communication ──────────────────────────────────────────────

async function apiGet(path) {
  try {
    var resp = await fetch(API_BASE + path, { method: 'GET' });
    if (!resp.ok) return null;
    return await resp.json();
  } catch (e) {
    return null;
  }
}

async function apiPost(path, body) {
  try {
    var resp = await fetch(API_BASE + path, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(body),
    });
    if (!resp.ok) return null;
    return await resp.json();
  } catch (e) {
    return null;
  }
}

// ─── Rule Fetching & Compilation ────────────────────────────────────

async function fetchRules() {
  var rules = await apiGet('/api/rules');
  if (!rules) {
    updateBadge(false);
    return;
  }

  // Check if rules changed
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

  // Check all open tabs against new rules
  enforceOnAllTabs();
}

// ─── Domain Matching ────────────────────────────────────────────────

function isDomainBlocked(hostname, url) {
  hostname = (hostname || '').toLowerCase();
  url = (url || '').toLowerCase();

  // Check exceptions first
  if (isAllowed(hostname)) return false;

  // Entire internet mode
  if (blockEntireInternet) return true;

  // Exact domain match (including subdomains)
  if (blockedDomains.has(hostname)) return true;
  var parts = hostname.split('.');
  for (var i = 1; i < parts.length; i++) {
    var parent = parts.slice(i).join('.');
    if (blockedDomains.has(parent)) return true;
  }

  // Keyword match (in full URL)
  for (var k = 0; k < blockedKeywords.length; k++) {
    if (url.indexOf(blockedKeywords[k]) !== -1) return true;
  }

  // URL path match
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

// ─── Tab Enforcement ────────────────────────────────────────────────

function enforceOnAllTabs() {
  chrome.tabs.query({}, function(tabs) {
    tabs.forEach(function(tab) {
      if (tab.url) {
        checkAndBlockTab(tab);
      }
    });
  });
}

function checkAndBlockTab(tab) {
  try {
    var url = new URL(tab.url);
    // Skip internal pages
    if (url.protocol === 'chrome:' || url.protocol === 'chrome-extension:' ||
        url.protocol === 'about:' || url.protocol === 'moz-extension:' ||
        url.protocol === 'edge:') return;

    if (isDomainBlocked(url.hostname, tab.url)) {
      var blockUrl = chrome.runtime.getURL('blocked.html') +
        '?domain=' + encodeURIComponent(url.hostname) +
        '&url=' + encodeURIComponent(tab.url);
      // Only redirect if not already on block page
      if (!tab.url.startsWith(chrome.runtime.getURL('blocked.html'))) {
        chrome.tabs.update(tab.id, { url: blockUrl });
      }
    }
  } catch (e) {
    // Invalid URL, skip
  }
}

// ─── Navigation Listener ────────────────────────────────────────────

chrome.webNavigation.onBeforeNavigate.addListener(function(details) {
  // Only handle main frame
  if (details.frameId !== 0) return;

  try {
    var url = new URL(details.url);
    if (url.protocol === 'chrome:' || url.protocol === 'chrome-extension:' ||
        url.protocol === 'about:' || url.protocol === 'moz-extension:' ||
        url.protocol === 'edge:') return;

    if (isDomainBlocked(url.hostname, details.url)) {
      var blockUrl = chrome.runtime.getURL('blocked.html') +
        '?domain=' + encodeURIComponent(url.hostname) +
        '&url=' + encodeURIComponent(details.url);
      chrome.tabs.update(details.tabId, { url: blockUrl });
    }
  } catch (e) {
    // Invalid URL
  }
});

// Also catch completed navigations (for pages that loaded before rules updated)
chrome.webNavigation.onCompleted.addListener(function(details) {
  if (details.frameId !== 0) return;
  chrome.tabs.get(details.tabId, function(tab) {
    if (tab && tab.url) checkAndBlockTab(tab);
  });
});

// ─── Badge ──────────────────────────────────────────────────────────

function updateBadge(connected) {
  if (!connected) {
    chrome.action.setBadgeText({ text: '!' });
    chrome.action.setBadgeBackgroundColor({ color: '#ef4444' });
    chrome.action.setTitle({ title: 'Focuser — App not running' });
    return;
  }

  var count = blockedDomains.size + blockedKeywords.length + blockedUrlPaths.length;
  if (blockEntireInternet) count = '∞';

  chrome.action.setBadgeText({ text: count > 0 ? String(count) : '' });
  chrome.action.setBadgeBackgroundColor({ color: '#4e8fff' });
  chrome.action.setTitle({ title: 'Focuser — ' + count + ' sites blocked' });
}

// ─── Polling Loop ───────────────────────────────────────────────────

setInterval(fetchRules, POLL_INTERVAL_MS);
fetchRules(); // Initial fetch

// ─── Message Handler (from popup) ───────────────────────────────────

chrome.runtime.onMessage.addListener(function(msg, sender, sendResponse) {
  if (msg.type === 'get-rules') {
    sendResponse({ rules: currentRules, connected: currentRules !== null });
    return;
  }
  if (msg.type === 'refresh') {
    fetchRules().then(function() {
      sendResponse({ ok: true });
    });
    return true; // async
  }
  if (msg.type === 'check-domain') {
    var blocked = isDomainBlocked(msg.hostname, msg.url);
    sendResponse({ blocked: blocked });
    return;
  }
});

/**
 * Focuser Extension — Popup UI
 */

var API = 'http://127.0.0.1:17549';
var currentDomain = '';
var lists = [];

document.addEventListener('DOMContentLoaded', async function() {
  // Get current tab domain
  chrome.tabs.query({ active: true, currentWindow: true }, async function(tabs) {
    if (tabs[0] && tabs[0].url) {
      try {
        var url = new URL(tabs[0].url);
        currentDomain = url.hostname;
        document.getElementById('current-domain').textContent = currentDomain;
      } catch (e) {
        currentDomain = '';
      }
    }

    await loadData();
  });

  // Button handlers
  document.getElementById('btn-add').addEventListener('click', addOrRemoveSite);
  document.getElementById('btn-open-app').addEventListener('click', async function() {
    try {
      await apiPost('/api/show', {});
    } catch (e) {}
    window.close();
  });
  document.getElementById('list-select').addEventListener('change', function() {
    document.getElementById('btn-add').disabled = !this.value;
  });
});

async function loadData() {
  // Fetch status
  var status = await apiGet('/api/status');
  var statusEl = document.getElementById('status');

  if (!status) {
    statusEl.textContent = 'App not running';
    statusEl.className = 'status offline';
    document.getElementById('btn-add').disabled = true;
    return;
  }

  statusEl.textContent = 'Connected';
  statusEl.className = 'status online';
  document.getElementById('stat-blocked').textContent = status.blocked_sites || 0;
  document.getElementById('stat-today').textContent = status.blocked_today || 0;

  // Fetch lists for dropdown
  lists = await apiGet('/api/lists') || [];
  var select = document.getElementById('list-select');
  select.innerHTML = '<option value="">Select list...</option>';
  lists.forEach(function(l) {
    var opt = document.createElement('option');
    opt.value = l.id;
    opt.textContent = l.name + (l.enabled ? '' : ' (disabled)');
    select.appendChild(opt);
  });

  // Check if current domain is blocked
  if (currentDomain) {
    var check = await apiGet('/api/check?domain=' + encodeURIComponent(currentDomain));
    var statusBadge = document.getElementById('current-status');
    var addBtn = document.getElementById('btn-add');

    if (check && check.blocked) {
      statusBadge.textContent = 'BLOCKED';
      statusBadge.className = 'site-status blocked';
      addBtn.textContent = '− Unblock this site';
      addBtn.className = 'btn-add blocked';
      addBtn.disabled = false;
      addBtn.dataset.mode = 'unblock';
    } else {
      statusBadge.textContent = 'ALLOWED';
      statusBadge.className = 'site-status allowed';
      addBtn.textContent = '+ Block this site';
      addBtn.className = 'btn-add';
      addBtn.dataset.mode = 'block';
    }
  }
}

async function addOrRemoveSite() {
  var btn = document.getElementById('btn-add');
  var mode = btn.dataset.mode;

  if (mode === 'unblock') {
    // Find and remove the rule for this domain from all lists
    var rules = await apiGet('/api/rules');
    if (!rules) return;

    // We need to find which list contains this domain and remove it
    // For simplicity, fetch full lists and find the matching rule
    var fullLists = await apiGet('/api/lists');
    // The API doesn't return full rules in /api/lists, so we use the compiled rules
    // For unblock, we need to try removing from each list
    for (var i = 0; i < lists.length; i++) {
      await apiPost('/api/remove-site', {
        list_id: lists[i].id,
        domain: currentDomain,
      });
    }

    // Refresh background rules
    chrome.runtime.sendMessage({ type: 'refresh' });
    await loadData();
    return;
  }

  // Block mode
  var listId = document.getElementById('list-select').value;
  if (!listId) return;

  btn.disabled = true;
  btn.textContent = 'Adding...';

  var result = await apiPost('/api/add-site', {
    list_id: listId,
    domain: currentDomain,
    rule_type: 'domain',
  });

  if (result && result.ok) {
    // Refresh background rules immediately
    chrome.runtime.sendMessage({ type: 'refresh' });
    // Reload data to show updated status
    await loadData();

    // Block the current tab immediately
    chrome.tabs.query({ active: true, currentWindow: true }, function(tabs) {
      if (tabs[0]) {
        var blockUrl = chrome.runtime.getURL('blocked.html') +
          '?domain=' + encodeURIComponent(currentDomain);
        chrome.tabs.update(tabs[0].id, { url: blockUrl });
      }
    });
  } else {
    btn.disabled = false;
    btn.textContent = '+ Block this site';
  }
}

async function apiGet(path) {
  try {
    var resp = await fetch(API + path);
    if (!resp.ok) return null;
    return await resp.json();
  } catch (e) { return null; }
}

async function apiPost(path, body) {
  try {
    var resp = await fetch(API + path, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(body),
    });
    if (!resp.ok) return null;
    return await resp.json();
  } catch (e) { return null; }
}

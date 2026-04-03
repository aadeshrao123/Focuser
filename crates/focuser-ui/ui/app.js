/**
 * Focuser UI — Main application logic.
 * All event handling via addEventListener — zero inline handlers.
 */

// ─── Tauri Bridge ───────────────────────────────────────────────────

async function invoke(cmd, args) {
  args = args || {};
  if (window.__TAURI__) {
    return window.__TAURI__.core.invoke(cmd, args);
  }
  throw new Error('Not running in Tauri');
}

// ─── State ──────────────────────────────────────────────────────────

const state = {
  blockLists: [],
  serviceOnline: false,
  currentPage: 'dashboard',
};

// ─── UI Controller ──────────────────────────────────────────────────

const ui = {
  navigateTo(page) {
    state.currentPage = page;
    document.querySelectorAll('.page').forEach(function(p) { p.classList.remove('active'); });
    document.querySelectorAll('.nav-item').forEach(function(n) { n.classList.remove('active'); });

    var pageEl = document.getElementById('page-' + page);
    var navEl = document.querySelector('[data-page="' + page + '"]');
    if (pageEl) pageEl.classList.add('active');
    if (navEl) navEl.classList.add('active');

    switch (page) {
      case 'dashboard': this.refreshDashboard(); break;
      case 'blocklists': this.refreshBlockLists(); break;
      case 'websites': this.refreshWebsites(); break;
      case 'apps': this.refreshApps(); break;
      case 'schedule': this.refreshSchedule(); break;
      case 'statistics': this.refreshStatistics(); break;
      case 'settings': this.refreshSettings(); break;
    }
  },

  async refreshDashboard() {
    await this.refreshStatus();
    this.renderDashboardCharts();
  },

  async refreshStatus() {
    try {
      var status = await invoke('get_status');
      state.serviceOnline = true;
      document.getElementById('stat-active-blocks').textContent = status.active_blocks.length;
      document.getElementById('stat-blocked-today').textContent = status.total_blocked_today;
      document.getElementById('stat-uptime').textContent = formatUptime(status.uptime_seconds);
      var totalSites = status.active_blocks.reduce(function(sum, b) { return sum + b.blocked_websites; }, 0);
      document.getElementById('stat-total-sites').textContent = totalSites;

      var container = document.getElementById('dashboard-active-lists');
      if (status.active_blocks.length === 0) {
        container.innerHTML = '<div class="empty-state">No active blocks</div>';
      } else {
        container.innerHTML = status.active_blocks.map(function(b) {
          return '<div class="rule-item"><div class="rule-info">' +
            '<span class="rule-value">' + escHtml(b.block_list_name) + '</span>' +
            '<span class="rule-list-name">' + b.blocked_websites + ' sites, ' + b.blocked_apps + ' apps</span>' +
            '</div><span class="rule-type-badge domain">Active</span></div>';
        }).join('');
      }
      this.updateServiceIndicator(true);
      var el = document.getElementById('setting-service-status');
      if (el) el.textContent = 'Running — uptime ' + formatUptime(status.uptime_seconds);
    } catch (e) {
      state.serviceOnline = false;
      this.updateServiceIndicator(false);
      var el2 = document.getElementById('setting-service-status');
      if (el2) el2.textContent = 'Offline — start the service';
    }
  },

  updateServiceIndicator(online) {
    var el = document.getElementById('service-status');
    if (!el) return;
    el.className = 'status-indicator ' + (online ? 'online' : 'offline');
    el.querySelector('.status-text').textContent = online ? 'Service Online' : 'Service Offline';
  },

  renderDashboardCharts() {
    var hours = [];
    for (var i = 0; i < 12; i++) hours.push(((i + 8) % 24) + ':00');
    var hourlyData = [];
    for (var j = 0; j < 12; j++) hourlyData.push(Math.floor(Math.random() * 20));
    Charts.bar('chart-today', { labels: hours, values: hourlyData, color: Charts.colors.red });

    var days = ['Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat', 'Sun'];
    var weeklyData = [];
    for (var k = 0; k < 7; k++) weeklyData.push(Math.floor(Math.random() * 100));
    Charts.line('chart-weekly', { labels: days, values: weeklyData, color: Charts.colors.blue });
  },

  // ── Block Lists ─────────────────────────────────────────────
  async refreshBlockLists() {
    try { state.blockLists = await invoke('list_block_lists'); }
    catch (e) { state.blockLists = []; }
    this.renderBlockLists();
    this.updateListSelects();
  },

  renderBlockLists() {
    var container = document.getElementById('blocklists-container');
    if (state.blockLists.length === 0) {
      container.innerHTML = '<div class="empty-state"><p>No block lists yet.</p><p class="muted">Create one to start blocking distracting websites and apps.</p></div>';
      return;
    }

    container.innerHTML = state.blockLists.map(function(list) {
      return '<div class="blocklist-card" data-id="' + list.id + '">' +
        '<div class="blocklist-card-header">' +
          '<span class="blocklist-card-name">' + escHtml(list.name) + '</span>' +
          '<label class="toggle"><input type="checkbox" data-action="toggle-list" data-list-id="' + list.id + '"' + (list.enabled ? ' checked' : '') + '><span class="toggle-slider"></span></label>' +
        '</div>' +
        '<div class="blocklist-card-meta">' +
          '<span>' + svgIcon('globe', 13) + ' ' + list.websites.length + ' websites</span>' +
          '<span>' + svgIcon('monitor', 13) + ' ' + list.applications.length + ' apps</span>' +
          '<span>' + svgIcon('shield', 13) + ' ' + list.exceptions.length + ' exceptions</span>' +
        '</div>' +
        '<div class="blocklist-card-actions">' +
          '<button class="btn btn-secondary btn-sm" data-action="edit-list" data-list-id="' + list.id + '">Edit</button>' +
          '<button class="btn btn-danger btn-sm" data-action="delete-list" data-list-id="' + list.id + '" data-list-name="' + escHtml(list.name) + '">Delete</button>' +
        '</div>' +
      '</div>';
    }).join('');
  },

  updateListSelects() {
    var options = state.blockLists.map(function(l) {
      return '<option value="' + l.id + '">' + escHtml(l.name) + '</option>';
    }).join('');
    var defaultOpt = '<option value="">Select a block list...</option>';
    ['website-list-select', 'app-list-select', 'schedule-list-select'].forEach(function(id) {
      var el = document.getElementById(id);
      if (el) el.innerHTML = defaultOpt + options;
    });
  },

  async toggleList(id, enabled) {
    try {
      await invoke('toggle_block_list', { id: id, enabled: enabled });
      toast(enabled ? 'Block list enabled' : 'Block list disabled', 'success');
    } catch (e) { toast('Failed: ' + e, 'error'); }
  },

  async deleteList(id, name) {
    if (!confirm('Delete "' + name + '"? This cannot be undone.')) return;
    try {
      await invoke('delete_block_list', { id: id });
      toast('Block list deleted', 'success');
      this.refreshBlockLists();
    } catch (e) { toast('Failed: ' + e, 'error'); }
  },

  editList(id) {
    var list = state.blockLists.find(function(l) { return l.id === id; });
    if (!list) return;
    toast('Editing "' + list.name + '" — use Websites/Apps pages to modify rules', 'info');
  },

  // ── Modal ───────────────────────────────────────────────────
  showCreateListModal() {
    document.getElementById('modal-title').textContent = 'Create Block List';
    document.getElementById('modal-body').innerHTML =
      '<label style="display:block;margin-bottom:6px;font-size:13px;color:var(--text-secondary);">Block list name</label>' +
      '<input type="text" id="modal-list-name" class="input" style="width:100%;" placeholder="e.g., Social Media">';
    var confirmBtn = document.getElementById('modal-confirm');
    confirmBtn.textContent = 'Create';
    confirmBtn.setAttribute('data-action', 'confirm-create-list');
    document.getElementById('modal-overlay').classList.remove('hidden');
    setTimeout(function() {
      var inp = document.getElementById('modal-list-name');
      if (inp) inp.focus();
    }, 100);
  },

  async createList() {
    var input = document.getElementById('modal-list-name');
    var name = input ? input.value.trim() : '';
    if (!name) { toast('Enter a name', 'error'); return; }
    try {
      await invoke('create_block_list', { name: name });
      toast('Created "' + name + '"', 'success');
      this.closeModal();
      this.refreshBlockLists();
    } catch (e) { toast('Failed: ' + e, 'error'); }
  },

  closeModal() {
    document.getElementById('modal-overlay').classList.add('hidden');
  },

  // ── Websites ────────────────────────────────────────────────
  refreshWebsites() {
    this.updateListSelects();
    var container = document.getElementById('websites-list');
    var allRules = [];
    state.blockLists.forEach(function(list) {
      list.websites.forEach(function(rule) {
        allRules.push({ id: rule.id, match_type: rule.match_type, enabled: rule.enabled, listName: list.name, listId: list.id });
      });
    });

    if (allRules.length === 0) {
      container.innerHTML = '<div class="empty-state">No websites blocked yet</div>';
      return;
    }
    container.innerHTML = allRules.map(function(r) {
      var type = getMatchTypeName(r.match_type);
      var value = getMatchTypeValue(r.match_type);
      return '<div class="rule-item">' +
        '<div class="rule-info">' +
          '<span class="rule-type-badge ' + type + '">' + type + '</span>' +
          '<span class="rule-value">' + escHtml(value) + '</span>' +
          '<span class="rule-list-name">' + escHtml(r.listName) + '</span>' +
        '</div>' +
        '<button class="btn-icon" data-action="remove-website" data-list-id="' + r.listId + '" data-rule-id="' + r.id + '" title="Remove">' +
          svgIcon('x', 16) +
        '</button></div>';
    }).join('');
  },

  async addWebsite() {
    var listId = document.getElementById('website-list-select').value;
    var ruleType = document.getElementById('website-type-select').value;
    var value = document.getElementById('website-input').value.trim();
    if (!listId) { toast('Select a block list', 'error'); return; }
    if (!value) { toast('Enter a website', 'error'); return; }

    try {
      await invoke('add_website_rule', { listId: listId, ruleType: ruleType, value: value });
      document.getElementById('website-input').value = '';
      toast('Blocked ' + value, 'success');
      await this.refreshBlockLists();
      this.refreshWebsites();
    } catch (e) {
      toast('Failed: ' + e, 'error');
    }
  },

  async removeWebsite(listId, ruleId) {
    try {
      await invoke('remove_website_rule', { listId: listId, ruleId: ruleId });
      toast('Removed', 'success');
      await this.refreshBlockLists();
      this.refreshWebsites();
    } catch (e) { toast('Failed: ' + e, 'error'); }
  },

  // ── Applications ────────────────────────────────────────────
  refreshApps() {
    this.updateListSelects();
    var container = document.getElementById('apps-list');
    var allRules = [];
    state.blockLists.forEach(function(list) {
      list.applications.forEach(function(rule) {
        allRules.push({ id: rule.id, match_type: rule.match_type, enabled: rule.enabled, listName: list.name, listId: list.id });
      });
    });
    if (allRules.length === 0) {
      container.innerHTML = '<div class="empty-state">No applications blocked yet</div>';
      return;
    }
    container.innerHTML = allRules.map(function(r) {
      var type = getAppMatchTypeName(r.match_type);
      var value = getAppMatchTypeValue(r.match_type);
      return '<div class="rule-item"><div class="rule-info">' +
        '<span class="rule-type-badge">' + type + '</span>' +
        '<span class="rule-value">' + escHtml(value) + '</span>' +
        '<span class="rule-list-name">' + escHtml(r.listName) + '</span>' +
        '</div>' +
        '<button class="btn-icon" data-action="remove-app" data-list-id="' + r.listId + '" data-rule-id="' + r.id + '" title="Remove">' +
          svgIcon('x', 16) +
        '</button></div>';
    }).join('');
  },

  async addApp() {
    var listId = document.getElementById('app-list-select').value;
    var ruleType = document.getElementById('app-type-select').value;
    var value = document.getElementById('app-input').value.trim();
    if (!listId) { toast('Select a block list', 'error'); return; }
    if (!value) { toast('Enter an application', 'error'); return; }

    try {
      await invoke('add_app_rule', { listId: listId, ruleType: ruleType, value: value });
      document.getElementById('app-input').value = '';
      toast('Blocked ' + value, 'success');
      await this.refreshBlockLists();
      this.refreshApps();
    } catch (e) {
      toast('Failed: ' + e, 'error');
    }
  },

  async removeApp(listId, ruleId) {
    try {
      await invoke('remove_app_rule', { listId: listId, ruleId: ruleId });
      toast('Removed', 'success');
      await this.refreshBlockLists();
      this.refreshApps();
    } catch (e) { toast('Failed: ' + e, 'error'); }
  },

  // ── Schedule ────────────────────────────────────────────────
  refreshSchedule() {
    this.updateListSelects();
    this.renderScheduleGrid();
  },

  renderScheduleGrid() {
    var grid = document.getElementById('schedule-grid');
    var days = ['Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat', 'Sun'];
    var html = '<div class="schedule-header"></div>';
    for (var h = 0; h < 24; h++) {
      html += '<div class="schedule-header">' + h + '</div>';
    }
    for (var d = 0; d < days.length; d++) {
      html += '<div class="schedule-day-label">' + days[d] + '</div>';
      for (var hr = 0; hr < 24; hr++) {
        html += '<div class="schedule-cell" data-action="toggle-schedule" data-day="' + days[d] + '" data-hour="' + hr + '"></div>';
      }
    }
    grid.innerHTML = html;
  },

  // ── Statistics ──────────────────────────────────────────────
  refreshStatistics() {
    var days = [];
    for (var i = 6; i >= 0; i--) {
      var d = new Date();
      d.setDate(d.getDate() - i);
      days.push(d.toLocaleDateString('en', { weekday: 'short' }));
    }
    var vals = [];
    for (var j = 0; j < 7; j++) vals.push(Math.floor(Math.random() * 80));
    Charts.line('chart-stats-timeline', { labels: days, values: vals, color: Charts.colors.blue });

    Charts.horizontalBar('chart-stats-top', {
      labels: ['reddit.com', 'twitter.com', 'youtube.com', 'facebook.com', 'instagram.com'],
      values: [142, 98, 87, 64, 51],
    });
  },

  // ── Settings ────────────────────────────────────────────────
  async refreshSettings() {
    await this.refreshStatus();
    try {
      var caps = await invoke('get_capabilities');
      var parts = [];
      if (caps.hosts_file) parts.push('Hosts file blocking');
      if (caps.extension_connected) parts.push('Browser extension');
      if (parts.length === 0) parts.push('None — service may not be running');
      document.getElementById('setting-capabilities').textContent = parts.join(', ');
    } catch (e) {
      document.getElementById('setting-capabilities').textContent = 'Cannot check — service offline';
    }
  },
};

// ─── SVG Icon Helper ────────────────────────────────────────────────

function svgIcon(name, size) {
  var s = size || 16;
  var icons = {
    x: '<line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/>',
    globe: '<circle cx="12" cy="12" r="10"/><line x1="2" y1="12" x2="22" y2="12"/><path d="M12 2a15.3 15.3 0 0 1 4 10 15.3 15.3 0 0 1-4 10"/>',
    monitor: '<rect x="2" y="3" width="20" height="14" rx="2"/><line x1="8" y1="21" x2="16" y2="21"/><line x1="12" y1="17" x2="12" y2="21"/>',
    shield: '<path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z"/>',
  };
  var paths = icons[name] || '';
  return '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="' + s + '" height="' + s + '">' + paths + '</svg>';
}

// ─── Helpers ────────────────────────────────────────────────────────

function escHtml(s) {
  if (!s) return '';
  return s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');
}

function formatUptime(seconds) {
  if (!seconds || seconds < 0) return '--';
  var h = Math.floor(seconds / 3600);
  var m = Math.floor((seconds % 3600) / 60);
  if (h > 0) return h + 'h ' + m + 'm';
  return m + 'm';
}

function getMatchTypeName(mt) {
  if (mt.Domain !== undefined) return 'domain';
  if (mt.Keyword !== undefined) return 'keyword';
  if (mt.Wildcard !== undefined) return 'wildcard';
  if (mt.UrlPath !== undefined) return 'url_path';
  if (mt === 'EntireInternet') return 'all';
  return 'unknown';
}

function getMatchTypeValue(mt) {
  if (mt.Domain !== undefined) return mt.Domain;
  if (mt.Keyword !== undefined) return mt.Keyword;
  if (mt.Wildcard !== undefined) return mt.Wildcard;
  if (mt.UrlPath !== undefined) return mt.UrlPath;
  if (mt === 'EntireInternet') return '* (Entire Internet)';
  return JSON.stringify(mt);
}

function getAppMatchTypeName(mt) {
  if (mt.ExecutableName !== undefined) return 'exe';
  if (mt.ExecutablePath !== undefined) return 'path';
  if (mt.WindowTitle !== undefined) return 'title';
  if (mt.BundleId !== undefined) return 'bundle';
  return 'unknown';
}

function getAppMatchTypeValue(mt) {
  return mt.ExecutableName || mt.ExecutablePath || mt.WindowTitle || mt.BundleId || JSON.stringify(mt);
}

function toast(message, type) {
  type = type || 'info';
  var container = document.getElementById('toast-container');
  var el = document.createElement('div');
  el.className = 'toast ' + type;
  el.textContent = message;
  container.appendChild(el);
  setTimeout(function() {
    el.style.opacity = '0';
    setTimeout(function() { el.remove(); }, 200);
  }, 3000);
}


// ─── Global Event Delegation ────────────────────────────────────────

document.addEventListener('click', function(e) {
  // Walk up from target to find the closest element with data-action
  var el = e.target;
  while (el && el !== document.body) {
    var action = el.getAttribute('data-action');
    if (action) {
      e.preventDefault();
      handleAction(action, el);
      return;
    }
    // Check for nav-item
    if (el.classList && el.classList.contains('nav-item') && el.dataset.page) {
      ui.navigateTo(el.dataset.page);
      return;
    }
    el = el.parentElement;
  }

  // Modal overlay close (click on overlay background)
  if (e.target.id === 'modal-overlay') {
    ui.closeModal();
  }
});

// Handle checkbox changes via delegation
document.addEventListener('change', function(e) {
  var el = e.target;
  if (el.getAttribute('data-action') === 'toggle-list') {
    var listId = el.getAttribute('data-list-id');
    ui.toggleList(listId, el.checked);
  }
});

function handleAction(action, el) {
  switch (action) {
    case 'delete-list':
      ui.deleteList(el.getAttribute('data-list-id'), el.getAttribute('data-list-name'));
      break;
    case 'edit-list':
      ui.editList(el.getAttribute('data-list-id'));
      break;
    case 'remove-website':
      ui.removeWebsite(el.getAttribute('data-list-id'), el.getAttribute('data-rule-id'));
      break;
    case 'remove-app':
      ui.removeApp(el.getAttribute('data-list-id'), el.getAttribute('data-rule-id'));
      break;
    case 'toggle-schedule':
      el.classList.toggle('active');
      break;
    case 'confirm-create-list':
      ui.createList();
      break;
  }
}

// ─── Static Button Bindings ─────────────────────────────────────────

document.addEventListener('DOMContentLoaded', async function() {
  // Navigation is handled by delegation above

  // Static buttons
  var btnNew = document.getElementById('btn-new-blocklist');
  if (btnNew) btnNew.addEventListener('click', function() { ui.showCreateListModal(); });

  var btnAddWeb = document.getElementById('btn-add-website');
  if (btnAddWeb) btnAddWeb.addEventListener('click', function() { ui.addWebsite(); });

  var btnAddApp = document.getElementById('btn-add-app');
  if (btnAddApp) btnAddApp.addEventListener('click', function() { ui.addApp(); });

  var btnRefresh = document.getElementById('btn-refresh-service');
  if (btnRefresh) btnRefresh.addEventListener('click', function() { ui.refreshStatus(); });

  var btnModalClose = document.getElementById('btn-modal-close');
  if (btnModalClose) btnModalClose.addEventListener('click', function() { ui.closeModal(); });

  var btnModalCancel = document.getElementById('btn-modal-cancel');
  if (btnModalCancel) btnModalCancel.addEventListener('click', function() { ui.closeModal(); });

  // Escape to close modal
  document.addEventListener('keydown', function(e) {
    if (e.key === 'Escape') ui.closeModal();
  });

  // Enter to submit in modal
  document.addEventListener('keydown', function(e) {
    if (e.key === 'Enter') {
      var overlay = document.getElementById('modal-overlay');
      if (overlay && !overlay.classList.contains('hidden')) {
        var confirmBtn = document.getElementById('modal-confirm');
        if (confirmBtn) confirmBtn.click();
      }
    }
  });

  // Load initial data
  try { state.blockLists = await invoke('list_block_lists'); }
  catch (e) { state.blockLists = []; }

  // Render dashboard
  ui.navigateTo('dashboard');

  // Periodic refresh
  setInterval(function() {
    if (state.currentPage === 'dashboard') ui.refreshStatus();
  }, 5000);

  console.log('[Focuser] UI initialized');
});

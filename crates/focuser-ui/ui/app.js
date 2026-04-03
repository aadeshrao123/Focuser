/**
 * Focuser UI — Real data only, no mocks.
 */

async function invoke(cmd, args) {
  args = args || {};
  if (window.__TAURI__) return window.__TAURI__.core.invoke(cmd, args);
  throw new Error('Not running in Tauri');
}

var state = { blockLists: [], currentPage: 'dashboard' };

// ─── Disable right-click context menu ───────────────────────────────
document.addEventListener('contextmenu', function(e) { e.preventDefault(); });

// ─── UI ─────────────────────────────────────────────────────────────

var ui = {
  navigateTo: function(page) {
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
    }
  },

  async refreshDashboard() {
    try {
      var status = await invoke('get_status');
      document.getElementById('stat-active-blocks').textContent = status.active_blocks.length;
      document.getElementById('stat-blocked-today').textContent = status.total_blocked_today;
      var totalSites = 0, totalApps = 0;
      status.active_blocks.forEach(function(b) { totalSites += b.blocked_websites; totalApps += b.blocked_apps; });
      document.getElementById('stat-total-sites').textContent = totalSites;
      document.getElementById('stat-total-apps').textContent = totalApps;

      var container = document.getElementById('dashboard-active-lists');
      if (status.active_blocks.length === 0) {
        container.innerHTML = '<div class="empty-state">No active blocks</div>';
      } else {
        container.innerHTML = status.active_blocks.map(function(b) {
          return '<div class="rule-item"><div class="rule-info">' +
            '<span class="rule-value">' + esc(b.block_list_name) + '</span>' +
            '<span class="rule-list-name">' + b.blocked_websites + ' sites, ' + b.blocked_apps + ' apps</span>' +
            '</div></div>';
        }).join('');
      }
    } catch (e) {}
  },

  async refreshBlockLists() {
    try { state.blockLists = await invoke('list_block_lists'); } catch (e) { state.blockLists = []; }
    this.renderBlockLists();
    this.updateSelects();
  },

  renderBlockLists: function() {
    var c = document.getElementById('blocklists-container');
    if (state.blockLists.length === 0) { c.innerHTML = '<div class="empty-state">No block lists yet</div>'; return; }
    c.innerHTML = state.blockLists.map(function(l) {
      return '<div class="blocklist-card" data-id="' + l.id + '">' +
        '<div class="blocklist-card-header"><span class="blocklist-card-name">' + esc(l.name) + '</span>' +
        '<label class="toggle"><input type="checkbox" data-action="toggle-list" data-list-id="' + l.id + '"' + (l.enabled ? ' checked' : '') + '><span class="toggle-slider"></span></label></div>' +
        '<div class="blocklist-card-meta"><span>' + l.websites.length + ' sites</span><span>' + l.applications.length + ' apps</span><span>' + l.exceptions.length + ' exceptions</span></div>' +
        '<div class="blocklist-card-actions">' +
        '<button class="btn btn-danger btn-sm" data-action="delete-list" data-list-id="' + l.id + '" data-list-name="' + esc(l.name) + '">Delete</button></div></div>';
    }).join('');
  },

  updateSelects: function() {
    var opts = state.blockLists.map(function(l) { return '<option value="' + l.id + '">' + esc(l.name) + '</option>'; }).join('');
    var def = '<option value="">Select list...</option>';
    ['website-list-select','app-list-select','schedule-list-select'].forEach(function(id) {
      var el = document.getElementById(id);
      if (el) el.innerHTML = def + opts;
    });
  },

  async toggleList(id, enabled) {
    try { await invoke('toggle_block_list', { id: id, enabled: enabled }); toast(enabled ? 'Enabled' : 'Disabled', 'success'); }
    catch (e) { toast('Failed: ' + e, 'error'); }
  },

  async deleteList(id, name) {
    if (!confirm('Delete "' + name + '"?')) return;
    try { await invoke('delete_block_list', { id: id }); toast('Deleted', 'success'); this.refreshBlockLists(); }
    catch (e) { toast('Failed: ' + e, 'error'); }
  },

  showCreateListModal: function() {
    document.getElementById('modal-title').textContent = 'New Block List';
    document.getElementById('modal-body').innerHTML = '<label style="display:block;margin-bottom:4px;font-size:12px;color:var(--text-muted);">Name</label><input type="text" id="modal-list-name" class="input" style="width:100%;" placeholder="e.g. Social Media">';
    document.getElementById('modal-confirm').textContent = 'Create';
    document.getElementById('modal-confirm').setAttribute('data-action', 'confirm-create-list');
    document.getElementById('modal-overlay').classList.remove('hidden');
    setTimeout(function() { var i = document.getElementById('modal-list-name'); if (i) i.focus(); }, 80);
  },

  async createList() {
    var i = document.getElementById('modal-list-name');
    var name = i ? i.value.trim() : '';
    if (!name) { toast('Enter a name', 'error'); return; }
    try { await invoke('create_block_list', { name: name }); toast('Created', 'success'); this.closeModal(); this.refreshBlockLists(); }
    catch (e) { toast('Failed: ' + e, 'error'); }
  },

  closeModal: function() { document.getElementById('modal-overlay').classList.add('hidden'); },

  refreshWebsites: function() {
    this.updateSelects();
    var c = document.getElementById('websites-list');
    var all = [];
    state.blockLists.forEach(function(l) { l.websites.forEach(function(r) { all.push({ id: r.id, match_type: r.match_type, listName: l.name, listId: l.id }); }); });
    if (all.length === 0) { c.innerHTML = '<div class="empty-state">No websites blocked</div>'; return; }
    c.innerHTML = all.map(function(r) {
      var t = mtName(r.match_type), v = mtVal(r.match_type);
      return '<div class="rule-item"><div class="rule-info"><span class="rule-type-badge ' + t + '">' + t + '</span><span class="rule-value">' + esc(v) + '</span><span class="rule-list-name">' + esc(r.listName) + '</span></div>' +
        '<button class="btn-icon" data-action="remove-website" data-list-id="' + r.listId + '" data-rule-id="' + r.id + '">' + ico('x',14) + '</button></div>';
    }).join('');
  },

  async addWebsite() {
    var lid = document.getElementById('website-list-select').value;
    var rt = document.getElementById('website-type-select').value;
    var v = document.getElementById('website-input').value.trim();
    if (!lid) { toast('Select a list', 'error'); return; }
    if (!v) { toast('Enter a website', 'error'); return; }
    try { await invoke('add_website_rule', { listId: lid, ruleType: rt, value: v }); document.getElementById('website-input').value = ''; toast('Blocked ' + v, 'success'); await this.refreshBlockLists(); this.refreshWebsites(); }
    catch (e) { toast('Failed: ' + e, 'error'); }
  },

  async removeWebsite(lid, rid) {
    try { await invoke('remove_website_rule', { listId: lid, ruleId: rid }); toast('Removed', 'success'); await this.refreshBlockLists(); this.refreshWebsites(); }
    catch (e) { toast('Failed: ' + e, 'error'); }
  },

  refreshApps: function() {
    this.updateSelects();
    var c = document.getElementById('apps-list');
    var all = [];
    state.blockLists.forEach(function(l) { l.applications.forEach(function(r) { all.push({ id: r.id, match_type: r.match_type, listName: l.name, listId: l.id }); }); });
    if (all.length === 0) { c.innerHTML = '<div class="empty-state">No applications blocked</div>'; return; }
    c.innerHTML = all.map(function(r) {
      var t = amtName(r.match_type), v = amtVal(r.match_type);
      return '<div class="rule-item"><div class="rule-info"><span class="rule-type-badge">' + t + '</span><span class="rule-value">' + esc(v) + '</span><span class="rule-list-name">' + esc(r.listName) + '</span></div>' +
        '<button class="btn-icon" data-action="remove-app" data-list-id="' + r.listId + '" data-rule-id="' + r.id + '">' + ico('x',14) + '</button></div>';
    }).join('');
  },

  async addApp() {
    var lid = document.getElementById('app-list-select').value;
    var rt = document.getElementById('app-type-select').value;
    var v = document.getElementById('app-input').value.trim();
    if (!lid) { toast('Select a list', 'error'); return; }
    if (!v) { toast('Enter an application', 'error'); return; }
    try { await invoke('add_app_rule', { listId: lid, ruleType: rt, value: v }); document.getElementById('app-input').value = ''; toast('Blocked ' + v, 'success'); await this.refreshBlockLists(); this.refreshApps(); }
    catch (e) { toast('Failed: ' + e, 'error'); }
  },

  async removeApp(lid, rid) {
    try { await invoke('remove_app_rule', { listId: lid, ruleId: rid }); toast('Removed', 'success'); await this.refreshBlockLists(); this.refreshApps(); }
    catch (e) { toast('Failed: ' + e, 'error'); }
  },

  refreshSchedule: function() {
    this.updateSelects();
    var grid = document.getElementById('schedule-grid');
    var days = ['Mon','Tue','Wed','Thu','Fri','Sat','Sun'];
    var html = '<div class="schedule-header"></div>';
    for (var h = 0; h < 24; h++) html += '<div class="schedule-header">' + h + '</div>';
    for (var d = 0; d < days.length; d++) {
      html += '<div class="schedule-day-label">' + days[d] + '</div>';
      for (var hr = 0; hr < 24; hr++) html += '<div class="schedule-cell" data-action="toggle-schedule" data-day="' + days[d] + '" data-hour="' + hr + '"></div>';
    }
    grid.innerHTML = html;
  },

  async refreshStatistics() {
    var c = document.getElementById('stats-content');
    try {
      var today = new Date().toISOString().split('T')[0];
      var week = new Date(Date.now() - 7*86400000).toISOString().split('T')[0];
      var stats = await invoke('get_stats', { from: week, to: today });
      if (!stats || stats.length === 0) { c.innerHTML = '<div class="empty-state">No data yet. Blocked attempts will appear here.</div>'; return; }
      var html = '<table class="data-table"><thead><tr><th>Domain / App</th><th>Blocked</th><th>Date</th></tr></thead><tbody>';
      stats.forEach(function(s) { html += '<tr><td style="font-family:var(--font-mono);font-size:13px;">' + esc(s.domain_or_app) + '</td><td>' + s.blocked_attempts + '</td><td style="color:var(--text-muted)">' + s.date + '</td></tr>'; });
      html += '</tbody></table>';
      c.innerHTML = html;
    } catch (e) { c.innerHTML = '<div class="empty-state">No data yet</div>'; }
  },

  // ── Exceptions ──────────────────────────────────────────────
  refreshExceptions: function() {
    this.updateSelects();
    var sel = document.getElementById('exception-list-select');
    if (sel) {
      var opts = state.blockLists.map(function(l) { return '<option value="' + l.id + '">' + esc(l.name) + '</option>'; }).join('');
      sel.innerHTML = '<option value="">Select list...</option>' + opts;
    }
    var c = document.getElementById('exceptions-list');
    var all = [];
    state.blockLists.forEach(function(l) {
      l.exceptions.forEach(function(e) {
        var val = '';
        if (e.exception_type.Domain !== undefined) val = e.exception_type.Domain;
        else if (e.exception_type.Wildcard !== undefined) val = e.exception_type.Wildcard;
        else if (e.exception_type === 'LocalFiles') val = 'file://*';
        all.push({ id: e.id, value: val, listName: l.name, listId: l.id });
      });
    });
    if (all.length === 0) { c.innerHTML = '<div class="empty-state">No exceptions — all matching sites will be blocked</div>'; return; }
    c.innerHTML = all.map(function(r) {
      return '<div class="rule-item"><div class="rule-info"><span class="rule-type-badge" style="background:var(--success-muted);color:var(--success);">allowed</span><span class="rule-value">' + esc(r.value) + '</span><span class="rule-list-name">' + esc(r.listName) + '</span></div>' +
        '<button class="btn-icon" data-action="remove-exception" data-list-id="' + r.listId + '" data-exc-id="' + r.id + '">' + ico('x',14) + '</button></div>';
    }).join('');
  },

  async addException() {
    var lid = document.getElementById('exception-list-select').value;
    var v = document.getElementById('exception-input').value.trim();
    if (!lid) { toast('Select a list', 'error'); return; }
    if (!v) { toast('Enter a domain', 'error'); return; }
    try {
      await invoke('add_exception', { listId: lid, domain: v, exceptionType: 'domain' });
      document.getElementById('exception-input').value = '';
      toast('Exception added: ' + v, 'success');
      await this.refreshBlockLists();
      this.refreshExceptions();
    } catch (e) { toast('Failed: ' + e, 'error'); }
  },

  async removeException(lid, eid) {
    try {
      await invoke('remove_exception', { listId: lid, exceptionId: eid });
      toast('Exception removed', 'success');
      await this.refreshBlockLists();
      this.refreshExceptions();
    } catch (e) { toast('Failed: ' + e, 'error'); }
  },

  // ── Import ──────────────────────────────────────────────────
  async importFromText() {
    var lid = document.getElementById('website-list-select').value;
    if (!lid) { toast('Select a list first', 'error'); return; }

    var input = document.createElement('input');
    input.type = 'file';
    input.accept = '.txt,.csv,.json';
    input.onchange = async function(e) {
      var file = e.target.files[0];
      if (!file) return;
      var text = await file.text();
      var domains = [];

      if (file.name.endsWith('.json')) {
        try {
          var data = JSON.parse(text);
          if (Array.isArray(data)) domains = data;
          else if (data.domains) domains = data.domains;
        } catch (err) { toast('Invalid JSON', 'error'); return; }
      } else {
        domains = text.split(/[\r\n,;]+/).map(function(s) { return s.trim(); }).filter(function(s) { return s && !s.startsWith('#'); });
      }

      if (domains.length === 0) { toast('No domains found in file', 'error'); return; }
      try {
        var result = await invoke('bulk_import_websites', { listId: lid, domains: domains, ruleType: 'domain' });
        toast('Imported ' + result.added + ' domains', 'success');
        await ui.refreshBlockLists();
        ui.refreshWebsites();
      } catch (err) { toast('Import failed: ' + err, 'error'); }
    };
    input.click();
  },

  async importPremadeList(category) {
    var lid = document.getElementById('website-list-select').value;
    if (!lid) { toast('Select a list first', 'error'); return; }

    try {
      var resp = await fetch('premade-lists.json');
      var data = await resp.json();
      var cat = data.categories[category];
      if (!cat) { toast('Category not found', 'error'); return; }
      if (!cat.domains || cat.domains.length === 0) { toast(cat.name + ' list is empty — add websites to premade-lists.json', 'info'); return; }
      var result = await invoke('bulk_import_websites', { listId: lid, domains: cat.domains, ruleType: 'domain' });
      toast('Imported ' + result.added + ' ' + cat.name + ' domains', 'success');
      await this.refreshBlockLists();
      this.refreshWebsites();
    } catch (e) { toast('Failed: ' + e, 'error'); }
  },

  async importEntireInternet() {
    var lid = document.getElementById('website-list-select').value;
    if (!lid) { toast('Select a list first', 'error'); return; }
    if (!confirm('Block the entire internet? Only exceptions will be accessible.')) return;
    try {
      await invoke('add_website_rule', { listId: lid, ruleType: 'entire_internet', value: '*' });
      toast('Entire internet blocked', 'success');
      await this.refreshBlockLists();
      this.refreshWebsites();
    } catch (e) { toast('Failed: ' + e, 'error'); }
  },

  async importKeywordPrompt() {
    var lid = document.getElementById('website-list-select').value;
    if (!lid) { toast('Select a list first', 'error'); return; }
    var kw = prompt('Block all URLs containing this word:');
    if (!kw || !kw.trim()) return;
    try {
      await invoke('add_website_rule', { listId: lid, ruleType: 'keyword', value: kw.trim() });
      toast('Keyword blocked: ' + kw.trim(), 'success');
      await this.refreshBlockLists();
      this.refreshWebsites();
    } catch (e) { toast('Failed: ' + e, 'error'); }
  },

  toggleImportDropdown: function() {
    var dd = document.getElementById('import-dropdown');
    dd.classList.toggle('hidden');
  },

  async loadPremadeLists() {
    try {
      var resp = await fetch('premade-lists.json');
      var data = await resp.json();
      var container = document.getElementById('premade-list-items');
      if (!container) return;
      var html = '';
      Object.keys(data.categories).forEach(function(key) {
        var cat = data.categories[key];
        html += '<button class="dropdown-item" data-action="import-premade" data-category="' + key + '">' + esc(cat.name) + '</button>';
      });
      container.innerHTML = html;
    } catch (e) {}
  },
};

// ─── Helpers ────────────────────────────────────────────────────────

function esc(s) { return s ? s.replace(/&/g,'&amp;').replace(/</g,'&lt;').replace(/>/g,'&gt;').replace(/"/g,'&quot;') : ''; }

function mtName(m) { if (m.Domain !== undefined) return 'domain'; if (m.Keyword !== undefined) return 'keyword'; if (m.Wildcard !== undefined) return 'wildcard'; if (m.UrlPath !== undefined) return 'url_path'; return 'other'; }
function mtVal(m) { return m.Domain || m.Keyword || m.Wildcard || m.UrlPath || JSON.stringify(m); }
function amtName(m) { if (m.ExecutableName !== undefined) return 'exe'; if (m.ExecutablePath !== undefined) return 'path'; if (m.WindowTitle !== undefined) return 'title'; return 'other'; }
function amtVal(m) { return m.ExecutableName || m.ExecutablePath || m.WindowTitle || JSON.stringify(m); }

function ico(name, sz) {
  var p = { x: '<line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/>' };
  return '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="' + (sz||16) + '" height="' + (sz||16) + '">' + (p[name]||'') + '</svg>';
}

function toast(msg, type) {
  var c = document.getElementById('toast-container');
  var el = document.createElement('div');
  el.className = 'toast ' + (type||'info');
  el.textContent = msg;
  c.appendChild(el);
  setTimeout(function() { el.style.opacity = '0'; setTimeout(function() { el.remove(); }, 180); }, 2500);
}

// ─── Event Delegation ───────────────────────────────────────────────

document.addEventListener('click', function(e) {
  var el = e.target;
  while (el && el !== document.body) {
    var a = el.getAttribute('data-action');
    if (a) { e.preventDefault(); doAction(a, el); return; }
    if (el.classList && el.classList.contains('nav-item') && el.dataset.page) { ui.navigateTo(el.dataset.page); return; }
    el = el.parentElement;
  }
  if (e.target.id === 'modal-overlay') ui.closeModal();
});

document.addEventListener('change', function(e) {
  if (e.target.getAttribute('data-action') === 'toggle-list') ui.toggleList(e.target.getAttribute('data-list-id'), e.target.checked);
});

function doAction(a, el) {
  switch (a) {
    case 'delete-list': ui.deleteList(el.getAttribute('data-list-id'), el.getAttribute('data-list-name')); break;
    case 'remove-website': ui.removeWebsite(el.getAttribute('data-list-id'), el.getAttribute('data-rule-id')); break;
    case 'remove-app': ui.removeApp(el.getAttribute('data-list-id'), el.getAttribute('data-rule-id')); break;
    case 'remove-exception': ui.removeException(el.getAttribute('data-list-id'), el.getAttribute('data-exc-id')); break;
    case 'toggle-schedule': el.classList.toggle('active'); break;
    case 'confirm-create-list': ui.createList(); break;
    case 'switch-tab':
      var tabId = el.getAttribute('data-tab');
      el.parentElement.querySelectorAll('.tab').forEach(function(t) { t.classList.remove('active'); });
      el.classList.add('active');
      document.querySelectorAll('.tab-content').forEach(function(tc) { tc.classList.remove('active'); });
      var target = document.getElementById(tabId);
      if (target) target.classList.add('active');
      if (tabId === 'websites-tab-exceptions') ui.refreshExceptions();
      break;
    case 'import-text': ui.importFromText(); closeAllDropdowns(); break;
    case 'import-json': ui.importFromText(); closeAllDropdowns(); break;
    case 'import-premade': ui.importPremadeList(el.getAttribute('data-category')); closeAllDropdowns(); break;
    case 'import-keyword-prompt': ui.importKeywordPrompt(); closeAllDropdowns(); break;
    case 'import-entire-internet': ui.importEntireInternet(); closeAllDropdowns(); break;
  }
}

function closeAllDropdowns() {
  document.querySelectorAll('.dropdown-menu').forEach(function(d) { d.classList.add('hidden'); });
}

// ─── Init ───────────────────────────────────────────────────────────

document.addEventListener('DOMContentLoaded', async function() {
  var b = document.getElementById('btn-new-blocklist'); if (b) b.addEventListener('click', function() { ui.showCreateListModal(); });
  var bw = document.getElementById('btn-add-website'); if (bw) bw.addEventListener('click', function() { ui.addWebsite(); });
  var be = document.getElementById('btn-add-exception'); if (be) be.addEventListener('click', function() { ui.addException(); });
  var ba = document.getElementById('btn-add-app'); if (ba) ba.addEventListener('click', function() { ui.addApp(); });
  var bi = document.getElementById('btn-import-dropdown'); if (bi) bi.addEventListener('click', function(e) { e.stopPropagation(); ui.toggleImportDropdown(); });
  // Close dropdowns on outside click
  document.addEventListener('click', function() { closeAllDropdowns(); });
  var mc = document.getElementById('btn-modal-close'); if (mc) mc.addEventListener('click', function() { ui.closeModal(); });
  var mx = document.getElementById('btn-modal-cancel'); if (mx) mx.addEventListener('click', function() { ui.closeModal(); });

  document.addEventListener('keydown', function(e) {
    if (e.key === 'Escape') ui.closeModal();
    if (e.key === 'Enter') { var ov = document.getElementById('modal-overlay'); if (ov && !ov.classList.contains('hidden')) { var cb = document.getElementById('modal-confirm'); if (cb) cb.click(); } }
  });

  try { state.blockLists = await invoke('list_block_lists'); } catch (e) { state.blockLists = []; }
  ui.loadPremadeLists();
  ui.navigateTo('dashboard');
  setInterval(function() { if (state.currentPage === 'dashboard') ui.refreshDashboard(); }, 5000);
});

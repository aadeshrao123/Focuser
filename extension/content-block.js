/**
 * Focuser — Content script injected into blocked pages.
 * Shows category-aware progressive messages based on per-target visit count.
 */

(function() {
  if (document.getElementById('focuser-blocked')) return;

  var domain = window.location.hostname;
  var category = window.__focuserCategory || 'default';
  var count = window.__focuserCount || 1;
  var target = window.__focuserTarget || domain;
  var reason = window.__focuserReason || 'domain';
  var actualDomain = window.__focuserDomain || domain;

  // Note: reporting is now done by background.js via reportBlockedAndGetCount.
  // Do NOT send report-blocked here — it would double-count.

  window.stop();

  function ordinal(n) {
    var s = ['th', 'st', 'nd', 'rd'];
    var v = n % 100;
    return n + (s[(v - 20) % 10] || s[v] || s[0]);
  }

  function esc(s) {
    return s ? s.replace(/&/g,'&amp;').replace(/</g,'&lt;').replace(/>/g,'&gt;').replace(/"/g,'&quot;') : '';
  }

  function buildPage(category, count, messages) {
    var cats = messages.categories || {};
    var cat = cats[category] || cats['default'];
    if (!cat) cat = { label: 'Blocked Site', icon: 'shield', color: '#8b5cf6', tiers: [] };

    var tier = cat.tiers[cat.tiers.length - 1];
    for (var i = 0; i < cat.tiers.length; i++) {
      if (count >= cat.tiers[i].min && count <= cat.tiers[i].max) {
        tier = cat.tiers[i];
        break;
      }
    }

    var msgs = tier ? tier.messages : ['This site is blocked.'];
    var msg = msgs[Math.floor(Math.random() * msgs.length)];
    msg = msg.replace(/\{count\}/g, count).replace(/\{domain\}/g, target);

    var isKeyword = reason === 'keyword';
    var displayTarget = esc(target);
    var targetLabel = isKeyword
      ? 'Blocked keyword: <strong>' + displayTarget + '</strong>'
      : esc(actualDomain);

    var attemptText = ordinal(count) + ' attempt today' + (isKeyword ? ' for this keyword' : ' for this site');
    var accentColor = cat.color || '#8b5cf6';

    var css =
      '@import url("https://fonts.googleapis.com/css2?family=Inter:wght@400;500;600;700&display=swap");' +
      '*, *::before, *::after { margin: 0; padding: 0; box-sizing: border-box; }' +
      'body {' +
        'min-height: 100vh; display: flex; align-items: center; justify-content: center;' +
        'background: radial-gradient(ellipse at 50% 20%, ' + accentColor + '14 0%, transparent 50%),' +
          'radial-gradient(ellipse at 80% 80%, ' + accentColor + '0a 0%, transparent 50%),' +
          '#07070a;' +
        'color: #f0f0f4;' +
        'font-family: "Inter", -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;' +
        '-webkit-font-smoothing: antialiased; overflow: hidden;' +
      '}' +
      '.c { text-align: center; max-width: 640px; padding: 40px 32px; animation: fadeUp 0.6s cubic-bezier(0.4,0,0.2,1); position: relative; z-index: 1; }' +
      '@keyframes fadeUp { from { opacity:0; transform:translateY(24px); } to { opacity:1; transform:translateY(0); } }' +
      '@keyframes pulse {' +
        '0%,100% { box-shadow: inset 0 1px 0 rgba(255,255,255,0.08), 0 8px 24px ' + accentColor + '30, 0 0 50px ' + accentColor + '14; }' +
        '50% { box-shadow: inset 0 1px 0 rgba(255,255,255,0.08), 0 8px 32px ' + accentColor + '4d, 0 0 80px ' + accentColor + '22; }' +
      '}' +
      '@keyframes float { 0%,100% { transform: translateY(0); } 50% { transform: translateY(-6px); } }' +
      '.shield {' +
        'width: 88px; height: 88px; margin: 0 auto 32px;' +
        'background: linear-gradient(180deg, ' + accentColor + '38 0%, ' + accentColor + '10 100%), rgba(20,20,32,0.6);' +
        'backdrop-filter: blur(24px); -webkit-backdrop-filter: blur(24px);' +
        'border: 1px solid ' + accentColor + '40; border-top-color: ' + accentColor + '5a;' +
        'border-radius: 24px; display: flex; align-items: center; justify-content: center;' +
        'animation: pulse 4s ease-in-out infinite, float 6s ease-in-out infinite;' +
      '}' +
      '.shield svg { width: 44px; height: 44px; color: ' + accentColor + '; filter: drop-shadow(0 0 14px ' + accentColor + '99); }' +
      '.cat-label {' +
        'font-size: 11px; font-weight: 700; text-transform: uppercase; letter-spacing: 0.1em;' +
        'color: ' + accentColor + '; margin-bottom: 8px; display: flex; align-items: center;' +
        'justify-content: center; gap: 6px;' +
      '}' +
      '.cat-label .dot { width: 6px; height: 6px; border-radius: 50%; background: ' + accentColor + ';' +
        'box-shadow: 0 0 8px ' + accentColor + '80; }' +
      'h1 {' +
        'font-size: 34px; font-weight: 700; margin-bottom: 14px; letter-spacing: -0.04em;' +
        'background: linear-gradient(135deg, #fff 30%, #b4b4c8 100%);' +
        '-webkit-background-clip: text; -webkit-text-fill-color: transparent; background-clip: text;' +
      '}' +
      '.target-badge {' +
        'font-family: "JetBrains Mono", "Cascadia Code", monospace;' +
        'font-size: 13px; font-weight: 500; color: ' + accentColor + '; margin-bottom: 24px;' +
        'display: inline-block; padding: 6px 16px; border-radius: 8px;' +
        'background: linear-gradient(135deg, ' + accentColor + '24, ' + accentColor + '10);' +
        'border: 1px solid ' + accentColor + '30; box-shadow: 0 0 16px ' + accentColor + '14;' +
      '}' +
      '.target-badge strong { color: ' + accentColor + '; font-weight: 700; }' +
      '.msg {' +
        'color: #b4b4c8; font-size: 16px; line-height: 1.8; margin-bottom: 28px;' +
        'max-width: 500px; margin-left: auto; margin-right: auto;' +
      '}' +
      '.attempt {' +
        'display: inline-flex; align-items: center; gap: 6px;' +
        'font-size: 12px; font-weight: 500; color: #7a7a94; margin-bottom: 32px;' +
        'padding: 6px 14px; border-radius: 20px;' +
        'background: rgba(255,255,255,0.03); border: 1px solid rgba(255,255,255,0.06);' +
      '}' +
      '.badge {' +
        'display: inline-flex; align-items: center; gap: 8px;' +
        'font-size: 12px; font-weight: 500; color: #5e5e72;' +
        'background: linear-gradient(180deg, rgba(255,255,255,0.03) 0%, transparent 100%), rgba(14,14,22,0.5);' +
        'backdrop-filter: blur(12px); -webkit-backdrop-filter: blur(12px);' +
        'border: 1px solid rgba(255,255,255,0.05); border-radius: 10px; padding: 10px 18px;' +
        'transition: all 0.3s cubic-bezier(0.4,0,0.2,1);' +
      '}' +
      '.badge:hover { border-color: ' + accentColor + '26; color: #8a8aa0; }' +
      '.badge svg { color: ' + accentColor + '; filter: drop-shadow(0 0 6px ' + accentColor + '80); }' +
      '.glow { position: fixed; width: 500px; height: 500px; pointer-events: none; z-index: 0;' +
        'background: radial-gradient(circle, ' + accentColor + '0f 0%, transparent 60%);' +
        'transform: translate(-50%, -50%); opacity: 0.7;' +
      '}';

    var shieldIcon = isKeyword
      ? '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><circle cx="11" cy="11" r="8"/><path d="m21 21-4.3-4.3"/></svg>'
      : '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z"/></svg>';

    var html =
      '<div class="glow" id="glow"></div>' +
      '<div class="c" id="focuser-blocked">' +
        '<div class="shield">' + shieldIcon + '</div>' +
        '<div class="cat-label"><span class="dot"></span> ' + esc(cat.label) + '</div>' +
        '<h1>This ' + (isKeyword ? 'search' : 'website') + ' is blocked.</h1>' +
        '<div class="target-badge">' + targetLabel + '</div>' +
        '<p class="msg">' + esc(msg) + '</p>' +
        '<div class="attempt">' + attemptText + '</div><br>' +
        '<div class="badge"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="14" height="14"><path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z"/></svg>Protected by Focuser</div>' +
      '</div>';

    var fullDoc = '<!DOCTYPE html><html><head><meta charset="UTF-8"><title>Blocked by Focuser</title>' +
      '<style>' + css + '</style></head><body>' + html + '</body></html>';
    var parsed = new DOMParser().parseFromString(fullDoc, 'text/html');

    while (document.head.firstChild) document.head.firstChild.remove();
    while (document.body.firstChild) document.body.firstChild.remove();

    Array.from(parsed.head.childNodes).forEach(function(n) {
      document.head.appendChild(document.adoptNode(n));
    });
    Array.from(parsed.body.childNodes).forEach(function(n) {
      document.body.appendChild(document.adoptNode(n));
    });

    document.addEventListener('mousemove', function(e) {
      var g = document.getElementById('glow');
      if (g) { g.style.left = e.clientX + 'px'; g.style.top = e.clientY + 'px'; }
    });
  }

  try {
    var url = chrome.runtime.getURL('messages.json');
    fetch(url).then(function(r) { return r.json(); }).then(function(messages) {
      buildPage(category, count, messages);
    }).catch(function() {
      buildPage(category, count, { categories: {} });
    });
  } catch (e) {
    buildPage(category, count, { categories: {} });
  }
})();

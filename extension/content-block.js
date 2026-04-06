/**
 * Focuser — Content script injected into blocked pages.
 * Replaces the entire page content with the block page.
 * The URL bar stays as the original blocked domain.
 */

(function() {
  if (document.getElementById('focuser-blocked')) return;

  var domain = window.location.hostname;

  try {
    chrome.runtime.sendMessage({ type: 'report-blocked', hostname: domain, url: window.location.href });
  } catch (e) {}

  var quotes = [
    "The secret of getting ahead is getting started. —Mark Twain",
    "Focus on being productive instead of busy. —Tim Ferriss",
    "You can\u2019t use up creativity. The more you use, the more you have. —Maya Angelou",
    "It\u2019s not that I\u2019m so smart, it\u2019s just that I stay with problems longer. —Albert Einstein",
    "The best time to plant a tree was 20 years ago. The second best time is now.",
    "Do the hard jobs first. The easy jobs will take care of themselves. —Dale Carnegie",
    "Discipline is choosing between what you want now and what you want most.",
    "Your future is created by what you do today, not tomorrow. —Robert Kiyosaki",
  ];
  var quote = quotes[Math.floor(Math.random() * quotes.length)];

  window.stop();

  var css =
    '@import url("https://fonts.googleapis.com/css2?family=Inter:wght@400;500;600;700&display=swap");' +
    '*, *::before, *::after { margin: 0; padding: 0; box-sizing: border-box; }' +

    'body {' +
      'min-height: 100vh; display: flex; align-items: center; justify-content: center;' +
      'background: radial-gradient(ellipse at 50% 20%, rgba(139,92,246,0.10) 0%, transparent 50%),' +
        'radial-gradient(ellipse at 80% 80%, rgba(139,92,246,0.05) 0%, transparent 50%),' +
        '#07070a;' +
      'color: #f0f0f4;' +
      'font-family: "Inter", -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;' +
      '-webkit-font-smoothing: antialiased;' +
      'overflow: hidden;' +
    '}' +

    '.c { text-align: center; max-width: 640px; padding: 40px 32px; animation: fadeUp 0.6s cubic-bezier(0.4,0,0.2,1); }' +

    '@keyframes fadeUp { from { opacity:0; transform:translateY(24px); } to { opacity:1; transform:translateY(0); } }' +
    '@keyframes pulse {' +
      '0%,100% { box-shadow: inset 0 1px 0 rgba(255,255,255,0.08), 0 8px 24px rgba(139,92,246,0.18), 0 0 50px rgba(139,92,246,0.08); }' +
      '50% { box-shadow: inset 0 1px 0 rgba(255,255,255,0.08), 0 8px 32px rgba(139,92,246,0.30), 0 0 80px rgba(139,92,246,0.14); }' +
    '}' +
    '@keyframes float {' +
      '0%,100% { transform: translateY(0); }' +
      '50% { transform: translateY(-6px); }' +
    '}' +

    '.shield {' +
      'width: 88px; height: 88px; margin: 0 auto 36px;' +
      'background: linear-gradient(180deg, rgba(139,92,246,0.22) 0%, rgba(139,92,246,0.06) 100%), rgba(20,20,32,0.6);' +
      'backdrop-filter: blur(24px); -webkit-backdrop-filter: blur(24px);' +
      'border: 1px solid rgba(139,92,246,0.25); border-top-color: rgba(139,92,246,0.35);' +
      'border-radius: 24px;' +
      'display: flex; align-items: center; justify-content: center;' +
      'box-shadow: inset 0 1px 0 rgba(255,255,255,0.08), 0 8px 24px rgba(139,92,246,0.18), 0 0 50px rgba(139,92,246,0.08);' +
      'animation: pulse 4s ease-in-out infinite, float 6s ease-in-out infinite;' +
    '}' +

    '.shield svg { width: 44px; height: 44px; color: #a078ff; filter: drop-shadow(0 0 14px rgba(139,92,246,0.6)); }' +

    'h1 {' +
      'font-size: 36px; font-weight: 700; margin-bottom: 12px; letter-spacing: -0.04em; line-height: 1.2;' +
      'background: linear-gradient(135deg, #fff 30%, #b4b4c8 100%);' +
      '-webkit-background-clip: text; -webkit-text-fill-color: transparent; background-clip: text;' +
    '}' +

    '.domain {' +
      'font-family: "JetBrains Mono", "Cascadia Code", "Fira Code", monospace;' +
      'font-size: 14px; font-weight: 500; color: #8b5cf6; margin-bottom: 28px;' +
      'display: inline-block; padding: 6px 16px; border-radius: 8px;' +
      'background: linear-gradient(135deg, rgba(139,92,246,0.14), rgba(139,92,246,0.06));' +
      'border: 1px solid rgba(139,92,246,0.18);' +
      'box-shadow: 0 0 16px rgba(139,92,246,0.08);' +
      'letter-spacing: 0.01em;' +
    '}' +

    '.quote {' +
      'color: #7a7a94; font-size: 15px; line-height: 1.8; margin-bottom: 40px;' +
      'font-style: italic; max-width: 480px; margin-left: auto; margin-right: auto;' +
    '}' +


    '.badge {' +
      'display: inline-flex; align-items: center; gap: 8px;' +
      'font-size: 12px; font-weight: 500; color: #5e5e72;' +
      'background: linear-gradient(180deg, rgba(255,255,255,0.03) 0%, transparent 100%), rgba(14,14,22,0.5);' +
      'backdrop-filter: blur(12px); -webkit-backdrop-filter: blur(12px);' +
      'border: 1px solid rgba(255,255,255,0.05); border-radius: 10px; padding: 10px 18px;' +
      'box-shadow: inset 0 1px 0 rgba(255,255,255,0.02);' +
      'transition: all 0.3s cubic-bezier(0.4,0,0.2,1);' +
    '}' +
    '.badge:hover { border-color: rgba(139,92,246,0.15); color: #8a8aa0; box-shadow: 0 0 12px rgba(139,92,246,0.06); }' +
    '.badge svg { color: #8b5cf6; filter: drop-shadow(0 0 6px rgba(139,92,246,0.5)); }' +

    '.glow { position: fixed; width: 500px; height: 500px; pointer-events: none; z-index: 0;' +
      'background: radial-gradient(circle, rgba(139,92,246,0.06) 0%, transparent 60%);' +
      'transform: translate(-50%, -50%); opacity: 0.7;' +
    '}' +
    '.c { position: relative; z-index: 1; }';

  var html =
    '<div class="glow" id="glow"></div>' +
    '<div class="c" id="focuser-blocked">' +
      '<div class="shield"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z"/></svg></div>' +
      '<h1>This website is blocked.</h1>' +
      '<div class="domain">' + domain.replace(/&/g,'&amp;').replace(/</g,'&lt;') + '</div>' +
      '<p class="quote">' + quote.replace(/&/g,'&amp;').replace(/</g,'&lt;') + '</p>' +
      '<div class="badge"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="14" height="14"><path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z"/></svg>Protected by Focuser</div>' +
    '</div>';

  document.documentElement.innerHTML =
    '<head><meta charset="UTF-8"><title>Blocked by Focuser</title><style>' + css + '</style></head>' +
    '<body>' + html + '</body>';

  // Cursor glow effect
  document.addEventListener('mousemove', function(e) {
    var g = document.getElementById('glow');
    if (g) { g.style.left = e.clientX + 'px'; g.style.top = e.clientY + 'px'; }
  });
})();

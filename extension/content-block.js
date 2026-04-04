/**
 * Focuser — Content script injected into blocked pages.
 * Replaces the entire page content with the block page.
 * The URL bar stays as the original blocked domain.
 */

(function() {
  // Prevent running twice
  if (document.getElementById('focuser-blocked')) return;

  var domain = window.location.hostname;

  var quotes = [
    "The secret of getting ahead is getting started. —Mark Twain",
    "Focus on being productive instead of busy. —Tim Ferriss",
    "You can't use up creativity. The more you use, the more you have. —Maya Angelou",
    "It's not that I'm so smart, it's just that I stay with problems longer. —Albert Einstein",
    "The best time to plant a tree was 20 years ago. The second best time is now.",
    "Do the hard jobs first. The easy jobs will take care of themselves. —Dale Carnegie",
    "Discipline is choosing between what you want now and what you want most.",
    "Your future is created by what you do today, not tomorrow. —Robert Kiyosaki",
  ];
  var quote = quotes[Math.floor(Math.random() * quotes.length)];

  // Stop all page loading and scripts
  window.stop();

  // Replace entire document
  document.documentElement.innerHTML = '<head>' +
    '<meta charset="UTF-8">' +
    '<title>Blocked by Focuser</title>' +
    '<style>' +
    '* { margin: 0; padding: 0; box-sizing: border-box; }' +
    'body { min-height: 100vh; display: flex; align-items: center; justify-content: center; background: #0c0c0e; color: #f0f0f3; font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", system-ui, sans-serif; }' +
    '.c { text-align: center; max-width: 600px; padding: 40px 32px; }' +
    '.s { width: 72px; height: 72px; margin: 0 auto 32px; background: rgba(139,92,246,0.1); border-radius: 18px; display: flex; align-items: center; justify-content: center; }' +
    '.s svg { width: 36px; height: 36px; color: #8b5cf6; }' +
    'h1 { font-size: 32px; font-weight: 600; margin-bottom: 20px; letter-spacing: -0.02em; }' +
    '.q { color: #a0a0ab; font-size: 16px; line-height: 1.7; margin-bottom: 36px; font-style: italic; }' +
    '.a { margin-bottom: 40px; }' +
    '.b { padding: 10px 28px; background: #1a1a1f; color: #a0a0ab; border: 1px solid rgba(255,255,255,0.08); border-radius: 8px; font-size: 13px; cursor: pointer; font-family: inherit; }' +
    '.b:hover { background: #232329; color: #f0f0f3; }' +
    '.g { display: inline-flex; align-items: center; gap: 8px; font-size: 12px; color: #5c5c66; background: #111113; border: 1px solid rgba(255,255,255,0.06); border-radius: 8px; padding: 8px 16px; }' +
    '.g svg { color: #8b5cf6; width: 14px; height: 14px; }' +
    '</style>' +
    '</head><body>' +
    '<div class="c" id="focuser-blocked">' +
    '<div class="s"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z"/></svg></div>' +
    '<h1>This website is blocked.</h1>' +
    '<p class="q">' + quote.replace(/&/g,'&amp;').replace(/</g,'&lt;') + '</p>' +
    '<div class="a"><button class="b" onclick="history.back()">Go Back</button></div>' +
    '<div class="g"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><circle cx="12" cy="12" r="10"/><path d="M12 6v6l4 2"/></svg>Protected by Focuser</div>' +
    '</div></body>';
})();

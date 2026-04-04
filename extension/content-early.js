/**
 * Runs at document_start — before ANY page content renders.
 * Immediately hides the page if domain is blocked, then asks
 * background to confirm and inject the full block page.
 */
(function() {
  // Immediately hide everything — this runs before any DOM content
  var style = document.createElement('style');
  style.textContent = 'html{background:#0c0c0e!important;visibility:hidden!important}';
  (document.head || document.documentElement).appendChild(style);

  // Ask background script if this domain is actually blocked
  chrome.runtime.sendMessage(
    { type: 'check-domain', hostname: window.location.hostname, url: window.location.href },
    function(response) {
      if (response && response.blocked) {
        // Domain is blocked — keep hidden, background will inject block page
      } else {
        // Not blocked — remove the hiding style
        style.remove();
      }
    }
  );
})();

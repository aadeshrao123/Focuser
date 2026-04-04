// Extract domain from all possible sources
var params = new URLSearchParams(window.location.search);
var domain = params.get('domain') || '';
if (!domain) {
  try { domain = new URLSearchParams(window.location.hash.substring(1)).get('domain') || ''; } catch(e) {}
}
if (!domain) domain = 'this site';

// Set tab title
document.title = 'Blocked by Focuser';

// Motivational quotes
var quotes = [
  "The secret of getting ahead is getting started. \u2014Mark Twain",
  "Focus on being productive instead of busy. \u2014Tim Ferriss",
  "You can\u2019t use up creativity. The more you use, the more you have. \u2014Maya Angelou",
  "It\u2019s not that I\u2019m so smart, it\u2019s just that I stay with problems longer. \u2014Albert Einstein",
  "The best time to plant a tree was 20 years ago. The second best time is now. \u2014Chinese Proverb",
  "Do the hard jobs first. The easy jobs will take care of themselves. \u2014Dale Carnegie",
  "Discipline is choosing between what you want now and what you want most. \u2014Abraham Lincoln",
  "Your future is created by what you do today, not tomorrow. \u2014Robert Kiyosaki",
];
document.getElementById('quote').textContent = quotes[Math.floor(Math.random() * quotes.length)];

document.getElementById('btn-back').addEventListener('click', function() {
  window.location.href = 'about:home';
});

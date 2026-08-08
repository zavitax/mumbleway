/* The narrow-screen menu.
 *
 * Progressive enhancement rather than a hard dependency: the markup ships with
 * every link present and the button hidden, so a page with no JavaScript has a
 * navigation that wraps onto two lines. That is uglier and completely usable.
 * A hamburger that does nothing would not be.
 *
 * The button carries `aria-expanded`, so a screen reader announces the state
 * rather than leaving somebody to guess whether the list is there. Whether the
 * button is *shown* at all is CSS's business, not this file's — there is no
 * width measured here, so there is no breakpoint to keep in step with the
 * stylesheet.
 */
(function () {
  var bar = document.querySelector('.topbar');
  var button = document.querySelector('.menu-button');
  var nav = document.getElementById('sections');
  if (!bar || !button || !nav) return;

  // Only now does the button exist as far as the user is concerned, and only
  // now may the stylesheet collapse the list — the class is what licenses it.
  button.hidden = false;
  bar.classList.add('has-menu');

  function setOpen(open) {
    bar.classList.toggle('open', open);
    button.setAttribute('aria-expanded', open ? 'true' : 'false');
  }

  button.addEventListener('click', function () {
    setOpen(bar.classList.contains('open') === false);
  });

  // Following a link on the page you are already on changes nothing visible,
  // so the menu would sit there open over the content it was meant to reveal.
  nav.addEventListener('click', function (e) {
    if (e.target.closest('a')) setOpen(false);
  });

  document.addEventListener('keydown', function (e) {
    if (e.key === 'Escape' && bar.classList.contains('open')) {
      setOpen(false);
      button.focus();
    }
  });

  // A tap outside it should shut it, the way every other menu on the phone does.
  document.addEventListener('click', function (e) {
    if (bar.classList.contains('open') && !bar.contains(e.target)) setOpen(false);
  });

  // Rotating to landscape can cross the breakpoint and leave the list hidden
  // with no button showing it. Closing on resize costs nothing and cannot
  // strand anybody.
  window.addEventListener('resize', function () {
    if (bar.classList.contains('open')) setOpen(false);
  }, { passive: true });
})();

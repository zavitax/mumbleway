/* The narrow-screen menu.
 *
 * Progressive enhancement rather than a hard dependency: the markup ships with
 * every link present and the button hidden, so a page with no JavaScript has a
 * navigation that wraps onto two lines. That is uglier and completely usable.
 * A hamburger that does nothing would not be.
 *
 * The button carries `aria-expanded`, so a screen reader announces the state
 * rather than leaving somebody to guess whether the list is there.
 *
 * **Whether to collapse at all is measured, not guessed.** A pixel breakpoint
 * has to know how wide the labels are and cannot: the set of links grows as
 * pages are added, and every label changes length again in another language.
 * The first version was tuned for six items, a seventh arrived, and the row
 * began wrapping onto two lines across a band of widths nobody had looked at.
 * So the row is measured against the space it has, and the answer is whatever
 * it actually is.
 */
(function () {
  var bar = document.querySelector('.topbar');
  var button = document.querySelector('.menu-button');
  var nav = document.getElementById('sections');
  if (!bar || !button || !nav) return;

  // Only now does the button exist as far as the user is concerned.
  button.hidden = false;

  var brand = bar.querySelector('.brand');

  /// Does the row of links fit beside the wordmark?
  ///
  /// Measured with wrapping forced off, because a wrapped row reports the
  /// width it settled for rather than the width it wanted. Reading a layout
  /// property flushes the change synchronously and nothing paints between
  /// these statements, so the forced state is never visible.
  function fits() {
    var was = bar.classList.contains('compact');
    bar.classList.remove('compact');
    nav.style.flexWrap = 'nowrap';

    var room = bar.clientWidth
      - (brand ? brand.offsetWidth : 0)
      - button.offsetWidth
      - 48;                       // the bar's own gaps and padding
    var need = nav.scrollWidth;

    nav.style.flexWrap = '';
    if (was) bar.classList.add('compact');
    return need <= room;
  }

  function measure() {
    var compact = !fits();
    if (compact === bar.classList.contains('compact')) return;
    bar.classList.toggle('compact', compact);
    if (!compact) setOpen(false);
  }

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

  // Re-measured on resize, and closed while it happens: rotating to landscape
  // can cross the threshold, and a list left open as the row expands would sit
  // over content it was meant to reveal.
  window.addEventListener('resize', function () {
    if (bar.classList.contains('open')) setOpen(false);
    measure();
  }, { passive: true });

  // Labels are what is being measured, so the measurement is wrong until the
  // face they are set in has loaded — a fallback font of a different width
  // gives a different answer.
  measure();
  if (document.fonts && document.fonts.ready) {
    document.fonts.ready.then(measure);
  }
})();

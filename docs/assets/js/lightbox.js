/* Click a screenshot, see it full size.
 *
 * Every screenshot on this site is drawn at one width so a page of them reads
 * as a set rather than as a pile of different shapes. That is the right call
 * for the page and the wrong one for a desktop window at 16rem, which is
 * unreadable — so the detail has to be one click away, and this is the click.
 *
 * **Progressive enhancement, not a dependency.** The markup ships as plain
 * `<img>`; this wraps each one in a link to the image file. With no script
 * there is no wrapper and the pictures are exactly as they were; with the
 * script but a failure anywhere after it, the link still opens the full image
 * in a tab. The overlay is the enhancement, not the mechanism.
 */
(function () {
  var shots = document.querySelectorAll('.shots img, .shot-wide img');
  if (!shots.length) return;

  var closeLabel = document.body.getAttribute('data-close') || 'Close';

  // Wrap each screenshot in a link to itself. A link rather than a click
  // handler on the image, so it is reachable by keyboard and by every other
  // means a link is, without inventing any of that here.
  shots.forEach(function (img) {
    if (img.closest('.shot-link')) return;
    var a = document.createElement('a');
    a.className = 'shot-link';
    a.href = img.currentSrc || img.src;
    img.parentNode.insertBefore(a, img);
    a.appendChild(img);
  });

  var box = null;
  var opener = null;

  function close() {
    if (!box) return;
    box.remove();
    box = null;
    document.documentElement.style.overflow = '';
    // Back where they were. Somebody who opened this from the keyboard is
    // otherwise returned to the top of the document.
    if (opener) opener.focus();
    opener = null;
  }

  function open(link, img) {
    close();

    box = document.createElement('div');
    box.className = 'lightbox';
    box.setAttribute('role', 'dialog');
    box.setAttribute('aria-modal', 'true');

    var figure = document.createElement('figure');
    var full = document.createElement('img');
    full.src = link.href;
    // The same description the page already gives it. A lightbox that drops
    // the alt text is a picture with no name at the moment it fills the screen.
    full.alt = img.alt || '';
    figure.appendChild(full);

    // `.shots` wraps each picture in a <figure>; `.shot-wide` does not, and
    // puts the <figcaption> straight in the div. Both carry a caption worth
    // keeping, so look for either container.
    var holder = img.closest('figure') || img.closest('.shot-wide');
    var caption = holder && holder.querySelector('figcaption');
    if (caption) {
      var c = document.createElement('figcaption');
      c.textContent = caption.textContent;
      figure.appendChild(c);
    }

    var button = document.createElement('button');
    button.type = 'button';
    button.className = 'lightbox-close';
    button.setAttribute('aria-label', closeLabel);
    button.innerHTML = '&times;';
    button.addEventListener('click', close);

    box.appendChild(figure);
    box.appendChild(button);

    // Anywhere off the picture shuts it, which is what the zoom-out cursor
    // over the backdrop promises.
    box.addEventListener('click', function (e) {
      if (e.target === box || e.target === figure) close();
    });

    document.body.appendChild(box);
    // The page behind must not scroll under the overlay.
    document.documentElement.style.overflow = 'hidden';
    button.focus();
  }

  document.addEventListener('click', function (e) {
    var link = e.target.closest && e.target.closest('.shot-link');
    if (!link) return;
    // Leave modified clicks alone: ctrl, shift and the middle button all mean
    // "open this somewhere else", and the link already does that correctly.
    if (e.metaKey || e.ctrlKey || e.shiftKey || e.altKey || e.button !== 0) return;
    e.preventDefault();
    opener = link;
    open(link, link.querySelector('img'));
  });

  document.addEventListener('keydown', function (e) {
    if (e.key === 'Escape') close();
  });
})();

/* The hero runs the app's own diagnostics analyser.
 *
 * Three traces, the same three the panel draws: what the microphone heard,
 * what survived suppression, and what actually left for the server. It is the
 * most characteristic thing this app does — most voice clients tell you
 * nothing, and the whole argument here is that you can watch the decision
 * being made — so it is what the page opens with rather than a stock image of
 * a motorcycle.
 *
 * Synthesised, obviously. It is a drawing of the shape of a thing, not a
 * measurement, and it is marked aria-hidden because there is nothing in it for
 * a screen reader.
 */
(function () {
  var canvas = document.getElementById('analyser');
  if (!canvas) return;

  var reduced = window.matchMedia('(prefers-reduced-motion: reduce)');
  var ctx = canvas.getContext('2d');
  var BANDS = 28;
  var dpr = 1;
  var w = 0;
  var h = 0;

  // Per band: current value and the phase of its own slow wander, so the bars
  // do not move as one body.
  var mic = [], sup = [], sent = [], phase = [];
  for (var i = 0; i < BANDS; i++) {
    mic[i] = sup[i] = sent[i] = 0;
    phase[i] = Math.random() * Math.PI * 2;
  }

  function resize() {
    dpr = Math.min(window.devicePixelRatio || 1, 2);
    w = canvas.clientWidth;
    h = canvas.clientHeight;
    canvas.width = Math.floor(w * dpr);
    canvas.height = Math.floor(h * dpr);
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  }

  // A speech-shaped envelope: energy concentrated low-mid, falling away at the
  // top, with a syllable rhythm at roughly four a second.
  function target(i, t) {
    var x = i / (BANDS - 1);
    var tilt = Math.exp(-Math.pow((x - 0.22) / 0.42, 2));
    var syllable = 0.55 + 0.45 * Math.pow(Math.max(0, Math.sin(t * 2.2 + 0.6)), 1.8);
    var wander = 0.72 + 0.28 * Math.sin(t * 1.3 + phase[i]);
    return Math.max(0.03, tilt * syllable * wander);
  }

  function draw(now) {
    var t = now / 1000;
    ctx.clearRect(0, 0, w, h);

    var gap = 3;
    var bw = (w - gap * (BANDS - 1)) / BANDS;

    for (var i = 0; i < BANDS; i++) {
      var m = target(i, t);
      // Suppression takes the top off the high bands and the floor everywhere.
      var s = m * (0.92 - 0.5 * Math.pow(i / (BANDS - 1), 2)) - 0.06;
      // What is sent is gated: below the threshold nothing goes at all.
      var open = m > 0.34;
      var g = open ? Math.max(0, s) * 0.86 : 0;

      // Asymmetric smoothing, fast to rise and slow to fall, as a meter is.
      mic[i]  += (m - mic[i])  * (m > mic[i]  ? 0.35 : 0.09);
      sup[i]  += (Math.max(0, s) - sup[i]) * (s > sup[i] ? 0.35 : 0.09);
      sent[i] += (g - sent[i]) * (g > sent[i] ? 0.40 : 0.07);

      var x = i * (bw + gap);

      // Microphone: a hairline outline, the widest and the least important.
      ctx.fillStyle = 'rgba(134,149,166,0.30)';
      ctx.fillRect(x, h - mic[i] * h, bw, mic[i] * h);

      // After suppression.
      ctx.fillStyle = 'rgba(62,207,126,0.34)';
      ctx.fillRect(x, h - sup[i] * h, bw, sup[i] * h);

      // Transmitted, drawn last and lit, because it is the one that matters.
      if (sent[i] > 0.005) {
        ctx.fillStyle = 'rgba(255,165,60,0.85)';
        ctx.fillRect(x, h - sent[i] * h, bw, sent[i] * h);
      }
    }
    raf = requestAnimationFrame(draw);
  }

  function still() {
    // One frame, held. The shape is the point; the movement is decoration, and
    // decoration is the part somebody asked not to be shown.
    resize();
    draw(0);
    cancelAnimationFrame(raf);
  }

  var raf = 0;
  resize();
  window.addEventListener('resize', resize, { passive: true });

  if (reduced.matches) {
    still();
  } else {
    raf = requestAnimationFrame(draw);
    // Stop when off screen: this is decoration and has no business spending a
    // battery on a tab nobody is looking at.
    document.addEventListener('visibilitychange', function () {
      if (document.hidden) { cancelAnimationFrame(raf); }
      else { raf = requestAnimationFrame(draw); }
    });
  }
})();

# Giving the look-ahead back

**Nothing here is built.** It is the design for covering the whole onset
distribution without carrying the latency for it, written down with the
measurements that motivate it.

## The problem, measured

Voice activation cannot decide a block is speech until it has the block, so the
sound that opens the gate has already gone by. The chain answers that by
delaying the audio and not the decision — `ONSET_LOOKAHEAD_MS` in
`core/src/audio/engine.rs`.

It was 80 ms, chosen because "a leading fricative runs 50–100 ms". A rider
reported word starts on "p", "sh" and "ch" being swallowed, and
`tools/vad/onset_lead.py` measured what real openings need. Walking back from
each opening through the microphone signal to where it left its own **local**
background, across 69 openings on three rides:

| Look-ahead | Openings fully covered | Still clipped |
|---|---|---|
| 80 ms | 89.9% | 7 |
| 120 ms | 91.3% | 6 |
| **160 ms** (now) | **94.2%** | **4** |
| 240 ms | 95.7% | 3 |
| 320 ms | 98.6% | 1 |

One word start in ten was losing its first sound. 160 ms is the knee and is
shipped. The tail runs to **390 ms**.

> The first version of that measurement used a global noise floor and reported
> `p75 350 ms, max 400 ms` on a road clip — which was the 400 ms search horizon
> showing through, not a lead time. On a loud ride the background never falls
> below a global threshold, so the walk-back never terminated. The local
> minimum over the preceding half second is the background *that opening* rose
> out of, and it is the only one the question is about. Openings that still
> never find quiet are excluded and counted rather than folded in as large
> numbers.

## Why not simply raise it further

The look-ahead is **one-way latency on every transmission, for ever, to protect
the first tenth of a second of an utterance.** At 320 ms, mouth-to-ear —
look-ahead, enhancer, encoder, network, jitter buffer — lands near half a
second. That is a worse conversation than an occasional clipped consonant, and
riders talk in short exchanges where turn-taking latency is felt directly.

The cost is also paid when it buys nothing: p50 of the distribution is **10 ms**.
Nine openings in ten need almost no lead at all, and every one of them pays the
full delay.

## The shape of the fix: pay it down

Keep a **long** look-ahead — 400 ms, enough for the whole measured
distribution — but stop carrying it once the phrase is under way.

At an opening the buffer holds up to 400 ms of pre-roll, and all of it is
transmitted. From then until the buffer drains to a small floor, transmit
**slightly faster than real time**: emit 10 ms of audio in a little under 10 ms
of wall clock by removing whole pitch periods. Latency falls back toward the
floor within a second or two and stays there for the rest of the utterance.

- **The first word is intact**, because the pre-roll went out.
- **Steady-state latency is low**, because the debt is repaid.
- **The compression is inaudible.** Speech time-scaled by 5–10% with
  pitch-synchronous period removal is not perceptible as speed; it is the same
  operation the receive side already performs.

**The machinery exists.** `core/src/audio/stretch.rs` already plays off a
backlog at up to 2× by pitch-period removal, on the receive path, and has
tests. The send side needs the same primitive driven by a different signal —
"how far behind real time am I" instead of "how deep is the jitter buffer".

## What to decide before building it

1. **Where the floor sits.** Zero is wrong: some slack absorbs a late block
   without a dropout. 40–60 ms is the obvious starting point and should be
   measured against `capture_dropped_ms`.
2. **How fast to repay.** 5% is inaudible and takes ~7 s to clear 350 ms; 10%
   takes ~3.5 s and is still comfortable. The receive side's 2× is far more
   aggressive than anything wanted here.
3. **What happens to the recorder.** The diagnostic `.s16` must stay the
   microphone at real time — the corpus depends on it — so the compression has
   to sit *after* the recorder's tap, like the enhancer does not. Getting this
   wrong would silently time-warp every recording taken afterwards, which is
   the same class of fault as recording the enhancer's output.
4. **Whether it interacts with the echo canceller.** The reference is what was
   played, and the near end would now be time-scaled relative to it. The AEC
   adapts on a signal whose timebase moves during the first second of every
   phrase — this needs checking on a speakerphone route before it ships.

Point 4 is the one that could sink it, and it should be tested first rather
than last.

## What is deliberately not proposed

**Opening the gate earlier.** It looks like the cheaper fix — less lead needed
if the detector fires sooner — and `core/src/audio/denoise.rs` records three
attempts at it: periodicity, level override, and syllabic modulation. All three
either recovered nothing or put 12–76% of synthetic engine and traffic noise on
the wire. The decision is not the lever; the delay is.

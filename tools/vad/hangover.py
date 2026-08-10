"""What the transmit envelope costs, measured on real rides.

The capture chain holds the channel open past the detector's last speech
block. `engine.rs` derives the hold as::

    VAD_HOLD = VAD_TAIL_MS + ONSET_LOOKAHEAD_MS - VAD_FADE_MS
             = 200 + 160 - 30 = 330 ms

so a listener hears 200 ms of audio after the detector drops, plus a 30 ms
fade. The question this answers is what that buys and what it costs, from the
rides in the corpus rather than from an argument.

**It needs no reimplementation of the envelope**, which is the point. Every
recorded ride carries both columns:

* ``speaking``    - the instantaneous decision, stamped at capture from that
  block's own analysis.
* ``transmitting`` - the post-envelope answer, stamped when *that same block*
  reached the transmit decision 160 ms later, which is what the pending queue
  in ``run_worker`` exists to arrange.

Both therefore describe the same audio, and a row with ``transmitting=1`` and
``speaking=0`` is precisely a block that went out on the envelope rather than
on the detector. That is the shipped behaviour, on real audio, with nothing
modelled.

Only voice-activated rides (``mode=0``) are read. In push-to-talk and
continuous the envelope is not what decides, so including them would average
a measurement with something that is not being measured.

    python tools/vad/hangover.py C:\\ml_data\\rides
"""

import sys
from pathlib import Path

BLOCK_MS = 10
LOOKAHEAD_MS = 160


def read(csv):
    """`(speaking, transmitting)` per block, or None if not a mode=0 ride."""
    speaking, sending = [], []
    at = None
    muted = 0
    for line in csv.read_text(errors="replace").splitlines():
        if not line or line.startswith("#"):
            continue
        parts = line.split(",")
        if not parts[0].isdigit():
            at = {n: i for i, n in enumerate(parts)}
            # Columns are found by name because they were added over time, and
            # a file from before one of them exists is not a file to guess at.
            if "transmitting" not in at or "speaking" not in at:
                return None
            continue
        if at is None:
            return None
        if max(at.values()) >= len(parts):
            continue
        # A ride is only usable here if it was voice-activated throughout;
        # `mode` arrived after some recordings were already on phones, so its
        # absence means an older file that predates the column.
        if "mode" in at:
            if parts[at["mode"]] != "0":
                return None
            # A muted ride judges speech and transmits none of it, so its
            # envelope share is 0% for a reason that has nothing to do with the
            # envelope. Including it drags the aggregate towards a number about
            # the mute button.
            if parts[at["muted"]] == "1":
                muted += 1
        speaking.append(parts[at["speaking"]] == "1")
        sending.append(parts[at["transmitting"]] == "1")
    if not speaking or muted > len(speaking) // 2:
        return None
    # And a ride with nothing transmitted has nothing to say here — the dead
    # microphone in `20260810-1912` is three files of digital silence.
    if not any(sending):
        return None
    return speaking, sending


def runs(flags):
    """[(start, end)) of every True run."""
    out, start = [], None
    for i, f in enumerate(flags):
        if f and start is None:
            start = i
        elif not f and start is not None:
            out.append((start, i))
            start = None
    if start is not None:
        out.append((start, len(flags)))
    return out


def envelope(speech, n, hold_ms):
    """Blocks a hold of `hold_ms` would transmit, given the speech runs.

    Each run is opened `LOOKAHEAD_MS` early -- that is what the look-ahead
    delay buys -- and closed `hold_ms` late. Overlapping intervals merge,
    which is the bridging the tail does inside a phrase.
    """
    lead, tail = LOOKAHEAD_MS // BLOCK_MS, hold_ms // BLOCK_MS
    sent = bytearray(n)
    for a, b in speech:
        for i in range(max(0, a - lead), min(n, b + tail)):
            sent[i] = 1
    return sent


def main(root):
    rides = []
    for csv in sorted(Path(root).glob("*.csv")):
        got = read(csv)
        if got:
            rides.append((csv.stem, *got))
    if not rides:
        print("no voice-activated rides found in", root)
        return

    print(f"{len(rides)} voice-activated rides\n")
    print(f"{'ride':<20}{'blocks':>8}{'speak%':>8}{'sent%':>8}{'envelope%':>11}")
    tot_blocks = tot_speak = tot_sent = tot_env = 0
    all_gaps = []
    for name, speaking, sending in rides:
        n = len(speaking)
        sp = sum(speaking)
        se = sum(sending)
        # Sent without the detector agreeing: the envelope's own contribution.
        env = sum(1 for s, t in zip(speaking, sending) if t and not s)
        print(
            f"{name:<20}{n:>8}{100.0 * sp / n:>7.1f}%{100.0 * se / n:>7.1f}%"
            f"{(100.0 * env / se if se else 0):>10.1f}%"
        )
        tot_blocks += n
        tot_speak += sp
        tot_sent += se
        tot_env += env
        sr = runs(speaking)
        all_gaps += [sr[i + 1][0] - sr[i][1] for i in range(len(sr) - 1)]

    print(
        f"\n{'all':<20}{tot_blocks:>8}{100.0 * tot_speak / tot_blocks:>7.1f}%"
        f"{100.0 * tot_sent / tot_blocks:>7.1f}%"
        f"{(100.0 * tot_env / tot_sent if tot_sent else 0):>10.1f}%"
    )
    print(
        f"\n{tot_env} of {tot_sent} transmitted blocks rode the envelope "
        f"— {tot_env * BLOCK_MS / 1000:.0f} s of "
        f"{tot_sent * BLOCK_MS / 1000:.0f} s of airtime."
    )

    # **The number that decides whether the tail can be shortened.** A tail
    # that trails off into a long silence is only airtime; a tail that bridges
    # a short gap inside a phrase is holding a sentence together, and cutting
    # it splits words apart. So: how long are the gaps it is bridging?
    all_gaps.sort()
    print(f"\ngaps between speech runs ({len(all_gaps)} of them), in ms:")
    for q in (10, 25, 50, 75, 90, 95, 99):
        print(f"  p{q:<3} {all_gaps[len(all_gaps) * q // 100] * BLOCK_MS:>6}")
    # **A gap closes at `hold + LOOKAHEAD_MS`, not at `hold`.** The run after it
    # opens the look-ahead early, so the two envelopes meet that much sooner.
    # Counting against the hold alone understates what the tail bridges, and
    # disagreed with the reconstruction below — 14 gaps "newly split" where it
    # only produced 7 more runs, which is what caught it.
    for h in (100, 200, 330, 500):
        bridged = sum(1 for g in all_gaps if g * BLOCK_MS <= h + LOOKAHEAD_MS)
        print(
            f"  a {h:>3} ms hold bridges gaps to {h + LOOKAHEAD_MS:>3} ms: "
            f"{bridged:>5} of {len(all_gaps)} "
            f"({100.0 * bridged / len(all_gaps):.1f}%)"
        )

    # And what each candidate hold would actually transmit, reconstructed from
    # the speech runs the same way the worker builds it. More runs at the same
    # airtime is the shape of a phrase being chopped into pieces.
    #
    # **This table is a model and the one above is a measurement.** It will not
    # reproduce the measured airtime exactly, and should not: the older rides
    # were recorded when the look-ahead was 80 ms rather than 160, so their
    # recorded envelope is narrower than this reconstruction assumes. Read the
    # column against the 330 ms row, not against the seconds above.
    print(
        f"\n{'hold':>8}{'airtime':>10}{'vs 330':>9}{'runs':>8}{'+runs':>7}"
        f"{'median run':>12}{'newly split':>13}{'per min':>9}"
    )
    shipped = shipped_runs = shipped_bridged = None
    for h in (330, 300, 275, 250, 225, 200, 150, 100, 50):
        blocks, nruns, lens = 0, 0, []
        for _, speaking, _ in rides:
            sent = envelope(runs(speaking), len(speaking), h)
            blocks += sum(sent)
            r = runs([bool(x) for x in sent])
            nruns += len(r)
            lens += [b - a for a, b in r]
        lens.sort()
        bridged = sum(1 for g in all_gaps if g * BLOCK_MS <= h + LOOKAHEAD_MS)
        if shipped is None:
            shipped, shipped_runs, shipped_bridged = blocks, nruns, bridged
        minutes = blocks * BLOCK_MS / 1000 / 60
        print(
            f"{h:>6} ms{blocks * BLOCK_MS / 1000:>9.0f}s"
            f"{100.0 * blocks / shipped - 100:>8.1f}%"
            f"{nruns:>8}{nruns - shipped_runs:>+7}"
            f"{lens[len(lens) // 2] * BLOCK_MS:>10} ms"
            f"{shipped_bridged - bridged:>13}"
            f"{(nruns - shipped_runs) / minutes:>9.1f}"
        )
    print(
        "\n`newly split` is gaps the shipped hold bridges and this one does "
        "not:\neach is a phrase that comes apart. `per min` is the extra gate "
        "closures\nper minute of airtime that buys."
    )


if __name__ == "__main__":
    main(sys.argv[1] if len(sys.argv) > 1 else r"C:\ml_data\rides")

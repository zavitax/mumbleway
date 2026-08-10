"""How much earlier than the gate did the word actually start?

The look-ahead exists because a detector cannot decide a block is speech until
it has the block, so the sound that opened the gate has already gone by. 80 ms
was chosen for "a leading fricative, which runs 50-100 ms" -- reasoned, not
measured.

This measures it. `transmitting` in the log is already stamped against the
audio the decision applied to, so the block where it turns on *is* the first
audio the far end hears. Walking back from there through the microphone signal
until the level falls to the background says how much of the word was already
under way and was lost. That distance is exactly how much more lead is needed.

Deliberately measured on the raw microphone rather than the chain's output: the
consonant that went missing is missing from the output by definition, and the
question is whether it was ever there.
"""
import numpy as np
import sys

SR = 48000
BLOCK = 480


def load(stem):
    raw = np.fromfile(r"C:\ml_data\rides\%s.raw" % stem, dtype="<f4")
    tx = []
    at = None
    for line in open(r"C:\ml_data\rides\%s.csv" % stem):
        if line.startswith("#") or not line.strip():
            continue
        p = line.rstrip().split(",")
        if not p[0].isdigit():
            at = p.index("transmitting")
            continue
        if at is not None:
            tx.append(p[at] == "1")
    return raw, np.array(tx, dtype=bool)


def block_db(raw, n):
    usable = raw[: n * BLOCK].reshape(-1, BLOCK)
    rms = np.sqrt((usable.astype(np.float64) ** 2).mean(axis=1))
    return 20 * np.log10(rms + 1e-12)


for stem in sys.argv[1:]:
    try:
        raw, tx = load(stem)
    except OSError:
        print("%s: missing" % stem)
        continue
    n = min(len(tx), len(raw) // BLOCK)
    tx, db = tx[:n], block_db(raw, n)

    # A *local* background, not a global one.
    #
    # The first version of this used the 20th percentile of the whole ride, and
    # on a road clip that is meaningless: the background there sits above it
    # continuously, so the walk-back never terminated and every opening
    # reported the full 400 ms horizon. `p75 350 ms, max 400 ms` is not a
    # measurement, it is the horizon showing through. The quietest block in the
    # half-second before each opening is the background *that opening* rose
    # out of, and it is the only one the question is about.
    margin = 6.0          # dB above the local background to count as "sounding"
    horizon = 40          # blocks to look back: 400 ms, well past any onset
    saturated = 0

    leads = []
    for i in range(1, n):
        if not (tx[i] and not tx[i - 1]):
            continue
        lo = max(0, i - 50)
        local = db[lo:i].min() if i > lo else db[i]
        j = i - 1
        while j >= 0 and i - j <= horizon and db[j] > local + margin:
            j -= 1
        lead = (i - 1 - j) * 10
        if i - j > horizon:
            # Ran to the edge: the walk never found quiet, so this opening
            # cannot say what it needed. Counted and excluded rather than
            # folded in as a large number, which is what made the first
            # version wrong.
            saturated += 1
            continue
        leads.append(lead)

    if not leads:
        print("%s: no openings" % stem)
        continue
    leads = np.array(leads)
    print(
        "\n%s  --  %d openings measured, %d excluded (never found quiet)"
        % (stem, len(leads), saturated)
    )
    print(
        "   lead needed: p50 %d  p75 %d  p90 %d  p99 %d  max %d ms"
        % (
            np.percentile(leads, 50),
            np.percentile(leads, 75),
            np.percentile(leads, 90),
            np.percentile(leads, 99),
            leads.max(),
        )
    )
    # The decision table: what each candidate look-ahead buys, and what it
    # costs. The cost is one-way latency on every transmission, paid always;
    # the benefit is only at an opening.
    print("   look-ahead   openings fully covered   words still clipped")
    for cand in (80, 120, 160, 200, 240, 320):
        covered = 100.0 * np.mean(leads <= cand)
        missed = int(np.sum(leads > cand))
        mark = "  <- today" if cand == 80 else ""
        print(
            "     %3d ms          %5.1f%%                  %2d%s"
            % (cand, covered, missed, mark)
        )

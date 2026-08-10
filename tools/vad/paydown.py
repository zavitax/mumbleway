"""Can the look-ahead pay-down actually repay, on real phrase lengths?

`docs/ONSET_LATENCY.md` proposes keeping a long look-ahead — 400 ms, enough to
cover the whole measured onset distribution — and then transmitting slightly
faster than real time until the debt drains to a floor. It was written before
transmit runs had been measured, and the run length is what decides whether the
debt clears: the repayment happens *during* a transmission, so a phrase that
ends first ends with the debt still outstanding.

`tools/vad/hangover.py` measured the median transmit run at 1.83 s. This asks
what that means for the proposal, from the same rides.

Latency at block `i` of a run is::

    max(floor, lookahead - repay_ms_per_block * i)

averaged over every transmitted block, which weights by airtime rather than by
phrase — the right weighting, because a listener experiences latency per second
of speech and not per utterance.

Onset coverage per look-ahead is from the table in that doc, measured by
`tools/vad/onset_lead.py` over 69 real openings.

    python tools/vad/paydown.py C:\\ml_data\\rides
"""

import sys
from pathlib import Path

from hangover import BLOCK_MS, read, runs

# Openings fully covered, from docs/ONSET_LATENCY.md. 400 ms is beyond the
# measured points and is marked as such rather than interpolated into a figure
# that would look measured.
COVERAGE = {80: "89.9%", 120: "91.3%", 160: "94.2%", 240: "95.7%", 320: "98.6%"}


def latency(run_blocks, lookahead, repay_per_block, floor):
    """Mean one-way delay across one transmit run, in ms."""
    total = 0.0
    for i in range(run_blocks):
        total += max(floor, lookahead - repay_per_block * i)
    return total


def main(root):
    lens = []
    for csv in sorted(Path(root).glob("*.csv")):
        got = read(csv)
        if got:
            _, sending = got
            lens += [b - a for a, b in runs(sending)]
    if not lens:
        print("no usable rides in", root)
        return
    lens.sort()
    blocks = sum(lens)
    print(
        f"{len(lens)} transmit runs, {blocks * BLOCK_MS / 1000:.0f} s of airtime\n"
        f"run length: p25 {lens[len(lens) // 4] * BLOCK_MS} ms, "
        f"p50 {lens[len(lens) // 2] * BLOCK_MS} ms, "
        f"p75 {lens[3 * len(lens) // 4] * BLOCK_MS} ms\n"
    )

    print(
        f"{'configuration':<32}{'mean':>10}{'p50 ends':>11}{'onset':>10}{'vs ships':>10}"
    )
    cases = [
        ("160 ms flat (ships)", 160, 0.0, 160),
        ("400 ms flat", 400, 0.0, 400),
        ("400 ms, repay 5%", 400, 0.5, 60),
        ("400 ms, repay 10%", 400, 1.0, 60),
        ("400 ms, repay 20%", 400, 2.0, 60),
        ("320 ms, repay 10%", 320, 1.0, 60),
        ("320 ms, repay 20%", 320, 2.0, 60),
        ("240 ms, repay 5%", 240, 0.5, 60),
        ("240 ms, repay 10%", 240, 1.0, 60),
        ("240 ms, repay 10%, floor 40", 240, 1.0, 40),
    ]
    ships = None
    for name, look, repay, floor in cases:
        mean = sum(latency(n, look, repay, floor) for n in lens) / blocks
        if ships is None:
            ships = mean
        p50 = lens[len(lens) // 2]
        at_end = max(floor, look - repay * p50)
        cover = COVERAGE.get(look, "~99%*")
        print(
            f"{name:<32}{mean:>7.0f} ms{at_end:>8.0f} ms{cover:>10}"
            f"{mean - ships:>+9.0f} ms"
        )
    print("\n* beyond the points onset_lead.py measured; not an interpolation.")

    print(
        "\n`mean latency` is per transmitted block, so it weights by airtime.\n"
        "`end of p50 run` is where a median-length phrase finishes paying."
    )


if __name__ == "__main__":
    main(sys.argv[1] if len(sys.argv) > 1 else r"C:\ml_data\rides")

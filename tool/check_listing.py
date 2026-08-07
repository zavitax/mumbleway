"""Check docs/STORE_LISTING.md against each store's field limits.

A listing that is three characters too long is rejected by a form at the end of
a long submission, which is the worst moment to find out. This reads the fenced
blocks out of the document and measures them.

    python tool/check_listing.py

Limits are the stores' own, and they are counted in characters rather than
bytes -- every store counts what a person would count, so an em-dash is one.
"""

import re
import sys
from pathlib import Path

DOC = Path(__file__).resolve().parent.parent / "docs" / "STORE_LISTING.md"

# Heading fragment -> (limit, which stores it has to satisfy)
LIMITS = [
    ("## Name", 30, "all stores"),
    ("## Subtitle", 30, "App Store, Mac App Store"),
    ("## Short description - Google Play", 80, "Google Play"),
    ("## Short description - Microsoft Store", 500, "Microsoft Store"),
    ("## Promotional text", 170, "App Store"),
    ("## Keywords", 100, "App Store"),
    ("## Full description", 4000, "App Store, Play, Microsoft Store"),
]


def normalise(s: str) -> str:
    """Fold the dashes so a heading matches however it was typed."""
    return s.replace("—", "-").replace("–", "-")


def blocks(text: str):
    """Every fenced block, paired with the nearest heading above it."""
    found = {}
    heading = None
    in_fence = False
    body: list[str] = []
    for line in text.splitlines():
        if line.startswith("##"):
            if not in_fence:
                heading = normalise(line.strip())
        elif line.startswith("```"):
            if in_fence:
                # Only the first block under a heading counts; later ones are
                # alternatives offered in prose.
                found.setdefault(heading, "\n".join(body))
                body = []
            in_fence = not in_fence
        elif in_fence:
            body.append(line)
    return found


def main() -> int:
    if not DOC.exists():
        print(f"missing {DOC}", file=sys.stderr)
        return 2
    text = DOC.read_text(encoding="utf-8")
    found = blocks(text)

    failures = 0
    print(f"{'field':<42} {'chars':>6} {'limit':>6}  status")
    print("-" * 72)
    for fragment, limit, stores in LIMITS:
        match = next((h for h in found if h and h.startswith(fragment)), None)
        if match is None:
            # The Name row is a table rather than a fence.
            if fragment == "## Name":
                m = re.search(r"\|\s*All\s*\|\s*30\s*\|\s*`([^`]+)`", text)
                value = m.group(1) if m else None
            else:
                value = None
            if value is None:
                print(f"{fragment:<42} {'-':>6} {limit:>6}  NO BLOCK FOUND")
                failures += 1
                continue
        else:
            value = found[match]

        n = len(value)
        ok = n <= limit
        status = "ok" if ok else f"OVER by {n - limit}"
        if not ok:
            failures += 1
        label = fragment.replace("## ", "")
        print(f"{label:<42} {n:>6} {limit:>6}  {status}   ({stores})")

    # Apple counts keywords including the separators; a trailing comma or a
    # stray space is wasted budget rather than an error, so it is only noted.
    kw = next((found[h] for h in found if h and h.startswith("## Keywords")), "")
    if kw:
        if " " in kw:
            print("\nnote: keywords contain a space; Apple counts it against the 100.")
        if kw.strip().endswith(","):
            print("\nnote: keywords end with a comma, which wastes a character.")
        print(f"\nkeywords: {len(kw.split(','))} terms")

    print()
    if failures:
        print(f"{failures} field(s) will be rejected by a submission form.")
        return 1
    print("every field is inside its limit.")
    return 0


if __name__ == "__main__":
    sys.exit(main())

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

DOCS = Path(__file__).resolve().parent.parent / "docs"
LISTING = DOCS / "STORE_LISTING.md"
DESCRIPTION = DOCS / "STORE_DESCRIPTION.md"

# The per-field variants live in one document and the two pieces of prose in
# another, because the description is the thing people copy and it should not
# have to be found among the field limits.
#
# heading fragment -> (source, limit, which stores it has to satisfy)
LIMITS = [
    ("## Name", LISTING, 30, "all stores"),
    ("## Subtitle", LISTING, 30, "App Store, Mac App Store"),
    ("## Short description - Google Play", LISTING, 80, "Google Play"),
    ("## Short description - Microsoft Store", LISTING, 500, "Microsoft Store"),
    ("## Promotional text", LISTING, 170, "App Store"),
    ("## Keywords", LISTING, 100, "App Store"),
    ("## Description", DESCRIPTION, 4000, "App Store, Play, Microsoft Store"),
    # The Russian half, against the same limits -- the stores count characters,
    # not bytes, so Cyrillic is measured exactly as Latin is. It still runs
    # longer for the same meaning, which is the reason these need checking
    # rather than assuming a translation of a passing field also passes.
    ("## Russian subtitle", LISTING, 30, "App Store, Mac App Store (ru)"),
    ("## Russian short description - Google Play", LISTING, 80, "Google Play (ru)"),
    ("## Russian short description - Microsoft Store", LISTING, 500, "Microsoft Store (ru)"),
    ("## Russian promotional text", LISTING, 170, "App Store (ru)"),
    ("## Russian keywords", LISTING, 100, "App Store (ru)"),
    ("## Russian description", DESCRIPTION, 4000, "App Store, Play, Microsoft Store (ru)"),
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
    for doc in (LISTING, DESCRIPTION):
        if not doc.exists():
            print(f"missing {doc}", file=sys.stderr)
            return 2

    texts = {doc: doc.read_text(encoding="utf-8") for doc in (LISTING, DESCRIPTION)}
    parsed = {doc: blocks(text) for doc, text in texts.items()}

    failures = 0
    print(f"{'field':<42} {'chars':>6} {'limit':>6}  status")
    print("-" * 72)
    for fragment, doc, limit, stores in LIMITS:
        found = parsed[doc]
        match = next((h for h in found if h and h.startswith(fragment)), None)
        if match is None:
            # The Name row is a table rather than a fence.
            if fragment == "## Name":
                m = re.search(r"\|\s*All\s*\|\s*30\s*\|\s*`([^`]+)`", texts[doc])
                value = m.group(1) if m else None
            else:
                value = None
            if value is None:
                print(f"{fragment:<42} {'-':>6} {limit:>6}  NO BLOCK FOUND in {doc.name}")
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
    listing = parsed[LISTING]
    kw = next((listing[h] for h in listing if h and h.startswith("## Keywords")), "")
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

#!/usr/bin/env python3
"""Finds UTF-8 text that has been round-tripped through a legacy codepage.

    python tool/check_encoding.py            # report, exit 1 if anything found
    python tool/check_encoding.py --write    # repair in place

# Why this exists

`Get-Content | Set-Content -Encoding utf8` on Windows reads with the system
codepage and writes UTF-8. An em-dash goes in as E2 80 94 and comes out as three
characters that encode to nine bytes, and it stays that way forever.

Nothing catches it downstream. Rust and Dart do not care what is inside a
comment, the file still compiles, the tests still pass, and the diff looks like
whatever change was actually intended. It has been committed and pushed
unnoticed twice in this repository.

# Why it takes two passes

The first repair here looked only for cp1252 shapes -- `Ã¢`, `â` -- because that
was the damage in front of it. The system codepage on the machine this was
written on is cp1251, whose renderings share no leading character with those, so
that check passed a file that was damaged and the second incident was found by
eye months later. Both alphabets are listed below for that reason.

Add a codepage here rather than writing a third script.
"""

import io
import sys
from pathlib import Path

sys.stdout = io.TextIOWrapper(sys.stdout.buffer, encoding="utf-8", errors="replace")

REPO = Path(__file__).resolve().parent.parent

# Marker -> the codepage whose round trip produces it. The marker is what the
# leading byte of a UTF-8 punctuation sequence (E2 80 xx, or C2/C3 xx) looks
# like when read as that codepage.
SUSPECTS = {
    "вЂ": "cp1251",  # E2 80 -- dashes and smart quotes
    "РІ": "cp1251",  # doubly-encoded Cyrillic
    "â€": "cp1252",  # E2 80 -- the same punctuation, other alphabet
    "Ã¢": "cp1252",
    "Ð¿": "cp1252",  # Cyrillic seen through cp1252
}

GLOBS = [
    "app/lib/**/*.dart",
    "app/test/**/*.dart",
    "app/lib/l10n/*.arb",
    "core/src/**/*.rs",
    "core/tests/**/*.rs",
    "app/rust/src/**/*.rs",
    "docs/*.md",
    "*.md",
    "tool/*.py",
    "tools/**/*.py",
    ".github/workflows/*.yml",
]


def repair(text: str, marker: str, codepage: str) -> str:
    """Undoes one round trip of `codepage`, leaving everything else alone."""
    out: list[str] = []
    i = 0
    while i < len(text):
        if text.startswith(marker, i):
            # UTF-8 punctuation is three bytes, so the damaged form is three
            # characters. Anything that will not round-trip cleanly is left
            # exactly as it is rather than guessed at.
            chunk = text[i : i + len(marker) + 1]
            try:
                out.append(chunk.encode(codepage).decode("utf-8"))
                i += len(chunk)
                continue
            except (UnicodeEncodeError, UnicodeDecodeError):
                pass
        out.append(text[i])
        i += 1
    return "".join(out)


def main() -> int:
    write = "--write" in sys.argv
    total = 0
    touched = 0

    me = Path(__file__).resolve()
    for pattern in GLOBS:
        for path in sorted(REPO.glob(pattern)):
            if not path.is_file():
                continue
            # This file holds the damaged forms as literals in SUSPECTS, so
            # scanning it finds every one of them and reports itself as broken.
            if path.resolve() == me:
                continue
            try:
                text = path.read_text(encoding="utf-8")
            except UnicodeDecodeError:
                print(f"{path.relative_to(REPO).as_posix()}: not valid UTF-8 at all")
                total += 1
                continue

            hits = {m: text.count(m) for m in SUSPECTS if m in text}
            if not hits:
                continue

            fixed = text
            for marker, count in hits.items():
                fixed = repair(fixed, marker, SUSPECTS[marker])
                total += count

            rel = path.relative_to(REPO).as_posix()
            summary = ", ".join(f"{n}x {m!r}" for m, n in hits.items())
            print(f"{rel}: {summary}")
            first = min(text.find(m) for m in hits)
            print(f"    {text[max(0, first - 30):first + 6]!r}")
            touched += 1
            if write and fixed != text:
                # newline="" so the file's existing line endings survive.
                path.write_text(fixed, encoding="utf-8", newline="")

    print()
    if total == 0:
        print("no codepage round-trip damage found.")
        return 0
    if write:
        print(f"repaired {total} sequence(s) in {touched} file(s).")
        return 0
    print(f"{total} sequence(s) in {touched} file(s). Run with --write to repair.")
    return 1


if __name__ == "__main__":
    sys.exit(main())

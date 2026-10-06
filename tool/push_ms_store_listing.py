"""Put the Microsoft Store listing text that this repository holds into the store.

    python tool/push_ms_store_listing.py                   # show what differs; change nothing
    python tool/push_ms_store_listing.py --push            # write it to the draft submission
    python tool/push_ms_store_listing.py --push --submit    # and start certification

`tool/check_listing.py` measures the copy here against each store's limits and
`tool/read_app_store_listing.mjs` reads Apple's back. This is the third corner:
the Microsoft Store's description and release notes, read and -- only when asked
twice -- written.

## Why this exists

`publish.yml` runs `msstore publish <package>`, which creates a submission,
uploads the package and commits it in one step. It carries the listing Partner
Center already holds straight through. So every release so far has shipped new
code behind last release's words: 1.1.148.0 went out with the 1.0.1 description
and the 1.0.1 release notes, and nothing in the pipeline could have noticed.

`docs/RELEASE_NOTES.md` used to say Microsoft's fields were not scriptable. They
are -- `msstore submission update` takes the whole product back. What stays true
is the reason that file gives for keeping them out of `publish.yml`: a listing
change starts a certification run, which is a decision rather than a step. Hence
a script a person runs, defaulting to telling you what it would do.

## What it touches, and what it cannot

Two fields per language, `Description` and `ReleaseNotes`. Everything else in
the submission -- packages, screenshots, keywords, features, pricing, the age
rating -- is read from the store and written back byte for byte, because the
payload is the object `submission get` returned with two strings replaced.

That is not tidiness, it is the whole safety argument, and
`docs/RELEASING.md` 4e-bis records both halves of it being learned the expensive
way: a listing sent without `Features` or `Keywords` is a listing with those
fields *cleared*, unrecoverable once committed; and `msstore submission
updateMetadata` refuses a partial payload at all, inflating the fragment into a
whole product and complaining about a `'NotSet'` that appears nowhere in what
was sent. So: `update`, with the complete product, always.

The product ID is not in this repository -- `zavitax/mumbleway` is public. Pass
it, or set MSIX_STORE_PRODUCT_ID. Credentials belong to the `msstore` CLI's own
credential store, are never read here and never printed.

## The draft, and why the order matters

**A submission in flight blocks the next one**, and certification takes hours to
days. So text and package belong in the *same* submission, which is why
`publish.yml` stages with `msstore publish --noCommit` and then runs this with
`--push --submit`: stage, write the text, commit, one certification run.

Run on its own with no draft open, the store clones the last published
submission -- package included -- and that clone is what gets edited. A draft
changes nothing a customer sees, and `msstore submission delete <id>
--no-confirm` removes it. Only `--submit` commits.

`--submit` commits whether or not the text needed changing, because by then
there may be a staged package waiting on it. `--expect-version` is the guard
against committing a draft the package never attached to.
"""

import argparse
import io
import json
import os
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / "tool"))

from check_listing import blocks, normalise  # noqa: E402

# Microsoft's own limits for the two fields, which are not the limits the copy
# was written to: docs/RELEASE_NOTES.md writes release notes to Google Play's
# 500 so one text serves every store, and the description to 4000 for the same
# reason. Checking against the store's number rather than the writing target
# keeps this a gate on what the store will accept, not a second opinion about
# house style -- that is tool/check_listing.py's job.
LIMITS = {"Description": 10000, "ReleaseNotes": 1500}

# Store listing language -> where each field's text lives.
#
# The release notes come from distribution/whatsnew/, not from the prose in
# docs/RELEASE_NOTES.md, for the reason that file states: those two files are
# what ships to Play, so reading them here means Microsoft and Google cannot
# drift apart without somebody editing one and not the other.
SOURCES = {
    "en-us": {
        "Description": ("docs/STORE_DESCRIPTION.md", "## Description"),
        "ReleaseNotes": ("distribution/whatsnew/whatsnew-en-US", None),
    },
    "ru": {
        "Description": ("docs/STORE_DESCRIPTION.md", "## Russian description"),
        "ReleaseNotes": ("distribution/whatsnew/whatsnew-ru-RU", None),
    },
}


def read_text(relative, heading):
    """A fenced block under a heading, or a whole file when there is no heading."""
    path = ROOT / relative
    text = io.open(path, encoding="utf-8").read()
    if heading is None:
        return text.strip()
    found = {normalise(k).strip(): v for k, v in blocks(text).items()}
    if heading not in found:
        sys.exit(f"{relative}: no fenced block under {heading!r}")
    return found[heading].strip()


def msstore(*args, capture=True):
    """Run the CLI. Its progress goes to stderr, so stdout is the JSON alone."""
    proc = subprocess.run(
        ["msstore", *args],
        capture_output=capture,
        text=True,
        encoding="utf-8",
        errors="replace",
    )
    if proc.returncode != 0:
        if capture:
            sys.stderr.write(proc.stderr or "")
        sys.exit("msstore " + " ".join(args[:2]) + " failed")
    return proc.stdout or ""


def submission(product_id):
    """The draft if there is one, else the last published submission."""
    out = msstore("submission", "get", product_id)
    start = out.find("{")
    if start < 0:
        sys.exit("msstore submission get returned no JSON")
    return json.loads(out[start:])


def tidy(s):
    return s.replace("\r\n", "\n").strip()


def main():
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument(
        "product_id",
        nargs="?",
        default=os.environ.get("MSIX_STORE_PRODUCT_ID"),
        help="Store product ID (default: MSIX_STORE_PRODUCT_ID)",
    )
    ap.add_argument("--push", action="store_true", help="write the text to the draft submission")
    ap.add_argument("--submit", action="store_true", help="with --push, start certification")
    ap.add_argument(
        "--expect-version",
        metavar="X.Y.Z.0",
        help="refuse to commit unless the draft carries this package version",
    )
    args = ap.parse_args()

    if not args.product_id:
        sys.exit("Pass the Store product ID, or set MSIX_STORE_PRODUCT_ID.")
    if args.submit and not args.push:
        sys.exit("--submit does nothing without --push.")

    wanted = {
        lang: {field: read_text(*src) for field, src in fields.items()}
        for lang, fields in SOURCES.items()
    }

    too_long = [
        f"{lang} {field}: {len(text)} characters, limit {LIMITS[field]}"
        for lang, fields in wanted.items()
        for field, text in fields.items()
        if len(text) > LIMITS[field]
    ]
    if too_long:
        sys.exit("The store would refuse this:\n  " + "\n  ".join(too_long))

    product = submission(args.product_id)
    print(f"Submission {product.get('FriendlyName')} - status {product.get('Status')}")
    packages = product.get("ApplicationPackages") or []
    for package in packages:
        print(f"  package {package.get('FileName')} ({package.get('FileStatus')})")
    print()

    # `docs/RELEASING.md` 4e-bis: after staging, the draft should show the new
    # version as PendingUpload beside the old one as PendingDelete, and anything
    # else means the package did not attach. Committing then spends a
    # certification run on the text alone and leaves the build behind -- which
    # is the failure this whole script exists to stop, in a new costume.
    if args.expect_version:
        live = {p.get("Version") for p in packages}
        if args.expect_version not in live:
            sys.exit(
                f"The draft carries {sorted(v for v in live if v)} but {args.expect_version} "
                "was expected, so the package did not attach. Refusing to commit."
            )
        print(f"  {args.expect_version} is attached, as expected.\n")

    listings = product.get("Listings") or {}
    absent = [lang for lang in wanted if lang not in listings]
    if absent:
        sys.exit(
            f"The store has no listing for: {', '.join(absent)}. Add it in Partner Center first."
        )

    changes = 0
    for lang, fields in wanted.items():
        base = listings[lang]["BaseListing"]
        for field, text in fields.items():
            live = tidy(base.get(field) or "")
            if live == tidy(text):
                print(f"  {lang} {field}: already current ({len(text)} chars)")
                continue
            changes += 1
            print(f"  {lang} {field}: {len(live)} chars in the store -> {len(text)} here")
            base[field] = text

    if not args.push:
        if not changes:
            print("\nNothing to do: the store already has this text.")
            return
        print(f"\n{changes} field(s) differ. Nothing was changed.")
        print("Re-run with --push to write them to the draft submission,")
        print("and --push --submit to start certification as well.")
        return

    if changes:
        # The whole object goes back, so nothing outside those fields can move.
        payload = ROOT / "build" / "ms-store-listing.json"
        payload.parent.mkdir(parents=True, exist_ok=True)
        io.open(payload, "w", encoding="utf-8", newline="\n").write(
            json.dumps(product, ensure_ascii=False, indent=2)
        )
        print(f"\nPayload written to {payload.relative_to(ROOT)} ({payload.stat().st_size} bytes)")
        # If this is ever refused over `Pricing`, that is the known one: this
        # product has `IsAdvancedPricingModel` with per-market entries, and the
        # submission API does not always take its own base price back. Nothing
        # has changed when that happens -- the fix is `msstore publish
        # --priceId Free`, not editing the pricing block here, because a single
        # base price would flatten the per-market ones.
        print("Updating the draft submission...")
        msstore("submission", "update", args.product_id, "--payload", str(payload), capture=False)
    else:
        print("\nThe text in the draft is already current; nothing to update.")

    if not args.submit:
        print("\nNothing is in certification. The draft is open, so either")
        print("re-run with --push --submit, or press Submit in Partner Center.")
        print("`msstore submission delete <id> --no-confirm` discards it instead.")
        return

    # Committed even when the text needed no change: by this point a staged
    # package may be waiting on exactly this call.
    print("Committing the submission...")
    msstore("submission", "publish", args.product_id, capture=False)
    print("\nSubmitted. Certification takes hours to days, and one in flight")
    print("blocks the next. `msstore submission status <id>` follows it.")


if __name__ == "__main__":
    main()

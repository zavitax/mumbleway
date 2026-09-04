# Release notes

What each store shows a rider who already has the app and is being offered an
update. One set of words, in both languages, cut to the shortest limit so the
same text goes everywhere.

**The living copy for Google Play is not here.** It is
`distribution/whatsnew/`, which the publish workflow uploads with the bundle —
two files rather than a fenced block, because the action reads a directory.
This file is where the text is written and reviewed; those files are what ships.
Change both, or Play gets last release's notes.

Only Play is uploaded by the workflow. Apple's two fields are scriptable and
Microsoft's are not; [the table below](#where-each-one-goes) says which is
which, and why that is a choice rather than a gap.

Limits, shortest first: **Google Play 500**, Microsoft Store 1500, App Store
"What's New" 4000, TestFlight "What to Test" 4000. Writing to 500 means one text
serves all four.

---

## 1.0.1, build 144

The first release since 1.0 build 142, published 17 August. It went out as
1.0.1 rather than 1.0.0 because a shipped version closes its train and Apple
refuses another build under it — see `CLAUDE.md`.

Two things changed under the floor and one of them is worth a rider's
attention. The review prompt is not mentioned: it announces itself, and a
release note that says "we added a request for a review" reads as a request for
a review.

### English — 410 characters

```
Steadier voice on a poor mobile signal.

When two packets went missing together, the repair took the wrong copy and damaged three pieces of audio instead of one — a click or a stutter where a word should have been. That is fixed. Error correction now runs at full strength all the time: measuring it showed the stronger setting costs no extra data at all, so there was nothing to save by being sparing with it.
```

### Russian — 380 characters

```
Голос стабильнее на слабой мобильной связи.

Когда подряд терялись два пакета, восстановление брало не ту копию и портило втрое больше звука, чем должно было: вместо слова слышался щелчок или заикание. Это исправлено. Защита от потерь теперь всегда работает на полную — измерения показали, что более сильная настройка не добавляет трафика, так что экономить на ней было не на чем.
```

**«Защита от потерь» rather than a translation of "forward error correction".**
The English term names the mechanism; the Russian names what it does, which is
what a rider reading an update notice wants. A calque here would be
«упреждающая коррекция ошибок», which is correct, unreadable, and would be the
only phrase in the notice nobody could say out loud.

---

## Where each one goes

<div class="table-wrap" markdown="1">

| Store | Field | How it gets there |
|---|---|---|
| Google Play | *What's new* | **Automatic.** `distribution/whatsnew/` is uploaded with the bundle by `publish.yml`. |
| TestFlight | *What to Test* | API: `betaBuildLocalizations.whatsNew`, per build. |
| App Store | *What's New in This Version* | API: `appStoreVersionLocalizations.whatsNew` — **needs an editable version record.** A released version's notes cannot be changed. |
| Mac App Store | *What's New in This Version* | Same, and separately from iOS: two version records, and they drift. |
| Microsoft Store | *What's new in this version* | Part of a submission, and a submission in certification locks it. |

**Apple has two release-note fields and they are not connected.** The App Store
one lives on a *version* localization and is what a customer reads; TestFlight's
lives on a *build* as a `betaBuildLocalization` and is what a tester reads.
Writing one does nothing for the other, so on Apple release notes are always
done twice. Both were empty on 1.0.1 until they were filled deliberately.

Two traps in the API, each of which reads as something else:

- **`/apps/{id}/builds` refuses `sort` and returns an unordered page.** Asking
  it for "the newest" gave a build from three weeks earlier and hid the two
  uploaded that morning entirely — which looks exactly like a build that failed
  to upload. Use `/builds?filter[app]=…&sort=-uploadedDate`, which sorts.
- **A new version record does not inherit promotional text.** Every other field
  clones and that one arrives empty, so a version submitted without noticing
  publishes with Apple's one review-free field blank.

</div>

**Only Play is wired into `publish.yml`, and that stays true even though the
Apple half turned out to be scriptable.** Apple's notes belong to a version
record that does not exist until somebody decides to ship a version, and
Microsoft's belong to a submission that starts a certification run. Both are
decisions rather than steps, and a workflow that made them automatically would
be making them on nobody's authority. Scripting them for a human to run is a
different thing from a release doing it unasked.

## Writing the next one

- **Say what a rider will notice**, not what changed in the code. "A click or a
  stutter where a word should have been" is the same fact as "the FEC copy was
  read from the wrong packet", and only one of them means anything at 100 km/h.
- **Measure it.** 500 characters is the binding limit and Russian runs longer
  than English for the same meaning — see `tool/check_listing.py` for the same
  trap in the listing copy.
- **Leave out what announces itself.** A new button in Settings does not need a
  line here; a change to how the app sounds does.
- **Do not promise what was not measured.** This project has a standing rule
  about that in `CLAUDE.md`, and a release note is the easiest place to break
  it: "much better on poor networks" is a claim, "two lost packets no longer
  damage three" is a fact.

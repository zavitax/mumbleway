# Release notes

What each store shows a rider who already has the app and is being offered an
update. One set of words, in both languages, cut to the shortest limit so the
same text goes everywhere.

**The living copy for Google Play is not here.** It is
`distribution/whatsnew/`, which the publish workflow uploads with the bundle —
two files rather than a fenced block, because the action reads a directory.
This file is where the text is written and reviewed; those files are what ships.
Change both, or Play gets last release's notes.

The other three stores have no route from this repository and are pasted by
hand. Where they are pasted is in [the table below](#where-each-one-goes).

Limits, shortest first: **Google Play 500**, Microsoft Store 1500, App Store
"What's New" 4000, TestFlight "What to Test" 4000. Writing to 500 means one text
serves all four.

---

## Build 143

The first release since 142, published 17 August.

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
| TestFlight | *What to Test* | By hand, in App Store Connect, per build. |
| App Store | *What's New in This Version* | By hand — **and it needs a new version record.** A released version's notes cannot be edited. |
| Mac App Store | *What's New in This Version* | Same, and separately from iOS: they are different version records. |
| Microsoft Store | *What's new in this version* | By hand in Partner Center, as part of a submission. |

</div>

**Only Play is wired up.** That is not an oversight to fix casually: Apple's
notes belong to a version record that does not exist until somebody decides to
ship a version, and Microsoft's belong to a submission that starts a
certification run. Both are decisions rather than steps, and a workflow that
made them automatically would be making them on nobody's authority.

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

# Release notes

What each store shows a rider who already has the app and is being offered an
update. One set of words, in both languages, cut to the shortest limit so the
same text goes everywhere.

**The living copy for Google Play is not here.** It is
`distribution/whatsnew/`, which the publish workflow uploads with the bundle —
two files rather than a fenced block, because the action reads a directory.
This file is where the text is written and reviewed; those files are what ships.
Change both, or Play gets last release's notes.

Only Play is uploaded by the workflow. Apple's two fields are scriptable —
`tool/push_testflight_notes.mjs` for TestFlight and
`tool/push_app_store_listing.mjs` for the App Store — and Microsoft's through
`tool/push_ms_store_listing.py`;
[the table below](#where-each-one-goes) says which is which, and why keeping
them out of `publish.yml` is a choice rather than a gap.

Limits, shortest first: **Google Play 500**, Microsoft Store 1500, App Store
"What's New" 4000, TestFlight "What to Test" 4000. Writing to 500 means one text
serves all four.

---

## 1.2.0

**A minor bump, for the same reason 1.1.0 was**: it adds a feature rather than
repairing the last one.

Shipped to **TestFlight and to Google Play's internal and beta tracks only.**
macOS and the Microsoft Store are deliberately left on 1.1.0 — the change is
about giving a Bluetooth headset's hands-free profile back while only
listening, and neither of those platforms has one. Sending them an identical
build with nothing in it for their users would cost a Mac App Store upload
against the six-per-hour limit and a Microsoft certification run of hours to
days, for nothing. The `mac_app_store` input exists for exactly this and
defaults to true, so only a deliberate dispatch can leave macOS out.

```
Music at full quality while you listen.

Holding a headset's hands-free profile for a whole call drops everything you
hear — the group and any music — to telephone bandwidth. MumbleWay now gives it
back while you are only listening, and takes it again when you want to talk.

Turn on "Tap the phone to talk" in Settings to open the microphone by tapping
the phone through a pocket or a bag. Off by default, so nothing changes until
you ask for it.
```

448 characters against Google Play's 500. The Russian is 360 — shorter, because
it says the same thing without the clause about what is lost, which Russian
makes longer than English for no gain.

**What is not in the notes, deliberately.** The tap gesture's thresholds are
unmeasured: `tools/tap/` exists to settle them against hand-labelled rides and
has no corpus yet, so the notes describe the feature as something to turn on
rather than something that works well. The honest claim is the bandwidth, which
is measurable and certain; the gesture's reliability is not, and promising it
before `false arms per hour` has a number would be the sort of thing
`docs/CAPTURE_ON_DEMAND.md` warns about at length.

Also absent: the three route-reporting bugs fixed on the way. A rider cannot act
on "the diagnostic log labelled recordings with the wrong microphone", and the
notes are 500 characters shared with the thing they came here for.

## 1.1.0

The first release since 1.0.1 build 144, published 4 September. **A minor bump
rather than a patch**: this adds features rather than repairing the last lot,
and the number should say which it is before anybody opens the notes.

It could not have stayed at 1.0.1 in any case — that version is
`READY_FOR_SALE` on both of Apple's stores, which closes its train. Checked
against App Store Connect before the build rather than discovered by a red one
twenty minutes in.

**The headline is the proxy**, and it exists because of a measured fault: on
one network the TLS control channel to a server was being cut ten to twenty
seconds in while UDP kept flowing, so the app reconnected for ever and the
server was innocent. A proxy carries the control channel past whatever is doing
that. It can be set for one server or for all of them, speaks HTTP CONNECT or
SOCKS5, and voice either goes straight to the server — lower latency, and
enough on its own for that fault — or through the proxy too, for a network that
blocks UDP as well. A proxy travels by link, QR code or profile file like a
server does, and arrives behind a confirmation that names the machine.

The rest is what a rider meets rather than configures: links in a server's
welcome message and in channel descriptions now work instead of being flattened
into plain words; a channel description is drawn under the channel you are in
and the ones you are listening to; a channel you may not speak in is marked on
its row before you move there; being muted by an admin reads as somebody else's
decision rather than your own, and the meter stops claiming you are heard; and
the connection quality beside your own name is measured at last, having asked
the server about everybody except you.

For whoever runs the server: groups, access lists and registration can be
managed from the app, which is also newly written up on the site.

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
| TestFlight | *What to Test* | `tool/push_testflight_notes.mjs`, which reads `distribution/whatsnew/` so a tester and a Play tester see the same words. Per build, so there is nothing to create first and nothing to submit. |
| App Store | *What's New in This Version* | `tool/push_app_store_listing.mjs`, which carries the description with it. **Needs an editable version record** — a released version's notes cannot be changed. |
| Mac App Store | *What's New in This Version* | Same script, same run, but a separate version record from iOS: two records, and they drift. |
| Microsoft Store | *What's new in this version* | `tool/push_ms_store_listing.py`, which carries the description with it. Part of a submission, so writing it starts a certification run. |

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
  `tool/push_app_store_listing.mjs` now fills it from the previous version of
  the *same platform* when it finds it empty — not from this repository, which
  holds one set of keywords and promotional text for two platforms whose own are
  deliberately different. Those two fields it reports and never writes.

</div>

**Only Play is wired into `publish.yml`, and that stays true even though both of
the others turned out to be scriptable.** Apple's notes belong to a version
record that does not exist until somebody decides to ship a version, and
Microsoft's belong to a submission that starts a certification run. Both are
decisions rather than steps, and a workflow that made them automatically would
be making them on nobody's authority. Scripting them for a human to run is a
different thing from a release doing it unasked.

### `msstore publish` carries the old listing forward, silently

`publish.yml` runs `msstore publish <package>`, which creates a submission,
uploads the package and commits it in one step. The listing it submits is
**whatever Partner Center already held** — so a release ships new code behind
the previous release's words, and nothing in the pipeline is in a position to
notice. Four fields, not one: the description drifts along with the notes.

Measured on 1.1.0: the Microsoft Store published `1.1.148.0`, which is the right
package, with the 1.0.1 description in both languages — no proxy bullet, no
administration bullet, still carrying the "Available in English and Russian"
line that had been cut to make room — and the 1.0.1 release notes, *Steadier
voice on a poor mobile signal*. The store said `Published`, the workflow said
success, and both were telling the truth about the only thing they checked.

This is why the text needs a run of its own, after the package submission
publishes: `python tool/push_ms_store_listing.py` reports the four fields it
would change and touches nothing, and `--push --submit` makes the text-only
submission. Do it as a step of releasing, not as a thing remembered later.

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

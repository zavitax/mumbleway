# Store listing copy

Every store wants the same thing in a different shape and a different length.
This holds one set of words, cut to each field, so the app does not describe
itself differently depending on where somebody found it.

**Nothing here claims anything the app does not do.** That is not only honesty —
a listing that oversells noise cancellation earns one-star reviews from riders
who tried it at 120 km/h, and those are the reviews that stay at the top.

Character limits are the stores' own, and each field below is inside its limit.
Re-check with `tool/check_listing.py` after editing.

**This file is what the listing should say. [STORE_SURVEY.md](STORE_SURVEY.md)
is what it does**, read from the stores themselves — and the two are not the
same. Nothing measures the gap on its own: `check_listing.py` reads this
repository and cannot know whether a field was ever pasted into a store, or how
long ago. Re-survey after any release that changes what the app does.

---

## Name

| Store | Limit | Value |
|---|---|---|
| All | 30 | `MumbleWay` |

Keep it the bundle name. `MumbleWay: Rider Intercom` also fits if the plain name
turns out to be too quiet in search, but the app is called MumbleWay everywhere
else and a store name that disagrees with the icon label is its own small
confusion.

## Subtitle — App Store, Mac App Store (30)

```
Mumble voice for motorcycles
```

## Short description — Google Play (80)

```
Talk to your group over Mumble. Built for wind and engine noise inside a helmet.
```

## Short description — Microsoft Store (500)

The limit is 500 and this uses a fifth of it, deliberately. The Microsoft Store
shows this line under the app name in search results and on the product page
above the fold, where it is read in the same glance as the title — a paragraph
there is scrolled past, and the full description is directly below it anyway.

```
Voice for bikers. A Mumble client built for wind and engine noise inside a helmet. Talk through the wind!
```

<details>
<summary>The paragraph-length version this replaced</summary>

Written to the 500 and applied to the store before anyone had looked at how
that store renders it. Kept because it is the one place the "you need a server"
caveat and the office-tuning contrast are stated in one breath, and it may suit
a store that gives the field more room.

```
A Mumble voice client built for talking from inside a motorcycle helmet at speed. Wind and engine noise defeat the suppression in ordinary voice apps, which was tuned for offices; MumbleWay's is tuned for a bike. Connects to any Mumble server, works over Bluetooth intercom headsets, stays reachable over your navigation app, and reconnects itself when signal comes back. You need a Mumble server to talk to — it is a client, not a service, so there is no account and no subscription.
```

</details>

## Promotional text — App Store (170)

Changeable without a review, so use it for what is new.

Two sentences and then two more. The first pair says what the app is to
somebody who has never heard of Mumble; the second says what it does about the
one thing a rider cares about. Nothing here needs a review to change, so it can
follow whatever is worth saying this month.

The Russian is 165 characters against the English's 164, which is the rare case
of Russian not overrunning — it does here only because «Ради одного» carries
"built for one job" in three words. The whole of it is within five characters
of the limit in both languages, so a single added word breaks it; run
`tool/check_listing.py` rather than trusting an edit to be small.

```
Talk through the wind. Voice chat for bikers.

Built for one job: being heard from inside a helmet at speed. Wind and engine noise suppression with AI voice filter.
```

## Keywords — App Store (100, comma-separated, no spaces)

**Four fields, two platforms, and they are staged on 1.0.1 rather than live.**
Keywords cannot be edited on a released version, so these sit on the 1.0.1
records and go out when that version does. Version 1.0 keeps the old sets until
then. Re-read with `tool/read_app_store_listing.mjs` rather than trusting this
block — it has been wrong before.

iOS, English — 97/100. Drops `mumble` and `motorcycle`, which the subtitle
already indexes, and buys the room back as `bluetooth`, `radio` and `moto`:

```
helmet,intercom,bluetooth,walkie,talkie,radio,ptt,voip,rider,group,bike,moto,headset,murmur,comms
```

iOS, Russian — 90/100. Drops `mumble`, which the Russian subtitle carries, and
buys «шумоподавление» — the word Russian shoppers actually type, and one `шум`
will not match, since Apple's Russian stemming does not reach from a root to a
compound:

```
мотоцикл,шлем,интерком,рация,связь,байкер,группа,гарнитура,шумоподавление,мото,voip,murmur
```

Mac App Store, English — 93/100. No motorcycle vocabulary at all:

```
murmur,voip,chat,intercom,ptt,walkie,talkie,group,server,radio,headset,opus,client,comms,talk
```

Mac App Store, Russian — 88/100:

```
рация,интерком,связь,голосовой,чат,гарнитура,сервер,шумоподавление,murmur,voip,ptt,канал
```

<details>
<summary>What 1.0 still carries, until 1.0.1 ships</summary>

iOS English was 99/100 and the Mac held the older iOS set at 94 — the two had
drifted, and neither matched what this file said. That is the drift the survey
exists to catch.

```
motorcycle,helmet,intercom,mumble,voip,rider,ptt,walkie,talkie,group,bike,murmur,headset,voice chat
motorcycle,helmet,intercom,mumble,voip,rider,ptt,walkie,talkie,group,bike,comms,murmur,headset
```

</details>

**iOS and the Mac App Store have separate keyword fields, and this file used to
have one.** They live on different version localisations and can hold
different words, which they should: nobody searches the Mac App Store for a
helmet. A Mac buyer arrives having wanted a Mumble client, a push-to-talk key or
a voice server. [STORE_SURVEY.md](STORE_SURVEY.md) proposes a set for each.

Two of the fourteen words above are also being paid for twice. Apple indexes the
name, the subtitle and the keyword field as one bag, and the subtitle is already
"Mumble voice for motorcycles" — so `mumble` and `motorcycle` here are eighteen
characters spent on words Apple already has.

## Promotional text — Mac App Store (170)

**Different copy from iOS, and this file did not know it existed** until the
store was read back on 4 September 2026. It has since been replaced, on the
live 1.0 as well as on 1.0.1 — promotional text is the one App Store field that
changes without a review, so there was no reason to leave it wrong.

English — 156/170:

```
A Mumble client that shows its work.

Push to talk, open mic or voice activation, volume per person, and a panel showing what each stage does to your voice.
```

Russian — 153/170:

```
Клиент Mumble, который показывает свою работу.

Передача по кнопке, открытый микрофон или голосовая активация, громкость по каждому и панель диагностики.
```

<details>
<summary>What it replaced, and why it had to go</summary>

At exactly 170 of 170 in both languages, and it opened on the app getting
worse:

```
On a slow phone the noise chain steps down in a measured order and says which stages went. Word starts survive. Hear a recording as the far end did, or through the chain.
```

**Promotional text is the first thing a shopper reads** — it sits above the
description on the product page. That one spent its first sentence on a caveat
about degrading on slow hardware. Every clause of it is true and the graceful
step-down is a good piece of engineering; none of that makes it the thing to
lead with. The replacement says what the Mac version is *for*, and keeps the
diagnostics panel as the reason to prefer it rather than as an apology.

</details>

**A new version record does not inherit promotional text.** It comes up empty
while every other field is cloned, so a version submitted without noticing
publishes with the field blank — the one Apple field that can be changed
without a review, silently dropped. Copy it forward per platform, since these
two are not the same text.

## Russian subtitle — App Store, Mac App Store (30)

```
Mumble для мотоциклистов
```

## Russian short description — Google Play (80)

```
Разговор с группой через Mumble. Сделано под шум ветра и мотора в шлеме.
```

## Russian short description — Microsoft Store (500)

Short for the same reason as the English above.

```
MumbleWay — клиент Mumble с крутым шумоподавлением для разговора в мотоциклетном шлеме на скорости.
```

**«Крутым» here is doing two jobs, and only one was meant.** In the chain it is
the slope of a filter — «крутой фильтр верхних частот», which is what the
description says and where the sense is unmistakable. Standing alone beside
«шумоподавление» a general reader takes the colloquial one, *cool* noise
suppression, which is a claim rather than a specification. Left as it is
because it is what the store carries and it reads well; noted because the next
person to edit this line should know which sense they are inheriting.

<details>
<summary>The paragraph-length version this replaced</summary>

```
Клиент Mumble для разговора в мотоциклетном шлеме на скорости. Ветер и гул мотора сводят на нет шумоподавление обычных приложений — оно настраивалось под офисы; у MumbleWay оно настроено под мотоцикл. Приложение подключается к любому серверу Mumble, работает через Bluetooth-интеркомы, остаётся доступным поверх навигации и переподключается само, когда связь возвращается. Нужен сервер Mumble, чтобы было с кем говорить: это клиент, а не сервис, поэтому нет ни учётной записи, ни подписки.
```

</details>

## Russian promotional text — App Store (170)

```
Говорите сквозь ветер. Голосовая связь для байкеров.

Ради одного: чтобы вас слышали в шлеме на скорости. Подавление шума ветра и мотора, нейросетевой фильтр голоса.
```

## Russian keywords — App Store (100, comma-separated, no spaces)

Latin terms stay Latin: somebody looking for a Mumble client types "mumble",
not a transliteration of it, and the Russian App Store searches both alphabets.

```
мотоцикл,шлем,интерком,рация,связь,байкер,группа,гарнитура,mumble,voip,ptt,murmur,шум
```

## Microsoft Store — features and search terms

Two fields no other store has, and they were live for months without appearing
in this file: read back out of the Store rather than written here first, which
is why they read differently from everything above.

**Features** (up to 20) are shown as a bulleted list on the product page.
**Search terms** (up to 7) are never displayed and exist only for the store's
own index, so they carry the phrases somebody would type rather than the
vocabulary the rest of the listing uses.

| | English | Russian |
|---|---|---|
| Features | Voice communication · Mumble client · Murmur client · Helmet noise cancellation | Шумоподавление · Созвон · Коммуникация · Шумодав |
| Search terms | Mumble voice client · Mumble headset app · Motorcycle communication app · MumbleWay | Mumble клиент · Mumble с шумоподавлением |

Both languages use under half of what the fields allow, and the Russian search
terms are two of a possible seven — the cheapest discoverability left anywhere
in this document, since nobody reads them and there is no cost to being wrong.
Fill them with what a person would actually type: «рация для мотоциклистов»,
«интерком», «связь в шлеме», «переговорное устройство».

## Full description (4000 — fits App Store, Play and Microsoft Store)

**Lives in [STORE_DESCRIPTION.md](STORE_DESCRIPTION.md)**, with the tagline, and
is measured from there by `tool/check_listing.py`.

Deliberately not repeated here. Two copies of three thousand characters drift,
and the copy that ends up pasted into a store is never reliably the one that was
edited.

**Which is exactly what happened, and it is still live.** Every store carries a
copy one line behind this repository: the look-ahead is described as paid back
to 60 ms where `FLOOR_MS` is 200. Six copies, four stores, both languages —
Apple, Microsoft and the Play Console alike. The rest is byte-identical, so it
is one edit per store and nothing else needs re-reading. See
[STORE_SURVEY.md](STORE_SURVEY.md).

---

## What is deliberately not claimed

- **No brand names of headsets.** It has been ridden with one model. "Bluetooth
  intercom headsets" is what can be supported in general; a named brand reads as
  a compatibility promise, and it would be tested by the first person who owns a
  different one.
- **No numbers about the suppression.** No decibels, no percentages. Every
  measurement this project has of its own noise chain came from synthetic audio
  or from recordings later found to be off the wrong microphone. Nothing here is
  ready to be printed on a shelf.
- **No "crystal clear" or "studio quality".** At 120 km/h it will not be, and
  the rider who tries it will say so in public.
- **The server requirement is stated early and twice.** It is the single
  commonest reason a listing like this earns a one-star review: somebody
  installs a voice app, finds nobody to talk to, and concludes it is broken.

## Screenshots, per store

Four stores now, not three — the Microsoft Store listing is live. Each wants a
different shape, and the sizes below were read off each store's own
documentation on **2026-08-11**. They move; check the linked page before a
submission rather than trusting this table, which exists so nobody has to find
the four pages again from scratch.

<div class="table-wrap" markdown="1">

| Store | Size | Count | Format |
|---|---|---|---|
| [App Store, iPhone](https://developer.apple.com/help/app-store-connect/reference/screenshot-specifications/) | **6.9"** 1260 × 2736, or 6.5" 1284 × 2778 if no 6.9" is supplied | 1–10 per display size | PNG or JPEG, **no alpha channel** |
| App Store, iPad | **13"** 2064 × 2752, or 12.9" 2048 × 2732 | 1–10 | as above |
| [Mac App Store](https://developer.apple.com/help/app-store-connect/reference/screenshot-specifications/) | 1280 × 800, 1440 × 900, 2560 × 1600 or 2880 × 1800 — 16:10 | 1–10 | as above |
| [Google Play](https://support.google.com/googleplay/android-developer/answer/9866151) | 320–3840 px, and the long edge no more than twice the short one | **at least 2 to publish**, up to 8 per device type | PNG or JPEG |
| [Microsoft Store](https://learn.microsoft.com/en-us/windows/apps/publish/publish-your-app/screenshots-and-images) | desktop **1366 × 768 or larger**, 4K supported | 1 required, up to 10 desktop | **PNG only**, under 50 MB |

</div>

Three things that are easy to get wrong:

- **Google Play needs a 512 × 512 icon and a 1024 × 500 feature graphic** as
  well, and wants four screenshots of at least 1080 px to be eligible for
  promotion.
- **Apple rejects an alpha channel.** A screenshot saved with transparency
  fails at upload, not at review.
- **Microsoft puts text overlays on the bottom third**, so nothing that has to
  be read belongs there.

The site's own captures are in `docs/assets/img/shots/` and are cropped for the
web rather than to any of these sizes — a store screenshot is a whole device
frame, so it is a separate pass and not a resize of these.

## What is submitted, and what is not

Three of the four are live: the App Store, the Mac App Store and the Microsoft
Store. Screenshots, age ratings and Apple's App Privacy answers are all done —
this section used to list them as blockers and it outlived that by several
releases.

**Google Play is the one that is not, and it is not the listing's fault.** The
Play listing is complete in the console — both languages, an icon, a feature
graphic and screenshots at every size Play asks for — but the production track
has no releases, so there is no public page at all and the product URL 404s.
`publish.yml` uploads to the internal track and puts every wider track up as a
draft, on purpose, so this is a decision waiting to be made rather than work
waiting to be done. [STORE_SURVEY.md](STORE_SURVEY.md) has the track listing.

**The privacy policy URL is done**:
<https://zavitax.github.io/mumbleway/privacy>. Mandatory on every store here for
an app that records a microphone — Microsoft Store Policy 10.5.1 and Apple's App
Privacy both require it, and submission is blocked without it. The text above
says the app collects nothing, so the policy has to keep saying the same or the
listing is contradicted by its own fine print.

# Store listing copy

Every store wants the same thing in a different shape and a different length.
This holds one set of words, cut to each field, so the app does not describe
itself differently depending on where somebody found it.

**Nothing here claims anything the app does not do.** That is not only honesty —
a listing that oversells noise cancellation earns one-star reviews from riders
who tried it at 120 km/h, and those are the reviews that stay at the top.

Character limits are the stores' own, and each field below is inside its limit.
Re-check with `tool/check_listing.py` after editing.

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

```
On a slow phone the noise chain steps down in a measured order and says which stages went. Word starts survive. Hear a recording as the far end did, or through the chain.
```

## Keywords — App Store (100, comma-separated, no spaces)

```
motorcycle,helmet,intercom,mumble,voip,rider,ptt,walkie,talkie,group,bike,comms,murmur,headset
```

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
На медленном телефоне цепочка отключает ступени в заранее измеренном порядке и говорит, каких не стало. Начала слов целы. Запись звучит так, как её услышал бы собеседник.
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

## Still needed before any of this can be submitted

- **Screenshots at the sizes above**, per store and per device size.
- **An age rating** questionnaire per store.
- **Apple App Privacy answers**: the microphone is used for the app's core
  function and audio is not collected — but that questionnaire is answered in
  App Store Connect, not here.

**The privacy policy URL is done**:
<https://zavitax.github.io/mumbleway/privacy>. Mandatory on every store here for
an app that records a microphone — Microsoft Store Policy 10.5.1 and Apple's App
Privacy both require it, and submission is blocked without it. The text above
says the app collects nothing, so the policy has to keep saying the same or the
listing is contradicted by its own fine print.

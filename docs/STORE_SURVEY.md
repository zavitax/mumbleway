# What the stores are actually serving

Read on **3 September 2026**, from the stores themselves rather than from this
repository. [STORE_LISTING.md](STORE_LISTING.md) is what the listing *should*
say; this is what it *does*, which turns out not to be the same thing.

A listing is prose about a program, and nothing regenerates it when the program
changes. `tool/check_listing.py` measures the copy in this repository against
each store's limits — it cannot know whether that copy was ever pasted into a
store, or how long ago. This file is the other half of that check, and it needs
redoing after any release that changes what the app does.

**Not published to the site.** It is on the `exclude:` list in
[`_config.yml`](_config.yml), alongside the other two store documents. Anything
in `docs/` that is not excluded becomes a page, and a page without `lang` front
matter renders with no navigation and no footer while building perfectly.

---

## How each store was read

| Store | Route | What came back |
|---|---|---|
| App Store, iOS | App Store Connect API, `tool/read_app_store_listing.mjs` | Everything, keywords included |
| Mac App Store | same | Everything, and it is a separate version record after all |
| Google Play | Play Developer API, `edits.listings.get` | Listings, tracks and graphics, both languages |
| Microsoft Store | the Store's own product endpoint, `en-us` and `ru-ru` | Everything including the Features list |

Two things could not be read at all, and both are worth knowing before somebody
spends an afternoon trying again.

**App Store keyword fields — since solved, and the obstacle was not a
permission.** The `app-store-connect` MCP is refused what looks like a
permissions error: `The resource 'appStoreVersionLocalizations' does not allow
'GET_COLLECTION'`. It is the wrong URL. Apple offers no top-level collection
there, only the relationship under a version, and walking the relationships
returns everything including the keywords. `tool/read_app_store_listing.mjs`
does that, taking its credentials from whatever the MCP is already configured
with rather than keeping a second copy of them.

**`play.google.com` from the Windows machine.** TLS is reset immediately after
the Client Hello, which is SNI filtering rather than an outage: DNS resolves,
`www.google.com` answers, and the same request from the Mac over SSH completes.
`androidpublisher.googleapis.com` is *not* filtered, so the Developer API works
from either machine and is the better route anyway.

### Repeating it

```bash
# Apple, both storefronts — description, version, languages, genres
curl -s "https://itunes.apple.com/lookup?id=6797305046&country=us" | python -m json.tool
curl -s "https://itunes.apple.com/lookup?id=6797305046&country=ru" | python -m json.tool

# Microsoft — the whole product payload, including Features
curl -s "https://storeedgefd.dsx.mp.microsoft.com/v9.0/products/9PNZ7PWDVLTB?market=US&locale=en-us&deviceFamily=Windows.Desktop"

# Google Play — needs the service account key that publish.yml uses
node tool/read_play_listing.mjs "$GOOGLE_APPLICATION_CREDENTIALS"
```

---

## What is live

Every count below was measured on the fetched string, not estimated.

### App Store and Mac App Store

`Social Networking, Lifestyle` · 4+ · free · version 1.0, released 23 August
2026 · no ratings yet.

Name and subtitle are shared: they live on the app record, not on a version,
and change without a release.

| Field | Limit | iOS | Mac App Store |
|---|---|---|---|
| Name | 30 | 9 | 9 |
| Subtitle, EN | 30 | 28 | 28 |
| Subtitle, RU | 30 | 24 | 24 |
| Keywords, EN | 100 | **99** | **94** |
| Keywords, RU | 100 | 85 | 85 |
| Promotional text, EN | 170 | 164 | **170** |
| Promotional text, RU | 170 | 165 | 170 |
| Description, EN | 4000 | 3924 | **3978** |
| Description, RU | 4000 | 3954 | 3981 |
| What's New | 4000 | empty | empty |

**They are two version records and they have drifted apart.** Same subtitle,
different keywords, different promotional text, different description — see
findings 5 and 6. "What's New" is empty on both, which is correct: neither has
shipped a version after 1.0, and Apple shows nothing for a first release.

### Google Play

Read from the console. Tracks:

| Track | Releases |
|---|---|
| production | **none** |
| beta | none |
| alpha | `142 (1.0.0)` completed, plus one empty draft |
| internal | `1.0.0`, build 142, completed |

| Field | Limit | In the console |
|---|---|---|
| Title | 30 | 9 |
| Short description, EN | 80 | 80 |
| Short description, RU | 80 | 72 |
| Description, EN | 4000 | 3978 |
| Description, RU | 4000 | 3981 |
| Keywords | — | Play has no keyword field |

Graphics are complete in both languages: icon, feature graphic, four phone
screenshots, a ten-inch tablet screenshot. `en-US` also has a seven-inch
screenshot that `ru-RU` lacks, which Play covers from the default language.

### Microsoft Store

`Social` · read live in both languages.

| Field | Limit | Live |
|---|---|---|
| Short description, EN | 500 | 105 |
| Short description, RU | 500 | 99 |
| Description, EN | 10000 | 3978 |
| Description, RU | 10000 | 3981 |
| Features, EN | 20 | 4 |
| Features, RU | 20 | 4 |
| Search terms | 7 | not public; recorded as 4 EN, 2 RU |

The description is under half the allowance because it is shared with Apple,
whose 4000 is the binding limit. That is deliberate and should stay.

---

## Findings

### 1. Every store says the delay settles at 60 ms. It is 200.

Six copies across four stores — Apple EN and RU, Microsoft EN and RU, Play EN
and RU. All carry "the delay is then paid back to 60 ms" / «задержка снижается
до 60 мс». `FLOOR_MS` is 200.

`STORE_DESCRIPTION.md` was corrected, and so was the site. **No store was.**
Diffed against the repository copy, this one line is the *only* substantive
difference in all six — everything else is byte-identical, so one edit per store
fixes the lot and nothing else needs re-reading first.

This is the fault `CLAUDE.md` describes as the one no reader will report,
because it looks exactly like the rest of the sentence.

### 2. Google Play had no public listing — resolved, and not instant.

`play.google.com/store/apps/details?id=com.mumbleway.mumbleway` returns 404 —
byte for byte the same "Not Found" page as a package name that was never
registered, in `en_US`, `ru` and three storefronts. A control fetch of a
well-known app from the same machine returns 200, so it is the listing and not
the fetcher.

The console gave the reason in one line: **production had no releases.** The
listing itself was finished — both languages written, every required graphic
present — so nothing was blocking publication except the decision to publish.
Which followed from `publish.yml` uploading to the internal track and putting
every wider track up as a draft, on purpose.

**That decision was taken on 6 September 2026** and production now carries
`144 (1.0.1)`, with release notes in both languages.

**The page did not appear with it**, and the reason cannot be read from the
API. Hours later the product URL still answered 404 to an anonymous request in
**six storefronts** — US, GB, DE, IL, IN and RU — while a control fetch of a
well-known app through the same path returned 200 every time.

Measurement rules out most of the usual answers. Not caching: a cache-busted
request 404s and the control does not. Not the rollout: the release is
`status: completed` with no `userFraction`, so it is at 100%. Not country
targeting on the release: there is no `countryTargeting` field on it. Not one
misbehaving storefront: all six agree.

The country list turned out to be full, which kills the second of the two
candidates that were left, and one more probe killed the rest of the guesswork.

**The app appears nowhere in Play search** — zero hits for its package in the
results, while a control app appears in its own search through the same
fetcher. So this is not a detail page that is broken. The app has never entered
the public catalogue.

And the instrument was checked before that conclusion was drawn, which mattered:
a graded set of packages — an enormous one, a mid-sized one and
`se.lublin.mumla`, a small open-source Mumble client much closer to this app's
profile — all return 200 from the same path. Only this package 404s. Blaming
the fetcher would have been the comfortable answer and it is wrong.

What is left is **app-level**, and all of it is invisible to the API:

- **Still in review.** The survey found production with no releases at all on
  3 September, so the release two days later was the app's first ever, and a
  first production release is reviewed before the app joins the catalogue.
- **Managed publishing is on.** Approved changes are then held until somebody
  presses Publish — track completed, review possibly finished, nothing public.
- **A declaration is unresolved.** Data safety, content rating, target
  audience, app access, ads, privacy policy: any one of them incomplete and the
  app never goes live.

**The API cannot tell them apart, and that is the thing worth writing down.**
`androidpublisher` v3 has no app-level publishing or review status at all, so a
track reading `completed` is a record of what *you* asked for and says nothing
about what Google did with it. Country availability is not readable either:
`edits/{id}/countryavailability/{track}` was removed and now answers 404 itself,
which is easy to misread as another symptom rather than a dead endpoint.

So this one ends in the console: the app dashboard shows the review state, and
**Production → Countries/regions** shows the list. Everything up to that point
is in `tool/read_play_listing.mjs`; past it, a browser is required.

The general lesson is the same one this file keeps finding: "completed" on the
track and "live on the store" are different things, and the gap between them is
the window in which sharing the link sends people to a 404.

### 3. Beta carries the build without its release notes.

Every other track has them in both languages. Beta has `144 (1.0.1)` and
nothing to read, because it was promoted in the console rather than through
`whatsNewDirectory`, and a console promotion carries no notes unless somebody
types them.

Which is the same silent failure the reader was taught to report: nothing
distinguishes "this release has no notes" from "the notes never arrived".

### 4. The alpha track does not match what the workflow claims.

`publish.yml` uploads every track wider than internal as a draft, so that a
release cannot go live because a workflow ran. Alpha carries a **completed**
release named `142 (1.0.0)` — finished by hand — and beside it a second release
that is a draft with no name and no version codes at all.

An empty draft is what a run leaves behind when a bundle is uploaded and never
assigned. Harmless until the next promotion, which it can quietly block.

### 5. Apple says the app speaks English only.

The Russian storefront serves a fully Russian description, so the listing
localisation is there. But the product page's *Languages* row reads `EN` alone,
because that row comes from the binary's `CFBundleLocalizations` and not from
the listing. `ios/Runner/Info.plist` declares `en` and `ru` and a test asserts
it, so either the shipped 1.0 predates that or Apple's metadata is stale.

Either way a Russian shopper is told the app does not speak Russian, which is
the sort of thing that stops a download before the description is read.

### 6. The two Apple listings are not the same, and the Mac one is the odd one.

This was recorded the wrong way round on the first pass, when the two could not
be told apart. Read directly, they differ:

- **iOS** carries the trimmed description, 3924 characters, whose floating-window
  bullet names only Picture in Picture on iPhone and iPad.
- **The Mac App Store** carries the *full* shared text, 3978 characters — the one
  whose bullet begins "A floating window over your navigation app on Android".

So the worry was that the Mac listing described an iPhone feature. It is worse
than that and in the other direction: the Mac listing opens its
not-looking-at-the-screen section by naming **Android**, on Apple's own store,
and mentions the Mac panel third. It passed review, so this is a quality problem
rather than a compliance one — but it is the first thing a Mac buyer reads about
what the app does when it is not in front of them.

Their promotional texts differ too: iOS English sits at 164 of 170 and the Mac
at exactly 170.

### 7. The Mac keyword field is a copy of the iOS one, and the record of both is stale.

Now readable, and both halves of the recommendation turn out to be evidenced
rather than argued. The Mac App Store keyword field is:

```
motorcycle,helmet,intercom,mumble,voip,rider,ptt,walkie,talkie,group,bike,comms,murmur,headset
```

— which is the iOS set, word for word, on a store where nobody searches for a
helmet. And it is the *old* iOS set: live iOS English is

```
motorcycle,helmet,intercom,mumble,voip,rider,ptt,walkie,talkie,group,bike,murmur,headset,voice chat
```

at 99 of 100, where `comms` has been replaced by `voice chat`. **Neither of
those is what `STORE_LISTING.md` records**, which still has the 94-character
version with `comms`. Somebody edited iOS in the console and the repository
never heard about it — exactly the drift this survey exists to catch, and the
reason the reading tool is now committed rather than improvised.

Note also that `voice chat` spends a character on a space. Apple splits on
commas, so a space inside a term buys nothing that `voice,chat` would not.

### 8. The Microsoft Store is running at a quarter of its metadata.

Four Features of twenty. Four search terms of seven in English, two of seven in
Russian. Search terms are never displayed and cost nothing to be wrong about,
which makes them the cheapest discoverability anywhere in this project —
[STORE_LISTING.md](STORE_LISTING.md) said exactly that months ago and the slots
are still empty.

### 9. The Russian Features list drops both words a Russian user would search for.

English reads *Voice communication · Mumble client · Murmur client · Helmet
noise cancellation*. Russian reads «Шумоподавление · Созвон · Коммуникация ·
Шумодав» — four bare nouns, no «Mumble», no «Murmur», and two of them near
synonyms. Somebody looking for a Mumble client in Russian will not find that
list.

### 10. Play's short description never names a motorcycle.

"Talk to your group over Mumble. Built for wind and engine noise inside a
helmet." — no *motorcycle*, no *bike*, no *rider*, no *Bluetooth*, no
*intercom*, and all 80 characters spent. Play has no keyword field, so the short
description **is** the keyword field, and it is the most heavily weighted text
on the listing after the title.

### Also worth a look

The Play listing's public contact address is a personal Gmail account, which
Play displays on the product page once one exists.

---

## Recommended keywords

**Applied on 4 September 2026 for Apple, staged on the 1.0.1 version records
and live when that version ships.** Microsoft's are *not* applied and could not
be: the search terms and Features sit in the listing, and the listing was
locked by a submission that had already gone to certification — one in flight
blocks the next. They were free to change in that same submission and were
missed, which cost a certification cycle. Play has no keyword field; its
short-description rewrite below is unapplied.

Everything else below. Every string is measured against its field's
limit; re-measure after editing rather than trusting an edit to be small.

### The App Store field is worth more than 100 characters suggests

Apple indexes the name, the subtitle and the keyword field as one bag of words.
The subtitle is already "Mumble voice for motorcycles", so `mumble` and
`motorcycle` in the keyword field are eighteen characters spent on words Apple
already has.

**iOS, English — 97/100.** Adds `bluetooth`, `radio` and `moto`; drops `mumble`
and `motorcycle`.

```
helmet,intercom,bluetooth,walkie,talkie,radio,ptt,voip,rider,group,bike,moto,headset,murmur,comms
```

`bluetooth` because "bluetooth intercom" is the phrase riders type — it names
the hardware already on their helmet. `radio` because Apple builds phrases only
from words that were actually supplied, and "walkie talkie radio" needs it.
`moto` because it is four characters that read as the short form in Russian,
Spanish, Portuguese, Italian and French at once.

**iOS, Russian — 90/100.** Drops `mumble`, which the Russian subtitle already
carries, and buys «шумоподавление» with the room.

```
мотоцикл,шлем,интерком,рация,связь,байкер,группа,гарнитура,шумоподавление,мото,voip,murmur
```

Fourteen characters for one word, and worth it: it is the term Russian shoppers
type, and `шум` alone will not match it — Apple's Russian stemming does not
reach from a root to a compound.

### The Mac keyword field is separate, and should not be a copy

**This document has never distinguished them**, and they are different fields on
different version localisations. Nobody searches the Mac App Store for a helmet:
a Mac buyer arrives having wanted a Mumble client, a push-to-talk key or a voice
server. The motorcycle is why the app is good, not why they are looking.

**Mac, English — 93/100.**

```
murmur,voip,chat,intercom,ptt,walkie,talkie,group,server,radio,headset,opus,client,comms,talk
```

**Mac, Russian — 88/100.**

```
рация,интерком,связь,голосовой,чат,гарнитура,сервер,шумоподавление,murmur,voip,ptt,канал
```

### Google Play has no keyword field, so the short description is one

The full description is at 3979 of 4000 in English and 3982 in Russian, so a
term added there has to displace another. That is just as well — Play penalises
stuffing. The room is in the short description, and in the title.

**Short description, English — 76/80.**

```
Motorcycle helmet intercom on Mumble. Group voice chat built for wind noise.
```

**Short description, Russian — 79/80.**

```
Мотоинтерком на Mumble: групповая рация в шлеме, шумоподавление ветра и мотора.
```

Both keep Mumble and buy what the current line lacks. The English gives up
"engine" and the Russian «разговор с группой», neither of which anybody
searches for.

**The title is the biggest lever on Play and the one with a real argument
against it.** Play weights the title above everything else and nine of thirty
characters are in use; `MumbleWay: Rider Intercom` is 25 and buys the two
highest-value words on the listing. [STORE_LISTING.md](STORE_LISTING.md) already
argued the other way, and the argument is good — a store name that disagrees
with the icon label is its own small confusion. A judgement call, not a finding;
worth making deliberately rather than by default.

Terms absent from the English description entirely, if any can be worked in
while something else comes out: *rider*, *VoIP*, *radio*, *two-way*, *wind
noise*, *noise suppression*, *murmur*, *iOS*, *Windows*. Absent from the
Russian: «байкер», «рация», «гарнитура», «бесплатно», «шифрование».

### Microsoft Store — the cheapest fields anywhere

**Search terms, English, all 7.** Never displayed, so there is no cost to being
wrong; fill them with what a person would type.

```
Mumble voice client
Mumble headset app
motorcycle intercom
bluetooth helmet intercom
walkie talkie for bikers
helmet communication app
MumbleWay
```

**Search terms, Russian, all 7.** Four of these were already written down in
`STORE_LISTING.md` and never entered.

```
Mumble клиент
рация для мотоциклистов
интерком для мотоцикла
связь в шлеме
переговорное устройство
мотогарнитура
шумоподавление ветра
```

**Features, English — 14 of 20.**

```
Voice communication
Mumble client
Murmur client
Helmet noise cancellation
Wind noise suppression
Bluetooth intercom headsets
Push to talk
Voice activation
Group voice chat
Works with any Mumble server
No account, no subscription
Encrypted voice (AES-128)
Runs with the screen locked
Free and open source
```

**Features, Russian — 14 of 20.** Rewritten to match the English in kind —
phrases naming a capability rather than four bare nouns — and with «Mumble» and
«Murmur» put back.

```
Голосовая связь
Клиент Mumble
Клиент Murmur
Шумоподавление в шлеме
Подавление шума ветра
Bluetooth-гарнитуры и интеркомы
Передача по кнопке
Голосовая активация
Групповая связь
Работает с любым сервером Mumble
Без учётной записи и подписки
Шифрование голоса (AES-128)
Работает с выключенным экраном
Бесплатно, открытый исходный код
```

«Передача по кнопке» rather than «Рация», because the app's own Russian for
that mode is «По нажатию» and a Features list that names a mode should name it
the way Settings does. That is the check `CLAUDE.md` makes mechanical: every
control name in prose, looked up in the `.arb`.

---

## Keywords not to buy

The obvious next move in any ASO guide is competitor names — *Cardo*, *Sena*,
*Zello*, *TeamSpeak*, *Discord*, *Interphone*. Between them they are most of the
search volume in this niche, and all of them should stay out of the metadata.

- **Apple rejects it.** Review guideline 2.3.7 covers keywords specifically, and
  a metadata rejection costs a submission cycle rather than an edit.
- **Microsoft's store policies say the same** about metadata using marks nobody
  here has rights to.
- **And this repository already decided it**, for a better reason than either.
  [STORE_LISTING.md](STORE_LISTING.md) forbids naming headset brands because "a
  named brand reads as a compatibility promise, and it would be tested by the
  first person who owns a different one." The app has been ridden with one model
  of headset. That is what can be claimed.

`mumble` and `murmur` are a different case and stay: they are the protocol and
the server daemon this app connects to, which is a description of function
rather than a borrowed name.

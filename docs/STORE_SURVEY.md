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
| App Store, iOS | `itunes.apple.com/lookup`, US and RU storefronts, plus the public product page | Description, subtitle, promotional text |
| Mac App Store | same | Apple returns the iOS payload for this id; the Mac version could not be isolated |
| Google Play | Play Developer API, `edits.listings.get` | Listings, tracks and graphics, both languages |
| Microsoft Store | the Store's own product endpoint, `en-us` and `ru-ru` | Everything including the Features list |

Two things could not be read at all, and both are worth knowing before somebody
spends an afternoon trying again.

**App Store keyword fields.** Apple never publishes them, and the App Store
Connect API key this project uses is refused a collection read on version
localisations — `The resource 'appStoreVersionLocalizations' does not allow
'GET_COLLECTION'`. The instance read is allowed, so a localisation id would be
enough, and there is no way to obtain one without the collection read. This
needs a browser.

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

| Field | Limit | Live |
|---|---|---|
| Name | 30 | 9 |
| Subtitle, EN | 30 | 28 |
| Promotional text, EN | 170 | 164 |
| Promotional text, RU | 170 | 165 |
| Description, EN | 4000 | 3924 |
| Description, RU | 4000 | 3954 |
| Keywords | 100 | not readable |

Apple's description is a platform-trimmed variant: where the shared copy names
three platforms, Apple's says only "A floating Picture in Picture window on
iPhone and iPad". That is right for iOS and a problem for the Mac — see the
findings.

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

### 2. Google Play has no public listing.

`play.google.com/store/apps/details?id=com.mumbleway.mumbleway` returns 404 —
byte for byte the same "Not Found" page as a package name that was never
registered, in `en_US`, `ru` and three storefronts. A control fetch of a
well-known app from the same machine returns 200, so it is the listing and not
the fetcher.

The console gives the reason in one line: **production has no releases.** The
listing itself is finished — both languages written, every required graphic
present — so nothing is blocking publication except the decision to publish.
Which follows from `publish.yml` uploading to the internal track and putting
every wider track up as a draft, on purpose.

Worth stating because "we are on four stores" is easy to believe and is three.

### 3. The alpha track does not match what the workflow claims.

`publish.yml` uploads every track wider than internal as a draft, so that a
release cannot go live because a workflow ran. Alpha carries a **completed**
release named `142 (1.0.0)` — finished by hand — and beside it a second release
that is a draft with no name and no version codes at all.

An empty draft is what a run leaves behind when a bundle is uploaded and never
assigned. Harmless until the next promotion, which it can quietly block.

### 4. Apple says the app speaks English only.

The Russian storefront serves a fully Russian description, so the listing
localisation is there. But the product page's *Languages* row reads `EN` alone,
because that row comes from the binary's `CFBundleLocalizations` and not from
the listing. `ios/Runner/Info.plist` declares `en` and `ru` and a test asserts
it, so either the shipped 1.0 predates that or Apple's metadata is stale.

Either way a Russian shopper is told the app does not speak Russian, which is
the sort of thing that stops a download before the description is read.

### 5. Apple's copy drops the Mac, and the Mac listing may be reading it.

The trimmed bullet in Apple's description mentions Picture in Picture on iPhone
and iPad and nothing else. If the Mac version localisation carries the same
string, the Mac listing describes an iPhone feature and never mentions the Mac
panel at all. **Unverified** — Apple's lookup returns the iOS payload for this
id, so the two could not be told apart from here.

### 6. The Microsoft Store is running at a quarter of its metadata.

Four Features of twenty. Four search terms of seven in English, two of seven in
Russian. Search terms are never displayed and cost nothing to be wrong about,
which makes them the cheapest discoverability anywhere in this project —
[STORE_LISTING.md](STORE_LISTING.md) said exactly that months ago and the slots
are still empty.

### 7. The Russian Features list drops both words a Russian user would search for.

English reads *Voice communication · Mumble client · Murmur client · Helmet
noise cancellation*. Russian reads «Шумоподавление · Созвон · Коммуникация ·
Шумодав» — four bare nouns, no «Mumble», no «Murmur», and two of them near
synonyms. Somebody looking for a Mumble client in Russian will not find that
list.

### 8. Play's short description never names a motorcycle.

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

Nothing below has been applied. Every string is measured against its field's
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

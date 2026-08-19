# MumbleWay — tagline and description

Paste-ready copy. Field-by-field variants, character limits and the reasoning
behind what is *not* claimed are in [STORE_LISTING.md](STORE_LISTING.md); this
file is the two pieces of writing themselves, kept clean so they can be copied
without editing.

---

## Tagline

> **Talk through the wind.**

Four words, and they name the problem rather than the product category. Every
rider who has tried to speak into a phone at 100 km/h knows immediately what it
is about; nobody else needs to.

Alternates, if a store wants a different length or the primary is taken:

| | Tagline | Use |
|---|---|---|
| Short | **Talk through the wind.** | primary |
| Descriptive | **Group voice, built for a helmet at speed.** | where the category is not obvious |
| Plainest | **Mumble voice for motorcycles.** | app subtitle field, 28 characters |

---

## A hundred-word blurb

A standalone paragraph, for the places that want the app described once and
briefly: the opening of a long description, a forum or press post, a paragraph
on somebody else's site. **100–120 words.** Both languages are kept in this one
section rather than beside their own long descriptions, because the two have to
stay in step and a pair split across a file drifts apart.

**It does not fit any of the stores' short fields, and it is not meant to.**
Those are much tighter and already have their own entries in
`STORE_LISTING.md`, checked by `tool/check_listing.py`:

| Field | Limit | This blurb |
|---|---|---|
| Short description — Google Play | 80 characters | 707 (en) / 754 (ru) |
| Promotional text — App Store | 170 characters | far over |
| Short description — Microsoft Store | 500 characters | over |
| Description — all three | 4000 characters | fits, with room |

The section was first written claiming it was *for* the first three of those,
which it never could have been. Sizes are why the table gives characters as
well as words: a word count is the brief a person is given and a character
count is what a store enforces, and the two do not convert.

Counted rather than estimated: **English 120 words, Russian 112.** The Russian
is shorter for once in the helpful direction — it carries this particular
argument in fewer words. That is the opposite of the long description, where
Russian overruns and `tool/check_listing.py` exists to catch it.

**Do not count these with `wc -w`.** It is not UTF-8 aware in the shell on the
development machine and miscounts Cyrillic — it read this Russian text as 114
words where it is 112, and the English, being ASCII, agreed exactly. A tool
that is right on one language and quietly wrong on the other is worse than no
tool. Count with something that decodes first:

```bash
python -c "import io,sys; print(len(io.open(sys.argv[1],encoding='utf-8').read().split()))" file.txt
```

Both counts include the three standalone em-dashes as words, which no store
cares about at this length but is worth knowing before anybody argues about a
limit.

```
Talk through the wind. Voice chat for bikers.

MumbleWay is a client for Mumble servers, built for one job: being heard from inside a helmet at speed. Wind defeats the noise suppression in ordinary voice apps, tuned for offices. This one is tuned for a motorcycle: a neural speech cleaner running on the phone, wind and engine filters, and a transmit decision weighing your voice against a noise floor that climbs with your speed. A steady drone never clears it. Speech does.

Group voice with no range limit: one channel over mobile data, however far apart. Push to talk, voice activation or open mic, through your Bluetooth intercom.

No account, no subscription, no servers of ours. Free and open source.
```

```
Говорите сквозь ветер. Голосовая связь для байкеров.

MumbleWay — клиент Mumble, сделанный ради одного: чтобы вас слышали в шлеме на скорости. Ветер сбивает шумоподавление обычных голосовых приложений — их настраивали для офиса. Здешний тракт настроен под мотоцикл: нейросетевой очиститель речи прямо на телефоне, фильтры ветра и мотора и решение о передаче, которое сравнивает ваш голос с уровнем шума, а тот растёт вместе со скоростью. Ровный гул этот порог не берёт. Речь берёт.

Групповая связь без ограничения по дальности: все заходят в один канал через мобильный интернет, как бы далеко ни растянулись. По нажатию, по голосу или открытый микрофон — через вашу Bluetooth-гарнитуру.

Ни учётных записей, ни подписки, ни наших серверов. Свободное ПО.
```

**Every claim in it is checkable, and that constraint shaped it.** The enhancer
runs on the device; the noise floor is tracked and the transmit margin measured
against it, which is why a steady drone never clears it and speech does; and
"no servers of ours" is the sentence the privacy policy already makes. There is
deliberately not a single number in it that can go stale. The long description
below had one that did: it promised the look-ahead was paid back to 60 ms long
after `FLOOR_MS` became 200, and nothing in the repository was in a position to
notice.

The Russian uses the names the app and the site already use — «тракт»,
«очиститель речи», «По нажатию», «по голосу», «открытый микрофон» — so somebody
who installs it meets the same words on screen. Written as Russian rather than
translated across: «Ровный гул этот порог не берёт. Речь берёт.» is the pair the
whole argument turns on, and it had to land as a pair in both languages.

The opening line is two sentences on purpose. **Talk through the wind** names
the problem and nothing else, which is why it is the tagline; a reader who has
never heard of Mumble still needs telling what the thing *is*, and **Voice chat
for bikers** does that in four words without a category label nobody searches
for.

---

## Description

```
MumbleWay is a voice client for Mumble servers, built for talking from inside a motorcycle helmet at speed.

Wind is the hard part. At speed it is loud, broadband and relentless, and it defeats the suppression in ordinary voice apps, which was tuned for offices. MumbleWay's chain is built for a different problem: a steep high-pass that strips wind and engine rumble before anything else sees it, a neural denoiser for the broadband roar, and a transmit decision that measures your voice against a noise floor which climbs with speed. A steady drone raises that floor with it and never clears the margin, however loud. Speech does.

YOU NEED A MUMBLE SERVER
MumbleWay connects to any of them — a friend's box, one at the clubhouse, a public server from the built-in directory, or one you run yourself. It is a client, not a service: no account, no subscription, nobody's servers in the middle.

BUILT FOR A BIKE
• Noise profiles from Light to Helmet, or Auto, which picks from what it hears and shows where it landed.
• Voice activation opens 240 ms ahead of its own decision, so a word keeps its first consonant; the delay is then paid back to 200 ms.
• Push to talk, voice activation, or open microphone.
• Works over Bluetooth intercom headsets on the hands-free profile, where the boom microphone lives.
• Pair a handlebar Bluetooth remote and it learns whatever yours actually sends, rather than offering a list of keys that may not match it. Hold to talk, or tap to toggle.
• Walkie-talkie cues on key and unkey, so you know you are live without looking down.

FOR WHEN YOU ARE NOT LOOKING AT THE SCREEN
• A floating window over your navigation app on Android, Picture in Picture on iPhone and iPad, a panel on Mac.
• A falling two-tone when the connection drops and a rising one when it returns — you learn from the headset, not from silence.
• Automatic reconnection on everything except a disconnect you asked for, every ten seconds with the countdown on screen, and at once when your phone reports signal is back.
• Audio keeps running with the screen locked and the phone in a pocket.

TALKING TO PEOPLE
• Channel tree and roster, with per-person mute and volume.
• Group voice with no range limit: everyone joins one channel over mobile data.
• Two servers connected at once.
• Live ping per server, and whether voice goes direct over UDP or is tunnelled through TCP when a carrier will not pass it.
• A jitter buffer that plays a backlog off at up to double speed, rather than leaving everybody a second behind.
• Join by QR code or a mumble:// link.

PRIVATE BY DEFAULT
Voice is encrypted with AES-128 and the control channel runs over TLS. Server certificates are pinned on first connection and a changed one is refused until you say otherwise. MumbleWay has no servers of its own, collects no analytics and shows no advertising.

ON AN OLDER PHONE
A block of audio arrives every 10 ms and the chain has to finish before the next one. If your phone cannot manage that, MumbleWay gives stages up one at a time, cheapest first, in a measured order — and says which ones, rather than quietly sounding worse.

DIAGNOSTICS
An optional panel shows the chain working — the spectrum before and after suppression, and which stage stopped a sound reaching the far end. A recorder, off unless you switch it on, captures what your headset hears on a ride; play it back as the far end would have heard it, without a second phone.

NOT ONLY ON A BIKE
In a quiet room the Light profile takes almost nothing out, and diagnostics shows what each stage does to your voice — so you tune by looking, not guessing. At a desk or at 120 km/h, the same client: Opus at 48 kHz, no account, no telemetry.

Available in English and Russian.

MumbleWay is free and open source. It is an independent client, not affiliated with the Mumble project.

MADE BY
Ilya Melamed
Site: https://zavitax.github.io/mumbleway/
Source: https://github.com/zavitax/mumbleway
Chat: https://discord.gg/NTASPRFjm
```

Inside the 4000 that App Store, Google Play and Microsoft Store each allow —
and only just, so run `python tool/check_listing.py` after any edit rather than
counting by eye. The count in this line used to be written out and went stale
by nine hundred characters, which is worse than not stating it at all.

## Russian description

The same piece of writing, not a machine translation of it: the terms are the
ones the Russian site already uses, so somebody who reads the page and then the
listing meets the same words twice. Russian runs longer than English for the
same meaning, which is why several sentences here are shorter than their
English counterparts rather than translated clause for clause.

```
MumbleWay — клиент Mumble для разговора в мотоциклетном шлеме на скорости.

Самое трудное — ветер. На скорости он громкий, широкополосный и не прекращается, и он сводит на нет шумоподавление обычных приложений для звонков: его настраивали под офисы. Тракт MumbleWay сделан под другую задачу: крутой фильтр верхних частот убирает ветер и гул мотора до того, как до них дойдёт остальная цепочка, нейросетевой очиститель работает с широкополосным рёвом, а решение о передаче сравнивает голос с уровнем шума, который растёт вместе со скоростью. Ровный гул поднимает этот уровень вместе с собой и никогда его не превышает, как бы громко ни звучал. Речь — превышает.

НУЖЕН СЕРВЕР MUMBLE
MumbleWay подключается к любому: к серверу друга, к клубному, к публичному из встроенного каталога или к вашему собственному. Это клиент, а не сервис: ни учётной записи, ни подписки, ни чужих серверов посередине.

СДЕЛАНО ПОД МОТОЦИКЛ
• Профили шумоподавления от «Слабого» до «Шлема» и «Авто», который выбирает по тому, что слышит, и показывает, на чём остановился.
• Голосовая активация открывает передачу на 240 мс раньше, чем принимает решение, — у слова остаётся первый согласный. Потом задержка снижается до 200 мс.
• Кнопка передачи, голосовая активация или открытый микрофон.
• Работает через Bluetooth-интеркомы по профилю hands-free — там находится микрофон на штанге.
• Пульт на руле приложение опознаёт по тому, что он на самом деле присылает, а не по списку клавиш. Держать для передачи или нажимать для переключения.
• Сигналы рации при включении и выключении передачи: слышно, что вы в эфире, без взгляда на экран.

КОГДА ВЫ НЕ СМОТРИТЕ НА ЭКРАН
• Плавающее окно поверх навигации на Android, «картинка в картинке» на iPhone и iPad, панель на Mac.
• Нисходящий сигнал, когда связь оборвалась, и восходящий, когда вернулась, — вы узнаёте об этом из гарнитуры, а не из тишины.
• Автоматическое переподключение — кроме случая, когда вы отключились сами: раз в десять секунд с отсчётом на экране и сразу, как только телефон сообщит, что связь вернулась.
• Звук работает с заблокированным экраном и телефоном в кармане.

РАЗГОВОР С ЛЮДЬМИ
• Дерево каналов и список участников, у каждого своя громкость и своё «заглушить».
• Групповой разговор без ограничения по расстоянию: все в одном канале через мобильный интернет.
• Два сервера одновременно.
• Текущий пинг по каждому серверу и то, идёт ли голос напрямую по UDP или через TCP, когда оператор не пропускает первое.
• Буфер джиттера проигрывает накопленное до двух раз быстрее, а не оставляет всех на секунду позади.
• Подключение по QR-коду или ссылке mumble://.

ПРИВАТНОСТЬ ПО УМОЛЧАНИЮ
Голос шифруется AES-128, управляющий канал идёт по TLS. Сертификат сервера запоминается при первом подключении, а изменившийся — отклоняется, пока вы не подтвердите. У MumbleWay нет своих серверов, он не собирает аналитику и не показывает рекламу.

НА СТАРОМ ТЕЛЕФОНЕ
Блок звука приходит каждые 10 мс, и вся цепочка должна успеть до следующего. Если телефон не справляется, MumbleWay отключает ступени по одной, начиная с самых дешёвых, в измеренном заранее порядке — и говорит, каких не стало, вместо того чтобы молча звучать хуже.

ДИАГНОСТИКА
Панель показывает работу тракта: спектр до и после шумоподавления и то, какая ступень не пустила звук дальше. Там же запись — выключенная, пока вы её не включите: послушайте свою поездку так, как её слышал бы собеседник, без второго телефона.

НЕ ТОЛЬКО НА МОТОЦИКЛЕ
В тишине профиль «Слабое» почти ничего не убирает, а диагностика показывает, что каждая ступень делает с голосом, — настройка глазами, а не наугад. За столом и на скорости 120 км/ч это один и тот же клиент: Opus 48 кГц, без учётной записи и телеметрии.

Русский и английский языки.

MumbleWay — свободное приложение с открытым кодом. Независимый клиент, не связанный с проектом Mumble.

АВТОР
Илья Меламед
Сайт: https://zavitax.github.io/mumbleway/ru/
Исходный код: https://github.com/zavitax/mumbleway
Чат: https://discord.gg/NTASPRFjm
```

### A note on the bullet character

`•` rather than `-`, because Google Play and the Microsoft Store render the
description as near-plain text and a hyphen at the start of a line reads as a
dash mid-sentence once the text reflows. Apple wraps it the same way. None of
the three supports Markdown here, so the formatting has to survive being
treated as prose.

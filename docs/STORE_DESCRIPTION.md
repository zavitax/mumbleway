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

## Description

```
MumbleWay is a voice client for Mumble servers, built for talking from inside a motorcycle helmet at speed.

Wind is the hard part. At speed it is loud, broadband and relentless, and it defeats the suppression in ordinary voice apps, which was tuned for offices. MumbleWay's chain is built for a different problem: a steep high-pass that strips wind and engine rumble before anything else sees it, a neural denoiser for the broadband roar, and a transmit decision that measures your voice against a noise floor which climbs with speed. A steady drone raises that floor with it and never clears the margin, however loud. Speech does.

YOU NEED A MUMBLE SERVER
MumbleWay connects to any of them — a friend's box, one at the clubhouse, a public server from the built-in directory, or one you run yourself. It is a client, not a service: no account, no subscription, nobody's servers in the middle.

BUILT FOR A BIKE
• Noise profiles from Light to Helmet, or Auto, which picks from what it hears and shows where it landed.
• Voice activation opens 240 ms ahead of its own decision, so a word keeps its first consonant; the delay is then paid back to 60 ms.
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
Ilya Melamed — ilya77@gmail.com
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

Самое трудное — ветер. На скорости он громкий, широкополосный и не прекращается, и он сводит на нет шумоподавление обычных приложений для звонков: его настраивали под офисы. Тракт MumbleWay сделан под другую задачу: крутой фильтр верхних частот убирает ветер и гул мотора до того, как до них доберётся остальная цепочка, нейросетевой очиститель работает с широкополосным рёвом, а решение о передаче сравнивает голос с уровнем шума, который растёт вместе со скоростью. Ровный гул поднимает этот уровень вместе с собой и никогда его не превышает, как бы громко ни звучал. Речь — превышает.

НУЖЕН СЕРВЕР MUMBLE
MumbleWay подключается к любому: к серверу друга, к клубному, к публичному из встроенного каталога или к вашему собственному. Это клиент, а не сервис: ни учётной записи, ни подписки, ни чужих серверов посередине.

СДЕЛАНО ПОД МОТОЦИКЛ
• Профили шумоподавления от «Лёгкого» до «Шлема» и «Авто», который выбирает по тому, что слышит, и показывает, на чём остановился.
• Голосовая активация открывается на 240 мс раньше собственного решения, поэтому у слова остаётся первый согласный; задержка потом возвращается к 60 мс.
• Кнопка передачи, голосовая активация или открытый микрофон.
• Работает через Bluetooth-интеркомы по профилю hands-free — там живёт микрофон на штанге.
• Пульт на руле обучается тому, что он на самом деле присылает, а не выбирается из списка клавиш. Держать для передачи или нажимать для переключения.
• Сигналы рации при включении и выключении передачи: вы в эфире, не глядя на экран.

КОГДА ВЫ НЕ СМОТРИТЕ НА ЭКРАН
• Плавающее окно поверх навигации на Android, «картинка в картинке» на iPhone и iPad, панель на Mac.
• Нисходящий сигнал, когда связь оборвалась, и восходящий, когда вернулась, — вы узнаёте об этом из гарнитуры, а не из тишины.
• Автоматическое переподключение везде, кроме отключения, о котором вы попросили сами: раз в десять секунд с отсчётом на экране и сразу, как только телефон сообщит, что сеть вернулась.
• Звук работает с заблокированным экраном и телефоном в кармане.

РАЗГОВОР С ЛЮДЬМИ
• Дерево каналов и список участников, у каждого своя громкость и своё «заглушить».
• Групповой разговор без ограничения по расстоянию: все заходят в один канал через мобильный интернет.
• Два сервера одновременно.
• Текущий пинг по каждому серверу и то, идёт ли голос напрямую по UDP или через TCP, когда оператор не пропускает первое.
• Буфер джиттера проигрывает накопленное до двух раз быстрее, а не оставляет всех на секунду позади.
• Подключение по QR-коду или ссылке mumble://.

ПРИВАТНОСТЬ ПО УМОЛЧАНИЮ
Голос шифруется AES-128, управляющий канал идёт по TLS. Сертификат сервера запоминается при первом подключении, изменившийся отклоняется, пока вы не подтвердите. У MumbleWay нет своих серверов, он не собирает аналитику и не показывает рекламу.

НА СТАРОМ ТЕЛЕФОНЕ
Блок звука приходит каждые 10 мс, и вся цепочка должна успеть до следующего. Если телефон не справляется, MumbleWay отключает ступени по одной, начиная с самых дешёвых, в измеренном заранее порядке — и говорит, каких не стало, вместо того чтобы молча звучать хуже.

ДИАГНОСТИКА
Панель показывает работу тракта: спектр до и после шумоподавления и то, какая ступень не пустила звук дальше. Там же запись — выключенная, пока вы её не включите: послушайте свою поездку так, как её слышал бы собеседник, без второго телефона.

НЕ ТОЛЬКО НА МОТОЦИКЛЕ
В тишине профиль «Лёгкий» почти ничего не убирает, а диагностика показывает, что каждая ступень делает с голосом, — настройка глазами, а не наугад. За столом и на 120 км/ч это один и тот же клиент: Opus 48 кГц, без учётной записи и телеметрии.

Есть русский и английский языки.

MumbleWay — свободное приложение с открытым кодом. Независимый клиент, не связанный с проектом Mumble.

АВТОР
Илья Меламед — ilya77@gmail.com
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

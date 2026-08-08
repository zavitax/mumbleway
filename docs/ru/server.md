---
layout: default
lang: ru
ref: server
title: Свой сервер
description: Сервер Mumble на Windows, macOS или Linux примерно за десять минут.
---

MumbleWay работает с любым сервером [Mumble]({{ site.mumble }}). Серверная
программа называется **Mumble Server** (исторически *Murmur*, и исполняемый
файл часто до сих пор называется `mumble-server` или `murmurd`).

Понадобится одно из трёх:

- **Машина дома** — хватит запасного ПК, NAS или Raspberry Pi. Сервер Mumble
  для группы мотоциклистов почти не нагружает процессор и занимает несколько
  мегабайт памяти.
- **Дешёвый VPS** — самого младшего тарифа у любого провайдера более чем
  достаточно, и это избавляет от необходимости открывать порт дома.
- **Хостинг Mumble** — несколько компаний сдают такие серверы помесячно.

<div class="panel">
<p><strong>Порт 64738, TCP <em>и</em> UDP.</strong> Mumble использует TCP для
управления и UDP для голоса. Если UDP закрыт, голос уходит по TCP — это
работает и добавляет задержку. Пробросьте оба.</p>
</div>

## Linux

Самое привычное место для сервера Mumble, и возни здесь меньше всего.

### Debian, Ubuntu, Raspberry Pi OS

```bash
sudo apt update
sudo apt install mumble-server

# Задаёт пароль SuperUser и включает автозапуск службы.
sudo dpkg-reconfigure mumble-server
```

Конфигурация лежит в `/etc/mumble-server.ini` (в старых пакетах —
`/etc/murmur.ini`). После правки:

```bash
sudo systemctl restart mumble-server
sudo systemctl status mumble-server
```

### Fedora, RHEL

```bash
sudo dnf install mumble-server
sudo systemctl enable --now mumble-server
sudo mumble-server -supw ВАШ_ПАРОЛЬ_SUPERUSER
```

### Docker, где угодно

```bash
docker run -d --name mumble \
  -p 64738:64738 -p 64738:64738/udp \
  -v mumble-data:/data \
  --restart unless-stopped \
  mumblevoip/mumble-server:latest
```

Задайте пароль SuperUser при первом запуске:

```bash
docker exec -it mumble mumble-server -supw ВАШ_ПАРОЛЬ_SUPERUSER
```

## Windows

1. Скачайте пакет **сервера** с
   [mumble.info/downloads]({{ site.mumble }}downloads/) — это отдельная
   загрузка, не клиент.
2. Установите его. Установщик предложит запускать сервер как службу Windows;
   согласитесь, если хотите, чтобы он поднимался после перезагрузки.
3. Настройте `murmur.ini` (или `mumble-server.ini`) рядом с исполняемым файлом
   либо в `%ProgramFiles%\Mumble\`.
4. Задайте пароль SuperUser из консоли администратора:

```powershell
cd "C:\Program Files\Mumble"
.\mumble-server.exe -supw ВАШ_ПАРОЛЬ_SUPERUSER
```

5. Разрешите его в брандмауэре — оба протокола:

```powershell
New-NetFirewallRule -DisplayName "Mumble TCP" -Direction Inbound `
  -Protocol TCP -LocalPort 64738 -Action Allow
New-NetFirewallRule -DisplayName "Mumble UDP" -Direction Inbound `
  -Protocol UDP -LocalPort 64738 -Action Allow
```

<div class="panel warn">
<p>В старых выпусках исполняемый файл назывался <code>murmur.exe</code>, в новых
— <code>mumble-server.exe</code>. Используйте тот, что лежит в папке.</p>
</div>

## macOS

Homebrew — наименее болезненный путь:

```bash
brew install mumble-server
brew services start mumble-server
```

Задайте пароль SuperUser:

```bash
mumble-server -supw ВАШ_ПАРОЛЬ_SUPERUSER
```

Файл конфигурации лежит внутри префикса Homebrew — `/opt/homebrew/etc/` на
Apple Silicon и `/usr/local/etc/` на Intel. `brew info mumble-server` печатает
точные пути для вашей установки.

Домашний Mac — вполне приличный сервер для группы, но он не должен засыпать:
Системные настройки → Экономия энергии, отключить сон.

## Настройки, которые стоит поменять

В `mumble-server.ini` / `murmur.ini`:

<div class="table-wrap" markdown="1">

| Настройка | Что поставить | Зачем |
|---|---|---|
| `welcometext` | Название вашей группы | Показывается при подключении. |
| `serverpassword` | Что-нибудь, если сервер смотрит наружу | Простейший контроль доступа, какой бывает. |
| `port` | `64738` | Зарегистрированное значение по умолчанию. Меняйте, только если иначе никак. |
| `users` | `20` | Поставьте предел. Оставлять его без ограничения незачем. |
| `bandwidth` | `72000` | Бит в секунду на человека, с запасом для Opus. Снизьте, если исходящий канал узкий. |
| `registerName` | Название вашей группы | Имя корневого канала. |
| `registerUrl`, `registerHostname` | *оставить пустыми* | **Заполнив их, вы вносите сервер в публичный каталог.** Оставьте пустыми, чтобы остаться вне списка. |
| `allowping` | `false` | Не даёт посторонним опрашивать сервер о числе участников. |
| `sslCert`, `sslKey` | Пути к настоящему сертификату | Необязательно. Без него клиенты видят самоподписанный сертификат и запоминают его при первом подключении. |

</div>

## Подключение из MumbleWay

1. **Добавьте сервер** в приложении.
2. **Адрес** — ваш публичный IP, имя динамического DNS или имя хоста VPS.
3. **Порт** — 64738, если вы его не меняли.
4. **Имя пользователя** — любое; так вас будет видно в канале.
5. **Пароль** — `serverpassword`, если вы его задали.

Дальше поделитесь с группой: откройте **QR-код** сервера в приложении и дайте
его отсканировать — это лучше, чем диктовать IP-адрес через шлем.

<div class="panel good">
<p><strong>Регистрация участников.</strong> Подключитесь один раз настольным
клиентом Mumble как <code>SuperUser</code> с заданным паролем и
зарегистрируйте всех. Mumble опознаёт людей по клиентскому сертификату, а не по
паролю, так что зарегистрированный участник с этого момента узнаётся
автоматически — потому и стоит беречь настройку <em>Идентификация</em> в
приложении.</p>
</div>

<div class="shots">
  <figure>
    <img src="{{ '/assets/img/shots/addserver-phone.webp' | relative_url }}"
         alt="Форма добавления сервера: отображаемое имя, адрес, порт, имя
              пользователя и необязательный пароль, а также кнопки для
              просмотра публичных серверов, импорта файла и сканирования
              QR-кода."
         width="560" height="1217" loading="lazy" decoding="async">
    <figcaption>Ввести один раз — или отсканировать QR-код, который делает
    приложение.</figcaption>
  </figure>
</div>

## Дальше

Здесь описано ровно столько, чтобы группа заговорила. В Mumble есть заметно
больше — списки доступа и группы, права на каналы, администрирование через
Ice/gRPC, боты, позиционный звук, аутентификация через LDAP:

<div class="panel">
<p><a href="{{ site.mumble_docs }}"><strong>Документация Mumble →</strong></a><br>
<span class="muted">Настройка сервера, администрирование и сам протокол, от
проекта Mumble. На английском языке.</span></p>
<p><a href="{{ site.mumble }}"><strong>mumble.info →</strong></a><br>
<span class="muted">Загрузки, сообщество и новости.</span></p>
</div>

---
layout: default
lang: ru
ref: licences
title: Лицензии
description: Собственная лицензия MumbleWay и каждый сторонний компонент, из которых он собран.
---

## MumbleWay

**GNU General Public License, версия 3.** Полный текст —
[в репозитории]({{ site.repo }}/blob/main/LICENSE).

Программу можно использовать, изучать, изменять и распространять. Если вы
распространяете изменённую версию, она должна выходить под той же лицензией, а
её исходный код — быть доступным.

<div class="panel">
<p><strong>С проектом Mumble не связан.</strong> MumbleWay — независимый
клиент, говорящий по протоколу Mumble. Название Mumble и товарные знаки
принадлежат проекту Mumble, и это приложение он не одобрял и не поддерживает.
О проблемах с приложением пишите <a href="{{ site.repo }}/issues">сюда</a>, а
не им.</p>
</div>

## Протокол

Протокол Mumble описан открыто, и здесь он реализован независимо, по этим
описаниям. Сам Mumble распространяется под лицензией **BSD 3-Clause**, см.
[mumble.info]({{ site.mumble }}).

## Звук

То, что делает собственно работу.

<div class="table-wrap" markdown="1">

| Компонент | Роль | Лицензия |
|---|---|---|
| [Opus](https://opus-codec.org/) | Голосовой кодек, через крейты `opus` и `audiopus_sys` | BSD 3-Clause |
| [RNNoise](https://jmvalin.ca/demo/rnnoise/), как [`nnnoiseless`](https://crates.io/crates/nnnoiseless) | Нейросетевое шумоподавление и детектор речевой активности | BSD 3-Clause |
| [DeepFilterNet](https://github.com/Rikorose/DeepFilterNet) | Очистка речи в начале цепочки записи, вместе с весами модели. Низколатентная DFN3 идёт из крейта `deep_filter`, обычная DFN3 лежит в `core/models/` — для нижней ступени лестницы производительности | MIT / Apache-2.0 |
| [`tract`](https://github.com/sonos/tract) | Исполняет эту модель — на чистом Rust, без нативной среды, которую пришлось бы собирать под каждую платформу | MIT / Apache-2.0 |
| [`cpal`](https://crates.io/crates/cpal) | Кроссплатформенный доступ к аудиоустройствам | Apache-2.0 |
| [`dasp_sample`](https://crates.io/crates/dasp_sample) | Преобразование форматов отсчётов | MIT / Apache-2.0 |
| [YAMNet](https://github.com/tensorflow/models/tree/master/research/audioset/yamnet) | Классификатор звука, благодаря которому «Автоматически» слышит мотор и берёт шлемный профиль. Поставляется как `assets/models/yamnet.tflite` | Apache-2.0 |
| [LiteRT / TensorFlow Lite](https://ai.google.dev/edge/litert) | Запускает эту модель. На Android и iOS — из Maven и CocoaPods самой Google, на macOS — универсальная `libtensorflowlite_c` из состава `tflite_flutter` | Apache-2.0 |
| [`tflite_flutter`](https://github.com/tensorflow/flutter-tflite) | Обвязка для Dart; лежит в `app/third_party` с правкой в одну строку, чтобы её библиотека для macOS загружалась из `Contents/Frameworks`, как того требует Apple | Apache-2.0 |

</div>

Всё остальное в тракте — эхоподавитель, гейт, экспандер, спектральный
вычитатель, лимитер, АРУ, защита от обратной связи, детектор основного тона и
буфер джиттера — написано для этого проекта и выходит под его же GPL v3.

## Rust

<div class="table-wrap" markdown="1">

| Компонент | Роль | Лицензия |
|---|---|---|
| [`tokio`](https://tokio.rs/), `tokio-rustls` | Асинхронная среда выполнения и транспорт TLS | MIT |
| [`rustls`](https://github.com/rustls/rustls), `rustls-pemfile`, `webpki-roots` | TLS без OpenSSL | Apache-2.0 / MIT / ISC |
| [`rcgen`](https://crates.io/crates/rcgen) | Генерация клиентского сертификата | MIT / Apache-2.0 |
| [`prost`](https://crates.io/crates/prost) | Protocol Buffers для управляющего канала Mumble | Apache-2.0 |
| [`aes`](https://crates.io/crates/aes), `aes-gcm`, `cipher` | OCB/AES для шифрования голоса по UDP | MIT / Apache-2.0 |
| [`sha2`](https://crates.io/crates/sha2), `hex` | Отпечатки сертификатов | MIT / Apache-2.0 |
| [`serde`](https://serde.rs/), `serde_json` | Сериализация настроек и списка серверов | MIT / Apache-2.0 |
| [`tracing`](https://crates.io/crates/tracing) | Структурированное журналирование | MIT |
| [`parking_lot`](https://crates.io/crates/parking_lot) | Блокировки на звуковом тракте | MIT / Apache-2.0 |
| [`rand`](https://crates.io/crates/rand) | Одноразовые значения и джиттер | MIT / Apache-2.0 |
| [`anyhow`](https://crates.io/crates/anyhow), [`thiserror`](https://crates.io/crates/thiserror) | Обработка ошибок | MIT / Apache-2.0 |
| [`bytes`](https://crates.io/crates/bytes), [`url`](https://crates.io/crates/url) | Буферы и разбор URL | MIT / Apache-2.0 |

</div>

## Flutter и Dart

<div class="table-wrap" markdown="1">

| Компонент | Роль | Лицензия |
|---|---|---|
| [Flutter](https://flutter.dev/) и Dart SDK | Каркас приложения | BSD 3-Clause |
| [`flutter_rust_bridge`](https://cjycode.com/flutter_rust_bridge/) | Мост между интерфейсом на Dart и движком на Rust | MIT |
| [`shared_preferences`](https://pub.dev/packages/shared_preferences), [`path_provider`](https://pub.dev/packages/path_provider), [`share_plus`](https://pub.dev/packages/share_plus), [`package_info_plus`](https://pub.dev/packages/package_info_plus), [`file_selector`](https://pub.dev/packages/file_selector) | Платформенная обвязка | BSD 3-Clause |
| [`http`](https://pub.dev/packages/http), [`intl`](https://pub.dev/packages/intl) | Сеть и локализация | BSD 3-Clause |
| [`qr_flutter`](https://pub.dev/packages/qr_flutter), [`qr`](https://pub.dev/packages/qr) | Рисуют приглашения на сервер | BSD 3-Clause |
| [`mobile_scanner`](https://pub.dev/packages/mobile_scanner) | Считывает их обратно | BSD 3-Clause |
| [`image`](https://pub.dev/packages/image), [`archive`](https://pub.dev/packages/archive) | Работа с изображениями и диагностический архив | MIT |
| [`flutter_svg`](https://pub.dev/packages/flutter_svg) | Векторная графика | MIT |
| [`freezed`](https://pub.dev/packages/freezed) | Кодогенерация | MIT |

</div>

## Шрифты

**[Exo 2](https://fonts.google.com/specimen/Exo+2)** Натанаэля Гамы, под
лицензией **SIL Open Font License 1.1**. Используется и в приложении, и на
этом сайте.

Этот сайт также набран шрифтами **[Atkinson
Hyperlegible](https://www.brailleinstitute.org/freefont/)** (Braille Institute,
SIL OFL 1.1) и **[IBM Plex Mono](https://www.ibm.com/plex/)** (IBM,
SIL OFL 1.1).

## TEN VAD — использован в исследовании, но не поставляется

<div class="panel warn">
<p><strong>TEN VAD в приложение не входит.</strong> Он не подключён ни к
одной сборке, и ни в одном выпуске его нет. Лежит он в
<a href="{{ site.repo }}/tree/main/tools/vad"><code>tools/vad/</code></a> — это
отдельный инструментарий, которым на записях из шлема сравнивали детекторы
речи. Упомянут он здесь потому, что использовался при разработке, хотя вместе
с приложением и не поставляется.</p>
</div>

Если решите взять его оттуда, стоит знать две вещи — обе записаны в
[`tools/vad/README.md`]({{ site.repo }}/blob/main/tools/vad/README.md):

- Это **«Apache 2.0 с дополнительными условиями»**, а не просто Apache 2.0.
  Дополнительные условия там действительно есть, так что прочитайте `LICENSE`
  первоисточника, а не исходите из условий Apache.
- Его `pitch_est.cc` содержит код под **BSD-2 и BSD-3** из
  [LPCNet](https://github.com/xiph/LPCNet).

Первоисточник: [TEN-framework/ten-vad](https://github.com/TEN-framework/ten-vad).

В начале того же README сама эта оценка снабжена **опровержением**: измерения,
на которых она построена, были сделаны на звуке с собственного микрофона
телефона, а не с выносного микрофона гарнитуры, и два вывода в результате
отозваны. Текст оставлен на месте и помечен, а не удалён.

## Точность этой страницы

Лицензии перечислены по условиям, опубликованным самими проектами, и мы
считаем их верными, — но эта страница остаётся сводкой, а не юридическим
документом.
**Достоверным является текст, поставляемый с каждым пакетом.** Часть крейтов
Rust распространяется под двойной лицензией — MIT *или* Apache-2.0, на ваш
выбор, — и отмечена выше как «MIT / Apache-2.0».

Нашли ошибку или пропуск? [Заведите issue]({{ site.repo }}/issues): неверно
указанная лицензия — это дефект, и он будет исправлен.

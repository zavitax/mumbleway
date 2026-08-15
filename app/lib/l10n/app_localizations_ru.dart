// ignore: unused_import
import 'package:intl/intl.dart' as intl;
import 'app_localizations.dart';

// ignore_for_file: type=lint

/// The translations for Russian (`ru`).
class LRu extends L {
  LRu([String locale = 'ru']) : super(locale);

  @override
  String get appTitle => 'MumbleWay';

  @override
  String get cancel => 'Отмена';

  @override
  String get save => 'Сохранить';

  @override
  String get add => 'Добавить';

  @override
  String get remove => 'Удалить';

  @override
  String get delete => 'Удалить';

  @override
  String get settings => 'Настройки';

  @override
  String get language => 'Язык';

  @override
  String get deafen => 'Выключить звук';

  @override
  String get undeafen => 'Включить звук';

  @override
  String get muteMicrophone => 'Выключить микрофон';

  @override
  String get unmuteMicrophone => 'Включить микрофон';

  @override
  String get exportServers => 'Экспорт серверов…';

  @override
  String get importFromFile => 'Импорт из файла…';

  @override
  String get noServersTitle => 'Серверов пока нет';

  @override
  String get noServersBody =>
      'Добавьте сервер Mumble, чтобы начать разговор. Можно оставаться подключённым к двум одновременно.';

  @override
  String get addServer => 'Добавить сервер';

  @override
  String get addAnotherServer => 'Добавить ещё сервер';

  @override
  String maxServersNote(int count) {
    return 'Одновременно можно подключить до $count серверов; остальные останутся сохранёнными.';
  }

  @override
  String allSlotsInUse(int count) {
    return 'Уже идёт разговор на $count серверах. Сначала покиньте один из них.';
  }

  @override
  String get micIdleWithTalkButton =>
      'Кнопка разговора и индикатор микрофона появятся здесь после подключения к серверу.';

  @override
  String get micIdleMeterOnly =>
      'Индикатор микрофона появится здесь после подключения к серверу.';

  @override
  String get micIdleWhy =>
      'До этого микрофон остаётся выключенным: запись не ведётся, а гарнитура сохраняет качество звука для других приложений.';

  @override
  String get micUnavailable =>
      'Не удалось включить микрофон. Возможно, его использует другое приложение.';

  @override
  String get notConnectedAny => 'Нет подключения ни к одному серверу';

  @override
  String get talkingOnOne => 'Разговор на 1 сервере';

  @override
  String talkingOnMany(int count) {
    return 'Разговор одновременно на $count серверах';
  }

  @override
  String get audioFailedTitle => 'Не удалось запустить звук';

  @override
  String get audioFailedBody =>
      'MumbleWay нужен микрофон. Проверьте, что он подключён и разрешение выдано, затем перезапустите приложение.';

  @override
  String get statusConnected => 'Подключено';

  @override
  String get statusConnecting => 'Подключение';

  @override
  String get statusAuthenticating => 'Аутентификация';

  @override
  String get statusReconnecting => 'Переподключение';

  @override
  String get statusError => 'Ошибка';

  @override
  String get statusDisconnected => 'Отключено';

  @override
  String get statusNotConnected => 'Не подключено';

  @override
  String get pttHoldToTalk => 'УДЕРЖИВАЙТЕ';

  @override
  String get pttTransmitting => 'ПЕРЕДАЧА';

  @override
  String get pttMicrophoneMuted => 'МИКРОФОН ВЫКЛЮЧЕН';

  @override
  String get pttVoiceActivated => 'ПО ГОЛОСУ';

  @override
  String get pttOpenMic => 'МИКРОФОН ОТКРЫТ';

  @override
  String get probeChecking => 'Проверка…';

  @override
  String get probeNotResponding => 'Не отвечает';

  @override
  String get connect => 'Подключиться';

  @override
  String get disconnect => 'Отключиться';

  @override
  String get joining => 'вход…';

  @override
  String get shareInviteLink => 'Поделиться ссылкой';

  @override
  String get shareProfileFile => 'Поделиться файлом профиля';

  @override
  String get duplicate => 'Дублировать';

  @override
  String get removeServerTitle => 'Удалить сервер?';

  @override
  String removeServerBody(String name) {
    return '$name будет удалён из вашего списка.';
  }

  @override
  String get includePasswordTitle => 'Включить пароль?';

  @override
  String get includePasswordBody =>
      'Любой, кто получит это, сможет подключиться без запроса пароля. Ссылка останется действительной, пока действует пароль, — где бы сообщение ни оказалось.';

  @override
  String get withoutPassword => 'Без пароля';

  @override
  String get includeIt => 'Включить';

  @override
  String get certChangedTitle => 'Сертификат сервера изменился';

  @override
  String get certChangedBody =>
      'Это может означать, что сервер переустановили — или что кто-то выдаёт себя за него. Продолжайте, только если вы этого ожидали.';

  @override
  String get trustNewCertificate => 'Доверять новому сертификату';

  @override
  String reconnectingIn(int seconds, int attempt) {
    return 'Связь потеряна. Повтор через $seconds с (попытка $attempt).';
  }

  @override
  String get connectionLost => 'Связь потеряна.';

  @override
  String retryingInSeconds(int seconds, int attempt) {
    return 'Повтор через $seconds с (попытка $attempt).';
  }

  @override
  String retryingNow(int attempt) {
    return 'Повторяем сейчас (попытка $attempt)…';
  }

  @override
  String switchToLanguage(String name) {
    return 'Нажмите, чтобы переключиться на $name';
  }

  @override
  String get more => 'Ещё';

  @override
  String get edit => 'Изменить';

  @override
  String get editServer => 'Изменить сервер';

  @override
  String get saveChanges => 'Сохранить изменения';

  @override
  String get savingChanges => 'Сохранение…';

  @override
  String get displayName => 'Название';

  @override
  String get displayNameHint => 'Воскресный выезд';

  @override
  String get displayNameMissing => 'Введите название';

  @override
  String get serverAddress => 'Адрес сервера';

  @override
  String get serverAddressHint => 'mumble.example.com';

  @override
  String get serverAddressMissing => 'Введите адрес';

  @override
  String get port => 'Порт';

  @override
  String get portOutOfRange => 'Порт 1-65535';

  @override
  String get username => 'Имя пользователя';

  @override
  String get usernameMissing => 'Введите имя пользователя';

  @override
  String get passwordOptional => 'Пароль (необязательно)';

  @override
  String get passwordHelp => 'Только если сервер его требует';

  @override
  String get addingServer => 'Добавление…';

  @override
  String get quickerWays => 'Быстрые способы добавить сервер';

  @override
  String get browsePublic => 'Публичные серверы';

  @override
  String get importLabel => 'Импорт';

  @override
  String get publicServers => 'Публичные серверы';

  @override
  String get search => 'Поиск';

  @override
  String get reload => 'Обновить';

  @override
  String get addToMyServers => 'Добавить к моим серверам';

  @override
  String get noServersMatchSearch => 'Ничего не найдено по этому запросу.';

  @override
  String get importServers => 'Импорт серверов';

  @override
  String get addFromText => 'Добавить из текста';

  @override
  String get profileFileFormat => 'Формат файла профиля';

  @override
  String get serversAdded => 'Серверы добавлены';

  @override
  String get audioDevices => 'Аудиоустройства';

  @override
  String get levels => 'Уровни';

  @override
  String get network => 'Сеть';

  @override
  String get microphone => 'Микрофон';

  @override
  String get speakers => 'Динамики';

  @override
  String get systemDefault => 'Системное по умолчанию';

  @override
  String get detectedAutomatically => 'Определяется автоматически';

  @override
  String get recheckDevices => 'Обновить список устройств';

  @override
  String get testSpeakers => 'Проверить динамики';

  @override
  String get play => 'Воспроизвести';

  @override
  String get stop => 'Остановить';

  @override
  String get speakerVolume => 'Громкость динамиков';

  @override
  String get inputGain => 'Усиление микрофона';

  @override
  String get hearMyself => 'Слышать себя';

  @override
  String get hearMyselfHelp =>
      'Воспроизводит ваш обработанный голос. Используйте наушники — через динамики возникнет обратная связь.';

  @override
  String get useSystemProxy => 'Использовать системный прокси';

  @override
  String get overrideProxy => 'Задать прокси вручную';

  @override
  String get proxyOverride => 'Свой прокси';

  @override
  String get proxyHostPort => 'хост:порт';

  @override
  String get proxyHostPortHint => '127.0.0.1:8080';

  @override
  String get proxyAutoDetect => 'Оставьте пустым для автоопределения';

  @override
  String get copy => 'Копировать';

  @override
  String get copied => 'Скопировано';

  @override
  String get noiseSuppression => 'Шумоподавление';

  @override
  String get noiseOff => 'Выключено';

  @override
  String get noiseLight => 'Слабое';

  @override
  String get noiseStandard => 'Обычное';

  @override
  String get noiseHelmet => 'Шлем / мотоцикл';

  @override
  String get noiseAuto => 'Автоматически';

  @override
  String get micMode => 'Режим микрофона';

  @override
  String get micPushToTalk => 'По нажатию';

  @override
  String get micVoiceActivated => 'По голосу';

  @override
  String get micContinuous => 'Открытый микрофон';

  @override
  String get buttons => 'Кнопки';

  @override
  String get addBinding => 'Добавить кнопку…';

  @override
  String get removeBinding => 'Удалить привязку';

  @override
  String get action => 'Действие';

  @override
  String get pressAButton => 'Нажмите кнопку, которую хотите использовать';

  @override
  String get waitingForButton => 'Ожидание…';

  @override
  String get buttonActionTalk => 'Удерживать для передачи';

  @override
  String get buttonActionToggleTalk => 'Переключать передачу';

  @override
  String get buttonActionToggleMute => 'Переключать микрофон';

  @override
  String get buttonActionToggleDeafen => 'Переключать звук';

  @override
  String get floatingWindow => 'Показывать плавающее окно вызова';

  @override
  String get identityFingerprint => 'Отпечаток вашего сертификата';

  @override
  String get reverb => 'Эффект помещения';

  @override
  String get reverbBody =>
      'Добавляет короткий хвост к голосам собеседников, чтобы речь, обрезанная активацией по голосу, не обрывалась на полуслове.';

  @override
  String get simpleModel => 'Лёгкая модель шумоподавления';

  @override
  String get simpleModelBody =>
      'Использует уменьшённый очиститель речи — он втрое дешевле по нагрузке. На медленном телефоне это позволяет сохранить остальную цепочку обработки вместо того, чтобы отключать её по частям. Тихую речь обрабатывает чуть грубее и добавляет 20 мс задержки.';

  @override
  String get echoCancellation => 'Подавление эха';

  @override
  String get echoCancellationBody =>
      'Убирает из микрофона то, что играет в динамиках. Оставьте включённым при использовании динамиков; в наушниках эха нет, и подавление может только навредить.';

  @override
  String get noiseCancellation => 'Шумоподавление';

  @override
  String get noiseCancellationBody =>
      'Отфильтровывает из микрофона ветер, двигатель и дорожный шум. Изменения вступят в силу при следующем запуске приложения.';

  @override
  String get micModeBody =>
      'На скорости надёжнее всего режим «по нажатию»: ничто, задетое в дороге, не откроет канал случайно.';

  @override
  String get floatingCallWindow => 'Плавающее окно вызова';

  @override
  String get floatingCallWindowBody =>
      'Держит вызов на виду поверх других приложений, а органы управления — под рукой, без возврата в приложение.';

  @override
  String get buttonsBody =>
      'Привяжите Bluetooth-пульт на руле, кнопку гарнитуры или клавишу. На Android они продолжают работать, пока приложение свёрнуто.';

  @override
  String get networkBody =>
      'Загрузки — каталог публичных серверов и файлы профилей — идут через указанный здесь прокси.';

  @override
  String get identity => 'Идентификация';

  @override
  String get identityBody =>
      'Серверы Mumble узнают вас по сертификату, созданному этим приложением. Передайте этот отпечаток администратору сервера, чтобы зарегистрировать учётную запись.';

  @override
  String get noiseOffBody =>
      'Без подавления, только мягкий фильтр низких частот.';

  @override
  String get noiseLightBody =>
      'Для тихих помещений; сохраняет самое естественное звучание.';

  @override
  String get noiseStandardBody =>
      'Универсальный режим для большинства условий.';

  @override
  String get noiseHelmetBody =>
      'Крутой фильтр шума ветра, полное подавление и жёсткий порог. Рассчитан на микрофон в шлеме на скорости.';

  @override
  String get noiseAutoBody =>
      'Слушает фон и сам выбирает один из вариантов выше. На телефоне при этом работает небольшой классификатор звука: услышав мотор, ветер или музыку, он сразу берёт шлемный режим и держит его ещё пятнадцать секунд после того, как они стихли. Обратно возвращается медленнее: пятнадцать секунд тишины, чтобы уйти со шлемного режима, и ещё минута до самого лёгкого. Пригодится, когда одна поездка охватывает и тихую парковку, и трассу.';

  @override
  String get micAlwaysOn => 'Всегда включён';

  @override
  String get micPushToTalkBody => 'Передавать только при удержании кнопки.';

  @override
  String get micVoiceActivatedBody =>
      'Передавать автоматически, когда вы говорите.';

  @override
  String get micAlwaysOnBody =>
      'Передавать постоянно. Расходует больше всего трафика.';

  @override
  String get platformRoutesAudio =>
      'Эта платформа выбирает аудиоустройство сама — при подключении гарнитуры звук переключится на неё.';

  @override
  String get recheckDevicesBody => 'После подключения или сопряжения гарнитуры';

  @override
  String get testMicrophone => 'Проверить микрофон (слышать себя)';

  @override
  String get testMicrophoneBody =>
      'Воспроизводит ваш обработанный голос ровно так, как его слышат собеседники. Используйте наушники: через динамики возникнет обратная связь.';

  @override
  String get testSpeakersBody =>
      'Воспроизводит короткий сигнал на выбранном устройстве';

  @override
  String get microphoneGain => 'Усиление микрофона';

  @override
  String get levelsHint =>
      'При обычной речи индикатор должен доходить примерно до трёх четвертей.';

  @override
  String get noButtonsBound => 'Кнопки ещё не привязаны.';

  @override
  String boundButton(String name) {
    return 'Привязана $name';
  }

  @override
  String get learn => 'Обучить';

  @override
  String get pressButtonNow => 'Нажмите кнопку на пульте…';

  @override
  String get proxyOffDirect => 'Выключено — прямое подключение';

  @override
  String get proxyDirect => 'Прямое подключение';

  @override
  String proxySystemAt(String proxy) {
    return 'Системный прокси · $proxy';
  }

  @override
  String proxyEnvironmentAt(String proxy) {
    return 'Прокси из окружения · $proxy';
  }

  @override
  String proxyManualAt(String proxy) {
    return 'Прокси, заданный вручную · $proxy';
  }

  @override
  String get certificateFingerprint => 'Отпечаток сертификата';

  @override
  String inThisChannel(int count) {
    return 'В этом канале ($count)';
  }

  @override
  String channelsHeading(int count) {
    return 'Каналы ($count)';
  }

  @override
  String get channelsPlain => 'Каналы';

  @override
  String get noChannelsYet => 'Каналов пока нет.';

  @override
  String get nobodyElseHere => 'В этом канале больше никого нет.';

  @override
  String get joinAutomatically => 'Входить в этот канал автоматически';

  @override
  String get stopJoiningAutomatically =>
      'Не входить в этот канал автоматически';

  @override
  String get muteForMe => 'Заглушить для меня';

  @override
  String get unmuteForMe => 'Включить для меня';

  @override
  String get muteOnServer => 'Заглушить на сервере (для всех)';

  @override
  String get unmuteOnServer => 'Включить на сервере';

  @override
  String get deafenOnServer => 'Отключить звук на сервере';

  @override
  String get undeafenOnServer => 'Включить звук на сервере';

  @override
  String get kickFromServer => 'Отключить от сервера…';

  @override
  String kickTitle(String name) {
    return 'Отключить $name?';
  }

  @override
  String get kickBody =>
      'Пользователь будет отключён от сервера. Это не бан — он сможет сразу подключиться снова.';

  @override
  String get kickReasonLabel => 'Причина (необязательно)';

  @override
  String get kickReasonHint => 'Будет показана при отключении';

  @override
  String get kick => 'Отключить';

  @override
  String get kickSent =>
      'Команда отправлена. Если ничего не произошло, у вас нет права Kick.';

  @override
  String get userStatusTalking => 'говорит';

  @override
  String get userStatusSilent => 'молчит';

  @override
  String get userStatusMuted => 'заглушён';

  @override
  String get userStatusDeafened => 'без звука';

  @override
  String get userStatusMutedForYou => 'заглушён для вас';

  @override
  String get noServerSelected => 'Сервер не выбран';

  @override
  String get noServerSelectedBody =>
      'Добавьте сервер, чтобы увидеть его каналы и участников.';

  @override
  String get connectToSeeChannels =>
      'Подключитесь, чтобы увидеть список каналов и участников.';

  @override
  String get welcomeMessage => 'Приветствие сервера';

  @override
  String get messages => 'Сообщения';

  @override
  String get syncTitle => 'Синхронизация';

  @override
  String get syncServers =>
      'Синхронизировать серверы и настройки между устройствами';

  @override
  String get syncBodyICloud =>
      'Список серверов и настройки передаются через iCloud на все устройства, где выполнен вход в вашу учётную запись Apple. Пароли идут отдельно, через «Связку ключей iCloud», со сквозным шифрованием.';

  @override
  String get syncSignedOut =>
      'Войдите в iCloud на этом устройстве, чтобы включить синхронизацию.';

  @override
  String get syncNow => 'Синхронизировать сейчас';

  @override
  String syncFailed(String error) {
    return 'Последняя синхронизация не удалась: $error';
  }

  @override
  String get transmissionIndicator => 'Индикатор передачи';

  @override
  String get diagnostics => 'Диагностика';

  @override
  String get fingerprintCopied => 'Отпечаток скопирован';

  @override
  String get evenOutLoudness => 'Выровнять громкость собеседников';

  @override
  String get evenOutLoudnessBody =>
      'Приводит всех к сопоставимой громкости. Подстраивается под то, что слышит, поэтому если между фразами нарастает шипение, выключите и проверьте.';

  @override
  String get qrCodeTitle => 'QR-код';

  @override
  String get shareQrCode => 'Поделиться QR-кодом';

  @override
  String get qrCarriesPassword =>
      'В коде содержится пароль. Любой, кто его увидит — через плечо или на фотографии, — сможет подключиться от вашего имени.';

  @override
  String get shareQrImage => 'Отправить код';

  @override
  String get qrCouldNotRender => 'Не удалось построить код.';

  @override
  String joinMeOn(String name) {
    return 'Подключайтесь: $name';
  }

  @override
  String get scanQrCode => 'Сканировать QR-код';

  @override
  String get importQrImage => 'Загрузить изображение с QR-кодом';

  @override
  String get qrNoCodeFound => 'На изображении не найден QR-код.';

  @override
  String get qrNotAnInvite => 'Этот код не является приглашением MumbleWay.';

  @override
  String get qrCameraFailed =>
      'Не удалось запустить камеру. Возможно, её занимает другое приложение или это устройство не поддерживает нужный режим просмотра. Вместо этого можно загрузить изображение с кодом.';

  @override
  String get qrCameraDenied =>
      'Для сканирования кода нужен доступ к камере. Разрешите его в настройках системы и повторите попытку.';

  @override
  String get qrPointAtCode => 'Наведите камеру на код';

  @override
  String get jitterBuffer => 'Буфер входящего звука';

  @override
  String get jitterBufferBody =>
      'Сколько речи собеседников накапливается перед воспроизведением. Больший буфер сглаживает неустойчивую связь и убирает провалы; меньший — вы слышите собеседника раньше. При потерях пакетов MumbleWay наращивает буфер сам и потом возвращается к этому значению. Увеличьте, если счётчик провалов воспроизведения в диагностике продолжает расти.';

  @override
  String milliseconds(int ms) {
    return '$ms мс';
  }

  @override
  String get notAvailableHere => 'Недоступно на этой платформе.';

  @override
  String get pasteLinkOrProfile => 'Вставьте ссылку или профиль';

  @override
  String get downloadProfileFile => 'Загрузить файл профиля';

  @override
  String get downloadAndAdd => 'Загрузить и добавить';

  @override
  String get chooseUsername => 'Выберите имя пользователя';

  @override
  String get chooseUsernameHelp => 'Так вас увидят остальные на сервере';

  @override
  String get directConnection => 'Прямое подключение';

  @override
  String get tunnelledOverTcp =>
      'Туннелируется через TCP, так как UDP заблокирован';

  @override
  String get floatingNotAvailable => 'Плавающие окна здесь недоступны.';

  @override
  String get floatingCouldNotShow => 'Не удалось показать плавающее окно.';

  @override
  String get allowOverlayFirst =>
      'Сначала разрешите «поверх других приложений».';

  @override
  String get microphonePermissionNeeded =>
      'MumbleWay нужен доступ к микрофону. Разрешите его в настройках и откройте приложение снова.';

  @override
  String get noAudioInput =>
      'Устройство сейчас не предоставляет аудиовход. Если подключена гарнитура, попробуйте подключить её заново.';

  @override
  String get serverNoLongerInList => 'Этого сервера больше нет в вашем списке.';

  @override
  String get serversAlreadyAdded => 'Эти серверы уже есть в вашем списке.';

  @override
  String get noServersToExport => 'Нет серверов для экспорта.';

  @override
  String get serverProfilesFileType => 'Профили серверов';

  @override
  String get diagIncomingAudio => 'Входящий звук';

  @override
  String get diagInvented => 'Достроено для заполнения пропусков';

  @override
  String get diagGapsConcealed => 'Скрыто пропусков';

  @override
  String get diagSpeakersTracked => 'Отслеживается говорящих';

  @override
  String get diagMicrophoneDropped => 'Потеряно с микрофона';

  @override
  String get diagInputPeak => 'Пик микрофона';

  @override
  String get diagInputClipped => 'Перегрузка микрофона';

  @override
  String get diagInputTrim => 'Усиление снижено';

  @override
  String get diagGaugeSnr => 'SNR';

  @override
  String get diagGaugePitch => 'Тон';

  @override
  String get diagFloorHeld => 'Порог удерживается';

  @override
  String get diagFloorWatchdog => 'Сбросов удержания';

  @override
  String get diagMicrophoneLevel => 'После шумоподавления';

  @override
  String get diagReconnectAttempts => 'Попыток переподключения';

  @override
  String get diagReset => 'Сбросить';

  @override
  String get diagClose => 'Закрыть';

  @override
  String get diagDecoded => 'Декодировано';

  @override
  String get diagJitterBuffer => 'Буфер джиттера';

  @override
  String get diagThisDevice => 'Это устройство';

  @override
  String get diagLast30Seconds => 'Последние 30 секунд';

  @override
  String get diagPlaybackGaps => 'Пропуски воспроизведения';

  @override
  String get diagNoiseFloor => 'Уровень шума';

  @override
  String get diagOpensAt => 'Порог открытия';

  @override
  String get diagEchoGroup => 'Подавление эха';

  @override
  String get diagEchoRemoved => 'Убрано эха';

  @override
  String get diagEchoOff => 'Выключено';

  @override
  String get diagEchoDelay => 'Эхо найдено на';

  @override
  String get diagEchoNotFound => 'Не найдено';

  @override
  String get diagEchoConfidence => 'Уверенность';

  @override
  String get diagEchoCanceller => 'Эхоподавитель';

  @override
  String get diagEchoCancellerAec3 => 'AEC3';

  @override
  String get diagEchoCancellerFilter => 'Адаптивный фильтр';

  @override
  String get diagEchoShortened => 'Фильтр укорочен, чтобы успевать';

  @override
  String get diagEchoSecondPath => 'Второй путь, вне досягаемости';

  @override
  String get diagPlotFloor => 'шум';

  @override
  String get diagPlotOpensAt => 'порог';

  @override
  String get diagNetwork => 'Сеть';

  @override
  String get diagVoicePackets => 'Голосовые пакеты';

  @override
  String get diagMemory => 'Память';

  @override
  String get diagVoicePath => 'Путь голоса';

  @override
  String get diagUdpDirect => 'UDP напрямую';

  @override
  String get diagTcpTunnelled => 'Туннель TCP';

  @override
  String get diagPing => 'Задержка';

  @override
  String get diagInChannel => 'В канале';

  @override
  String get diagParticipants => 'Участников';

  @override
  String get diagRecording => 'Запись для диагностики';

  @override
  String get diagRecordingBody => 'Сохраняет звук микрофона на это устройство.';

  @override
  String diagRecordingShared(int count, int archives) {
    return 'Отправлено файлов: $count, архивов: $archives.';
  }

  @override
  String get diagAnalyserGivenUp =>
      'Анализатор спектра выключен. Устройство не успевало обрабатывать звук, а его отрисовка обходилась дороже, чем голос мог себе позволить.';

  @override
  String get diagChainReduced =>
      'Устройство не успевало обрабатывать звук, поэтому шумоподавление работает не в полную силу. Голос по-прежнему уходит в эфир, но звучит хуже, чем на более быстром телефоне.';

  @override
  String get diagChainDegradedShort => 'Часть шумоподавления отключена';

  @override
  String get diagProbing => 'Проверяем, что потянет это устройство';

  @override
  String get diagChainDegraded =>
      'Устройство не успевало обрабатывать звук, поэтому часть шумоподавления отключена — она зачёркнута выше. Голос по-прежнему уходит в эфир, но звучит хуже, чем на более быстром телефоне. На более мощном устройстве работала бы вся цепочка.';

  @override
  String get diagEnhancerEffort => 'Очиститель речи';

  @override
  String get diagPerCoreUnavailable =>
      'Загрузка отдельных ядер недоступна на этом устройстве: система не сообщает её приложению.';

  @override
  String get diagEnhancerModel => 'Модель';

  @override
  String get diagEnhancerModelFull => 'Малая задержка';

  @override
  String get diagEnhancerModelSimple => 'Лёгкая';

  @override
  String get diagEnhancerRungFull => 'Полностью';

  @override
  String get diagEnhancerRungReduced => 'Сокращённо';

  @override
  String get diagEnhancerRungLight => 'Облегчённо';

  @override
  String get diagEnhancerRungOff => 'Выключен';

  @override
  String get diagClassifierListening => 'Слушаем фон…';

  @override
  String get diagEnhancerReduced =>
      'Устройство не успевало, и очиститель речи сбавил обороты. Он по-прежнему работает, но самая глубокая фильтрация остаётся только там, где она нужнее всего.';

  @override
  String get diagEnhancerErbOnly =>
      'Устройство не успевало, и очиститель речи работает только в лёгком режиме. Речь по-прежнему проходит, но глубокая фильтрация выключена.';

  @override
  String get diagEnhancerBypassed =>
      'Устройство не успевало даже в самом лёгком режиме, поэтому очиститель речи отключён до конца сеанса.';

  @override
  String get diagPreviewTitle => 'Прослушать';

  @override
  String get diagPreviewBody =>
      'Это запись вашего собственного микрофона. Послушайте, что в ней, прежде чем куда-либо её отправлять.';

  @override
  String get diagPreviewPlay => 'Воспроизвести';

  @override
  String get diagPreviewPause => 'Пауза';

  @override
  String get diagPreviewDelete => 'Удалить эту запись';

  @override
  String get diagPreviewSentOnly => 'Проигрывать только переданное';

  @override
  String get diagPreviewSentOnlyOff => 'Проигрывать запись целиком';

  @override
  String get diagBlockCost => 'На что уходят 10 мс блока';

  @override
  String get diagCostInput => 'Вход и отводы';

  @override
  String get diagCostEnhancer => 'Улучшение речи';

  @override
  String get diagCostEcho => 'Подавление эха';

  @override
  String get diagCostSuppression => 'Шумоподавление';

  @override
  String get diagCostFeedback => 'Защита от самовозбуждения';

  @override
  String get diagCostDehiss => 'Подавление шипения';

  @override
  String get diagCostTransmit => 'Решение о передаче';

  @override
  String get diagCostEncode => 'Кодирование';

  @override
  String get diagBlockUnattributed => 'Вне этапов';

  @override
  String get diagBlockTotal => 'Блок целиком, среднее / худшее';

  @override
  String get diagBlockBacklog => 'Ожидает обработки, среднее / худшее';

  @override
  String get diagPreviewSentOnlyNone => 'Из этой записи ничего не передавалось';

  @override
  String get diagPreviewChain =>
      'Проигрывать через шумоподавление — так, как слышат остальные';

  @override
  String get diagPreviewChainOff => 'Проигрывать микрофон так, как он записан';

  @override
  String get diagPreviewNoneMuted => 'Ничего не ушло: микрофон был выключен.';

  @override
  String get diagPreviewNonePushToTalk =>
      'Ничего не ушло: был выбран режим «по кнопке», а кнопку не нажимали.';

  @override
  String get diagPreviewNoneUnexplained =>
      'Ничего не ушло. Запись всё равно стоит отправить — в журнале видно, почему.';

  @override
  String get diagPreviewShare => 'Отправить эту запись';

  @override
  String get diagPreviewDeleteTitle => 'Удалить эту запись?';

  @override
  String diagPreviewDeleteBody(String name) {
    return '$name и её журнал решений будут удалены с устройства. Поездку заново не записать.';
  }

  @override
  String get diagPreviewDeleteFailed =>
      'Эта запись сейчас занята и не удалена. Повторите через мгновение.';

  @override
  String get diagRecordingListen => 'Прослушать записи';

  @override
  String get diagRecordingDiscardTitle => 'Удалить записи?';

  @override
  String diagRecordingDiscardBody(int count) {
    return 'Файлов будет удалено: $count. Поездку заново не записать.';
  }

  @override
  String get diagRecordingActive => 'Идёт запись';

  @override
  String diagRecordingStopped(int count) {
    return 'Записано файлов: $count';
  }

  @override
  String diagRecordingDropped(int count) {
    return 'Потеряно блоков: $count — накопитель не успевал';
  }

  @override
  String get diagRecordingShare => 'Отправить записи';

  @override
  String get diagRecordingDiscard => 'Удалить записи';

  @override
  String get diagRecordingNone => 'Пока ничего не записано';

  @override
  String diagRecordingSize(String megabytes) {
    return '$megabytes МБ на устройстве';
  }

  @override
  String diagRecordingFailed(String reason) {
    return 'Не удалось начать запись: $reason';
  }

  @override
  String diagRecordingShareFailed(String reason) {
    return 'Не удалось отправить записи: $reason';
  }

  @override
  String get levelsHelp =>
      'Стремитесь к тому, чтобы при обычной речи индикатор доходил примерно до трёх четвертей. Избыточное усиление поднимает шум двигателя вместе с голосом.';

  @override
  String get floatingAndroidBody =>
      'Говорить, отключать микрофон и звук, завершать вызов поверх других приложений. Требуется разрешение «поверх других приложений».';

  @override
  String get floatingIosBody =>
      'Картинка в картинке — появляется, когда вы выходите из приложения. Система даёт три кнопки: воспроизведение/пауза говорит, назад отключает микрофон, вперёд завершает вызов (дважды для подтверждения).';

  @override
  String get actionPushToTalkHold => 'Рация (удерживать)';

  @override
  String get actionPushToTalkToggle => 'Рация (переключатель)';

  @override
  String get actionToggleMute => 'Выключить / включить микрофон';

  @override
  String get actionToggleDeafen => 'Выключить / включить звук';

  @override
  String get buttonsIosNote =>
      'Bluetooth-пульт сообщает о нажатии мультимедийных кнопок, но не об удержании, поэтому рация с удержанием с него не работает. Используйте действие-переключатель. Пока мультимедийная кнопка назначена, пульт управляет MumbleWay, а не музыкальным приложением.';

  @override
  String get remoteListening => 'Ожидание пульта';

  @override
  String get remoteNothingYet => 'кнопок пока не получено';

  @override
  String remoteLastButton(String name) {
    return 'последняя кнопка: $name';
  }

  @override
  String get pipOnAir => 'В ЭФИРЕ';

  @override
  String get pipTalking => 'Говорите';

  @override
  String get pipDeafened => 'Звук выключен';

  @override
  String get pipMuted => 'Микрофон выключен';

  @override
  String get pipListening => 'Слушаем, но\nне передаём';

  @override
  String get pipBadgeMuted => 'МИКРОФОН ВЫКЛ';

  @override
  String get pipBadgeDeafened => 'ЗВУК ВЫКЛ';

  @override
  String get pipNoise => 'шум';

  @override
  String get pipOpen => 'порог';

  @override
  String get pipTalk => 'говорить';

  @override
  String get pipClose => 'Скрыть это окно';

  @override
  String get pipHandsFreeVoice => 'без рук · по голосу';

  @override
  String get pipHandsFreeAlways => 'без рук · всегда включён';

  @override
  String get pipSpeaking => 'ГОВОРЯТ';

  @override
  String get pipNobodySpeaks => 'Никто не говорит';

  @override
  String get pipNotConnected => 'Нет подключения';

  @override
  String get pipNoConnection => 'Связь потеряна';

  @override
  String get pipConnected => 'Подключено';

  @override
  String pipConnectedCount(int count) {
    return 'подключено: $count';
  }

  @override
  String get pipReconnecting => 'Переподключение…';

  @override
  String pipUpAndReconnecting(int up, int count) {
    return '$up на связи · $count в переподключении';
  }

  @override
  String pipMoreSpeakers(int count) {
    return '+$count ещё';
  }

  @override
  String pipOthersOnline(int count) {
    String _temp0 = intl.Intl.pluralLogic(
      count,
      locale: localeName,
      other: 'ещё $count человека в сети',
      many: 'ещё $count человек в сети',
      few: 'ещё $count человека в сети',
      one: 'ещё $count человек в сети',
    );
    return '$_temp0';
  }

  @override
  String get pipNobodyElse => 'Больше здесь никого нет';

  @override
  String get feedbackGuard => 'Подавление гула обратной связи';

  @override
  String get feedbackGuardBody =>
      'На случай, когда микрофон слышит динамик. Эхоподавление убирает то, что может предсказать; эти режимы работают с остатком, и работают они по-разному.';

  @override
  String get feedbackOff => 'Без подавления обратной связи';

  @override
  String get feedbackOffBody =>
      'Только эхоподавление. Начните с этого и меняйте, только если слышите себя в ответ или нарастает вой.';

  @override
  String get feedbackDuck => 'Приглушать микрофон, пока говорят другие';

  @override
  String get feedbackDuckBody =>
      'Классический приём переговорных устройств и самый действенный, когда динамик в шлеме близко к микрофону. Плата за это — перебить собеседника становится труднее.';

  @override
  String get feedbackHowl => 'Обрывать только при нарастании воя';

  @override
  String get feedbackHowlBody =>
      'Совсем не трогает обычный разговор и резко обрывает звук, как только тон начинает нарастать. С лёгкими наводками не борется.';

  @override
  String get feedbackResidual => 'Подавлять остаток после эхоподавления';

  @override
  String get feedbackResidualBody =>
      'Ослабляет тем сильнее, чем больше звук похож на дальнюю сторону, а не на вас. Самый мягкий для живого разговора и самый слабый против настоящего воя.';

  @override
  String get dehiss => 'Подавление шипения';

  @override
  String get dehissBody =>
      'Для ровного шипения, которое микрофон добавляет ко всему. Это не то же самое, что шумоподавление, отвечающее за дорогу и ветер: те громкие и меняются со скоростью, а шипение тихое, высокое и неизменное.';

  @override
  String get dehissOff => 'Без подавления шипения';

  @override
  String get dehissOffBody =>
      'Ничего не менять. Начните отсюда — оба других варианта чем-то жертвуют, и связь, которая и так звучит нормально, менять не стоит.';

  @override
  String get dehissExpander => 'Сильнее приглушать тихие места';

  @override
  String get dehissExpanderBody =>
      'Ослабляет тем сильнее, чем ниже уровня шума оказался звук: речь остаётся нетронутой, а паузы между словами затихают. Голос не станет искусственным, но фон будет «дышать».';

  @override
  String get dehissSpectral => 'Выучить шипение и вычесть его';

  @override
  String get dehissSpectralBody =>
      'Измеряет шум, пока никто не говорит, и убирает его по частотам — шипение уходит и из-под речи, и из пауз. Самый сильный вариант; после него может оставаться лёгкое мерцание.';

  @override
  String get serverBusyChange =>
      'Отключитесь от этого сервера, прежде чем изменять или удалять его.';

  @override
  String get disconnectFirst => 'Сначала отключитесь';

  @override
  String get diagLog => 'Журнал движка';

  @override
  String get diagLogProblems => 'Только проблемы';

  @override
  String get diagLogAll => 'Показать все';

  @override
  String get diagLogCopy => 'Скопировать весь журнал';

  @override
  String get diagLogCopied => 'Журнал скопирован в буфер обмена.';

  @override
  String get diagLogClear => 'Очистить журнал';

  @override
  String get diagLogEmpty => 'Пока ничего не записано.';

  @override
  String get diagLogNoProblems => 'Предупреждений и ошибок нет.';

  @override
  String get diagAutoProfile => 'Авто выбрал';

  @override
  String get diagChosenProfile => 'Профиль';

  @override
  String get diagProfilePinned => '(закреплён)';

  @override
  String get diagStageBackground => 'Фон';

  @override
  String diagClassifierOnCpu(String ms) {
    return 'Ускорителя здесь нет, поэтому распознавание фона считает процессор — $ms мс на проверку, раз в две секунды.';
  }

  @override
  String get diagClassifierUnavailable =>
      'Распознавание фона на этой платформе недоступно, поэтому здесь шлемный профиль выбирается по уровням.';

  @override
  String get diagSpectrum => 'Тракт голоса';

  @override
  String get diagSpectrumWaiting => 'Ожидание звука';

  @override
  String get diagSpectrumStalled => 'Звуковой движок остановлен';

  @override
  String get diagTraceRaw => 'Микрофон';

  @override
  String get diagTracePreGate => 'После подавления';

  @override
  String get diagTraceSentLive => 'Передаётся';

  @override
  String get diagTraceSentIdle => 'Не передаётся';

  @override
  String get diagStageEnhancer => 'Очистка';

  @override
  String get diagStageEcho => 'Эхо';

  @override
  String get diagStageSuppressor => 'Подавление шума';

  @override
  String get diagStageVoice => 'Голос распознан';

  @override
  String get diagStageGate => 'Шумовые ворота';

  @override
  String get diagStageLevel => 'Выравнивание';

  @override
  String get diagStageHiss => 'Шипение';

  @override
  String get diagStageFeedback => 'Обратная связь';

  @override
  String get diagStageTransmit => 'На сервер';

  @override
  String get website => 'Сайт';

  @override
  String get openWebsite => 'Открыть сайт MumbleWay';

  @override
  String get helpForThisScreen => 'Справка по этому экрану';

  @override
  String get couldNotOpenLink =>
      'Не удалось открыть ссылку: ни один браузер не откликнулся.';

  @override
  String serverRefused(String reason) {
    return 'Сервер отказал: $reason';
  }

  @override
  String get denyText => 'Сервер не стал передавать это сообщение.';

  @override
  String get denyPermission => 'Сервер отказал: нет прав на это действие.';

  @override
  String get denySuperUser => 'Эту учётную запись нельзя изменить из клиента.';

  @override
  String get denyChannelName => 'Сервер не принял такое название канала.';

  @override
  String get denyTextTooLong => 'Сообщение длиннее, чем разрешает сервер.';

  @override
  String get denyTemporaryChannel => 'В временном канале это невозможно.';

  @override
  String get denyMissingCertificate => 'Для этого серверу нужен сертификат.';

  @override
  String get denyUserName => 'Сервер не принял такое имя.';

  @override
  String get denyChannelFull => 'Канал заполнен.';

  @override
  String get denyNestingLimit =>
      'Глубже вкладывать каналы на этом сервере нельзя.';

  @override
  String get denyChannelCountLimit =>
      'На сервере уже столько каналов, сколько он допускает.';

  @override
  String get denyListenerLimit => 'Сервер достиг предела по числу слушателей.';
}

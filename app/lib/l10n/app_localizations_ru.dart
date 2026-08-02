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
  String get floatingWindow => 'Плавающее окно вызова';

  @override
  String get identityFingerprint => 'Отпечаток вашего сертификата';

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
  String get floatingTalkButton => 'Плавающая кнопка передачи';

  @override
  String get floatingTalkButtonBody =>
      'Небольшая перетаскиваемая кнопка передачи поверх остальных приложений.';

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
}

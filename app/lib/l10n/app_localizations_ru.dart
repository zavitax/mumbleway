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

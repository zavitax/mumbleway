// GENERATED CODE - DO NOT MODIFY BY HAND
// coverage:ignore-file
// ignore_for_file: type=lint
// ignore_for_file: unused_element, deprecated_member_use, deprecated_member_use_from_same_package, use_function_type_syntax_for_parameters, unnecessary_const, avoid_init_to_null, invalid_override_different_default_values_named, prefer_expression_function_bodies, annotate_overrides, invalid_annotation_target, unnecessary_question_mark

part of 'mumbleway.dart';

// **************************************************************************
// FreezedGenerator
// **************************************************************************

// dart format off
T _$identity<T>(T value) => value;
/// @nodoc
mixin _$AppEvent {





@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is AppEvent);
}


@override
int get hashCode => runtimeType.hashCode;

@override
String toString() {
  return 'AppEvent()';
}


}

/// @nodoc
class $AppEventCopyWith<$Res>  {
$AppEventCopyWith(AppEvent _, $Res Function(AppEvent) __);
}


/// Adds pattern-matching-related methods to [AppEvent].
extension AppEventPatterns on AppEvent {
/// A variant of `map` that fallback to returning `orElse`.
///
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case final Subclass value:
///     return ...;
///   case _:
///     return orElse();
/// }
/// ```

@optionalTypeArgs TResult maybeMap<TResult extends Object?>({TResult Function( AppEvent_Status value)?  status,TResult Function( AppEvent_Users value)?  users,TResult Function( AppEvent_Channels value)?  channels,TResult Function( AppEvent_Text value)?  text,TResult Function( AppEvent_Stats value)?  stats,TResult Function( AppEvent_InputLevel value)?  inputLevel,TResult Function( AppEvent_SpeakerLevels value)?  speakerLevels,TResult Function( AppEvent_Suppressed value)?  suppressed,TResult Function( AppEvent_Listening value)?  listening,TResult Function( AppEvent_Limits value)?  limits,TResult Function( AppEvent_Bandwidth value)?  bandwidth,TResult Function( AppEvent_ContextActions value)?  contextActions,TResult Function( AppEvent_Acl value)?  acl,TResult Function( AppEvent_UserNames value)?  userNames,TResult Function( AppEvent_Registered value)?  registered,TResult Function( AppEvent_UserDetails value)?  userDetails,TResult Function( AppEvent_Bans value)?  bans,TResult Function( AppEvent_ServerSuggests value)?  serverSuggests,TResult Function( AppEvent_Avatar value)?  avatar,TResult Function( AppEvent_Rights value)?  rights,TResult Function( AppEvent_ChannelRights value)?  channelRights,TResult Function( AppEvent_Moderated value)?  moderated,TResult Function( AppEvent_RemoteMuted value)?  remoteMuted,TResult Function( AppEvent_Certificate value)?  certificate,TResult Function( AppEvent_Refused value)?  refused,TResult Function( AppEvent_Welcome value)?  welcome,TResult Function( AppEvent_SelfSession value)?  selfSession,TResult Function( AppEvent_Log value)?  log,required TResult orElse(),}){
final _that = this;
switch (_that) {
case AppEvent_Status() when status != null:
return status(_that);case AppEvent_Users() when users != null:
return users(_that);case AppEvent_Channels() when channels != null:
return channels(_that);case AppEvent_Text() when text != null:
return text(_that);case AppEvent_Stats() when stats != null:
return stats(_that);case AppEvent_InputLevel() when inputLevel != null:
return inputLevel(_that);case AppEvent_SpeakerLevels() when speakerLevels != null:
return speakerLevels(_that);case AppEvent_Suppressed() when suppressed != null:
return suppressed(_that);case AppEvent_Listening() when listening != null:
return listening(_that);case AppEvent_Limits() when limits != null:
return limits(_that);case AppEvent_Bandwidth() when bandwidth != null:
return bandwidth(_that);case AppEvent_ContextActions() when contextActions != null:
return contextActions(_that);case AppEvent_Acl() when acl != null:
return acl(_that);case AppEvent_UserNames() when userNames != null:
return userNames(_that);case AppEvent_Registered() when registered != null:
return registered(_that);case AppEvent_UserDetails() when userDetails != null:
return userDetails(_that);case AppEvent_Bans() when bans != null:
return bans(_that);case AppEvent_ServerSuggests() when serverSuggests != null:
return serverSuggests(_that);case AppEvent_Avatar() when avatar != null:
return avatar(_that);case AppEvent_Rights() when rights != null:
return rights(_that);case AppEvent_ChannelRights() when channelRights != null:
return channelRights(_that);case AppEvent_Moderated() when moderated != null:
return moderated(_that);case AppEvent_RemoteMuted() when remoteMuted != null:
return remoteMuted(_that);case AppEvent_Certificate() when certificate != null:
return certificate(_that);case AppEvent_Refused() when refused != null:
return refused(_that);case AppEvent_Welcome() when welcome != null:
return welcome(_that);case AppEvent_SelfSession() when selfSession != null:
return selfSession(_that);case AppEvent_Log() when log != null:
return log(_that);case _:
  return orElse();

}
}
/// A `switch`-like method, using callbacks.
///
/// Callbacks receives the raw object, upcasted.
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case final Subclass value:
///     return ...;
///   case final Subclass2 value:
///     return ...;
/// }
/// ```

@optionalTypeArgs TResult map<TResult extends Object?>({required TResult Function( AppEvent_Status value)  status,required TResult Function( AppEvent_Users value)  users,required TResult Function( AppEvent_Channels value)  channels,required TResult Function( AppEvent_Text value)  text,required TResult Function( AppEvent_Stats value)  stats,required TResult Function( AppEvent_InputLevel value)  inputLevel,required TResult Function( AppEvent_SpeakerLevels value)  speakerLevels,required TResult Function( AppEvent_Suppressed value)  suppressed,required TResult Function( AppEvent_Listening value)  listening,required TResult Function( AppEvent_Limits value)  limits,required TResult Function( AppEvent_Bandwidth value)  bandwidth,required TResult Function( AppEvent_ContextActions value)  contextActions,required TResult Function( AppEvent_Acl value)  acl,required TResult Function( AppEvent_UserNames value)  userNames,required TResult Function( AppEvent_Registered value)  registered,required TResult Function( AppEvent_UserDetails value)  userDetails,required TResult Function( AppEvent_Bans value)  bans,required TResult Function( AppEvent_ServerSuggests value)  serverSuggests,required TResult Function( AppEvent_Avatar value)  avatar,required TResult Function( AppEvent_Rights value)  rights,required TResult Function( AppEvent_ChannelRights value)  channelRights,required TResult Function( AppEvent_Moderated value)  moderated,required TResult Function( AppEvent_RemoteMuted value)  remoteMuted,required TResult Function( AppEvent_Certificate value)  certificate,required TResult Function( AppEvent_Refused value)  refused,required TResult Function( AppEvent_Welcome value)  welcome,required TResult Function( AppEvent_SelfSession value)  selfSession,required TResult Function( AppEvent_Log value)  log,}){
final _that = this;
switch (_that) {
case AppEvent_Status():
return status(_that);case AppEvent_Users():
return users(_that);case AppEvent_Channels():
return channels(_that);case AppEvent_Text():
return text(_that);case AppEvent_Stats():
return stats(_that);case AppEvent_InputLevel():
return inputLevel(_that);case AppEvent_SpeakerLevels():
return speakerLevels(_that);case AppEvent_Suppressed():
return suppressed(_that);case AppEvent_Listening():
return listening(_that);case AppEvent_Limits():
return limits(_that);case AppEvent_Bandwidth():
return bandwidth(_that);case AppEvent_ContextActions():
return contextActions(_that);case AppEvent_Acl():
return acl(_that);case AppEvent_UserNames():
return userNames(_that);case AppEvent_Registered():
return registered(_that);case AppEvent_UserDetails():
return userDetails(_that);case AppEvent_Bans():
return bans(_that);case AppEvent_ServerSuggests():
return serverSuggests(_that);case AppEvent_Avatar():
return avatar(_that);case AppEvent_Rights():
return rights(_that);case AppEvent_ChannelRights():
return channelRights(_that);case AppEvent_Moderated():
return moderated(_that);case AppEvent_RemoteMuted():
return remoteMuted(_that);case AppEvent_Certificate():
return certificate(_that);case AppEvent_Refused():
return refused(_that);case AppEvent_Welcome():
return welcome(_that);case AppEvent_SelfSession():
return selfSession(_that);case AppEvent_Log():
return log(_that);}
}
/// A variant of `map` that fallback to returning `null`.
///
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case final Subclass value:
///     return ...;
///   case _:
///     return null;
/// }
/// ```

@optionalTypeArgs TResult? mapOrNull<TResult extends Object?>({TResult? Function( AppEvent_Status value)?  status,TResult? Function( AppEvent_Users value)?  users,TResult? Function( AppEvent_Channels value)?  channels,TResult? Function( AppEvent_Text value)?  text,TResult? Function( AppEvent_Stats value)?  stats,TResult? Function( AppEvent_InputLevel value)?  inputLevel,TResult? Function( AppEvent_SpeakerLevels value)?  speakerLevels,TResult? Function( AppEvent_Suppressed value)?  suppressed,TResult? Function( AppEvent_Listening value)?  listening,TResult? Function( AppEvent_Limits value)?  limits,TResult? Function( AppEvent_Bandwidth value)?  bandwidth,TResult? Function( AppEvent_ContextActions value)?  contextActions,TResult? Function( AppEvent_Acl value)?  acl,TResult? Function( AppEvent_UserNames value)?  userNames,TResult? Function( AppEvent_Registered value)?  registered,TResult? Function( AppEvent_UserDetails value)?  userDetails,TResult? Function( AppEvent_Bans value)?  bans,TResult? Function( AppEvent_ServerSuggests value)?  serverSuggests,TResult? Function( AppEvent_Avatar value)?  avatar,TResult? Function( AppEvent_Rights value)?  rights,TResult? Function( AppEvent_ChannelRights value)?  channelRights,TResult? Function( AppEvent_Moderated value)?  moderated,TResult? Function( AppEvent_RemoteMuted value)?  remoteMuted,TResult? Function( AppEvent_Certificate value)?  certificate,TResult? Function( AppEvent_Refused value)?  refused,TResult? Function( AppEvent_Welcome value)?  welcome,TResult? Function( AppEvent_SelfSession value)?  selfSession,TResult? Function( AppEvent_Log value)?  log,}){
final _that = this;
switch (_that) {
case AppEvent_Status() when status != null:
return status(_that);case AppEvent_Users() when users != null:
return users(_that);case AppEvent_Channels() when channels != null:
return channels(_that);case AppEvent_Text() when text != null:
return text(_that);case AppEvent_Stats() when stats != null:
return stats(_that);case AppEvent_InputLevel() when inputLevel != null:
return inputLevel(_that);case AppEvent_SpeakerLevels() when speakerLevels != null:
return speakerLevels(_that);case AppEvent_Suppressed() when suppressed != null:
return suppressed(_that);case AppEvent_Listening() when listening != null:
return listening(_that);case AppEvent_Limits() when limits != null:
return limits(_that);case AppEvent_Bandwidth() when bandwidth != null:
return bandwidth(_that);case AppEvent_ContextActions() when contextActions != null:
return contextActions(_that);case AppEvent_Acl() when acl != null:
return acl(_that);case AppEvent_UserNames() when userNames != null:
return userNames(_that);case AppEvent_Registered() when registered != null:
return registered(_that);case AppEvent_UserDetails() when userDetails != null:
return userDetails(_that);case AppEvent_Bans() when bans != null:
return bans(_that);case AppEvent_ServerSuggests() when serverSuggests != null:
return serverSuggests(_that);case AppEvent_Avatar() when avatar != null:
return avatar(_that);case AppEvent_Rights() when rights != null:
return rights(_that);case AppEvent_ChannelRights() when channelRights != null:
return channelRights(_that);case AppEvent_Moderated() when moderated != null:
return moderated(_that);case AppEvent_RemoteMuted() when remoteMuted != null:
return remoteMuted(_that);case AppEvent_Certificate() when certificate != null:
return certificate(_that);case AppEvent_Refused() when refused != null:
return refused(_that);case AppEvent_Welcome() when welcome != null:
return welcome(_that);case AppEvent_SelfSession() when selfSession != null:
return selfSession(_that);case AppEvent_Log() when log != null:
return log(_that);case _:
  return null;

}
}
/// A variant of `when` that fallback to an `orElse` callback.
///
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case Subclass(:final field):
///     return ...;
///   case _:
///     return orElse();
/// }
/// ```

@optionalTypeArgs TResult maybeWhen<TResult extends Object?>({TResult Function( StatusUpdate field0)?  status,TResult Function( String serverId,  List<UiUser> users)?  users,TResult Function( String serverId,  List<UiChannel> channels)?  channels,TResult Function( String serverId,  String from,  String message)?  text,TResult Function( UiStats field0)?  stats,TResult Function( double levelDb,  bool speaking,  double thresholdDb,  double noiseFloorDb)?  inputLevel,TResult Function( List<UiSpeakerLevel> levels)?  speakerLevels,TResult Function( String serverId,  bool suppressed)?  suppressed,TResult Function( String serverId,  Uint32List channels)?  listening,TResult Function( String serverId,  int messageLength,  int imageMessageLength)?  limits,TResult Function( String serverId,  int capBps,  int bitrateBps,  bool capped,  bool belowFloor)?  bandwidth,TResult Function( String serverId,  List<UiContextAction> actions)?  contextActions,TResult Function( String serverId,  UiChannelAcl acl)?  acl,TResult Function( String serverId,  List<UiUserName> names)?  userNames,TResult Function( String serverId,  List<UiRegisteredUser> users)?  registered,TResult Function( String serverId,  UiUserDetails details)?  userDetails,TResult Function( String serverId,  List<UiBan> bans)?  bans,TResult Function( String serverId,  bool? pushToTalk,  bool? positional)?  serverSuggests,TResult Function( String serverId,  int session,  Uint8List image)?  avatar,TResult Function( String serverId,  UiRights rights)?  rights,TResult Function( String serverId,  List<UiChannelRights> channels)?  channelRights,TResult Function( String serverId,  bool? muted,  bool? deafened,  String by)?  moderated,TResult Function( String serverId,  bool muted,  String by)?  remoteMuted,TResult Function( String serverId,  String fingerprint,  bool changed)?  certificate,TResult Function( String serverId,  String reason,  int kind)?  refused,TResult Function( String serverId,  String text)?  welcome,TResult Function( String serverId,  int session)?  selfSession,TResult Function( List<UiLogEntry> entries)?  log,required TResult orElse(),}) {final _that = this;
switch (_that) {
case AppEvent_Status() when status != null:
return status(_that.field0);case AppEvent_Users() when users != null:
return users(_that.serverId,_that.users);case AppEvent_Channels() when channels != null:
return channels(_that.serverId,_that.channels);case AppEvent_Text() when text != null:
return text(_that.serverId,_that.from,_that.message);case AppEvent_Stats() when stats != null:
return stats(_that.field0);case AppEvent_InputLevel() when inputLevel != null:
return inputLevel(_that.levelDb,_that.speaking,_that.thresholdDb,_that.noiseFloorDb);case AppEvent_SpeakerLevels() when speakerLevels != null:
return speakerLevels(_that.levels);case AppEvent_Suppressed() when suppressed != null:
return suppressed(_that.serverId,_that.suppressed);case AppEvent_Listening() when listening != null:
return listening(_that.serverId,_that.channels);case AppEvent_Limits() when limits != null:
return limits(_that.serverId,_that.messageLength,_that.imageMessageLength);case AppEvent_Bandwidth() when bandwidth != null:
return bandwidth(_that.serverId,_that.capBps,_that.bitrateBps,_that.capped,_that.belowFloor);case AppEvent_ContextActions() when contextActions != null:
return contextActions(_that.serverId,_that.actions);case AppEvent_Acl() when acl != null:
return acl(_that.serverId,_that.acl);case AppEvent_UserNames() when userNames != null:
return userNames(_that.serverId,_that.names);case AppEvent_Registered() when registered != null:
return registered(_that.serverId,_that.users);case AppEvent_UserDetails() when userDetails != null:
return userDetails(_that.serverId,_that.details);case AppEvent_Bans() when bans != null:
return bans(_that.serverId,_that.bans);case AppEvent_ServerSuggests() when serverSuggests != null:
return serverSuggests(_that.serverId,_that.pushToTalk,_that.positional);case AppEvent_Avatar() when avatar != null:
return avatar(_that.serverId,_that.session,_that.image);case AppEvent_Rights() when rights != null:
return rights(_that.serverId,_that.rights);case AppEvent_ChannelRights() when channelRights != null:
return channelRights(_that.serverId,_that.channels);case AppEvent_Moderated() when moderated != null:
return moderated(_that.serverId,_that.muted,_that.deafened,_that.by);case AppEvent_RemoteMuted() when remoteMuted != null:
return remoteMuted(_that.serverId,_that.muted,_that.by);case AppEvent_Certificate() when certificate != null:
return certificate(_that.serverId,_that.fingerprint,_that.changed);case AppEvent_Refused() when refused != null:
return refused(_that.serverId,_that.reason,_that.kind);case AppEvent_Welcome() when welcome != null:
return welcome(_that.serverId,_that.text);case AppEvent_SelfSession() when selfSession != null:
return selfSession(_that.serverId,_that.session);case AppEvent_Log() when log != null:
return log(_that.entries);case _:
  return orElse();

}
}
/// A `switch`-like method, using callbacks.
///
/// As opposed to `map`, this offers destructuring.
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case Subclass(:final field):
///     return ...;
///   case Subclass2(:final field2):
///     return ...;
/// }
/// ```

@optionalTypeArgs TResult when<TResult extends Object?>({required TResult Function( StatusUpdate field0)  status,required TResult Function( String serverId,  List<UiUser> users)  users,required TResult Function( String serverId,  List<UiChannel> channels)  channels,required TResult Function( String serverId,  String from,  String message)  text,required TResult Function( UiStats field0)  stats,required TResult Function( double levelDb,  bool speaking,  double thresholdDb,  double noiseFloorDb)  inputLevel,required TResult Function( List<UiSpeakerLevel> levels)  speakerLevels,required TResult Function( String serverId,  bool suppressed)  suppressed,required TResult Function( String serverId,  Uint32List channels)  listening,required TResult Function( String serverId,  int messageLength,  int imageMessageLength)  limits,required TResult Function( String serverId,  int capBps,  int bitrateBps,  bool capped,  bool belowFloor)  bandwidth,required TResult Function( String serverId,  List<UiContextAction> actions)  contextActions,required TResult Function( String serverId,  UiChannelAcl acl)  acl,required TResult Function( String serverId,  List<UiUserName> names)  userNames,required TResult Function( String serverId,  List<UiRegisteredUser> users)  registered,required TResult Function( String serverId,  UiUserDetails details)  userDetails,required TResult Function( String serverId,  List<UiBan> bans)  bans,required TResult Function( String serverId,  bool? pushToTalk,  bool? positional)  serverSuggests,required TResult Function( String serverId,  int session,  Uint8List image)  avatar,required TResult Function( String serverId,  UiRights rights)  rights,required TResult Function( String serverId,  List<UiChannelRights> channels)  channelRights,required TResult Function( String serverId,  bool? muted,  bool? deafened,  String by)  moderated,required TResult Function( String serverId,  bool muted,  String by)  remoteMuted,required TResult Function( String serverId,  String fingerprint,  bool changed)  certificate,required TResult Function( String serverId,  String reason,  int kind)  refused,required TResult Function( String serverId,  String text)  welcome,required TResult Function( String serverId,  int session)  selfSession,required TResult Function( List<UiLogEntry> entries)  log,}) {final _that = this;
switch (_that) {
case AppEvent_Status():
return status(_that.field0);case AppEvent_Users():
return users(_that.serverId,_that.users);case AppEvent_Channels():
return channels(_that.serverId,_that.channels);case AppEvent_Text():
return text(_that.serverId,_that.from,_that.message);case AppEvent_Stats():
return stats(_that.field0);case AppEvent_InputLevel():
return inputLevel(_that.levelDb,_that.speaking,_that.thresholdDb,_that.noiseFloorDb);case AppEvent_SpeakerLevels():
return speakerLevels(_that.levels);case AppEvent_Suppressed():
return suppressed(_that.serverId,_that.suppressed);case AppEvent_Listening():
return listening(_that.serverId,_that.channels);case AppEvent_Limits():
return limits(_that.serverId,_that.messageLength,_that.imageMessageLength);case AppEvent_Bandwidth():
return bandwidth(_that.serverId,_that.capBps,_that.bitrateBps,_that.capped,_that.belowFloor);case AppEvent_ContextActions():
return contextActions(_that.serverId,_that.actions);case AppEvent_Acl():
return acl(_that.serverId,_that.acl);case AppEvent_UserNames():
return userNames(_that.serverId,_that.names);case AppEvent_Registered():
return registered(_that.serverId,_that.users);case AppEvent_UserDetails():
return userDetails(_that.serverId,_that.details);case AppEvent_Bans():
return bans(_that.serverId,_that.bans);case AppEvent_ServerSuggests():
return serverSuggests(_that.serverId,_that.pushToTalk,_that.positional);case AppEvent_Avatar():
return avatar(_that.serverId,_that.session,_that.image);case AppEvent_Rights():
return rights(_that.serverId,_that.rights);case AppEvent_ChannelRights():
return channelRights(_that.serverId,_that.channels);case AppEvent_Moderated():
return moderated(_that.serverId,_that.muted,_that.deafened,_that.by);case AppEvent_RemoteMuted():
return remoteMuted(_that.serverId,_that.muted,_that.by);case AppEvent_Certificate():
return certificate(_that.serverId,_that.fingerprint,_that.changed);case AppEvent_Refused():
return refused(_that.serverId,_that.reason,_that.kind);case AppEvent_Welcome():
return welcome(_that.serverId,_that.text);case AppEvent_SelfSession():
return selfSession(_that.serverId,_that.session);case AppEvent_Log():
return log(_that.entries);}
}
/// A variant of `when` that fallback to returning `null`
///
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case Subclass(:final field):
///     return ...;
///   case _:
///     return null;
/// }
/// ```

@optionalTypeArgs TResult? whenOrNull<TResult extends Object?>({TResult? Function( StatusUpdate field0)?  status,TResult? Function( String serverId,  List<UiUser> users)?  users,TResult? Function( String serverId,  List<UiChannel> channels)?  channels,TResult? Function( String serverId,  String from,  String message)?  text,TResult? Function( UiStats field0)?  stats,TResult? Function( double levelDb,  bool speaking,  double thresholdDb,  double noiseFloorDb)?  inputLevel,TResult? Function( List<UiSpeakerLevel> levels)?  speakerLevels,TResult? Function( String serverId,  bool suppressed)?  suppressed,TResult? Function( String serverId,  Uint32List channels)?  listening,TResult? Function( String serverId,  int messageLength,  int imageMessageLength)?  limits,TResult? Function( String serverId,  int capBps,  int bitrateBps,  bool capped,  bool belowFloor)?  bandwidth,TResult? Function( String serverId,  List<UiContextAction> actions)?  contextActions,TResult? Function( String serverId,  UiChannelAcl acl)?  acl,TResult? Function( String serverId,  List<UiUserName> names)?  userNames,TResult? Function( String serverId,  List<UiRegisteredUser> users)?  registered,TResult? Function( String serverId,  UiUserDetails details)?  userDetails,TResult? Function( String serverId,  List<UiBan> bans)?  bans,TResult? Function( String serverId,  bool? pushToTalk,  bool? positional)?  serverSuggests,TResult? Function( String serverId,  int session,  Uint8List image)?  avatar,TResult? Function( String serverId,  UiRights rights)?  rights,TResult? Function( String serverId,  List<UiChannelRights> channels)?  channelRights,TResult? Function( String serverId,  bool? muted,  bool? deafened,  String by)?  moderated,TResult? Function( String serverId,  bool muted,  String by)?  remoteMuted,TResult? Function( String serverId,  String fingerprint,  bool changed)?  certificate,TResult? Function( String serverId,  String reason,  int kind)?  refused,TResult? Function( String serverId,  String text)?  welcome,TResult? Function( String serverId,  int session)?  selfSession,TResult? Function( List<UiLogEntry> entries)?  log,}) {final _that = this;
switch (_that) {
case AppEvent_Status() when status != null:
return status(_that.field0);case AppEvent_Users() when users != null:
return users(_that.serverId,_that.users);case AppEvent_Channels() when channels != null:
return channels(_that.serverId,_that.channels);case AppEvent_Text() when text != null:
return text(_that.serverId,_that.from,_that.message);case AppEvent_Stats() when stats != null:
return stats(_that.field0);case AppEvent_InputLevel() when inputLevel != null:
return inputLevel(_that.levelDb,_that.speaking,_that.thresholdDb,_that.noiseFloorDb);case AppEvent_SpeakerLevels() when speakerLevels != null:
return speakerLevels(_that.levels);case AppEvent_Suppressed() when suppressed != null:
return suppressed(_that.serverId,_that.suppressed);case AppEvent_Listening() when listening != null:
return listening(_that.serverId,_that.channels);case AppEvent_Limits() when limits != null:
return limits(_that.serverId,_that.messageLength,_that.imageMessageLength);case AppEvent_Bandwidth() when bandwidth != null:
return bandwidth(_that.serverId,_that.capBps,_that.bitrateBps,_that.capped,_that.belowFloor);case AppEvent_ContextActions() when contextActions != null:
return contextActions(_that.serverId,_that.actions);case AppEvent_Acl() when acl != null:
return acl(_that.serverId,_that.acl);case AppEvent_UserNames() when userNames != null:
return userNames(_that.serverId,_that.names);case AppEvent_Registered() when registered != null:
return registered(_that.serverId,_that.users);case AppEvent_UserDetails() when userDetails != null:
return userDetails(_that.serverId,_that.details);case AppEvent_Bans() when bans != null:
return bans(_that.serverId,_that.bans);case AppEvent_ServerSuggests() when serverSuggests != null:
return serverSuggests(_that.serverId,_that.pushToTalk,_that.positional);case AppEvent_Avatar() when avatar != null:
return avatar(_that.serverId,_that.session,_that.image);case AppEvent_Rights() when rights != null:
return rights(_that.serverId,_that.rights);case AppEvent_ChannelRights() when channelRights != null:
return channelRights(_that.serverId,_that.channels);case AppEvent_Moderated() when moderated != null:
return moderated(_that.serverId,_that.muted,_that.deafened,_that.by);case AppEvent_RemoteMuted() when remoteMuted != null:
return remoteMuted(_that.serverId,_that.muted,_that.by);case AppEvent_Certificate() when certificate != null:
return certificate(_that.serverId,_that.fingerprint,_that.changed);case AppEvent_Refused() when refused != null:
return refused(_that.serverId,_that.reason,_that.kind);case AppEvent_Welcome() when welcome != null:
return welcome(_that.serverId,_that.text);case AppEvent_SelfSession() when selfSession != null:
return selfSession(_that.serverId,_that.session);case AppEvent_Log() when log != null:
return log(_that.entries);case _:
  return null;

}
}

}

/// @nodoc


class AppEvent_Status extends AppEvent {
  const AppEvent_Status(this.field0): super._();
  

 final  StatusUpdate field0;

/// Create a copy of AppEvent
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$AppEvent_StatusCopyWith<AppEvent_Status> get copyWith => _$AppEvent_StatusCopyWithImpl<AppEvent_Status>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is AppEvent_Status&&(identical(other.field0, field0) || other.field0 == field0));
}


@override
int get hashCode => Object.hash(runtimeType,field0);

@override
String toString() {
  return 'AppEvent.status(field0: $field0)';
}


}

/// @nodoc
abstract mixin class $AppEvent_StatusCopyWith<$Res> implements $AppEventCopyWith<$Res> {
  factory $AppEvent_StatusCopyWith(AppEvent_Status value, $Res Function(AppEvent_Status) _then) = _$AppEvent_StatusCopyWithImpl;
@useResult
$Res call({
 StatusUpdate field0
});




}
/// @nodoc
class _$AppEvent_StatusCopyWithImpl<$Res>
    implements $AppEvent_StatusCopyWith<$Res> {
  _$AppEvent_StatusCopyWithImpl(this._self, this._then);

  final AppEvent_Status _self;
  final $Res Function(AppEvent_Status) _then;

/// Create a copy of AppEvent
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? field0 = null,}) {
  return _then(AppEvent_Status(
null == field0 ? _self.field0 : field0 // ignore: cast_nullable_to_non_nullable
as StatusUpdate,
  ));
}


}

/// @nodoc


class AppEvent_Users extends AppEvent {
  const AppEvent_Users({required this.serverId, required final  List<UiUser> users}): _users = users,super._();
  

 final  String serverId;
 final  List<UiUser> _users;
 List<UiUser> get users {
  if (_users is EqualUnmodifiableListView) return _users;
  // ignore: implicit_dynamic_type
  return EqualUnmodifiableListView(_users);
}


/// Create a copy of AppEvent
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$AppEvent_UsersCopyWith<AppEvent_Users> get copyWith => _$AppEvent_UsersCopyWithImpl<AppEvent_Users>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is AppEvent_Users&&(identical(other.serverId, serverId) || other.serverId == serverId)&&const DeepCollectionEquality().equals(other._users, _users));
}


@override
int get hashCode => Object.hash(runtimeType,serverId,const DeepCollectionEquality().hash(_users));

@override
String toString() {
  return 'AppEvent.users(serverId: $serverId, users: $users)';
}


}

/// @nodoc
abstract mixin class $AppEvent_UsersCopyWith<$Res> implements $AppEventCopyWith<$Res> {
  factory $AppEvent_UsersCopyWith(AppEvent_Users value, $Res Function(AppEvent_Users) _then) = _$AppEvent_UsersCopyWithImpl;
@useResult
$Res call({
 String serverId, List<UiUser> users
});




}
/// @nodoc
class _$AppEvent_UsersCopyWithImpl<$Res>
    implements $AppEvent_UsersCopyWith<$Res> {
  _$AppEvent_UsersCopyWithImpl(this._self, this._then);

  final AppEvent_Users _self;
  final $Res Function(AppEvent_Users) _then;

/// Create a copy of AppEvent
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? serverId = null,Object? users = null,}) {
  return _then(AppEvent_Users(
serverId: null == serverId ? _self.serverId : serverId // ignore: cast_nullable_to_non_nullable
as String,users: null == users ? _self._users : users // ignore: cast_nullable_to_non_nullable
as List<UiUser>,
  ));
}


}

/// @nodoc


class AppEvent_Channels extends AppEvent {
  const AppEvent_Channels({required this.serverId, required final  List<UiChannel> channels}): _channels = channels,super._();
  

 final  String serverId;
 final  List<UiChannel> _channels;
 List<UiChannel> get channels {
  if (_channels is EqualUnmodifiableListView) return _channels;
  // ignore: implicit_dynamic_type
  return EqualUnmodifiableListView(_channels);
}


/// Create a copy of AppEvent
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$AppEvent_ChannelsCopyWith<AppEvent_Channels> get copyWith => _$AppEvent_ChannelsCopyWithImpl<AppEvent_Channels>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is AppEvent_Channels&&(identical(other.serverId, serverId) || other.serverId == serverId)&&const DeepCollectionEquality().equals(other._channels, _channels));
}


@override
int get hashCode => Object.hash(runtimeType,serverId,const DeepCollectionEquality().hash(_channels));

@override
String toString() {
  return 'AppEvent.channels(serverId: $serverId, channels: $channels)';
}


}

/// @nodoc
abstract mixin class $AppEvent_ChannelsCopyWith<$Res> implements $AppEventCopyWith<$Res> {
  factory $AppEvent_ChannelsCopyWith(AppEvent_Channels value, $Res Function(AppEvent_Channels) _then) = _$AppEvent_ChannelsCopyWithImpl;
@useResult
$Res call({
 String serverId, List<UiChannel> channels
});




}
/// @nodoc
class _$AppEvent_ChannelsCopyWithImpl<$Res>
    implements $AppEvent_ChannelsCopyWith<$Res> {
  _$AppEvent_ChannelsCopyWithImpl(this._self, this._then);

  final AppEvent_Channels _self;
  final $Res Function(AppEvent_Channels) _then;

/// Create a copy of AppEvent
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? serverId = null,Object? channels = null,}) {
  return _then(AppEvent_Channels(
serverId: null == serverId ? _self.serverId : serverId // ignore: cast_nullable_to_non_nullable
as String,channels: null == channels ? _self._channels : channels // ignore: cast_nullable_to_non_nullable
as List<UiChannel>,
  ));
}


}

/// @nodoc


class AppEvent_Text extends AppEvent {
  const AppEvent_Text({required this.serverId, required this.from, required this.message}): super._();
  

 final  String serverId;
 final  String from;
 final  String message;

/// Create a copy of AppEvent
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$AppEvent_TextCopyWith<AppEvent_Text> get copyWith => _$AppEvent_TextCopyWithImpl<AppEvent_Text>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is AppEvent_Text&&(identical(other.serverId, serverId) || other.serverId == serverId)&&(identical(other.from, from) || other.from == from)&&(identical(other.message, message) || other.message == message));
}


@override
int get hashCode => Object.hash(runtimeType,serverId,from,message);

@override
String toString() {
  return 'AppEvent.text(serverId: $serverId, from: $from, message: $message)';
}


}

/// @nodoc
abstract mixin class $AppEvent_TextCopyWith<$Res> implements $AppEventCopyWith<$Res> {
  factory $AppEvent_TextCopyWith(AppEvent_Text value, $Res Function(AppEvent_Text) _then) = _$AppEvent_TextCopyWithImpl;
@useResult
$Res call({
 String serverId, String from, String message
});




}
/// @nodoc
class _$AppEvent_TextCopyWithImpl<$Res>
    implements $AppEvent_TextCopyWith<$Res> {
  _$AppEvent_TextCopyWithImpl(this._self, this._then);

  final AppEvent_Text _self;
  final $Res Function(AppEvent_Text) _then;

/// Create a copy of AppEvent
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? serverId = null,Object? from = null,Object? message = null,}) {
  return _then(AppEvent_Text(
serverId: null == serverId ? _self.serverId : serverId // ignore: cast_nullable_to_non_nullable
as String,from: null == from ? _self.from : from // ignore: cast_nullable_to_non_nullable
as String,message: null == message ? _self.message : message // ignore: cast_nullable_to_non_nullable
as String,
  ));
}


}

/// @nodoc


class AppEvent_Stats extends AppEvent {
  const AppEvent_Stats(this.field0): super._();
  

 final  UiStats field0;

/// Create a copy of AppEvent
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$AppEvent_StatsCopyWith<AppEvent_Stats> get copyWith => _$AppEvent_StatsCopyWithImpl<AppEvent_Stats>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is AppEvent_Stats&&(identical(other.field0, field0) || other.field0 == field0));
}


@override
int get hashCode => Object.hash(runtimeType,field0);

@override
String toString() {
  return 'AppEvent.stats(field0: $field0)';
}


}

/// @nodoc
abstract mixin class $AppEvent_StatsCopyWith<$Res> implements $AppEventCopyWith<$Res> {
  factory $AppEvent_StatsCopyWith(AppEvent_Stats value, $Res Function(AppEvent_Stats) _then) = _$AppEvent_StatsCopyWithImpl;
@useResult
$Res call({
 UiStats field0
});




}
/// @nodoc
class _$AppEvent_StatsCopyWithImpl<$Res>
    implements $AppEvent_StatsCopyWith<$Res> {
  _$AppEvent_StatsCopyWithImpl(this._self, this._then);

  final AppEvent_Stats _self;
  final $Res Function(AppEvent_Stats) _then;

/// Create a copy of AppEvent
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? field0 = null,}) {
  return _then(AppEvent_Stats(
null == field0 ? _self.field0 : field0 // ignore: cast_nullable_to_non_nullable
as UiStats,
  ));
}


}

/// @nodoc


class AppEvent_InputLevel extends AppEvent {
  const AppEvent_InputLevel({required this.levelDb, required this.speaking, required this.thresholdDb, required this.noiseFloorDb}): super._();
  

 final  double levelDb;
 final  bool speaking;
/// Level voice activation opens at, tracking the background noise.
 final  double thresholdDb;
/// The tracked background noise itself. The gap up to `threshold_db`
/// is the margin, which is what makes a rising floor readable as
/// wind rather than as a mis-set control.
 final  double noiseFloorDb;

/// Create a copy of AppEvent
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$AppEvent_InputLevelCopyWith<AppEvent_InputLevel> get copyWith => _$AppEvent_InputLevelCopyWithImpl<AppEvent_InputLevel>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is AppEvent_InputLevel&&(identical(other.levelDb, levelDb) || other.levelDb == levelDb)&&(identical(other.speaking, speaking) || other.speaking == speaking)&&(identical(other.thresholdDb, thresholdDb) || other.thresholdDb == thresholdDb)&&(identical(other.noiseFloorDb, noiseFloorDb) || other.noiseFloorDb == noiseFloorDb));
}


@override
int get hashCode => Object.hash(runtimeType,levelDb,speaking,thresholdDb,noiseFloorDb);

@override
String toString() {
  return 'AppEvent.inputLevel(levelDb: $levelDb, speaking: $speaking, thresholdDb: $thresholdDb, noiseFloorDb: $noiseFloorDb)';
}


}

/// @nodoc
abstract mixin class $AppEvent_InputLevelCopyWith<$Res> implements $AppEventCopyWith<$Res> {
  factory $AppEvent_InputLevelCopyWith(AppEvent_InputLevel value, $Res Function(AppEvent_InputLevel) _then) = _$AppEvent_InputLevelCopyWithImpl;
@useResult
$Res call({
 double levelDb, bool speaking, double thresholdDb, double noiseFloorDb
});




}
/// @nodoc
class _$AppEvent_InputLevelCopyWithImpl<$Res>
    implements $AppEvent_InputLevelCopyWith<$Res> {
  _$AppEvent_InputLevelCopyWithImpl(this._self, this._then);

  final AppEvent_InputLevel _self;
  final $Res Function(AppEvent_InputLevel) _then;

/// Create a copy of AppEvent
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? levelDb = null,Object? speaking = null,Object? thresholdDb = null,Object? noiseFloorDb = null,}) {
  return _then(AppEvent_InputLevel(
levelDb: null == levelDb ? _self.levelDb : levelDb // ignore: cast_nullable_to_non_nullable
as double,speaking: null == speaking ? _self.speaking : speaking // ignore: cast_nullable_to_non_nullable
as bool,thresholdDb: null == thresholdDb ? _self.thresholdDb : thresholdDb // ignore: cast_nullable_to_non_nullable
as double,noiseFloorDb: null == noiseFloorDb ? _self.noiseFloorDb : noiseFloorDb // ignore: cast_nullable_to_non_nullable
as double,
  ));
}


}

/// @nodoc


class AppEvent_SpeakerLevels extends AppEvent {
  const AppEvent_SpeakerLevels({required final  List<UiSpeakerLevel> levels}): _levels = levels,super._();
  

 final  List<UiSpeakerLevel> _levels;
 List<UiSpeakerLevel> get levels {
  if (_levels is EqualUnmodifiableListView) return _levels;
  // ignore: implicit_dynamic_type
  return EqualUnmodifiableListView(_levels);
}


/// Create a copy of AppEvent
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$AppEvent_SpeakerLevelsCopyWith<AppEvent_SpeakerLevels> get copyWith => _$AppEvent_SpeakerLevelsCopyWithImpl<AppEvent_SpeakerLevels>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is AppEvent_SpeakerLevels&&const DeepCollectionEquality().equals(other._levels, _levels));
}


@override
int get hashCode => Object.hash(runtimeType,const DeepCollectionEquality().hash(_levels));

@override
String toString() {
  return 'AppEvent.speakerLevels(levels: $levels)';
}


}

/// @nodoc
abstract mixin class $AppEvent_SpeakerLevelsCopyWith<$Res> implements $AppEventCopyWith<$Res> {
  factory $AppEvent_SpeakerLevelsCopyWith(AppEvent_SpeakerLevels value, $Res Function(AppEvent_SpeakerLevels) _then) = _$AppEvent_SpeakerLevelsCopyWithImpl;
@useResult
$Res call({
 List<UiSpeakerLevel> levels
});




}
/// @nodoc
class _$AppEvent_SpeakerLevelsCopyWithImpl<$Res>
    implements $AppEvent_SpeakerLevelsCopyWith<$Res> {
  _$AppEvent_SpeakerLevelsCopyWithImpl(this._self, this._then);

  final AppEvent_SpeakerLevels _self;
  final $Res Function(AppEvent_SpeakerLevels) _then;

/// Create a copy of AppEvent
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? levels = null,}) {
  return _then(AppEvent_SpeakerLevels(
levels: null == levels ? _self._levels : levels // ignore: cast_nullable_to_non_nullable
as List<UiSpeakerLevel>,
  ));
}


}

/// @nodoc


class AppEvent_Suppressed extends AppEvent {
  const AppEvent_Suppressed({required this.serverId, required this.suppressed}): super._();
  

 final  String serverId;
 final  bool suppressed;

/// Create a copy of AppEvent
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$AppEvent_SuppressedCopyWith<AppEvent_Suppressed> get copyWith => _$AppEvent_SuppressedCopyWithImpl<AppEvent_Suppressed>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is AppEvent_Suppressed&&(identical(other.serverId, serverId) || other.serverId == serverId)&&(identical(other.suppressed, suppressed) || other.suppressed == suppressed));
}


@override
int get hashCode => Object.hash(runtimeType,serverId,suppressed);

@override
String toString() {
  return 'AppEvent.suppressed(serverId: $serverId, suppressed: $suppressed)';
}


}

/// @nodoc
abstract mixin class $AppEvent_SuppressedCopyWith<$Res> implements $AppEventCopyWith<$Res> {
  factory $AppEvent_SuppressedCopyWith(AppEvent_Suppressed value, $Res Function(AppEvent_Suppressed) _then) = _$AppEvent_SuppressedCopyWithImpl;
@useResult
$Res call({
 String serverId, bool suppressed
});




}
/// @nodoc
class _$AppEvent_SuppressedCopyWithImpl<$Res>
    implements $AppEvent_SuppressedCopyWith<$Res> {
  _$AppEvent_SuppressedCopyWithImpl(this._self, this._then);

  final AppEvent_Suppressed _self;
  final $Res Function(AppEvent_Suppressed) _then;

/// Create a copy of AppEvent
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? serverId = null,Object? suppressed = null,}) {
  return _then(AppEvent_Suppressed(
serverId: null == serverId ? _self.serverId : serverId // ignore: cast_nullable_to_non_nullable
as String,suppressed: null == suppressed ? _self.suppressed : suppressed // ignore: cast_nullable_to_non_nullable
as bool,
  ));
}


}

/// @nodoc


class AppEvent_Listening extends AppEvent {
  const AppEvent_Listening({required this.serverId, required this.channels}): super._();
  

 final  String serverId;
 final  Uint32List channels;

/// Create a copy of AppEvent
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$AppEvent_ListeningCopyWith<AppEvent_Listening> get copyWith => _$AppEvent_ListeningCopyWithImpl<AppEvent_Listening>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is AppEvent_Listening&&(identical(other.serverId, serverId) || other.serverId == serverId)&&const DeepCollectionEquality().equals(other.channels, channels));
}


@override
int get hashCode => Object.hash(runtimeType,serverId,const DeepCollectionEquality().hash(channels));

@override
String toString() {
  return 'AppEvent.listening(serverId: $serverId, channels: $channels)';
}


}

/// @nodoc
abstract mixin class $AppEvent_ListeningCopyWith<$Res> implements $AppEventCopyWith<$Res> {
  factory $AppEvent_ListeningCopyWith(AppEvent_Listening value, $Res Function(AppEvent_Listening) _then) = _$AppEvent_ListeningCopyWithImpl;
@useResult
$Res call({
 String serverId, Uint32List channels
});




}
/// @nodoc
class _$AppEvent_ListeningCopyWithImpl<$Res>
    implements $AppEvent_ListeningCopyWith<$Res> {
  _$AppEvent_ListeningCopyWithImpl(this._self, this._then);

  final AppEvent_Listening _self;
  final $Res Function(AppEvent_Listening) _then;

/// Create a copy of AppEvent
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? serverId = null,Object? channels = null,}) {
  return _then(AppEvent_Listening(
serverId: null == serverId ? _self.serverId : serverId // ignore: cast_nullable_to_non_nullable
as String,channels: null == channels ? _self.channels : channels // ignore: cast_nullable_to_non_nullable
as Uint32List,
  ));
}


}

/// @nodoc


class AppEvent_Limits extends AppEvent {
  const AppEvent_Limits({required this.serverId, required this.messageLength, required this.imageMessageLength}): super._();
  

 final  String serverId;
 final  int messageLength;
 final  int imageMessageLength;

/// Create a copy of AppEvent
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$AppEvent_LimitsCopyWith<AppEvent_Limits> get copyWith => _$AppEvent_LimitsCopyWithImpl<AppEvent_Limits>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is AppEvent_Limits&&(identical(other.serverId, serverId) || other.serverId == serverId)&&(identical(other.messageLength, messageLength) || other.messageLength == messageLength)&&(identical(other.imageMessageLength, imageMessageLength) || other.imageMessageLength == imageMessageLength));
}


@override
int get hashCode => Object.hash(runtimeType,serverId,messageLength,imageMessageLength);

@override
String toString() {
  return 'AppEvent.limits(serverId: $serverId, messageLength: $messageLength, imageMessageLength: $imageMessageLength)';
}


}

/// @nodoc
abstract mixin class $AppEvent_LimitsCopyWith<$Res> implements $AppEventCopyWith<$Res> {
  factory $AppEvent_LimitsCopyWith(AppEvent_Limits value, $Res Function(AppEvent_Limits) _then) = _$AppEvent_LimitsCopyWithImpl;
@useResult
$Res call({
 String serverId, int messageLength, int imageMessageLength
});




}
/// @nodoc
class _$AppEvent_LimitsCopyWithImpl<$Res>
    implements $AppEvent_LimitsCopyWith<$Res> {
  _$AppEvent_LimitsCopyWithImpl(this._self, this._then);

  final AppEvent_Limits _self;
  final $Res Function(AppEvent_Limits) _then;

/// Create a copy of AppEvent
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? serverId = null,Object? messageLength = null,Object? imageMessageLength = null,}) {
  return _then(AppEvent_Limits(
serverId: null == serverId ? _self.serverId : serverId // ignore: cast_nullable_to_non_nullable
as String,messageLength: null == messageLength ? _self.messageLength : messageLength // ignore: cast_nullable_to_non_nullable
as int,imageMessageLength: null == imageMessageLength ? _self.imageMessageLength : imageMessageLength // ignore: cast_nullable_to_non_nullable
as int,
  ));
}


}

/// @nodoc


class AppEvent_Bandwidth extends AppEvent {
  const AppEvent_Bandwidth({required this.serverId, required this.capBps, required this.bitrateBps, required this.capped, required this.belowFloor}): super._();
  

 final  String serverId;
/// What this server allows each client, in bits per second.
 final  int capBps;
/// What the encoder is now aiming for.
 final  int bitrateBps;
/// Whether an allowance, rather than this app's own choice, decided it.
 final  bool capped;
/// Whether even the lowest usable bitrate does not fit — voice will be
/// dropped by the server, and nothing here can prevent it.
 final  bool belowFloor;

/// Create a copy of AppEvent
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$AppEvent_BandwidthCopyWith<AppEvent_Bandwidth> get copyWith => _$AppEvent_BandwidthCopyWithImpl<AppEvent_Bandwidth>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is AppEvent_Bandwidth&&(identical(other.serverId, serverId) || other.serverId == serverId)&&(identical(other.capBps, capBps) || other.capBps == capBps)&&(identical(other.bitrateBps, bitrateBps) || other.bitrateBps == bitrateBps)&&(identical(other.capped, capped) || other.capped == capped)&&(identical(other.belowFloor, belowFloor) || other.belowFloor == belowFloor));
}


@override
int get hashCode => Object.hash(runtimeType,serverId,capBps,bitrateBps,capped,belowFloor);

@override
String toString() {
  return 'AppEvent.bandwidth(serverId: $serverId, capBps: $capBps, bitrateBps: $bitrateBps, capped: $capped, belowFloor: $belowFloor)';
}


}

/// @nodoc
abstract mixin class $AppEvent_BandwidthCopyWith<$Res> implements $AppEventCopyWith<$Res> {
  factory $AppEvent_BandwidthCopyWith(AppEvent_Bandwidth value, $Res Function(AppEvent_Bandwidth) _then) = _$AppEvent_BandwidthCopyWithImpl;
@useResult
$Res call({
 String serverId, int capBps, int bitrateBps, bool capped, bool belowFloor
});




}
/// @nodoc
class _$AppEvent_BandwidthCopyWithImpl<$Res>
    implements $AppEvent_BandwidthCopyWith<$Res> {
  _$AppEvent_BandwidthCopyWithImpl(this._self, this._then);

  final AppEvent_Bandwidth _self;
  final $Res Function(AppEvent_Bandwidth) _then;

/// Create a copy of AppEvent
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? serverId = null,Object? capBps = null,Object? bitrateBps = null,Object? capped = null,Object? belowFloor = null,}) {
  return _then(AppEvent_Bandwidth(
serverId: null == serverId ? _self.serverId : serverId // ignore: cast_nullable_to_non_nullable
as String,capBps: null == capBps ? _self.capBps : capBps // ignore: cast_nullable_to_non_nullable
as int,bitrateBps: null == bitrateBps ? _self.bitrateBps : bitrateBps // ignore: cast_nullable_to_non_nullable
as int,capped: null == capped ? _self.capped : capped // ignore: cast_nullable_to_non_nullable
as bool,belowFloor: null == belowFloor ? _self.belowFloor : belowFloor // ignore: cast_nullable_to_non_nullable
as bool,
  ));
}


}

/// @nodoc


class AppEvent_ContextActions extends AppEvent {
  const AppEvent_ContextActions({required this.serverId, required final  List<UiContextAction> actions}): _actions = actions,super._();
  

 final  String serverId;
 final  List<UiContextAction> _actions;
 List<UiContextAction> get actions {
  if (_actions is EqualUnmodifiableListView) return _actions;
  // ignore: implicit_dynamic_type
  return EqualUnmodifiableListView(_actions);
}


/// Create a copy of AppEvent
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$AppEvent_ContextActionsCopyWith<AppEvent_ContextActions> get copyWith => _$AppEvent_ContextActionsCopyWithImpl<AppEvent_ContextActions>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is AppEvent_ContextActions&&(identical(other.serverId, serverId) || other.serverId == serverId)&&const DeepCollectionEquality().equals(other._actions, _actions));
}


@override
int get hashCode => Object.hash(runtimeType,serverId,const DeepCollectionEquality().hash(_actions));

@override
String toString() {
  return 'AppEvent.contextActions(serverId: $serverId, actions: $actions)';
}


}

/// @nodoc
abstract mixin class $AppEvent_ContextActionsCopyWith<$Res> implements $AppEventCopyWith<$Res> {
  factory $AppEvent_ContextActionsCopyWith(AppEvent_ContextActions value, $Res Function(AppEvent_ContextActions) _then) = _$AppEvent_ContextActionsCopyWithImpl;
@useResult
$Res call({
 String serverId, List<UiContextAction> actions
});




}
/// @nodoc
class _$AppEvent_ContextActionsCopyWithImpl<$Res>
    implements $AppEvent_ContextActionsCopyWith<$Res> {
  _$AppEvent_ContextActionsCopyWithImpl(this._self, this._then);

  final AppEvent_ContextActions _self;
  final $Res Function(AppEvent_ContextActions) _then;

/// Create a copy of AppEvent
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? serverId = null,Object? actions = null,}) {
  return _then(AppEvent_ContextActions(
serverId: null == serverId ? _self.serverId : serverId // ignore: cast_nullable_to_non_nullable
as String,actions: null == actions ? _self._actions : actions // ignore: cast_nullable_to_non_nullable
as List<UiContextAction>,
  ));
}


}

/// @nodoc


class AppEvent_Acl extends AppEvent {
  const AppEvent_Acl({required this.serverId, required this.acl}): super._();
  

 final  String serverId;
 final  UiChannelAcl acl;

/// Create a copy of AppEvent
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$AppEvent_AclCopyWith<AppEvent_Acl> get copyWith => _$AppEvent_AclCopyWithImpl<AppEvent_Acl>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is AppEvent_Acl&&(identical(other.serverId, serverId) || other.serverId == serverId)&&(identical(other.acl, acl) || other.acl == acl));
}


@override
int get hashCode => Object.hash(runtimeType,serverId,acl);

@override
String toString() {
  return 'AppEvent.acl(serverId: $serverId, acl: $acl)';
}


}

/// @nodoc
abstract mixin class $AppEvent_AclCopyWith<$Res> implements $AppEventCopyWith<$Res> {
  factory $AppEvent_AclCopyWith(AppEvent_Acl value, $Res Function(AppEvent_Acl) _then) = _$AppEvent_AclCopyWithImpl;
@useResult
$Res call({
 String serverId, UiChannelAcl acl
});




}
/// @nodoc
class _$AppEvent_AclCopyWithImpl<$Res>
    implements $AppEvent_AclCopyWith<$Res> {
  _$AppEvent_AclCopyWithImpl(this._self, this._then);

  final AppEvent_Acl _self;
  final $Res Function(AppEvent_Acl) _then;

/// Create a copy of AppEvent
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? serverId = null,Object? acl = null,}) {
  return _then(AppEvent_Acl(
serverId: null == serverId ? _self.serverId : serverId // ignore: cast_nullable_to_non_nullable
as String,acl: null == acl ? _self.acl : acl // ignore: cast_nullable_to_non_nullable
as UiChannelAcl,
  ));
}


}

/// @nodoc


class AppEvent_UserNames extends AppEvent {
  const AppEvent_UserNames({required this.serverId, required final  List<UiUserName> names}): _names = names,super._();
  

 final  String serverId;
 final  List<UiUserName> _names;
 List<UiUserName> get names {
  if (_names is EqualUnmodifiableListView) return _names;
  // ignore: implicit_dynamic_type
  return EqualUnmodifiableListView(_names);
}


/// Create a copy of AppEvent
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$AppEvent_UserNamesCopyWith<AppEvent_UserNames> get copyWith => _$AppEvent_UserNamesCopyWithImpl<AppEvent_UserNames>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is AppEvent_UserNames&&(identical(other.serverId, serverId) || other.serverId == serverId)&&const DeepCollectionEquality().equals(other._names, _names));
}


@override
int get hashCode => Object.hash(runtimeType,serverId,const DeepCollectionEquality().hash(_names));

@override
String toString() {
  return 'AppEvent.userNames(serverId: $serverId, names: $names)';
}


}

/// @nodoc
abstract mixin class $AppEvent_UserNamesCopyWith<$Res> implements $AppEventCopyWith<$Res> {
  factory $AppEvent_UserNamesCopyWith(AppEvent_UserNames value, $Res Function(AppEvent_UserNames) _then) = _$AppEvent_UserNamesCopyWithImpl;
@useResult
$Res call({
 String serverId, List<UiUserName> names
});




}
/// @nodoc
class _$AppEvent_UserNamesCopyWithImpl<$Res>
    implements $AppEvent_UserNamesCopyWith<$Res> {
  _$AppEvent_UserNamesCopyWithImpl(this._self, this._then);

  final AppEvent_UserNames _self;
  final $Res Function(AppEvent_UserNames) _then;

/// Create a copy of AppEvent
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? serverId = null,Object? names = null,}) {
  return _then(AppEvent_UserNames(
serverId: null == serverId ? _self.serverId : serverId // ignore: cast_nullable_to_non_nullable
as String,names: null == names ? _self._names : names // ignore: cast_nullable_to_non_nullable
as List<UiUserName>,
  ));
}


}

/// @nodoc


class AppEvent_Registered extends AppEvent {
  const AppEvent_Registered({required this.serverId, required final  List<UiRegisteredUser> users}): _users = users,super._();
  

 final  String serverId;
 final  List<UiRegisteredUser> _users;
 List<UiRegisteredUser> get users {
  if (_users is EqualUnmodifiableListView) return _users;
  // ignore: implicit_dynamic_type
  return EqualUnmodifiableListView(_users);
}


/// Create a copy of AppEvent
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$AppEvent_RegisteredCopyWith<AppEvent_Registered> get copyWith => _$AppEvent_RegisteredCopyWithImpl<AppEvent_Registered>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is AppEvent_Registered&&(identical(other.serverId, serverId) || other.serverId == serverId)&&const DeepCollectionEquality().equals(other._users, _users));
}


@override
int get hashCode => Object.hash(runtimeType,serverId,const DeepCollectionEquality().hash(_users));

@override
String toString() {
  return 'AppEvent.registered(serverId: $serverId, users: $users)';
}


}

/// @nodoc
abstract mixin class $AppEvent_RegisteredCopyWith<$Res> implements $AppEventCopyWith<$Res> {
  factory $AppEvent_RegisteredCopyWith(AppEvent_Registered value, $Res Function(AppEvent_Registered) _then) = _$AppEvent_RegisteredCopyWithImpl;
@useResult
$Res call({
 String serverId, List<UiRegisteredUser> users
});




}
/// @nodoc
class _$AppEvent_RegisteredCopyWithImpl<$Res>
    implements $AppEvent_RegisteredCopyWith<$Res> {
  _$AppEvent_RegisteredCopyWithImpl(this._self, this._then);

  final AppEvent_Registered _self;
  final $Res Function(AppEvent_Registered) _then;

/// Create a copy of AppEvent
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? serverId = null,Object? users = null,}) {
  return _then(AppEvent_Registered(
serverId: null == serverId ? _self.serverId : serverId // ignore: cast_nullable_to_non_nullable
as String,users: null == users ? _self._users : users // ignore: cast_nullable_to_non_nullable
as List<UiRegisteredUser>,
  ));
}


}

/// @nodoc


class AppEvent_UserDetails extends AppEvent {
  const AppEvent_UserDetails({required this.serverId, required this.details}): super._();
  

 final  String serverId;
 final  UiUserDetails details;

/// Create a copy of AppEvent
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$AppEvent_UserDetailsCopyWith<AppEvent_UserDetails> get copyWith => _$AppEvent_UserDetailsCopyWithImpl<AppEvent_UserDetails>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is AppEvent_UserDetails&&(identical(other.serverId, serverId) || other.serverId == serverId)&&(identical(other.details, details) || other.details == details));
}


@override
int get hashCode => Object.hash(runtimeType,serverId,details);

@override
String toString() {
  return 'AppEvent.userDetails(serverId: $serverId, details: $details)';
}


}

/// @nodoc
abstract mixin class $AppEvent_UserDetailsCopyWith<$Res> implements $AppEventCopyWith<$Res> {
  factory $AppEvent_UserDetailsCopyWith(AppEvent_UserDetails value, $Res Function(AppEvent_UserDetails) _then) = _$AppEvent_UserDetailsCopyWithImpl;
@useResult
$Res call({
 String serverId, UiUserDetails details
});




}
/// @nodoc
class _$AppEvent_UserDetailsCopyWithImpl<$Res>
    implements $AppEvent_UserDetailsCopyWith<$Res> {
  _$AppEvent_UserDetailsCopyWithImpl(this._self, this._then);

  final AppEvent_UserDetails _self;
  final $Res Function(AppEvent_UserDetails) _then;

/// Create a copy of AppEvent
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? serverId = null,Object? details = null,}) {
  return _then(AppEvent_UserDetails(
serverId: null == serverId ? _self.serverId : serverId // ignore: cast_nullable_to_non_nullable
as String,details: null == details ? _self.details : details // ignore: cast_nullable_to_non_nullable
as UiUserDetails,
  ));
}


}

/// @nodoc


class AppEvent_Bans extends AppEvent {
  const AppEvent_Bans({required this.serverId, required final  List<UiBan> bans}): _bans = bans,super._();
  

 final  String serverId;
 final  List<UiBan> _bans;
 List<UiBan> get bans {
  if (_bans is EqualUnmodifiableListView) return _bans;
  // ignore: implicit_dynamic_type
  return EqualUnmodifiableListView(_bans);
}


/// Create a copy of AppEvent
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$AppEvent_BansCopyWith<AppEvent_Bans> get copyWith => _$AppEvent_BansCopyWithImpl<AppEvent_Bans>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is AppEvent_Bans&&(identical(other.serverId, serverId) || other.serverId == serverId)&&const DeepCollectionEquality().equals(other._bans, _bans));
}


@override
int get hashCode => Object.hash(runtimeType,serverId,const DeepCollectionEquality().hash(_bans));

@override
String toString() {
  return 'AppEvent.bans(serverId: $serverId, bans: $bans)';
}


}

/// @nodoc
abstract mixin class $AppEvent_BansCopyWith<$Res> implements $AppEventCopyWith<$Res> {
  factory $AppEvent_BansCopyWith(AppEvent_Bans value, $Res Function(AppEvent_Bans) _then) = _$AppEvent_BansCopyWithImpl;
@useResult
$Res call({
 String serverId, List<UiBan> bans
});




}
/// @nodoc
class _$AppEvent_BansCopyWithImpl<$Res>
    implements $AppEvent_BansCopyWith<$Res> {
  _$AppEvent_BansCopyWithImpl(this._self, this._then);

  final AppEvent_Bans _self;
  final $Res Function(AppEvent_Bans) _then;

/// Create a copy of AppEvent
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? serverId = null,Object? bans = null,}) {
  return _then(AppEvent_Bans(
serverId: null == serverId ? _self.serverId : serverId // ignore: cast_nullable_to_non_nullable
as String,bans: null == bans ? _self._bans : bans // ignore: cast_nullable_to_non_nullable
as List<UiBan>,
  ));
}


}

/// @nodoc


class AppEvent_ServerSuggests extends AppEvent {
  const AppEvent_ServerSuggests({required this.serverId, this.pushToTalk, this.positional}): super._();
  

 final  String serverId;
 final  bool? pushToTalk;
 final  bool? positional;

/// Create a copy of AppEvent
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$AppEvent_ServerSuggestsCopyWith<AppEvent_ServerSuggests> get copyWith => _$AppEvent_ServerSuggestsCopyWithImpl<AppEvent_ServerSuggests>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is AppEvent_ServerSuggests&&(identical(other.serverId, serverId) || other.serverId == serverId)&&(identical(other.pushToTalk, pushToTalk) || other.pushToTalk == pushToTalk)&&(identical(other.positional, positional) || other.positional == positional));
}


@override
int get hashCode => Object.hash(runtimeType,serverId,pushToTalk,positional);

@override
String toString() {
  return 'AppEvent.serverSuggests(serverId: $serverId, pushToTalk: $pushToTalk, positional: $positional)';
}


}

/// @nodoc
abstract mixin class $AppEvent_ServerSuggestsCopyWith<$Res> implements $AppEventCopyWith<$Res> {
  factory $AppEvent_ServerSuggestsCopyWith(AppEvent_ServerSuggests value, $Res Function(AppEvent_ServerSuggests) _then) = _$AppEvent_ServerSuggestsCopyWithImpl;
@useResult
$Res call({
 String serverId, bool? pushToTalk, bool? positional
});




}
/// @nodoc
class _$AppEvent_ServerSuggestsCopyWithImpl<$Res>
    implements $AppEvent_ServerSuggestsCopyWith<$Res> {
  _$AppEvent_ServerSuggestsCopyWithImpl(this._self, this._then);

  final AppEvent_ServerSuggests _self;
  final $Res Function(AppEvent_ServerSuggests) _then;

/// Create a copy of AppEvent
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? serverId = null,Object? pushToTalk = freezed,Object? positional = freezed,}) {
  return _then(AppEvent_ServerSuggests(
serverId: null == serverId ? _self.serverId : serverId // ignore: cast_nullable_to_non_nullable
as String,pushToTalk: freezed == pushToTalk ? _self.pushToTalk : pushToTalk // ignore: cast_nullable_to_non_nullable
as bool?,positional: freezed == positional ? _self.positional : positional // ignore: cast_nullable_to_non_nullable
as bool?,
  ));
}


}

/// @nodoc


class AppEvent_Avatar extends AppEvent {
  const AppEvent_Avatar({required this.serverId, required this.session, required this.image}): super._();
  

 final  String serverId;
 final  int session;
 final  Uint8List image;

/// Create a copy of AppEvent
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$AppEvent_AvatarCopyWith<AppEvent_Avatar> get copyWith => _$AppEvent_AvatarCopyWithImpl<AppEvent_Avatar>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is AppEvent_Avatar&&(identical(other.serverId, serverId) || other.serverId == serverId)&&(identical(other.session, session) || other.session == session)&&const DeepCollectionEquality().equals(other.image, image));
}


@override
int get hashCode => Object.hash(runtimeType,serverId,session,const DeepCollectionEquality().hash(image));

@override
String toString() {
  return 'AppEvent.avatar(serverId: $serverId, session: $session, image: $image)';
}


}

/// @nodoc
abstract mixin class $AppEvent_AvatarCopyWith<$Res> implements $AppEventCopyWith<$Res> {
  factory $AppEvent_AvatarCopyWith(AppEvent_Avatar value, $Res Function(AppEvent_Avatar) _then) = _$AppEvent_AvatarCopyWithImpl;
@useResult
$Res call({
 String serverId, int session, Uint8List image
});




}
/// @nodoc
class _$AppEvent_AvatarCopyWithImpl<$Res>
    implements $AppEvent_AvatarCopyWith<$Res> {
  _$AppEvent_AvatarCopyWithImpl(this._self, this._then);

  final AppEvent_Avatar _self;
  final $Res Function(AppEvent_Avatar) _then;

/// Create a copy of AppEvent
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? serverId = null,Object? session = null,Object? image = null,}) {
  return _then(AppEvent_Avatar(
serverId: null == serverId ? _self.serverId : serverId // ignore: cast_nullable_to_non_nullable
as String,session: null == session ? _self.session : session // ignore: cast_nullable_to_non_nullable
as int,image: null == image ? _self.image : image // ignore: cast_nullable_to_non_nullable
as Uint8List,
  ));
}


}

/// @nodoc


class AppEvent_Rights extends AppEvent {
  const AppEvent_Rights({required this.serverId, required this.rights}): super._();
  

 final  String serverId;
 final  UiRights rights;

/// Create a copy of AppEvent
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$AppEvent_RightsCopyWith<AppEvent_Rights> get copyWith => _$AppEvent_RightsCopyWithImpl<AppEvent_Rights>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is AppEvent_Rights&&(identical(other.serverId, serverId) || other.serverId == serverId)&&(identical(other.rights, rights) || other.rights == rights));
}


@override
int get hashCode => Object.hash(runtimeType,serverId,rights);

@override
String toString() {
  return 'AppEvent.rights(serverId: $serverId, rights: $rights)';
}


}

/// @nodoc
abstract mixin class $AppEvent_RightsCopyWith<$Res> implements $AppEventCopyWith<$Res> {
  factory $AppEvent_RightsCopyWith(AppEvent_Rights value, $Res Function(AppEvent_Rights) _then) = _$AppEvent_RightsCopyWithImpl;
@useResult
$Res call({
 String serverId, UiRights rights
});




}
/// @nodoc
class _$AppEvent_RightsCopyWithImpl<$Res>
    implements $AppEvent_RightsCopyWith<$Res> {
  _$AppEvent_RightsCopyWithImpl(this._self, this._then);

  final AppEvent_Rights _self;
  final $Res Function(AppEvent_Rights) _then;

/// Create a copy of AppEvent
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? serverId = null,Object? rights = null,}) {
  return _then(AppEvent_Rights(
serverId: null == serverId ? _self.serverId : serverId // ignore: cast_nullable_to_non_nullable
as String,rights: null == rights ? _self.rights : rights // ignore: cast_nullable_to_non_nullable
as UiRights,
  ));
}


}

/// @nodoc


class AppEvent_ChannelRights extends AppEvent {
  const AppEvent_ChannelRights({required this.serverId, required final  List<UiChannelRights> channels}): _channels = channels,super._();
  

 final  String serverId;
 final  List<UiChannelRights> _channels;
 List<UiChannelRights> get channels {
  if (_channels is EqualUnmodifiableListView) return _channels;
  // ignore: implicit_dynamic_type
  return EqualUnmodifiableListView(_channels);
}


/// Create a copy of AppEvent
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$AppEvent_ChannelRightsCopyWith<AppEvent_ChannelRights> get copyWith => _$AppEvent_ChannelRightsCopyWithImpl<AppEvent_ChannelRights>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is AppEvent_ChannelRights&&(identical(other.serverId, serverId) || other.serverId == serverId)&&const DeepCollectionEquality().equals(other._channels, _channels));
}


@override
int get hashCode => Object.hash(runtimeType,serverId,const DeepCollectionEquality().hash(_channels));

@override
String toString() {
  return 'AppEvent.channelRights(serverId: $serverId, channels: $channels)';
}


}

/// @nodoc
abstract mixin class $AppEvent_ChannelRightsCopyWith<$Res> implements $AppEventCopyWith<$Res> {
  factory $AppEvent_ChannelRightsCopyWith(AppEvent_ChannelRights value, $Res Function(AppEvent_ChannelRights) _then) = _$AppEvent_ChannelRightsCopyWithImpl;
@useResult
$Res call({
 String serverId, List<UiChannelRights> channels
});




}
/// @nodoc
class _$AppEvent_ChannelRightsCopyWithImpl<$Res>
    implements $AppEvent_ChannelRightsCopyWith<$Res> {
  _$AppEvent_ChannelRightsCopyWithImpl(this._self, this._then);

  final AppEvent_ChannelRights _self;
  final $Res Function(AppEvent_ChannelRights) _then;

/// Create a copy of AppEvent
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? serverId = null,Object? channels = null,}) {
  return _then(AppEvent_ChannelRights(
serverId: null == serverId ? _self.serverId : serverId // ignore: cast_nullable_to_non_nullable
as String,channels: null == channels ? _self._channels : channels // ignore: cast_nullable_to_non_nullable
as List<UiChannelRights>,
  ));
}


}

/// @nodoc


class AppEvent_Moderated extends AppEvent {
  const AppEvent_Moderated({required this.serverId, this.muted, this.deafened, required this.by}): super._();
  

 final  String serverId;
 final  bool? muted;
 final  bool? deafened;
 final  String by;

/// Create a copy of AppEvent
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$AppEvent_ModeratedCopyWith<AppEvent_Moderated> get copyWith => _$AppEvent_ModeratedCopyWithImpl<AppEvent_Moderated>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is AppEvent_Moderated&&(identical(other.serverId, serverId) || other.serverId == serverId)&&(identical(other.muted, muted) || other.muted == muted)&&(identical(other.deafened, deafened) || other.deafened == deafened)&&(identical(other.by, by) || other.by == by));
}


@override
int get hashCode => Object.hash(runtimeType,serverId,muted,deafened,by);

@override
String toString() {
  return 'AppEvent.moderated(serverId: $serverId, muted: $muted, deafened: $deafened, by: $by)';
}


}

/// @nodoc
abstract mixin class $AppEvent_ModeratedCopyWith<$Res> implements $AppEventCopyWith<$Res> {
  factory $AppEvent_ModeratedCopyWith(AppEvent_Moderated value, $Res Function(AppEvent_Moderated) _then) = _$AppEvent_ModeratedCopyWithImpl;
@useResult
$Res call({
 String serverId, bool? muted, bool? deafened, String by
});




}
/// @nodoc
class _$AppEvent_ModeratedCopyWithImpl<$Res>
    implements $AppEvent_ModeratedCopyWith<$Res> {
  _$AppEvent_ModeratedCopyWithImpl(this._self, this._then);

  final AppEvent_Moderated _self;
  final $Res Function(AppEvent_Moderated) _then;

/// Create a copy of AppEvent
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? serverId = null,Object? muted = freezed,Object? deafened = freezed,Object? by = null,}) {
  return _then(AppEvent_Moderated(
serverId: null == serverId ? _self.serverId : serverId // ignore: cast_nullable_to_non_nullable
as String,muted: freezed == muted ? _self.muted : muted // ignore: cast_nullable_to_non_nullable
as bool?,deafened: freezed == deafened ? _self.deafened : deafened // ignore: cast_nullable_to_non_nullable
as bool?,by: null == by ? _self.by : by // ignore: cast_nullable_to_non_nullable
as String,
  ));
}


}

/// @nodoc


class AppEvent_RemoteMuted extends AppEvent {
  const AppEvent_RemoteMuted({required this.serverId, required this.muted, required this.by}): super._();
  

 final  String serverId;
 final  bool muted;
 final  String by;

/// Create a copy of AppEvent
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$AppEvent_RemoteMutedCopyWith<AppEvent_RemoteMuted> get copyWith => _$AppEvent_RemoteMutedCopyWithImpl<AppEvent_RemoteMuted>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is AppEvent_RemoteMuted&&(identical(other.serverId, serverId) || other.serverId == serverId)&&(identical(other.muted, muted) || other.muted == muted)&&(identical(other.by, by) || other.by == by));
}


@override
int get hashCode => Object.hash(runtimeType,serverId,muted,by);

@override
String toString() {
  return 'AppEvent.remoteMuted(serverId: $serverId, muted: $muted, by: $by)';
}


}

/// @nodoc
abstract mixin class $AppEvent_RemoteMutedCopyWith<$Res> implements $AppEventCopyWith<$Res> {
  factory $AppEvent_RemoteMutedCopyWith(AppEvent_RemoteMuted value, $Res Function(AppEvent_RemoteMuted) _then) = _$AppEvent_RemoteMutedCopyWithImpl;
@useResult
$Res call({
 String serverId, bool muted, String by
});




}
/// @nodoc
class _$AppEvent_RemoteMutedCopyWithImpl<$Res>
    implements $AppEvent_RemoteMutedCopyWith<$Res> {
  _$AppEvent_RemoteMutedCopyWithImpl(this._self, this._then);

  final AppEvent_RemoteMuted _self;
  final $Res Function(AppEvent_RemoteMuted) _then;

/// Create a copy of AppEvent
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? serverId = null,Object? muted = null,Object? by = null,}) {
  return _then(AppEvent_RemoteMuted(
serverId: null == serverId ? _self.serverId : serverId // ignore: cast_nullable_to_non_nullable
as String,muted: null == muted ? _self.muted : muted // ignore: cast_nullable_to_non_nullable
as bool,by: null == by ? _self.by : by // ignore: cast_nullable_to_non_nullable
as String,
  ));
}


}

/// @nodoc


class AppEvent_Certificate extends AppEvent {
  const AppEvent_Certificate({required this.serverId, required this.fingerprint, required this.changed}): super._();
  

 final  String serverId;
 final  String fingerprint;
 final  bool changed;

/// Create a copy of AppEvent
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$AppEvent_CertificateCopyWith<AppEvent_Certificate> get copyWith => _$AppEvent_CertificateCopyWithImpl<AppEvent_Certificate>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is AppEvent_Certificate&&(identical(other.serverId, serverId) || other.serverId == serverId)&&(identical(other.fingerprint, fingerprint) || other.fingerprint == fingerprint)&&(identical(other.changed, changed) || other.changed == changed));
}


@override
int get hashCode => Object.hash(runtimeType,serverId,fingerprint,changed);

@override
String toString() {
  return 'AppEvent.certificate(serverId: $serverId, fingerprint: $fingerprint, changed: $changed)';
}


}

/// @nodoc
abstract mixin class $AppEvent_CertificateCopyWith<$Res> implements $AppEventCopyWith<$Res> {
  factory $AppEvent_CertificateCopyWith(AppEvent_Certificate value, $Res Function(AppEvent_Certificate) _then) = _$AppEvent_CertificateCopyWithImpl;
@useResult
$Res call({
 String serverId, String fingerprint, bool changed
});




}
/// @nodoc
class _$AppEvent_CertificateCopyWithImpl<$Res>
    implements $AppEvent_CertificateCopyWith<$Res> {
  _$AppEvent_CertificateCopyWithImpl(this._self, this._then);

  final AppEvent_Certificate _self;
  final $Res Function(AppEvent_Certificate) _then;

/// Create a copy of AppEvent
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? serverId = null,Object? fingerprint = null,Object? changed = null,}) {
  return _then(AppEvent_Certificate(
serverId: null == serverId ? _self.serverId : serverId // ignore: cast_nullable_to_non_nullable
as String,fingerprint: null == fingerprint ? _self.fingerprint : fingerprint // ignore: cast_nullable_to_non_nullable
as String,changed: null == changed ? _self.changed : changed // ignore: cast_nullable_to_non_nullable
as bool,
  ));
}


}

/// @nodoc


class AppEvent_Refused extends AppEvent {
  const AppEvent_Refused({required this.serverId, required this.reason, required this.kind}): super._();
  

 final  String serverId;
/// The server's own words. Often empty: most servers send only a type.
 final  String reason;
/// Mumble's `DenyType`, so the UI has something translatable to say
/// when `reason` is empty.
 final  int kind;

/// Create a copy of AppEvent
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$AppEvent_RefusedCopyWith<AppEvent_Refused> get copyWith => _$AppEvent_RefusedCopyWithImpl<AppEvent_Refused>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is AppEvent_Refused&&(identical(other.serverId, serverId) || other.serverId == serverId)&&(identical(other.reason, reason) || other.reason == reason)&&(identical(other.kind, kind) || other.kind == kind));
}


@override
int get hashCode => Object.hash(runtimeType,serverId,reason,kind);

@override
String toString() {
  return 'AppEvent.refused(serverId: $serverId, reason: $reason, kind: $kind)';
}


}

/// @nodoc
abstract mixin class $AppEvent_RefusedCopyWith<$Res> implements $AppEventCopyWith<$Res> {
  factory $AppEvent_RefusedCopyWith(AppEvent_Refused value, $Res Function(AppEvent_Refused) _then) = _$AppEvent_RefusedCopyWithImpl;
@useResult
$Res call({
 String serverId, String reason, int kind
});




}
/// @nodoc
class _$AppEvent_RefusedCopyWithImpl<$Res>
    implements $AppEvent_RefusedCopyWith<$Res> {
  _$AppEvent_RefusedCopyWithImpl(this._self, this._then);

  final AppEvent_Refused _self;
  final $Res Function(AppEvent_Refused) _then;

/// Create a copy of AppEvent
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? serverId = null,Object? reason = null,Object? kind = null,}) {
  return _then(AppEvent_Refused(
serverId: null == serverId ? _self.serverId : serverId // ignore: cast_nullable_to_non_nullable
as String,reason: null == reason ? _self.reason : reason // ignore: cast_nullable_to_non_nullable
as String,kind: null == kind ? _self.kind : kind // ignore: cast_nullable_to_non_nullable
as int,
  ));
}


}

/// @nodoc


class AppEvent_Welcome extends AppEvent {
  const AppEvent_Welcome({required this.serverId, required this.text}): super._();
  

 final  String serverId;
 final  String text;

/// Create a copy of AppEvent
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$AppEvent_WelcomeCopyWith<AppEvent_Welcome> get copyWith => _$AppEvent_WelcomeCopyWithImpl<AppEvent_Welcome>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is AppEvent_Welcome&&(identical(other.serverId, serverId) || other.serverId == serverId)&&(identical(other.text, text) || other.text == text));
}


@override
int get hashCode => Object.hash(runtimeType,serverId,text);

@override
String toString() {
  return 'AppEvent.welcome(serverId: $serverId, text: $text)';
}


}

/// @nodoc
abstract mixin class $AppEvent_WelcomeCopyWith<$Res> implements $AppEventCopyWith<$Res> {
  factory $AppEvent_WelcomeCopyWith(AppEvent_Welcome value, $Res Function(AppEvent_Welcome) _then) = _$AppEvent_WelcomeCopyWithImpl;
@useResult
$Res call({
 String serverId, String text
});




}
/// @nodoc
class _$AppEvent_WelcomeCopyWithImpl<$Res>
    implements $AppEvent_WelcomeCopyWith<$Res> {
  _$AppEvent_WelcomeCopyWithImpl(this._self, this._then);

  final AppEvent_Welcome _self;
  final $Res Function(AppEvent_Welcome) _then;

/// Create a copy of AppEvent
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? serverId = null,Object? text = null,}) {
  return _then(AppEvent_Welcome(
serverId: null == serverId ? _self.serverId : serverId // ignore: cast_nullable_to_non_nullable
as String,text: null == text ? _self.text : text // ignore: cast_nullable_to_non_nullable
as String,
  ));
}


}

/// @nodoc


class AppEvent_SelfSession extends AppEvent {
  const AppEvent_SelfSession({required this.serverId, required this.session}): super._();
  

 final  String serverId;
 final  int session;

/// Create a copy of AppEvent
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$AppEvent_SelfSessionCopyWith<AppEvent_SelfSession> get copyWith => _$AppEvent_SelfSessionCopyWithImpl<AppEvent_SelfSession>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is AppEvent_SelfSession&&(identical(other.serverId, serverId) || other.serverId == serverId)&&(identical(other.session, session) || other.session == session));
}


@override
int get hashCode => Object.hash(runtimeType,serverId,session);

@override
String toString() {
  return 'AppEvent.selfSession(serverId: $serverId, session: $session)';
}


}

/// @nodoc
abstract mixin class $AppEvent_SelfSessionCopyWith<$Res> implements $AppEventCopyWith<$Res> {
  factory $AppEvent_SelfSessionCopyWith(AppEvent_SelfSession value, $Res Function(AppEvent_SelfSession) _then) = _$AppEvent_SelfSessionCopyWithImpl;
@useResult
$Res call({
 String serverId, int session
});




}
/// @nodoc
class _$AppEvent_SelfSessionCopyWithImpl<$Res>
    implements $AppEvent_SelfSessionCopyWith<$Res> {
  _$AppEvent_SelfSessionCopyWithImpl(this._self, this._then);

  final AppEvent_SelfSession _self;
  final $Res Function(AppEvent_SelfSession) _then;

/// Create a copy of AppEvent
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? serverId = null,Object? session = null,}) {
  return _then(AppEvent_SelfSession(
serverId: null == serverId ? _self.serverId : serverId // ignore: cast_nullable_to_non_nullable
as String,session: null == session ? _self.session : session // ignore: cast_nullable_to_non_nullable
as int,
  ));
}


}

/// @nodoc


class AppEvent_Log extends AppEvent {
  const AppEvent_Log({required final  List<UiLogEntry> entries}): _entries = entries,super._();
  

 final  List<UiLogEntry> _entries;
 List<UiLogEntry> get entries {
  if (_entries is EqualUnmodifiableListView) return _entries;
  // ignore: implicit_dynamic_type
  return EqualUnmodifiableListView(_entries);
}


/// Create a copy of AppEvent
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$AppEvent_LogCopyWith<AppEvent_Log> get copyWith => _$AppEvent_LogCopyWithImpl<AppEvent_Log>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is AppEvent_Log&&const DeepCollectionEquality().equals(other._entries, _entries));
}


@override
int get hashCode => Object.hash(runtimeType,const DeepCollectionEquality().hash(_entries));

@override
String toString() {
  return 'AppEvent.log(entries: $entries)';
}


}

/// @nodoc
abstract mixin class $AppEvent_LogCopyWith<$Res> implements $AppEventCopyWith<$Res> {
  factory $AppEvent_LogCopyWith(AppEvent_Log value, $Res Function(AppEvent_Log) _then) = _$AppEvent_LogCopyWithImpl;
@useResult
$Res call({
 List<UiLogEntry> entries
});




}
/// @nodoc
class _$AppEvent_LogCopyWithImpl<$Res>
    implements $AppEvent_LogCopyWith<$Res> {
  _$AppEvent_LogCopyWithImpl(this._self, this._then);

  final AppEvent_Log _self;
  final $Res Function(AppEvent_Log) _then;

/// Create a copy of AppEvent
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? entries = null,}) {
  return _then(AppEvent_Log(
entries: null == entries ? _self._entries : entries // ignore: cast_nullable_to_non_nullable
as List<UiLogEntry>,
  ));
}


}

// dart format on

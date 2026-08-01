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

@optionalTypeArgs TResult maybeMap<TResult extends Object?>({TResult Function( AppEvent_Status value)?  status,TResult Function( AppEvent_Users value)?  users,TResult Function( AppEvent_Channels value)?  channels,TResult Function( AppEvent_Text value)?  text,TResult Function( AppEvent_Stats value)?  stats,TResult Function( AppEvent_InputLevel value)?  inputLevel,TResult Function( AppEvent_Certificate value)?  certificate,TResult Function( AppEvent_Welcome value)?  welcome,TResult Function( AppEvent_SelfSession value)?  selfSession,required TResult orElse(),}){
final _that = this;
switch (_that) {
case AppEvent_Status() when status != null:
return status(_that);case AppEvent_Users() when users != null:
return users(_that);case AppEvent_Channels() when channels != null:
return channels(_that);case AppEvent_Text() when text != null:
return text(_that);case AppEvent_Stats() when stats != null:
return stats(_that);case AppEvent_InputLevel() when inputLevel != null:
return inputLevel(_that);case AppEvent_Certificate() when certificate != null:
return certificate(_that);case AppEvent_Welcome() when welcome != null:
return welcome(_that);case AppEvent_SelfSession() when selfSession != null:
return selfSession(_that);case _:
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

@optionalTypeArgs TResult map<TResult extends Object?>({required TResult Function( AppEvent_Status value)  status,required TResult Function( AppEvent_Users value)  users,required TResult Function( AppEvent_Channels value)  channels,required TResult Function( AppEvent_Text value)  text,required TResult Function( AppEvent_Stats value)  stats,required TResult Function( AppEvent_InputLevel value)  inputLevel,required TResult Function( AppEvent_Certificate value)  certificate,required TResult Function( AppEvent_Welcome value)  welcome,required TResult Function( AppEvent_SelfSession value)  selfSession,}){
final _that = this;
switch (_that) {
case AppEvent_Status():
return status(_that);case AppEvent_Users():
return users(_that);case AppEvent_Channels():
return channels(_that);case AppEvent_Text():
return text(_that);case AppEvent_Stats():
return stats(_that);case AppEvent_InputLevel():
return inputLevel(_that);case AppEvent_Certificate():
return certificate(_that);case AppEvent_Welcome():
return welcome(_that);case AppEvent_SelfSession():
return selfSession(_that);}
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

@optionalTypeArgs TResult? mapOrNull<TResult extends Object?>({TResult? Function( AppEvent_Status value)?  status,TResult? Function( AppEvent_Users value)?  users,TResult? Function( AppEvent_Channels value)?  channels,TResult? Function( AppEvent_Text value)?  text,TResult? Function( AppEvent_Stats value)?  stats,TResult? Function( AppEvent_InputLevel value)?  inputLevel,TResult? Function( AppEvent_Certificate value)?  certificate,TResult? Function( AppEvent_Welcome value)?  welcome,TResult? Function( AppEvent_SelfSession value)?  selfSession,}){
final _that = this;
switch (_that) {
case AppEvent_Status() when status != null:
return status(_that);case AppEvent_Users() when users != null:
return users(_that);case AppEvent_Channels() when channels != null:
return channels(_that);case AppEvent_Text() when text != null:
return text(_that);case AppEvent_Stats() when stats != null:
return stats(_that);case AppEvent_InputLevel() when inputLevel != null:
return inputLevel(_that);case AppEvent_Certificate() when certificate != null:
return certificate(_that);case AppEvent_Welcome() when welcome != null:
return welcome(_that);case AppEvent_SelfSession() when selfSession != null:
return selfSession(_that);case _:
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

@optionalTypeArgs TResult maybeWhen<TResult extends Object?>({TResult Function( StatusUpdate field0)?  status,TResult Function( String serverId,  List<UiUser> users)?  users,TResult Function( String serverId,  List<UiChannel> channels)?  channels,TResult Function( String serverId,  String from,  String message)?  text,TResult Function( UiStats field0)?  stats,TResult Function( double levelDb,  bool speaking)?  inputLevel,TResult Function( String serverId,  String fingerprint,  bool changed)?  certificate,TResult Function( String serverId,  String text)?  welcome,TResult Function( String serverId,  int session)?  selfSession,required TResult orElse(),}) {final _that = this;
switch (_that) {
case AppEvent_Status() when status != null:
return status(_that.field0);case AppEvent_Users() when users != null:
return users(_that.serverId,_that.users);case AppEvent_Channels() when channels != null:
return channels(_that.serverId,_that.channels);case AppEvent_Text() when text != null:
return text(_that.serverId,_that.from,_that.message);case AppEvent_Stats() when stats != null:
return stats(_that.field0);case AppEvent_InputLevel() when inputLevel != null:
return inputLevel(_that.levelDb,_that.speaking);case AppEvent_Certificate() when certificate != null:
return certificate(_that.serverId,_that.fingerprint,_that.changed);case AppEvent_Welcome() when welcome != null:
return welcome(_that.serverId,_that.text);case AppEvent_SelfSession() when selfSession != null:
return selfSession(_that.serverId,_that.session);case _:
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

@optionalTypeArgs TResult when<TResult extends Object?>({required TResult Function( StatusUpdate field0)  status,required TResult Function( String serverId,  List<UiUser> users)  users,required TResult Function( String serverId,  List<UiChannel> channels)  channels,required TResult Function( String serverId,  String from,  String message)  text,required TResult Function( UiStats field0)  stats,required TResult Function( double levelDb,  bool speaking)  inputLevel,required TResult Function( String serverId,  String fingerprint,  bool changed)  certificate,required TResult Function( String serverId,  String text)  welcome,required TResult Function( String serverId,  int session)  selfSession,}) {final _that = this;
switch (_that) {
case AppEvent_Status():
return status(_that.field0);case AppEvent_Users():
return users(_that.serverId,_that.users);case AppEvent_Channels():
return channels(_that.serverId,_that.channels);case AppEvent_Text():
return text(_that.serverId,_that.from,_that.message);case AppEvent_Stats():
return stats(_that.field0);case AppEvent_InputLevel():
return inputLevel(_that.levelDb,_that.speaking);case AppEvent_Certificate():
return certificate(_that.serverId,_that.fingerprint,_that.changed);case AppEvent_Welcome():
return welcome(_that.serverId,_that.text);case AppEvent_SelfSession():
return selfSession(_that.serverId,_that.session);}
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

@optionalTypeArgs TResult? whenOrNull<TResult extends Object?>({TResult? Function( StatusUpdate field0)?  status,TResult? Function( String serverId,  List<UiUser> users)?  users,TResult? Function( String serverId,  List<UiChannel> channels)?  channels,TResult? Function( String serverId,  String from,  String message)?  text,TResult? Function( UiStats field0)?  stats,TResult? Function( double levelDb,  bool speaking)?  inputLevel,TResult? Function( String serverId,  String fingerprint,  bool changed)?  certificate,TResult? Function( String serverId,  String text)?  welcome,TResult? Function( String serverId,  int session)?  selfSession,}) {final _that = this;
switch (_that) {
case AppEvent_Status() when status != null:
return status(_that.field0);case AppEvent_Users() when users != null:
return users(_that.serverId,_that.users);case AppEvent_Channels() when channels != null:
return channels(_that.serverId,_that.channels);case AppEvent_Text() when text != null:
return text(_that.serverId,_that.from,_that.message);case AppEvent_Stats() when stats != null:
return stats(_that.field0);case AppEvent_InputLevel() when inputLevel != null:
return inputLevel(_that.levelDb,_that.speaking);case AppEvent_Certificate() when certificate != null:
return certificate(_that.serverId,_that.fingerprint,_that.changed);case AppEvent_Welcome() when welcome != null:
return welcome(_that.serverId,_that.text);case AppEvent_SelfSession() when selfSession != null:
return selfSession(_that.serverId,_that.session);case _:
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
  const AppEvent_InputLevel({required this.levelDb, required this.speaking}): super._();
  

 final  double levelDb;
 final  bool speaking;

/// Create a copy of AppEvent
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$AppEvent_InputLevelCopyWith<AppEvent_InputLevel> get copyWith => _$AppEvent_InputLevelCopyWithImpl<AppEvent_InputLevel>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is AppEvent_InputLevel&&(identical(other.levelDb, levelDb) || other.levelDb == levelDb)&&(identical(other.speaking, speaking) || other.speaking == speaking));
}


@override
int get hashCode => Object.hash(runtimeType,levelDb,speaking);

@override
String toString() {
  return 'AppEvent.inputLevel(levelDb: $levelDb, speaking: $speaking)';
}


}

/// @nodoc
abstract mixin class $AppEvent_InputLevelCopyWith<$Res> implements $AppEventCopyWith<$Res> {
  factory $AppEvent_InputLevelCopyWith(AppEvent_InputLevel value, $Res Function(AppEvent_InputLevel) _then) = _$AppEvent_InputLevelCopyWithImpl;
@useResult
$Res call({
 double levelDb, bool speaking
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
@pragma('vm:prefer-inline') $Res call({Object? levelDb = null,Object? speaking = null,}) {
  return _then(AppEvent_InputLevel(
levelDb: null == levelDb ? _self.levelDb : levelDb // ignore: cast_nullable_to_non_nullable
as double,speaking: null == speaking ? _self.speaking : speaking // ignore: cast_nullable_to_non_nullable
as bool,
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

// dart format on

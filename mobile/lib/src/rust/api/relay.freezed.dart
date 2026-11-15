// GENERATED CODE - DO NOT MODIFY BY HAND
// coverage:ignore-file
// ignore_for_file: type=lint, type=warning, deprecated_member_use, deprecated_member_use_from_same_package
// ignore_for_file: unused_element, deprecated_member_use, deprecated_member_use_from_same_package, use_function_type_syntax_for_parameters, unnecessary_const, avoid_init_to_null, invalid_override_different_default_values_named, prefer_expression_function_bodies, annotate_overrides, invalid_annotation_target, unnecessary_question_mark

part of 'relay.dart';

// **************************************************************************
// FreezedGenerator
// **************************************************************************

// GENERATED CODE - DO NOT MODIFY BY HAND
// dart format off
T _$identity<T>(T value) => value;
/// @nodoc
mixin _$OpenedPacket {





@override
bool operator ==(Object other) {
    return identical(this, other) || (other.runtimeType == runtimeType&&other is OpenedPacket);
}


@override
int get hashCode => runtimeType.hashCode;

@override
String toString() {
    return 'OpenedPacket()';
}


}

/// @nodoc
class $OpenedPacketCopyWith<$Res>  {
$OpenedPacketCopyWith(OpenedPacket _, $Res Function(OpenedPacket) __);
}


/// Adds pattern-matching-related methods to [OpenedPacket].
extension OpenedPacketPatterns on OpenedPacket {
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

@optionalTypeArgs TResult maybeMap<TResult extends Object?>({TResult Function( OpenedPacket_Message value)?  message,TResult Function( OpenedPacket_Dropped value)?  dropped,TResult Function( OpenedPacket_PeerMustUpdate value)?  peerMustUpdate,TResult Function( OpenedPacket_WeMustUpdate value)?  weMustUpdate,required TResult orElse(),}){
final _that = this;
switch (_that) {
case OpenedPacket_Message() when message != null:
return message(_that);case OpenedPacket_Dropped() when dropped != null:
return dropped(_that);case OpenedPacket_PeerMustUpdate() when peerMustUpdate != null:
return peerMustUpdate(_that);case OpenedPacket_WeMustUpdate() when weMustUpdate != null:
return weMustUpdate(_that);case _:
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

@optionalTypeArgs TResult map<TResult extends Object?>({required TResult Function( OpenedPacket_Message value)  message,required TResult Function( OpenedPacket_Dropped value)  dropped,required TResult Function( OpenedPacket_PeerMustUpdate value)  peerMustUpdate,required TResult Function( OpenedPacket_WeMustUpdate value)  weMustUpdate,}){
final _that = this;
switch (_that) {
case OpenedPacket_Message():
return message(_that);case OpenedPacket_Dropped():
return dropped(_that);case OpenedPacket_PeerMustUpdate():
return peerMustUpdate(_that);case OpenedPacket_WeMustUpdate():
return weMustUpdate(_that);}
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

@optionalTypeArgs TResult? mapOrNull<TResult extends Object?>({TResult? Function( OpenedPacket_Message value)?  message,TResult? Function( OpenedPacket_Dropped value)?  dropped,TResult? Function( OpenedPacket_PeerMustUpdate value)?  peerMustUpdate,TResult? Function( OpenedPacket_WeMustUpdate value)?  weMustUpdate,}){
final _that = this;
switch (_that) {
case OpenedPacket_Message() when message != null:
return message(_that);case OpenedPacket_Dropped() when dropped != null:
return dropped(_that);case OpenedPacket_PeerMustUpdate() when peerMustUpdate != null:
return peerMustUpdate(_that);case OpenedPacket_WeMustUpdate() when weMustUpdate != null:
return weMustUpdate(_that);case _:
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

@optionalTypeArgs TResult maybeWhen<TResult extends Object?>({TResult Function( DesktopMessage field0)?  message,TResult Function()?  dropped,TResult Function()?  peerMustUpdate,TResult Function()?  weMustUpdate,required TResult orElse(),}) {final _that = this;
switch (_that) {
case OpenedPacket_Message() when message != null:
return message(_that.field0);case OpenedPacket_Dropped() when dropped != null:
return dropped();case OpenedPacket_PeerMustUpdate() when peerMustUpdate != null:
return peerMustUpdate();case OpenedPacket_WeMustUpdate() when weMustUpdate != null:
return weMustUpdate();case _:
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

@optionalTypeArgs TResult when<TResult extends Object?>({required TResult Function( DesktopMessage field0)  message,required TResult Function()  dropped,required TResult Function()  peerMustUpdate,required TResult Function()  weMustUpdate,}) {final _that = this;
switch (_that) {
case OpenedPacket_Message():
return message(_that.field0);case OpenedPacket_Dropped():
return dropped();case OpenedPacket_PeerMustUpdate():
return peerMustUpdate();case OpenedPacket_WeMustUpdate():
return weMustUpdate();}
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

@optionalTypeArgs TResult? whenOrNull<TResult extends Object?>({TResult? Function( DesktopMessage field0)?  message,TResult? Function()?  dropped,TResult? Function()?  peerMustUpdate,TResult? Function()?  weMustUpdate,}) {final _that = this;
switch (_that) {
case OpenedPacket_Message() when message != null:
return message(_that.field0);case OpenedPacket_Dropped() when dropped != null:
return dropped();case OpenedPacket_PeerMustUpdate() when peerMustUpdate != null:
return peerMustUpdate();case OpenedPacket_WeMustUpdate() when weMustUpdate != null:
return weMustUpdate();case _:
  return null;

}
}

}

/// @nodoc


class OpenedPacket_Message extends OpenedPacket {
  const OpenedPacket_Message(this.field0): super._();
  

 final  DesktopMessage field0;

/// Create a copy of OpenedPacket
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$OpenedPacket_MessageCopyWith<OpenedPacket_Message> get copyWith => _$OpenedPacket_MessageCopyWithImpl<OpenedPacket_Message>(this, _$identity);



@override
bool operator ==(Object other) {
    return identical(this, other) || (other.runtimeType == runtimeType&&other is OpenedPacket_Message&&(identical(other.field0, field0) || other.field0 == field0));
}


@override
int get hashCode {
    return Object.hash(runtimeType,field0);
}

@override
String toString() {
    return 'OpenedPacket.message(field0: $field0)';
}


}

/// @nodoc
abstract mixin class $OpenedPacket_MessageCopyWith<$Res> implements $OpenedPacketCopyWith<$Res> {
  factory $OpenedPacket_MessageCopyWith(OpenedPacket_Message value, $Res Function(OpenedPacket_Message) _then) = _$OpenedPacket_MessageCopyWithImpl;
@useResult
$Res call({
 DesktopMessage field0
});




}
/// @nodoc
class _$OpenedPacket_MessageCopyWithImpl<$Res>
    implements $OpenedPacket_MessageCopyWith<$Res> {
  _$OpenedPacket_MessageCopyWithImpl(this._self, this._then);

  final OpenedPacket_Message _self;
  final $Res Function(OpenedPacket_Message) _then;

/// Create a copy of OpenedPacket
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? field0 = null,}) {
  return _then(OpenedPacket_Message(
null == field0 ? _self.field0 : field0 // ignore: cast_nullable_to_non_nullable
as DesktopMessage,
  ));
}


}

/// @nodoc


class OpenedPacket_Dropped extends OpenedPacket {
  const OpenedPacket_Dropped(): super._();
  






@override
bool operator ==(Object other) {
    return identical(this, other) || (other.runtimeType == runtimeType&&other is OpenedPacket_Dropped);
}


@override
int get hashCode => runtimeType.hashCode;

@override
String toString() {
    return 'OpenedPacket.dropped()';
}


}




/// @nodoc


class OpenedPacket_PeerMustUpdate extends OpenedPacket {
  const OpenedPacket_PeerMustUpdate(): super._();
  






@override
bool operator ==(Object other) {
    return identical(this, other) || (other.runtimeType == runtimeType&&other is OpenedPacket_PeerMustUpdate);
}


@override
int get hashCode => runtimeType.hashCode;

@override
String toString() {
    return 'OpenedPacket.peerMustUpdate()';
}


}




/// @nodoc


class OpenedPacket_WeMustUpdate extends OpenedPacket {
  const OpenedPacket_WeMustUpdate(): super._();
  






@override
bool operator ==(Object other) {
    return identical(this, other) || (other.runtimeType == runtimeType&&other is OpenedPacket_WeMustUpdate);
}


@override
int get hashCode => runtimeType.hashCode;

@override
String toString() {
    return 'OpenedPacket.weMustUpdate()';
}


}




/// @nodoc
mixin _$RelayFrame {





@override
bool operator ==(Object other) {
    return identical(this, other) || (other.runtimeType == runtimeType&&other is RelayFrame);
}


@override
int get hashCode => runtimeType.hashCode;

@override
String toString() {
    return 'RelayFrame()';
}


}

/// @nodoc
class $RelayFrameCopyWith<$Res>  {
$RelayFrameCopyWith(RelayFrame _, $Res Function(RelayFrame) __);
}


/// Adds pattern-matching-related methods to [RelayFrame].
extension RelayFramePatterns on RelayFrame {
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

@optionalTypeArgs TResult maybeMap<TResult extends Object?>({TResult Function( RelayFrame_Challenge value)?  challenge,TResult Function( RelayFrame_Msg value)?  msg,TResult Function( RelayFrame_Presence value)?  presence,TResult Function( RelayFrame_Error value)?  error,TResult Function( RelayFrame_Queued value)?  queued,TResult Function( RelayFrame_Delivered value)?  delivered,TResult Function( RelayFrame_Dropped value)?  dropped,TResult Function( RelayFrame_Other value)?  other,required TResult orElse(),}){
final _that = this;
switch (_that) {
case RelayFrame_Challenge() when challenge != null:
return challenge(_that);case RelayFrame_Msg() when msg != null:
return msg(_that);case RelayFrame_Presence() when presence != null:
return presence(_that);case RelayFrame_Error() when error != null:
return error(_that);case RelayFrame_Queued() when queued != null:
return queued(_that);case RelayFrame_Delivered() when delivered != null:
return delivered(_that);case RelayFrame_Dropped() when dropped != null:
return dropped(_that);case RelayFrame_Other() when other != null:
return other(_that);case _:
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

@optionalTypeArgs TResult map<TResult extends Object?>({required TResult Function( RelayFrame_Challenge value)  challenge,required TResult Function( RelayFrame_Msg value)  msg,required TResult Function( RelayFrame_Presence value)  presence,required TResult Function( RelayFrame_Error value)  error,required TResult Function( RelayFrame_Queued value)  queued,required TResult Function( RelayFrame_Delivered value)  delivered,required TResult Function( RelayFrame_Dropped value)  dropped,required TResult Function( RelayFrame_Other value)  other,}){
final _that = this;
switch (_that) {
case RelayFrame_Challenge():
return challenge(_that);case RelayFrame_Msg():
return msg(_that);case RelayFrame_Presence():
return presence(_that);case RelayFrame_Error():
return error(_that);case RelayFrame_Queued():
return queued(_that);case RelayFrame_Delivered():
return delivered(_that);case RelayFrame_Dropped():
return dropped(_that);case RelayFrame_Other():
return other(_that);}
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

@optionalTypeArgs TResult? mapOrNull<TResult extends Object?>({TResult? Function( RelayFrame_Challenge value)?  challenge,TResult? Function( RelayFrame_Msg value)?  msg,TResult? Function( RelayFrame_Presence value)?  presence,TResult? Function( RelayFrame_Error value)?  error,TResult? Function( RelayFrame_Queued value)?  queued,TResult? Function( RelayFrame_Delivered value)?  delivered,TResult? Function( RelayFrame_Dropped value)?  dropped,TResult? Function( RelayFrame_Other value)?  other,}){
final _that = this;
switch (_that) {
case RelayFrame_Challenge() when challenge != null:
return challenge(_that);case RelayFrame_Msg() when msg != null:
return msg(_that);case RelayFrame_Presence() when presence != null:
return presence(_that);case RelayFrame_Error() when error != null:
return error(_that);case RelayFrame_Queued() when queued != null:
return queued(_that);case RelayFrame_Delivered() when delivered != null:
return delivered(_that);case RelayFrame_Dropped() when dropped != null:
return dropped(_that);case RelayFrame_Other() when other != null:
return other(_that);case _:
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

@optionalTypeArgs TResult maybeWhen<TResult extends Object?>({TResult Function( String nonce)?  challenge,TResult Function( String pid,  String from,  Uint8List packet)?  msg,TResult Function( String peer,  PeerState state)?  presence,TResult Function( RelayError code,  RelaySide? who)?  error,TResult Function( String pid)?  queued,TResult Function( String pid)?  delivered,TResult Function( String pid)?  dropped,TResult Function()?  other,required TResult orElse(),}) {final _that = this;
switch (_that) {
case RelayFrame_Challenge() when challenge != null:
return challenge(_that.nonce);case RelayFrame_Msg() when msg != null:
return msg(_that.pid,_that.from,_that.packet);case RelayFrame_Presence() when presence != null:
return presence(_that.peer,_that.state);case RelayFrame_Error() when error != null:
return error(_that.code,_that.who);case RelayFrame_Queued() when queued != null:
return queued(_that.pid);case RelayFrame_Delivered() when delivered != null:
return delivered(_that.pid);case RelayFrame_Dropped() when dropped != null:
return dropped(_that.pid);case RelayFrame_Other() when other != null:
return other();case _:
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

@optionalTypeArgs TResult when<TResult extends Object?>({required TResult Function( String nonce)  challenge,required TResult Function( String pid,  String from,  Uint8List packet)  msg,required TResult Function( String peer,  PeerState state)  presence,required TResult Function( RelayError code,  RelaySide? who)  error,required TResult Function( String pid)  queued,required TResult Function( String pid)  delivered,required TResult Function( String pid)  dropped,required TResult Function()  other,}) {final _that = this;
switch (_that) {
case RelayFrame_Challenge():
return challenge(_that.nonce);case RelayFrame_Msg():
return msg(_that.pid,_that.from,_that.packet);case RelayFrame_Presence():
return presence(_that.peer,_that.state);case RelayFrame_Error():
return error(_that.code,_that.who);case RelayFrame_Queued():
return queued(_that.pid);case RelayFrame_Delivered():
return delivered(_that.pid);case RelayFrame_Dropped():
return dropped(_that.pid);case RelayFrame_Other():
return other();}
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

@optionalTypeArgs TResult? whenOrNull<TResult extends Object?>({TResult? Function( String nonce)?  challenge,TResult? Function( String pid,  String from,  Uint8List packet)?  msg,TResult? Function( String peer,  PeerState state)?  presence,TResult? Function( RelayError code,  RelaySide? who)?  error,TResult? Function( String pid)?  queued,TResult? Function( String pid)?  delivered,TResult? Function( String pid)?  dropped,TResult? Function()?  other,}) {final _that = this;
switch (_that) {
case RelayFrame_Challenge() when challenge != null:
return challenge(_that.nonce);case RelayFrame_Msg() when msg != null:
return msg(_that.pid,_that.from,_that.packet);case RelayFrame_Presence() when presence != null:
return presence(_that.peer,_that.state);case RelayFrame_Error() when error != null:
return error(_that.code,_that.who);case RelayFrame_Queued() when queued != null:
return queued(_that.pid);case RelayFrame_Delivered() when delivered != null:
return delivered(_that.pid);case RelayFrame_Dropped() when dropped != null:
return dropped(_that.pid);case RelayFrame_Other() when other != null:
return other();case _:
  return null;

}
}

}

/// @nodoc


class RelayFrame_Challenge extends RelayFrame {
  const RelayFrame_Challenge({required this.nonce}): super._();
  

 final  String nonce;

/// Create a copy of RelayFrame
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$RelayFrame_ChallengeCopyWith<RelayFrame_Challenge> get copyWith => _$RelayFrame_ChallengeCopyWithImpl<RelayFrame_Challenge>(this, _$identity);



@override
bool operator ==(Object other) {
    return identical(this, other) || (other.runtimeType == runtimeType&&other is RelayFrame_Challenge&&(identical(other.nonce, nonce) || other.nonce == nonce));
}


@override
int get hashCode {
    return Object.hash(runtimeType,nonce);
}

@override
String toString() {
    return 'RelayFrame.challenge(nonce: $nonce)';
}


}

/// @nodoc
abstract mixin class $RelayFrame_ChallengeCopyWith<$Res> implements $RelayFrameCopyWith<$Res> {
  factory $RelayFrame_ChallengeCopyWith(RelayFrame_Challenge value, $Res Function(RelayFrame_Challenge) _then) = _$RelayFrame_ChallengeCopyWithImpl;
@useResult
$Res call({
 String nonce
});




}
/// @nodoc
class _$RelayFrame_ChallengeCopyWithImpl<$Res>
    implements $RelayFrame_ChallengeCopyWith<$Res> {
  _$RelayFrame_ChallengeCopyWithImpl(this._self, this._then);

  final RelayFrame_Challenge _self;
  final $Res Function(RelayFrame_Challenge) _then;

/// Create a copy of RelayFrame
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? nonce = null,}) {
  return _then(RelayFrame_Challenge(
nonce: null == nonce ? _self.nonce : nonce // ignore: cast_nullable_to_non_nullable
as String,
  ));
}


}

/// @nodoc


class RelayFrame_Msg extends RelayFrame {
  const RelayFrame_Msg({required this.pid, required this.from, required this.packet}): super._();
  

 final  String pid;
 final  String from;
 final  Uint8List packet;

/// Create a copy of RelayFrame
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$RelayFrame_MsgCopyWith<RelayFrame_Msg> get copyWith => _$RelayFrame_MsgCopyWithImpl<RelayFrame_Msg>(this, _$identity);



@override
bool operator ==(Object other) {
    return identical(this, other) || (other.runtimeType == runtimeType&&other is RelayFrame_Msg&&(identical(other.pid, pid) || other.pid == pid)&&(identical(other.from, from) || other.from == from)&&const DeepCollectionEquality().equals(other.packet, packet));
}


@override
int get hashCode {
    return Object.hash(runtimeType,pid,from,const DeepCollectionEquality().hash(packet));
}

@override
String toString() {
    return 'RelayFrame.msg(pid: $pid, from: $from, packet: $packet)';
}


}

/// @nodoc
abstract mixin class $RelayFrame_MsgCopyWith<$Res> implements $RelayFrameCopyWith<$Res> {
  factory $RelayFrame_MsgCopyWith(RelayFrame_Msg value, $Res Function(RelayFrame_Msg) _then) = _$RelayFrame_MsgCopyWithImpl;
@useResult
$Res call({
 String pid, String from, Uint8List packet
});




}
/// @nodoc
class _$RelayFrame_MsgCopyWithImpl<$Res>
    implements $RelayFrame_MsgCopyWith<$Res> {
  _$RelayFrame_MsgCopyWithImpl(this._self, this._then);

  final RelayFrame_Msg _self;
  final $Res Function(RelayFrame_Msg) _then;

/// Create a copy of RelayFrame
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? pid = null,Object? from = null,Object? packet = null,}) {
  return _then(RelayFrame_Msg(
pid: null == pid ? _self.pid : pid // ignore: cast_nullable_to_non_nullable
as String,from: null == from ? _self.from : from // ignore: cast_nullable_to_non_nullable
as String,packet: null == packet ? _self.packet : packet // ignore: cast_nullable_to_non_nullable
as Uint8List,
  ));
}


}

/// @nodoc


class RelayFrame_Presence extends RelayFrame {
  const RelayFrame_Presence({required this.peer, required this.state}): super._();
  

 final  String peer;
 final  PeerState state;

/// Create a copy of RelayFrame
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$RelayFrame_PresenceCopyWith<RelayFrame_Presence> get copyWith => _$RelayFrame_PresenceCopyWithImpl<RelayFrame_Presence>(this, _$identity);



@override
bool operator ==(Object other) {
    return identical(this, other) || (other.runtimeType == runtimeType&&other is RelayFrame_Presence&&(identical(other.peer, peer) || other.peer == peer)&&(identical(other.state, state) || other.state == state));
}


@override
int get hashCode {
    return Object.hash(runtimeType,peer,state);
}

@override
String toString() {
    return 'RelayFrame.presence(peer: $peer, state: $state)';
}


}

/// @nodoc
abstract mixin class $RelayFrame_PresenceCopyWith<$Res> implements $RelayFrameCopyWith<$Res> {
  factory $RelayFrame_PresenceCopyWith(RelayFrame_Presence value, $Res Function(RelayFrame_Presence) _then) = _$RelayFrame_PresenceCopyWithImpl;
@useResult
$Res call({
 String peer, PeerState state
});




}
/// @nodoc
class _$RelayFrame_PresenceCopyWithImpl<$Res>
    implements $RelayFrame_PresenceCopyWith<$Res> {
  _$RelayFrame_PresenceCopyWithImpl(this._self, this._then);

  final RelayFrame_Presence _self;
  final $Res Function(RelayFrame_Presence) _then;

/// Create a copy of RelayFrame
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? peer = null,Object? state = null,}) {
  return _then(RelayFrame_Presence(
peer: null == peer ? _self.peer : peer // ignore: cast_nullable_to_non_nullable
as String,state: null == state ? _self.state : state // ignore: cast_nullable_to_non_nullable
as PeerState,
  ));
}


}

/// @nodoc


class RelayFrame_Error extends RelayFrame {
  const RelayFrame_Error({required this.code, this.who}): super._();
  

 final  RelayError code;
 final  RelaySide? who;

/// Create a copy of RelayFrame
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$RelayFrame_ErrorCopyWith<RelayFrame_Error> get copyWith => _$RelayFrame_ErrorCopyWithImpl<RelayFrame_Error>(this, _$identity);



@override
bool operator ==(Object other) {
    return identical(this, other) || (other.runtimeType == runtimeType&&other is RelayFrame_Error&&(identical(other.code, code) || other.code == code)&&(identical(other.who, who) || other.who == who));
}


@override
int get hashCode {
    return Object.hash(runtimeType,code,who);
}

@override
String toString() {
    return 'RelayFrame.error(code: $code, who: $who)';
}


}

/// @nodoc
abstract mixin class $RelayFrame_ErrorCopyWith<$Res> implements $RelayFrameCopyWith<$Res> {
  factory $RelayFrame_ErrorCopyWith(RelayFrame_Error value, $Res Function(RelayFrame_Error) _then) = _$RelayFrame_ErrorCopyWithImpl;
@useResult
$Res call({
 RelayError code, RelaySide? who
});




}
/// @nodoc
class _$RelayFrame_ErrorCopyWithImpl<$Res>
    implements $RelayFrame_ErrorCopyWith<$Res> {
  _$RelayFrame_ErrorCopyWithImpl(this._self, this._then);

  final RelayFrame_Error _self;
  final $Res Function(RelayFrame_Error) _then;

/// Create a copy of RelayFrame
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? code = null,Object? who = freezed,}) {
  return _then(RelayFrame_Error(
code: null == code ? _self.code : code // ignore: cast_nullable_to_non_nullable
as RelayError,who: freezed == who ? _self.who : who // ignore: cast_nullable_to_non_nullable
as RelaySide?,
  ));
}


}

/// @nodoc


class RelayFrame_Queued extends RelayFrame {
  const RelayFrame_Queued({required this.pid}): super._();
  

 final  String pid;

/// Create a copy of RelayFrame
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$RelayFrame_QueuedCopyWith<RelayFrame_Queued> get copyWith => _$RelayFrame_QueuedCopyWithImpl<RelayFrame_Queued>(this, _$identity);



@override
bool operator ==(Object other) {
    return identical(this, other) || (other.runtimeType == runtimeType&&other is RelayFrame_Queued&&(identical(other.pid, pid) || other.pid == pid));
}


@override
int get hashCode {
    return Object.hash(runtimeType,pid);
}

@override
String toString() {
    return 'RelayFrame.queued(pid: $pid)';
}


}

/// @nodoc
abstract mixin class $RelayFrame_QueuedCopyWith<$Res> implements $RelayFrameCopyWith<$Res> {
  factory $RelayFrame_QueuedCopyWith(RelayFrame_Queued value, $Res Function(RelayFrame_Queued) _then) = _$RelayFrame_QueuedCopyWithImpl;
@useResult
$Res call({
 String pid
});




}
/// @nodoc
class _$RelayFrame_QueuedCopyWithImpl<$Res>
    implements $RelayFrame_QueuedCopyWith<$Res> {
  _$RelayFrame_QueuedCopyWithImpl(this._self, this._then);

  final RelayFrame_Queued _self;
  final $Res Function(RelayFrame_Queued) _then;

/// Create a copy of RelayFrame
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? pid = null,}) {
  return _then(RelayFrame_Queued(
pid: null == pid ? _self.pid : pid // ignore: cast_nullable_to_non_nullable
as String,
  ));
}


}

/// @nodoc


class RelayFrame_Delivered extends RelayFrame {
  const RelayFrame_Delivered({required this.pid}): super._();
  

 final  String pid;

/// Create a copy of RelayFrame
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$RelayFrame_DeliveredCopyWith<RelayFrame_Delivered> get copyWith => _$RelayFrame_DeliveredCopyWithImpl<RelayFrame_Delivered>(this, _$identity);



@override
bool operator ==(Object other) {
    return identical(this, other) || (other.runtimeType == runtimeType&&other is RelayFrame_Delivered&&(identical(other.pid, pid) || other.pid == pid));
}


@override
int get hashCode {
    return Object.hash(runtimeType,pid);
}

@override
String toString() {
    return 'RelayFrame.delivered(pid: $pid)';
}


}

/// @nodoc
abstract mixin class $RelayFrame_DeliveredCopyWith<$Res> implements $RelayFrameCopyWith<$Res> {
  factory $RelayFrame_DeliveredCopyWith(RelayFrame_Delivered value, $Res Function(RelayFrame_Delivered) _then) = _$RelayFrame_DeliveredCopyWithImpl;
@useResult
$Res call({
 String pid
});




}
/// @nodoc
class _$RelayFrame_DeliveredCopyWithImpl<$Res>
    implements $RelayFrame_DeliveredCopyWith<$Res> {
  _$RelayFrame_DeliveredCopyWithImpl(this._self, this._then);

  final RelayFrame_Delivered _self;
  final $Res Function(RelayFrame_Delivered) _then;

/// Create a copy of RelayFrame
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? pid = null,}) {
  return _then(RelayFrame_Delivered(
pid: null == pid ? _self.pid : pid // ignore: cast_nullable_to_non_nullable
as String,
  ));
}


}

/// @nodoc


class RelayFrame_Dropped extends RelayFrame {
  const RelayFrame_Dropped({required this.pid}): super._();
  

 final  String pid;

/// Create a copy of RelayFrame
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$RelayFrame_DroppedCopyWith<RelayFrame_Dropped> get copyWith => _$RelayFrame_DroppedCopyWithImpl<RelayFrame_Dropped>(this, _$identity);



@override
bool operator ==(Object other) {
    return identical(this, other) || (other.runtimeType == runtimeType&&other is RelayFrame_Dropped&&(identical(other.pid, pid) || other.pid == pid));
}


@override
int get hashCode {
    return Object.hash(runtimeType,pid);
}

@override
String toString() {
    return 'RelayFrame.dropped(pid: $pid)';
}


}

/// @nodoc
abstract mixin class $RelayFrame_DroppedCopyWith<$Res> implements $RelayFrameCopyWith<$Res> {
  factory $RelayFrame_DroppedCopyWith(RelayFrame_Dropped value, $Res Function(RelayFrame_Dropped) _then) = _$RelayFrame_DroppedCopyWithImpl;
@useResult
$Res call({
 String pid
});




}
/// @nodoc
class _$RelayFrame_DroppedCopyWithImpl<$Res>
    implements $RelayFrame_DroppedCopyWith<$Res> {
  _$RelayFrame_DroppedCopyWithImpl(this._self, this._then);

  final RelayFrame_Dropped _self;
  final $Res Function(RelayFrame_Dropped) _then;

/// Create a copy of RelayFrame
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? pid = null,}) {
  return _then(RelayFrame_Dropped(
pid: null == pid ? _self.pid : pid // ignore: cast_nullable_to_non_nullable
as String,
  ));
}


}

/// @nodoc


class RelayFrame_Other extends RelayFrame {
  const RelayFrame_Other(): super._();
  






@override
bool operator ==(Object other) {
    return identical(this, other) || (other.runtimeType == runtimeType&&other is RelayFrame_Other);
}


@override
int get hashCode => runtimeType.hashCode;

@override
String toString() {
    return 'RelayFrame.other()';
}


}




// dart format on

// GENERATED CODE - DO NOT MODIFY BY HAND
// coverage:ignore-file
// ignore_for_file: type=lint, type=warning, deprecated_member_use, deprecated_member_use_from_same_package
// ignore_for_file: unused_element, deprecated_member_use, deprecated_member_use_from_same_package, use_function_type_syntax_for_parameters, unnecessary_const, avoid_init_to_null, invalid_override_different_default_values_named, prefer_expression_function_bodies, annotate_overrides, invalid_annotation_target, unnecessary_question_mark

part of 'messages.dart';

// **************************************************************************
// FreezedGenerator
// **************************************************************************

// GENERATED CODE - DO NOT MODIFY BY HAND
// dart format off
T _$identity<T>(T value) => value;
/// @nodoc
mixin _$CommandInput {





@override
bool operator ==(Object other) {
    return identical(this, other) || (other.runtimeType == runtimeType&&other is CommandInput);
}


@override
int get hashCode => runtimeType.hashCode;

@override
String toString() {
    return 'CommandInput()';
}


}

/// @nodoc
class $CommandInputCopyWith<$Res>  {
$CommandInputCopyWith(CommandInput _, $Res Function(CommandInput) __);
}


/// Adds pattern-matching-related methods to [CommandInput].
extension CommandInputPatterns on CommandInput {
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

@optionalTypeArgs TResult maybeMap<TResult extends Object?>({TResult Function( CommandInput_Answer value)?  answer,TResult Function( CommandInput_Plan value)?  plan,TResult Function( CommandInput_Question value)?  question,TResult Function( CommandInput_Start value)?  start,TResult Function( CommandInput_Stop value)?  stop,required TResult orElse(),}){
final _that = this;
switch (_that) {
case CommandInput_Answer() when answer != null:
return answer(_that);case CommandInput_Plan() when plan != null:
return plan(_that);case CommandInput_Question() when question != null:
return question(_that);case CommandInput_Start() when start != null:
return start(_that);case CommandInput_Stop() when stop != null:
return stop(_that);case _:
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

@optionalTypeArgs TResult map<TResult extends Object?>({required TResult Function( CommandInput_Answer value)  answer,required TResult Function( CommandInput_Plan value)  plan,required TResult Function( CommandInput_Question value)  question,required TResult Function( CommandInput_Start value)  start,required TResult Function( CommandInput_Stop value)  stop,}){
final _that = this;
switch (_that) {
case CommandInput_Answer():
return answer(_that);case CommandInput_Plan():
return plan(_that);case CommandInput_Question():
return question(_that);case CommandInput_Start():
return start(_that);case CommandInput_Stop():
return stop(_that);}
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

@optionalTypeArgs TResult? mapOrNull<TResult extends Object?>({TResult? Function( CommandInput_Answer value)?  answer,TResult? Function( CommandInput_Plan value)?  plan,TResult? Function( CommandInput_Question value)?  question,TResult? Function( CommandInput_Start value)?  start,TResult? Function( CommandInput_Stop value)?  stop,}){
final _that = this;
switch (_that) {
case CommandInput_Answer() when answer != null:
return answer(_that);case CommandInput_Plan() when plan != null:
return plan(_that);case CommandInput_Question() when question != null:
return question(_that);case CommandInput_Start() when start != null:
return start(_that);case CommandInput_Stop() when stop != null:
return stop(_that);case _:
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

@optionalTypeArgs TResult maybeWhen<TResult extends Object?>({TResult Function( String id,  bool allow,  String shown)?  answer,TResult Function( String id,  bool approve,  String? note,  String shown)?  plan,TResult Function( String id,  List<String> answers,  String shown)?  question,TResult Function( String project,  String iid,  StartMode mode,  String promptHash)?  start,TResult Function( String session,  String request)?  stop,required TResult orElse(),}) {final _that = this;
switch (_that) {
case CommandInput_Answer() when answer != null:
return answer(_that.id,_that.allow,_that.shown);case CommandInput_Plan() when plan != null:
return plan(_that.id,_that.approve,_that.note,_that.shown);case CommandInput_Question() when question != null:
return question(_that.id,_that.answers,_that.shown);case CommandInput_Start() when start != null:
return start(_that.project,_that.iid,_that.mode,_that.promptHash);case CommandInput_Stop() when stop != null:
return stop(_that.session,_that.request);case _:
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

@optionalTypeArgs TResult when<TResult extends Object?>({required TResult Function( String id,  bool allow,  String shown)  answer,required TResult Function( String id,  bool approve,  String? note,  String shown)  plan,required TResult Function( String id,  List<String> answers,  String shown)  question,required TResult Function( String project,  String iid,  StartMode mode,  String promptHash)  start,required TResult Function( String session,  String request)  stop,}) {final _that = this;
switch (_that) {
case CommandInput_Answer():
return answer(_that.id,_that.allow,_that.shown);case CommandInput_Plan():
return plan(_that.id,_that.approve,_that.note,_that.shown);case CommandInput_Question():
return question(_that.id,_that.answers,_that.shown);case CommandInput_Start():
return start(_that.project,_that.iid,_that.mode,_that.promptHash);case CommandInput_Stop():
return stop(_that.session,_that.request);}
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

@optionalTypeArgs TResult? whenOrNull<TResult extends Object?>({TResult? Function( String id,  bool allow,  String shown)?  answer,TResult? Function( String id,  bool approve,  String? note,  String shown)?  plan,TResult? Function( String id,  List<String> answers,  String shown)?  question,TResult? Function( String project,  String iid,  StartMode mode,  String promptHash)?  start,TResult? Function( String session,  String request)?  stop,}) {final _that = this;
switch (_that) {
case CommandInput_Answer() when answer != null:
return answer(_that.id,_that.allow,_that.shown);case CommandInput_Plan() when plan != null:
return plan(_that.id,_that.approve,_that.note,_that.shown);case CommandInput_Question() when question != null:
return question(_that.id,_that.answers,_that.shown);case CommandInput_Start() when start != null:
return start(_that.project,_that.iid,_that.mode,_that.promptHash);case CommandInput_Stop() when stop != null:
return stop(_that.session,_that.request);case _:
  return null;

}
}

}

/// @nodoc


class CommandInput_Answer extends CommandInput {
  const CommandInput_Answer({required this.id, required this.allow, required this.shown}): super._();
  

 final  String id;
 final  bool allow;
 final  String shown;

/// Create a copy of CommandInput
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$CommandInput_AnswerCopyWith<CommandInput_Answer> get copyWith => _$CommandInput_AnswerCopyWithImpl<CommandInput_Answer>(this, _$identity);



@override
bool operator ==(Object other) {
    return identical(this, other) || (other.runtimeType == runtimeType&&other is CommandInput_Answer&&(identical(other.id, id) || other.id == id)&&(identical(other.allow, allow) || other.allow == allow)&&(identical(other.shown, shown) || other.shown == shown));
}


@override
int get hashCode {
    return Object.hash(runtimeType,id,allow,shown);
}

@override
String toString() {
    return 'CommandInput.answer(id: $id, allow: $allow, shown: $shown)';
}


}

/// @nodoc
abstract mixin class $CommandInput_AnswerCopyWith<$Res> implements $CommandInputCopyWith<$Res> {
  factory $CommandInput_AnswerCopyWith(CommandInput_Answer value, $Res Function(CommandInput_Answer) _then) = _$CommandInput_AnswerCopyWithImpl;
@useResult
$Res call({
 String id, bool allow, String shown
});




}
/// @nodoc
class _$CommandInput_AnswerCopyWithImpl<$Res>
    implements $CommandInput_AnswerCopyWith<$Res> {
  _$CommandInput_AnswerCopyWithImpl(this._self, this._then);

  final CommandInput_Answer _self;
  final $Res Function(CommandInput_Answer) _then;

/// Create a copy of CommandInput
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? id = null,Object? allow = null,Object? shown = null,}) {
  return _then(CommandInput_Answer(
id: null == id ? _self.id : id // ignore: cast_nullable_to_non_nullable
as String,allow: null == allow ? _self.allow : allow // ignore: cast_nullable_to_non_nullable
as bool,shown: null == shown ? _self.shown : shown // ignore: cast_nullable_to_non_nullable
as String,
  ));
}


}

/// @nodoc


class CommandInput_Plan extends CommandInput {
  const CommandInput_Plan({required this.id, required this.approve, this.note, required this.shown}): super._();
  

 final  String id;
 final  bool approve;
 final  String? note;
 final  String shown;

/// Create a copy of CommandInput
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$CommandInput_PlanCopyWith<CommandInput_Plan> get copyWith => _$CommandInput_PlanCopyWithImpl<CommandInput_Plan>(this, _$identity);



@override
bool operator ==(Object other) {
    return identical(this, other) || (other.runtimeType == runtimeType&&other is CommandInput_Plan&&(identical(other.id, id) || other.id == id)&&(identical(other.approve, approve) || other.approve == approve)&&(identical(other.note, note) || other.note == note)&&(identical(other.shown, shown) || other.shown == shown));
}


@override
int get hashCode {
    return Object.hash(runtimeType,id,approve,note,shown);
}

@override
String toString() {
    return 'CommandInput.plan(id: $id, approve: $approve, note: $note, shown: $shown)';
}


}

/// @nodoc
abstract mixin class $CommandInput_PlanCopyWith<$Res> implements $CommandInputCopyWith<$Res> {
  factory $CommandInput_PlanCopyWith(CommandInput_Plan value, $Res Function(CommandInput_Plan) _then) = _$CommandInput_PlanCopyWithImpl;
@useResult
$Res call({
 String id, bool approve, String? note, String shown
});




}
/// @nodoc
class _$CommandInput_PlanCopyWithImpl<$Res>
    implements $CommandInput_PlanCopyWith<$Res> {
  _$CommandInput_PlanCopyWithImpl(this._self, this._then);

  final CommandInput_Plan _self;
  final $Res Function(CommandInput_Plan) _then;

/// Create a copy of CommandInput
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? id = null,Object? approve = null,Object? note = freezed,Object? shown = null,}) {
  return _then(CommandInput_Plan(
id: null == id ? _self.id : id // ignore: cast_nullable_to_non_nullable
as String,approve: null == approve ? _self.approve : approve // ignore: cast_nullable_to_non_nullable
as bool,note: freezed == note ? _self.note : note // ignore: cast_nullable_to_non_nullable
as String?,shown: null == shown ? _self.shown : shown // ignore: cast_nullable_to_non_nullable
as String,
  ));
}


}

/// @nodoc


class CommandInput_Question extends CommandInput {
  const CommandInput_Question({required this.id, required  List<String> answers, required this.shown}): _answers = answers,super._();
  

 final  String id;
 final  List<String> _answers;
 List<String> get answers {
  if (_answers is EqualUnmodifiableListView) return _answers;
  // ignore: implicit_dynamic_type
  return EqualUnmodifiableListView(_answers);
}

 final  String shown;

/// Create a copy of CommandInput
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$CommandInput_QuestionCopyWith<CommandInput_Question> get copyWith => _$CommandInput_QuestionCopyWithImpl<CommandInput_Question>(this, _$identity);



@override
bool operator ==(Object other) {
    return identical(this, other) || (other.runtimeType == runtimeType&&other is CommandInput_Question&&(identical(other.id, id) || other.id == id)&&const DeepCollectionEquality().equals(other.answers, _answers)&&(identical(other.shown, shown) || other.shown == shown));
}


@override
int get hashCode {
    return Object.hash(runtimeType,id,const DeepCollectionEquality().hash(_answers),shown);
}

@override
String toString() {
    return 'CommandInput.question(id: $id, answers: $answers, shown: $shown)';
}


}

/// @nodoc
abstract mixin class $CommandInput_QuestionCopyWith<$Res> implements $CommandInputCopyWith<$Res> {
  factory $CommandInput_QuestionCopyWith(CommandInput_Question value, $Res Function(CommandInput_Question) _then) = _$CommandInput_QuestionCopyWithImpl;
@useResult
$Res call({
 String id, List<String> answers, String shown
});




}
/// @nodoc
class _$CommandInput_QuestionCopyWithImpl<$Res>
    implements $CommandInput_QuestionCopyWith<$Res> {
  _$CommandInput_QuestionCopyWithImpl(this._self, this._then);

  final CommandInput_Question _self;
  final $Res Function(CommandInput_Question) _then;

/// Create a copy of CommandInput
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? id = null,Object? answers = null,Object? shown = null,}) {
  return _then(CommandInput_Question(
id: null == id ? _self.id : id // ignore: cast_nullable_to_non_nullable
as String,answers: null == answers ? _self._answers : answers // ignore: cast_nullable_to_non_nullable
as List<String>,shown: null == shown ? _self.shown : shown // ignore: cast_nullable_to_non_nullable
as String,
  ));
}


}

/// @nodoc


class CommandInput_Start extends CommandInput {
  const CommandInput_Start({required this.project, required this.iid, required this.mode, required this.promptHash}): super._();
  

 final  String project;
 final  String iid;
 final  StartMode mode;
 final  String promptHash;

/// Create a copy of CommandInput
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$CommandInput_StartCopyWith<CommandInput_Start> get copyWith => _$CommandInput_StartCopyWithImpl<CommandInput_Start>(this, _$identity);



@override
bool operator ==(Object other) {
    return identical(this, other) || (other.runtimeType == runtimeType&&other is CommandInput_Start&&(identical(other.project, project) || other.project == project)&&(identical(other.iid, iid) || other.iid == iid)&&(identical(other.mode, mode) || other.mode == mode)&&(identical(other.promptHash, promptHash) || other.promptHash == promptHash));
}


@override
int get hashCode {
    return Object.hash(runtimeType,project,iid,mode,promptHash);
}

@override
String toString() {
    return 'CommandInput.start(project: $project, iid: $iid, mode: $mode, promptHash: $promptHash)';
}


}

/// @nodoc
abstract mixin class $CommandInput_StartCopyWith<$Res> implements $CommandInputCopyWith<$Res> {
  factory $CommandInput_StartCopyWith(CommandInput_Start value, $Res Function(CommandInput_Start) _then) = _$CommandInput_StartCopyWithImpl;
@useResult
$Res call({
 String project, String iid, StartMode mode, String promptHash
});




}
/// @nodoc
class _$CommandInput_StartCopyWithImpl<$Res>
    implements $CommandInput_StartCopyWith<$Res> {
  _$CommandInput_StartCopyWithImpl(this._self, this._then);

  final CommandInput_Start _self;
  final $Res Function(CommandInput_Start) _then;

/// Create a copy of CommandInput
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? project = null,Object? iid = null,Object? mode = null,Object? promptHash = null,}) {
  return _then(CommandInput_Start(
project: null == project ? _self.project : project // ignore: cast_nullable_to_non_nullable
as String,iid: null == iid ? _self.iid : iid // ignore: cast_nullable_to_non_nullable
as String,mode: null == mode ? _self.mode : mode // ignore: cast_nullable_to_non_nullable
as StartMode,promptHash: null == promptHash ? _self.promptHash : promptHash // ignore: cast_nullable_to_non_nullable
as String,
  ));
}


}

/// @nodoc


class CommandInput_Stop extends CommandInput {
  const CommandInput_Stop({required this.session, required this.request}): super._();
  

 final  String session;
 final  String request;

/// Create a copy of CommandInput
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$CommandInput_StopCopyWith<CommandInput_Stop> get copyWith => _$CommandInput_StopCopyWithImpl<CommandInput_Stop>(this, _$identity);



@override
bool operator ==(Object other) {
    return identical(this, other) || (other.runtimeType == runtimeType&&other is CommandInput_Stop&&(identical(other.session, session) || other.session == session)&&(identical(other.request, request) || other.request == request));
}


@override
int get hashCode {
    return Object.hash(runtimeType,session,request);
}

@override
String toString() {
    return 'CommandInput.stop(session: $session, request: $request)';
}


}

/// @nodoc
abstract mixin class $CommandInput_StopCopyWith<$Res> implements $CommandInputCopyWith<$Res> {
  factory $CommandInput_StopCopyWith(CommandInput_Stop value, $Res Function(CommandInput_Stop) _then) = _$CommandInput_StopCopyWithImpl;
@useResult
$Res call({
 String session, String request
});




}
/// @nodoc
class _$CommandInput_StopCopyWithImpl<$Res>
    implements $CommandInput_StopCopyWith<$Res> {
  _$CommandInput_StopCopyWithImpl(this._self, this._then);

  final CommandInput_Stop _self;
  final $Res Function(CommandInput_Stop) _then;

/// Create a copy of CommandInput
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? session = null,Object? request = null,}) {
  return _then(CommandInput_Stop(
session: null == session ? _self.session : session // ignore: cast_nullable_to_non_nullable
as String,request: null == request ? _self.request : request // ignore: cast_nullable_to_non_nullable
as String,
  ));
}


}

// dart format on

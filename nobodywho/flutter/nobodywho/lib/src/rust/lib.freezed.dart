// GENERATED CODE - DO NOT MODIFY BY HAND
// coverage:ignore-file
// ignore_for_file: type=lint
// ignore_for_file: unused_element, deprecated_member_use, deprecated_member_use_from_same_package, use_function_type_syntax_for_parameters, unnecessary_const, avoid_init_to_null, invalid_override_different_default_values_named, prefer_expression_function_bodies, annotate_overrides, invalid_annotation_target, unnecessary_question_mark

part of 'lib.dart';

// **************************************************************************
// FreezedGenerator
// **************************************************************************

// dart format off
T _$identity<T>(T value) => value;
/// @nodoc
mixin _$Message {

 Role get role; String get content;
/// Create a copy of Message
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$MessageCopyWith<Message> get copyWith => _$MessageCopyWithImpl<Message>(this as Message, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is Message&&(identical(other.role, role) || other.role == role)&&(identical(other.content, content) || other.content == content));
}


@override
int get hashCode => Object.hash(runtimeType,role,content);

@override
String toString() {
  return 'Message(role: $role, content: $content)';
}


}

/// @nodoc
abstract mixin class $MessageCopyWith<$Res>  {
  factory $MessageCopyWith(Message value, $Res Function(Message) _then) = _$MessageCopyWithImpl;
@useResult
$Res call({
 Role role, String content
});




}
/// @nodoc
class _$MessageCopyWithImpl<$Res>
    implements $MessageCopyWith<$Res> {
  _$MessageCopyWithImpl(this._self, this._then);

  final Message _self;
  final $Res Function(Message) _then;

/// Create a copy of Message
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') @override $Res call({Object? role = null,Object? content = null,}) {
  return _then(_self.copyWith(
role: null == role ? _self.role : role // ignore: cast_nullable_to_non_nullable
as Role,content: null == content ? _self.content : content // ignore: cast_nullable_to_non_nullable
as String,
  ));
}

}


/// Adds pattern-matching-related methods to [Message].
extension MessagePatterns on Message {
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

@optionalTypeArgs TResult maybeMap<TResult extends Object?>({TResult Function( Message_Message value)?  message,TResult Function( Message_ToolCalls value)?  toolCalls,TResult Function( Message_ToolResp value)?  toolResp,required TResult orElse(),}){
final _that = this;
switch (_that) {
case Message_Message() when message != null:
return message(_that);case Message_ToolCalls() when toolCalls != null:
return toolCalls(_that);case Message_ToolResp() when toolResp != null:
return toolResp(_that);case _:
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

@optionalTypeArgs TResult map<TResult extends Object?>({required TResult Function( Message_Message value)  message,required TResult Function( Message_ToolCalls value)  toolCalls,required TResult Function( Message_ToolResp value)  toolResp,}){
final _that = this;
switch (_that) {
case Message_Message():
return message(_that);case Message_ToolCalls():
return toolCalls(_that);case Message_ToolResp():
return toolResp(_that);}
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

@optionalTypeArgs TResult? mapOrNull<TResult extends Object?>({TResult? Function( Message_Message value)?  message,TResult? Function( Message_ToolCalls value)?  toolCalls,TResult? Function( Message_ToolResp value)?  toolResp,}){
final _that = this;
switch (_that) {
case Message_Message() when message != null:
return message(_that);case Message_ToolCalls() when toolCalls != null:
return toolCalls(_that);case Message_ToolResp() when toolResp != null:
return toolResp(_that);case _:
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

@optionalTypeArgs TResult maybeWhen<TResult extends Object?>({TResult Function( Role role,  String content)?  message,TResult Function( Role role,  String content,  List<ToolCall> toolCalls)?  toolCalls,TResult Function( Role role,  String name,  String content)?  toolResp,required TResult orElse(),}) {final _that = this;
switch (_that) {
case Message_Message() when message != null:
return message(_that.role,_that.content);case Message_ToolCalls() when toolCalls != null:
return toolCalls(_that.role,_that.content,_that.toolCalls);case Message_ToolResp() when toolResp != null:
return toolResp(_that.role,_that.name,_that.content);case _:
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

@optionalTypeArgs TResult when<TResult extends Object?>({required TResult Function( Role role,  String content)  message,required TResult Function( Role role,  String content,  List<ToolCall> toolCalls)  toolCalls,required TResult Function( Role role,  String name,  String content)  toolResp,}) {final _that = this;
switch (_that) {
case Message_Message():
return message(_that.role,_that.content);case Message_ToolCalls():
return toolCalls(_that.role,_that.content,_that.toolCalls);case Message_ToolResp():
return toolResp(_that.role,_that.name,_that.content);}
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

@optionalTypeArgs TResult? whenOrNull<TResult extends Object?>({TResult? Function( Role role,  String content)?  message,TResult? Function( Role role,  String content,  List<ToolCall> toolCalls)?  toolCalls,TResult? Function( Role role,  String name,  String content)?  toolResp,}) {final _that = this;
switch (_that) {
case Message_Message() when message != null:
return message(_that.role,_that.content);case Message_ToolCalls() when toolCalls != null:
return toolCalls(_that.role,_that.content,_that.toolCalls);case Message_ToolResp() when toolResp != null:
return toolResp(_that.role,_that.name,_that.content);case _:
  return null;

}
}

}

/// @nodoc


class Message_Message extends Message {
  const Message_Message({required this.role, required this.content}): super._();
  

@override final  Role role;
@override final  String content;

/// Create a copy of Message
/// with the given fields replaced by the non-null parameter values.
@override @JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$Message_MessageCopyWith<Message_Message> get copyWith => _$Message_MessageCopyWithImpl<Message_Message>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is Message_Message&&(identical(other.role, role) || other.role == role)&&(identical(other.content, content) || other.content == content));
}


@override
int get hashCode => Object.hash(runtimeType,role,content);

@override
String toString() {
  return 'Message.message(role: $role, content: $content)';
}


}

/// @nodoc
abstract mixin class $Message_MessageCopyWith<$Res> implements $MessageCopyWith<$Res> {
  factory $Message_MessageCopyWith(Message_Message value, $Res Function(Message_Message) _then) = _$Message_MessageCopyWithImpl;
@override @useResult
$Res call({
 Role role, String content
});




}
/// @nodoc
class _$Message_MessageCopyWithImpl<$Res>
    implements $Message_MessageCopyWith<$Res> {
  _$Message_MessageCopyWithImpl(this._self, this._then);

  final Message_Message _self;
  final $Res Function(Message_Message) _then;

/// Create a copy of Message
/// with the given fields replaced by the non-null parameter values.
@override @pragma('vm:prefer-inline') $Res call({Object? role = null,Object? content = null,}) {
  return _then(Message_Message(
role: null == role ? _self.role : role // ignore: cast_nullable_to_non_nullable
as Role,content: null == content ? _self.content : content // ignore: cast_nullable_to_non_nullable
as String,
  ));
}


}

/// @nodoc


class Message_ToolCalls extends Message {
  const Message_ToolCalls({required this.role, required this.content, required final  List<ToolCall> toolCalls}): _toolCalls = toolCalls,super._();
  

@override final  Role role;
@override final  String content;
 final  List<ToolCall> _toolCalls;
 List<ToolCall> get toolCalls {
  if (_toolCalls is EqualUnmodifiableListView) return _toolCalls;
  // ignore: implicit_dynamic_type
  return EqualUnmodifiableListView(_toolCalls);
}


/// Create a copy of Message
/// with the given fields replaced by the non-null parameter values.
@override @JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$Message_ToolCallsCopyWith<Message_ToolCalls> get copyWith => _$Message_ToolCallsCopyWithImpl<Message_ToolCalls>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is Message_ToolCalls&&(identical(other.role, role) || other.role == role)&&(identical(other.content, content) || other.content == content)&&const DeepCollectionEquality().equals(other._toolCalls, _toolCalls));
}


@override
int get hashCode => Object.hash(runtimeType,role,content,const DeepCollectionEquality().hash(_toolCalls));

@override
String toString() {
  return 'Message.toolCalls(role: $role, content: $content, toolCalls: $toolCalls)';
}


}

/// @nodoc
abstract mixin class $Message_ToolCallsCopyWith<$Res> implements $MessageCopyWith<$Res> {
  factory $Message_ToolCallsCopyWith(Message_ToolCalls value, $Res Function(Message_ToolCalls) _then) = _$Message_ToolCallsCopyWithImpl;
@override @useResult
$Res call({
 Role role, String content, List<ToolCall> toolCalls
});




}
/// @nodoc
class _$Message_ToolCallsCopyWithImpl<$Res>
    implements $Message_ToolCallsCopyWith<$Res> {
  _$Message_ToolCallsCopyWithImpl(this._self, this._then);

  final Message_ToolCalls _self;
  final $Res Function(Message_ToolCalls) _then;

/// Create a copy of Message
/// with the given fields replaced by the non-null parameter values.
@override @pragma('vm:prefer-inline') $Res call({Object? role = null,Object? content = null,Object? toolCalls = null,}) {
  return _then(Message_ToolCalls(
role: null == role ? _self.role : role // ignore: cast_nullable_to_non_nullable
as Role,content: null == content ? _self.content : content // ignore: cast_nullable_to_non_nullable
as String,toolCalls: null == toolCalls ? _self._toolCalls : toolCalls // ignore: cast_nullable_to_non_nullable
as List<ToolCall>,
  ));
}


}

/// @nodoc


class Message_ToolResp extends Message {
  const Message_ToolResp({required this.role, required this.name, required this.content}): super._();
  

@override final  Role role;
 final  String name;
@override final  String content;

/// Create a copy of Message
/// with the given fields replaced by the non-null parameter values.
@override @JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$Message_ToolRespCopyWith<Message_ToolResp> get copyWith => _$Message_ToolRespCopyWithImpl<Message_ToolResp>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is Message_ToolResp&&(identical(other.role, role) || other.role == role)&&(identical(other.name, name) || other.name == name)&&(identical(other.content, content) || other.content == content));
}


@override
int get hashCode => Object.hash(runtimeType,role,name,content);

@override
String toString() {
  return 'Message.toolResp(role: $role, name: $name, content: $content)';
}


}

/// @nodoc
abstract mixin class $Message_ToolRespCopyWith<$Res> implements $MessageCopyWith<$Res> {
  factory $Message_ToolRespCopyWith(Message_ToolResp value, $Res Function(Message_ToolResp) _then) = _$Message_ToolRespCopyWithImpl;
@override @useResult
$Res call({
 Role role, String name, String content
});




}
/// @nodoc
class _$Message_ToolRespCopyWithImpl<$Res>
    implements $Message_ToolRespCopyWith<$Res> {
  _$Message_ToolRespCopyWithImpl(this._self, this._then);

  final Message_ToolResp _self;
  final $Res Function(Message_ToolResp) _then;

/// Create a copy of Message
/// with the given fields replaced by the non-null parameter values.
@override @pragma('vm:prefer-inline') $Res call({Object? role = null,Object? name = null,Object? content = null,}) {
  return _then(Message_ToolResp(
role: null == role ? _self.role : role // ignore: cast_nullable_to_non_nullable
as Role,name: null == name ? _self.name : name // ignore: cast_nullable_to_non_nullable
as String,content: null == content ? _self.content : content // ignore: cast_nullable_to_non_nullable
as String,
  ));
}


}

// dart format on

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

 String get content;
/// Create a copy of Message
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$MessageCopyWith<Message> get copyWith => _$MessageCopyWithImpl<Message>(this as Message, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is Message&&(identical(other.content, content) || other.content == content));
}


@override
int get hashCode => Object.hash(runtimeType,content);

@override
String toString() {
  return 'Message(content: $content)';
}


}

/// @nodoc
abstract mixin class $MessageCopyWith<$Res>  {
  factory $MessageCopyWith(Message value, $Res Function(Message) _then) = _$MessageCopyWithImpl;
@useResult
$Res call({
 String content
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
@pragma('vm:prefer-inline') @override $Res call({Object? content = null,}) {
  return _then(_self.copyWith(
content: null == content ? _self.content : content // ignore: cast_nullable_to_non_nullable
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

@optionalTypeArgs TResult maybeMap<TResult extends Object?>({TResult Function( Message_User value)?  user,TResult Function( Message_Assistant value)?  assistant,TResult Function( Message_System value)?  system,TResult Function( Message_Tool value)?  tool,required TResult orElse(),}){
final _that = this;
switch (_that) {
case Message_User() when user != null:
return user(_that);case Message_Assistant() when assistant != null:
return assistant(_that);case Message_System() when system != null:
return system(_that);case Message_Tool() when tool != null:
return tool(_that);case _:
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

@optionalTypeArgs TResult map<TResult extends Object?>({required TResult Function( Message_User value)  user,required TResult Function( Message_Assistant value)  assistant,required TResult Function( Message_System value)  system,required TResult Function( Message_Tool value)  tool,}){
final _that = this;
switch (_that) {
case Message_User():
return user(_that);case Message_Assistant():
return assistant(_that);case Message_System():
return system(_that);case Message_Tool():
return tool(_that);}
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

@optionalTypeArgs TResult? mapOrNull<TResult extends Object?>({TResult? Function( Message_User value)?  user,TResult? Function( Message_Assistant value)?  assistant,TResult? Function( Message_System value)?  system,TResult? Function( Message_Tool value)?  tool,}){
final _that = this;
switch (_that) {
case Message_User() when user != null:
return user(_that);case Message_Assistant() when assistant != null:
return assistant(_that);case Message_System() when system != null:
return system(_that);case Message_Tool() when tool != null:
return tool(_that);case _:
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

@optionalTypeArgs TResult maybeWhen<TResult extends Object?>({TResult Function( String content,  List<Asset> assets)?  user,TResult Function( String content,  List<ToolCall>? toolCalls)?  assistant,TResult Function( String content)?  system,TResult Function( String name,  String content)?  tool,required TResult orElse(),}) {final _that = this;
switch (_that) {
case Message_User() when user != null:
return user(_that.content,_that.assets);case Message_Assistant() when assistant != null:
return assistant(_that.content,_that.toolCalls);case Message_System() when system != null:
return system(_that.content);case Message_Tool() when tool != null:
return tool(_that.name,_that.content);case _:
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

@optionalTypeArgs TResult when<TResult extends Object?>({required TResult Function( String content,  List<Asset> assets)  user,required TResult Function( String content,  List<ToolCall>? toolCalls)  assistant,required TResult Function( String content)  system,required TResult Function( String name,  String content)  tool,}) {final _that = this;
switch (_that) {
case Message_User():
return user(_that.content,_that.assets);case Message_Assistant():
return assistant(_that.content,_that.toolCalls);case Message_System():
return system(_that.content);case Message_Tool():
return tool(_that.name,_that.content);}
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

@optionalTypeArgs TResult? whenOrNull<TResult extends Object?>({TResult? Function( String content,  List<Asset> assets)?  user,TResult? Function( String content,  List<ToolCall>? toolCalls)?  assistant,TResult? Function( String content)?  system,TResult? Function( String name,  String content)?  tool,}) {final _that = this;
switch (_that) {
case Message_User() when user != null:
return user(_that.content,_that.assets);case Message_Assistant() when assistant != null:
return assistant(_that.content,_that.toolCalls);case Message_System() when system != null:
return system(_that.content);case Message_Tool() when tool != null:
return tool(_that.name,_that.content);case _:
  return null;

}
}

}

/// @nodoc


class Message_User extends Message {
  const Message_User({required this.content, final  List<Asset> assets = const []}): _assets = assets,super._();
  

@override final  String content;
 final  List<Asset> _assets;
@JsonKey() List<Asset> get assets {
  if (_assets is EqualUnmodifiableListView) return _assets;
  // ignore: implicit_dynamic_type
  return EqualUnmodifiableListView(_assets);
}


/// Create a copy of Message
/// with the given fields replaced by the non-null parameter values.
@override @JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$Message_UserCopyWith<Message_User> get copyWith => _$Message_UserCopyWithImpl<Message_User>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is Message_User&&(identical(other.content, content) || other.content == content)&&const DeepCollectionEquality().equals(other._assets, _assets));
}


@override
int get hashCode => Object.hash(runtimeType,content,const DeepCollectionEquality().hash(_assets));

@override
String toString() {
  return 'Message.user(content: $content, assets: $assets)';
}


}

/// @nodoc
abstract mixin class $Message_UserCopyWith<$Res> implements $MessageCopyWith<$Res> {
  factory $Message_UserCopyWith(Message_User value, $Res Function(Message_User) _then) = _$Message_UserCopyWithImpl;
@override @useResult
$Res call({
 String content, List<Asset> assets
});




}
/// @nodoc
class _$Message_UserCopyWithImpl<$Res>
    implements $Message_UserCopyWith<$Res> {
  _$Message_UserCopyWithImpl(this._self, this._then);

  final Message_User _self;
  final $Res Function(Message_User) _then;

/// Create a copy of Message
/// with the given fields replaced by the non-null parameter values.
@override @pragma('vm:prefer-inline') $Res call({Object? content = null,Object? assets = null,}) {
  return _then(Message_User(
content: null == content ? _self.content : content // ignore: cast_nullable_to_non_nullable
as String,assets: null == assets ? _self._assets : assets // ignore: cast_nullable_to_non_nullable
as List<Asset>,
  ));
}


}

/// @nodoc


class Message_Assistant extends Message {
  const Message_Assistant({required this.content, final  List<ToolCall>? toolCalls}): _toolCalls = toolCalls,super._();
  

@override final  String content;
 final  List<ToolCall>? _toolCalls;
 List<ToolCall>? get toolCalls {
  final value = _toolCalls;
  if (value == null) return null;
  if (_toolCalls is EqualUnmodifiableListView) return _toolCalls;
  // ignore: implicit_dynamic_type
  return EqualUnmodifiableListView(value);
}


/// Create a copy of Message
/// with the given fields replaced by the non-null parameter values.
@override @JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$Message_AssistantCopyWith<Message_Assistant> get copyWith => _$Message_AssistantCopyWithImpl<Message_Assistant>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is Message_Assistant&&(identical(other.content, content) || other.content == content)&&const DeepCollectionEquality().equals(other._toolCalls, _toolCalls));
}


@override
int get hashCode => Object.hash(runtimeType,content,const DeepCollectionEquality().hash(_toolCalls));

@override
String toString() {
  return 'Message.assistant(content: $content, toolCalls: $toolCalls)';
}


}

/// @nodoc
abstract mixin class $Message_AssistantCopyWith<$Res> implements $MessageCopyWith<$Res> {
  factory $Message_AssistantCopyWith(Message_Assistant value, $Res Function(Message_Assistant) _then) = _$Message_AssistantCopyWithImpl;
@override @useResult
$Res call({
 String content, List<ToolCall>? toolCalls
});




}
/// @nodoc
class _$Message_AssistantCopyWithImpl<$Res>
    implements $Message_AssistantCopyWith<$Res> {
  _$Message_AssistantCopyWithImpl(this._self, this._then);

  final Message_Assistant _self;
  final $Res Function(Message_Assistant) _then;

/// Create a copy of Message
/// with the given fields replaced by the non-null parameter values.
@override @pragma('vm:prefer-inline') $Res call({Object? content = null,Object? toolCalls = freezed,}) {
  return _then(Message_Assistant(
content: null == content ? _self.content : content // ignore: cast_nullable_to_non_nullable
as String,toolCalls: freezed == toolCalls ? _self._toolCalls : toolCalls // ignore: cast_nullable_to_non_nullable
as List<ToolCall>?,
  ));
}


}

/// @nodoc


class Message_System extends Message {
  const Message_System({required this.content}): super._();
  

@override final  String content;

/// Create a copy of Message
/// with the given fields replaced by the non-null parameter values.
@override @JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$Message_SystemCopyWith<Message_System> get copyWith => _$Message_SystemCopyWithImpl<Message_System>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is Message_System&&(identical(other.content, content) || other.content == content));
}


@override
int get hashCode => Object.hash(runtimeType,content);

@override
String toString() {
  return 'Message.system(content: $content)';
}


}

/// @nodoc
abstract mixin class $Message_SystemCopyWith<$Res> implements $MessageCopyWith<$Res> {
  factory $Message_SystemCopyWith(Message_System value, $Res Function(Message_System) _then) = _$Message_SystemCopyWithImpl;
@override @useResult
$Res call({
 String content
});




}
/// @nodoc
class _$Message_SystemCopyWithImpl<$Res>
    implements $Message_SystemCopyWith<$Res> {
  _$Message_SystemCopyWithImpl(this._self, this._then);

  final Message_System _self;
  final $Res Function(Message_System) _then;

/// Create a copy of Message
/// with the given fields replaced by the non-null parameter values.
@override @pragma('vm:prefer-inline') $Res call({Object? content = null,}) {
  return _then(Message_System(
content: null == content ? _self.content : content // ignore: cast_nullable_to_non_nullable
as String,
  ));
}


}

/// @nodoc


class Message_Tool extends Message {
  const Message_Tool({required this.name, required this.content}): super._();
  

 final  String name;
@override final  String content;

/// Create a copy of Message
/// with the given fields replaced by the non-null parameter values.
@override @JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$Message_ToolCopyWith<Message_Tool> get copyWith => _$Message_ToolCopyWithImpl<Message_Tool>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is Message_Tool&&(identical(other.name, name) || other.name == name)&&(identical(other.content, content) || other.content == content));
}


@override
int get hashCode => Object.hash(runtimeType,name,content);

@override
String toString() {
  return 'Message.tool(name: $name, content: $content)';
}


}

/// @nodoc
abstract mixin class $Message_ToolCopyWith<$Res> implements $MessageCopyWith<$Res> {
  factory $Message_ToolCopyWith(Message_Tool value, $Res Function(Message_Tool) _then) = _$Message_ToolCopyWithImpl;
@override @useResult
$Res call({
 String name, String content
});




}
/// @nodoc
class _$Message_ToolCopyWithImpl<$Res>
    implements $Message_ToolCopyWith<$Res> {
  _$Message_ToolCopyWithImpl(this._self, this._then);

  final Message_Tool _self;
  final $Res Function(Message_Tool) _then;

/// Create a copy of Message
/// with the given fields replaced by the non-null parameter values.
@override @pragma('vm:prefer-inline') $Res call({Object? name = null,Object? content = null,}) {
  return _then(Message_Tool(
name: null == name ? _self.name : name // ignore: cast_nullable_to_non_nullable
as String,content: null == content ? _self.content : content // ignore: cast_nullable_to_non_nullable
as String,
  ));
}


}

/// @nodoc
mixin _$PromptPart {





@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is PromptPart);
}


@override
int get hashCode => runtimeType.hashCode;

@override
String toString() {
  return 'PromptPart()';
}


}

/// @nodoc
class $PromptPartCopyWith<$Res>  {
$PromptPartCopyWith(PromptPart _, $Res Function(PromptPart) __);
}


/// Adds pattern-matching-related methods to [PromptPart].
extension PromptPartPatterns on PromptPart {
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

@optionalTypeArgs TResult maybeMap<TResult extends Object?>({TResult Function( PromptPart_Text value)?  text,TResult Function( PromptPart_Image value)?  image,TResult Function( PromptPart_Audio value)?  audio,required TResult orElse(),}){
final _that = this;
switch (_that) {
case PromptPart_Text() when text != null:
return text(_that);case PromptPart_Image() when image != null:
return image(_that);case PromptPart_Audio() when audio != null:
return audio(_that);case _:
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

@optionalTypeArgs TResult map<TResult extends Object?>({required TResult Function( PromptPart_Text value)  text,required TResult Function( PromptPart_Image value)  image,required TResult Function( PromptPart_Audio value)  audio,}){
final _that = this;
switch (_that) {
case PromptPart_Text():
return text(_that);case PromptPart_Image():
return image(_that);case PromptPart_Audio():
return audio(_that);}
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

@optionalTypeArgs TResult? mapOrNull<TResult extends Object?>({TResult? Function( PromptPart_Text value)?  text,TResult? Function( PromptPart_Image value)?  image,TResult? Function( PromptPart_Audio value)?  audio,}){
final _that = this;
switch (_that) {
case PromptPart_Text() when text != null:
return text(_that);case PromptPart_Image() when image != null:
return image(_that);case PromptPart_Audio() when audio != null:
return audio(_that);case _:
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

@optionalTypeArgs TResult maybeWhen<TResult extends Object?>({TResult Function( String content)?  text,TResult Function( String path)?  image,TResult Function( String path)?  audio,required TResult orElse(),}) {final _that = this;
switch (_that) {
case PromptPart_Text() when text != null:
return text(_that.content);case PromptPart_Image() when image != null:
return image(_that.path);case PromptPart_Audio() when audio != null:
return audio(_that.path);case _:
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

@optionalTypeArgs TResult when<TResult extends Object?>({required TResult Function( String content)  text,required TResult Function( String path)  image,required TResult Function( String path)  audio,}) {final _that = this;
switch (_that) {
case PromptPart_Text():
return text(_that.content);case PromptPart_Image():
return image(_that.path);case PromptPart_Audio():
return audio(_that.path);}
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

@optionalTypeArgs TResult? whenOrNull<TResult extends Object?>({TResult? Function( String content)?  text,TResult? Function( String path)?  image,TResult? Function( String path)?  audio,}) {final _that = this;
switch (_that) {
case PromptPart_Text() when text != null:
return text(_that.content);case PromptPart_Image() when image != null:
return image(_that.path);case PromptPart_Audio() when audio != null:
return audio(_that.path);case _:
  return null;

}
}

}

/// @nodoc


class PromptPart_Text extends PromptPart {
  const PromptPart_Text({required this.content}): super._();
  

 final  String content;

/// Create a copy of PromptPart
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$PromptPart_TextCopyWith<PromptPart_Text> get copyWith => _$PromptPart_TextCopyWithImpl<PromptPart_Text>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is PromptPart_Text&&(identical(other.content, content) || other.content == content));
}


@override
int get hashCode => Object.hash(runtimeType,content);

@override
String toString() {
  return 'PromptPart.text(content: $content)';
}


}

/// @nodoc
abstract mixin class $PromptPart_TextCopyWith<$Res> implements $PromptPartCopyWith<$Res> {
  factory $PromptPart_TextCopyWith(PromptPart_Text value, $Res Function(PromptPart_Text) _then) = _$PromptPart_TextCopyWithImpl;
@useResult
$Res call({
 String content
});




}
/// @nodoc
class _$PromptPart_TextCopyWithImpl<$Res>
    implements $PromptPart_TextCopyWith<$Res> {
  _$PromptPart_TextCopyWithImpl(this._self, this._then);

  final PromptPart_Text _self;
  final $Res Function(PromptPart_Text) _then;

/// Create a copy of PromptPart
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? content = null,}) {
  return _then(PromptPart_Text(
content: null == content ? _self.content : content // ignore: cast_nullable_to_non_nullable
as String,
  ));
}


}

/// @nodoc


class PromptPart_Image extends PromptPart {
  const PromptPart_Image({required this.path}): super._();
  

 final  String path;

/// Create a copy of PromptPart
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$PromptPart_ImageCopyWith<PromptPart_Image> get copyWith => _$PromptPart_ImageCopyWithImpl<PromptPart_Image>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is PromptPart_Image&&(identical(other.path, path) || other.path == path));
}


@override
int get hashCode => Object.hash(runtimeType,path);

@override
String toString() {
  return 'PromptPart.image(path: $path)';
}


}

/// @nodoc
abstract mixin class $PromptPart_ImageCopyWith<$Res> implements $PromptPartCopyWith<$Res> {
  factory $PromptPart_ImageCopyWith(PromptPart_Image value, $Res Function(PromptPart_Image) _then) = _$PromptPart_ImageCopyWithImpl;
@useResult
$Res call({
 String path
});




}
/// @nodoc
class _$PromptPart_ImageCopyWithImpl<$Res>
    implements $PromptPart_ImageCopyWith<$Res> {
  _$PromptPart_ImageCopyWithImpl(this._self, this._then);

  final PromptPart_Image _self;
  final $Res Function(PromptPart_Image) _then;

/// Create a copy of PromptPart
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? path = null,}) {
  return _then(PromptPart_Image(
path: null == path ? _self.path : path // ignore: cast_nullable_to_non_nullable
as String,
  ));
}


}

/// @nodoc


class PromptPart_Audio extends PromptPart {
  const PromptPart_Audio({required this.path}): super._();
  

 final  String path;

/// Create a copy of PromptPart
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$PromptPart_AudioCopyWith<PromptPart_Audio> get copyWith => _$PromptPart_AudioCopyWithImpl<PromptPart_Audio>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is PromptPart_Audio&&(identical(other.path, path) || other.path == path));
}


@override
int get hashCode => Object.hash(runtimeType,path);

@override
String toString() {
  return 'PromptPart.audio(path: $path)';
}


}

/// @nodoc
abstract mixin class $PromptPart_AudioCopyWith<$Res> implements $PromptPartCopyWith<$Res> {
  factory $PromptPart_AudioCopyWith(PromptPart_Audio value, $Res Function(PromptPart_Audio) _then) = _$PromptPart_AudioCopyWithImpl;
@useResult
$Res call({
 String path
});




}
/// @nodoc
class _$PromptPart_AudioCopyWithImpl<$Res>
    implements $PromptPart_AudioCopyWith<$Res> {
  _$PromptPart_AudioCopyWithImpl(this._self, this._then);

  final PromptPart_Audio _self;
  final $Res Function(PromptPart_Audio) _then;

/// Create a copy of PromptPart
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? path = null,}) {
  return _then(PromptPart_Audio(
path: null == path ? _self.path : path // ignore: cast_nullable_to_non_nullable
as String,
  ));
}


}

// dart format on

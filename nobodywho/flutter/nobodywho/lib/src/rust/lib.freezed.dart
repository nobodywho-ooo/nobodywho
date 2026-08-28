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
mixin _$ContentPart {





@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is ContentPart);
}


@override
int get hashCode => runtimeType.hashCode;

@override
String toString() {
  return 'ContentPart()';
}


}

/// @nodoc
class $ContentPartCopyWith<$Res>  {
$ContentPartCopyWith(ContentPart _, $Res Function(ContentPart) __);
}


/// Adds pattern-matching-related methods to [ContentPart].
extension ContentPartPatterns on ContentPart {
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

@optionalTypeArgs TResult maybeMap<TResult extends Object?>({TResult Function( ContentPart_Text value)?  text,TResult Function( ContentPart_Image value)?  image,TResult Function( ContentPart_Audio value)?  audio,required TResult orElse(),}){
final _that = this;
switch (_that) {
case ContentPart_Text() when text != null:
return text(_that);case ContentPart_Image() when image != null:
return image(_that);case ContentPart_Audio() when audio != null:
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

@optionalTypeArgs TResult map<TResult extends Object?>({required TResult Function( ContentPart_Text value)  text,required TResult Function( ContentPart_Image value)  image,required TResult Function( ContentPart_Audio value)  audio,}){
final _that = this;
switch (_that) {
case ContentPart_Text():
return text(_that);case ContentPart_Image():
return image(_that);case ContentPart_Audio():
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

@optionalTypeArgs TResult? mapOrNull<TResult extends Object?>({TResult? Function( ContentPart_Text value)?  text,TResult? Function( ContentPart_Image value)?  image,TResult? Function( ContentPart_Audio value)?  audio,}){
final _that = this;
switch (_that) {
case ContentPart_Text() when text != null:
return text(_that);case ContentPart_Image() when image != null:
return image(_that);case ContentPart_Audio() when audio != null:
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

@optionalTypeArgs TResult maybeWhen<TResult extends Object?>({TResult Function( String text)?  text,TResult Function( String path)?  image,TResult Function( String path)?  audio,required TResult orElse(),}) {final _that = this;
switch (_that) {
case ContentPart_Text() when text != null:
return text(_that.text);case ContentPart_Image() when image != null:
return image(_that.path);case ContentPart_Audio() when audio != null:
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

@optionalTypeArgs TResult when<TResult extends Object?>({required TResult Function( String text)  text,required TResult Function( String path)  image,required TResult Function( String path)  audio,}) {final _that = this;
switch (_that) {
case ContentPart_Text():
return text(_that.text);case ContentPart_Image():
return image(_that.path);case ContentPart_Audio():
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

@optionalTypeArgs TResult? whenOrNull<TResult extends Object?>({TResult? Function( String text)?  text,TResult? Function( String path)?  image,TResult? Function( String path)?  audio,}) {final _that = this;
switch (_that) {
case ContentPart_Text() when text != null:
return text(_that.text);case ContentPart_Image() when image != null:
return image(_that.path);case ContentPart_Audio() when audio != null:
return audio(_that.path);case _:
  return null;

}
}

}

/// @nodoc


class ContentPart_Text extends ContentPart {
  const ContentPart_Text({required this.text}): super._();
  

 final  String text;

/// Create a copy of ContentPart
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$ContentPart_TextCopyWith<ContentPart_Text> get copyWith => _$ContentPart_TextCopyWithImpl<ContentPart_Text>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is ContentPart_Text&&(identical(other.text, text) || other.text == text));
}


@override
int get hashCode => Object.hash(runtimeType,text);

@override
String toString() {
  return 'ContentPart.text(text: $text)';
}


}

/// @nodoc
abstract mixin class $ContentPart_TextCopyWith<$Res> implements $ContentPartCopyWith<$Res> {
  factory $ContentPart_TextCopyWith(ContentPart_Text value, $Res Function(ContentPart_Text) _then) = _$ContentPart_TextCopyWithImpl;
@useResult
$Res call({
 String text
});




}
/// @nodoc
class _$ContentPart_TextCopyWithImpl<$Res>
    implements $ContentPart_TextCopyWith<$Res> {
  _$ContentPart_TextCopyWithImpl(this._self, this._then);

  final ContentPart_Text _self;
  final $Res Function(ContentPart_Text) _then;

/// Create a copy of ContentPart
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? text = null,}) {
  return _then(ContentPart_Text(
text: null == text ? _self.text : text // ignore: cast_nullable_to_non_nullable
as String,
  ));
}


}

/// @nodoc


class ContentPart_Image extends ContentPart {
  const ContentPart_Image({required this.path}): super._();
  

 final  String path;

/// Create a copy of ContentPart
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$ContentPart_ImageCopyWith<ContentPart_Image> get copyWith => _$ContentPart_ImageCopyWithImpl<ContentPart_Image>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is ContentPart_Image&&(identical(other.path, path) || other.path == path));
}


@override
int get hashCode => Object.hash(runtimeType,path);

@override
String toString() {
  return 'ContentPart.image(path: $path)';
}


}

/// @nodoc
abstract mixin class $ContentPart_ImageCopyWith<$Res> implements $ContentPartCopyWith<$Res> {
  factory $ContentPart_ImageCopyWith(ContentPart_Image value, $Res Function(ContentPart_Image) _then) = _$ContentPart_ImageCopyWithImpl;
@useResult
$Res call({
 String path
});




}
/// @nodoc
class _$ContentPart_ImageCopyWithImpl<$Res>
    implements $ContentPart_ImageCopyWith<$Res> {
  _$ContentPart_ImageCopyWithImpl(this._self, this._then);

  final ContentPart_Image _self;
  final $Res Function(ContentPart_Image) _then;

/// Create a copy of ContentPart
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? path = null,}) {
  return _then(ContentPart_Image(
path: null == path ? _self.path : path // ignore: cast_nullable_to_non_nullable
as String,
  ));
}


}

/// @nodoc


class ContentPart_Audio extends ContentPart {
  const ContentPart_Audio({required this.path}): super._();
  

 final  String path;

/// Create a copy of ContentPart
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$ContentPart_AudioCopyWith<ContentPart_Audio> get copyWith => _$ContentPart_AudioCopyWithImpl<ContentPart_Audio>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is ContentPart_Audio&&(identical(other.path, path) || other.path == path));
}


@override
int get hashCode => Object.hash(runtimeType,path);

@override
String toString() {
  return 'ContentPart.audio(path: $path)';
}


}

/// @nodoc
abstract mixin class $ContentPart_AudioCopyWith<$Res> implements $ContentPartCopyWith<$Res> {
  factory $ContentPart_AudioCopyWith(ContentPart_Audio value, $Res Function(ContentPart_Audio) _then) = _$ContentPart_AudioCopyWithImpl;
@useResult
$Res call({
 String path
});




}
/// @nodoc
class _$ContentPart_AudioCopyWithImpl<$Res>
    implements $ContentPart_AudioCopyWith<$Res> {
  _$ContentPart_AudioCopyWithImpl(this._self, this._then);

  final ContentPart_Audio _self;
  final $Res Function(ContentPart_Audio) _then;

/// Create a copy of ContentPart
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? path = null,}) {
  return _then(ContentPart_Audio(
path: null == path ? _self.path : path // ignore: cast_nullable_to_non_nullable
as String,
  ));
}


}

/// @nodoc
mixin _$Message {

 MessageContent get content;
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
 MessageContent content
});


$MessageContentCopyWith<$Res> get content;

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
as MessageContent,
  ));
}
/// Create a copy of Message
/// with the given fields replaced by the non-null parameter values.
@override
@pragma('vm:prefer-inline')
$MessageContentCopyWith<$Res> get content {
  
  return $MessageContentCopyWith<$Res>(_self.content, (value) {
    return _then(_self.copyWith(content: value));
  });
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

@optionalTypeArgs TResult maybeWhen<TResult extends Object?>({TResult Function( MessageContent content)?  user,TResult Function( MessageContent content,  List<ToolCall>? toolCalls)?  assistant,TResult Function( MessageContent content)?  system,TResult Function( String name,  MessageContent content)?  tool,required TResult orElse(),}) {final _that = this;
switch (_that) {
case Message_User() when user != null:
return user(_that.content);case Message_Assistant() when assistant != null:
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

@optionalTypeArgs TResult when<TResult extends Object?>({required TResult Function( MessageContent content)  user,required TResult Function( MessageContent content,  List<ToolCall>? toolCalls)  assistant,required TResult Function( MessageContent content)  system,required TResult Function( String name,  MessageContent content)  tool,}) {final _that = this;
switch (_that) {
case Message_User():
return user(_that.content);case Message_Assistant():
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

@optionalTypeArgs TResult? whenOrNull<TResult extends Object?>({TResult? Function( MessageContent content)?  user,TResult? Function( MessageContent content,  List<ToolCall>? toolCalls)?  assistant,TResult? Function( MessageContent content)?  system,TResult? Function( String name,  MessageContent content)?  tool,}) {final _that = this;
switch (_that) {
case Message_User() when user != null:
return user(_that.content);case Message_Assistant() when assistant != null:
return assistant(_that.content,_that.toolCalls);case Message_System() when system != null:
return system(_that.content);case Message_Tool() when tool != null:
return tool(_that.name,_that.content);case _:
  return null;

}
}

}

/// @nodoc


class Message_User extends Message {
  const Message_User({required this.content}): super._();
  

@override final  MessageContent content;

/// Create a copy of Message
/// with the given fields replaced by the non-null parameter values.
@override @JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$Message_UserCopyWith<Message_User> get copyWith => _$Message_UserCopyWithImpl<Message_User>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is Message_User&&(identical(other.content, content) || other.content == content));
}


@override
int get hashCode => Object.hash(runtimeType,content);

@override
String toString() {
  return 'Message.user(content: $content)';
}


}

/// @nodoc
abstract mixin class $Message_UserCopyWith<$Res> implements $MessageCopyWith<$Res> {
  factory $Message_UserCopyWith(Message_User value, $Res Function(Message_User) _then) = _$Message_UserCopyWithImpl;
@override @useResult
$Res call({
 MessageContent content
});


@override $MessageContentCopyWith<$Res> get content;

}
/// @nodoc
class _$Message_UserCopyWithImpl<$Res>
    implements $Message_UserCopyWith<$Res> {
  _$Message_UserCopyWithImpl(this._self, this._then);

  final Message_User _self;
  final $Res Function(Message_User) _then;

/// Create a copy of Message
/// with the given fields replaced by the non-null parameter values.
@override @pragma('vm:prefer-inline') $Res call({Object? content = null,}) {
  return _then(Message_User(
content: null == content ? _self.content : content // ignore: cast_nullable_to_non_nullable
as MessageContent,
  ));
}

/// Create a copy of Message
/// with the given fields replaced by the non-null parameter values.
@override
@pragma('vm:prefer-inline')
$MessageContentCopyWith<$Res> get content {
  
  return $MessageContentCopyWith<$Res>(_self.content, (value) {
    return _then(_self.copyWith(content: value));
  });
}
}

/// @nodoc


class Message_Assistant extends Message {
  const Message_Assistant({required this.content, final  List<ToolCall>? toolCalls}): _toolCalls = toolCalls,super._();
  

@override final  MessageContent content;
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
 MessageContent content, List<ToolCall>? toolCalls
});


@override $MessageContentCopyWith<$Res> get content;

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
as MessageContent,toolCalls: freezed == toolCalls ? _self._toolCalls : toolCalls // ignore: cast_nullable_to_non_nullable
as List<ToolCall>?,
  ));
}

/// Create a copy of Message
/// with the given fields replaced by the non-null parameter values.
@override
@pragma('vm:prefer-inline')
$MessageContentCopyWith<$Res> get content {
  
  return $MessageContentCopyWith<$Res>(_self.content, (value) {
    return _then(_self.copyWith(content: value));
  });
}
}

/// @nodoc


class Message_System extends Message {
  const Message_System({required this.content}): super._();
  

@override final  MessageContent content;

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
 MessageContent content
});


@override $MessageContentCopyWith<$Res> get content;

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
as MessageContent,
  ));
}

/// Create a copy of Message
/// with the given fields replaced by the non-null parameter values.
@override
@pragma('vm:prefer-inline')
$MessageContentCopyWith<$Res> get content {
  
  return $MessageContentCopyWith<$Res>(_self.content, (value) {
    return _then(_self.copyWith(content: value));
  });
}
}

/// @nodoc


class Message_Tool extends Message {
  const Message_Tool({required this.name, required this.content}): super._();
  

 final  String name;
@override final  MessageContent content;

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
 String name, MessageContent content
});


@override $MessageContentCopyWith<$Res> get content;

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
as MessageContent,
  ));
}

/// Create a copy of Message
/// with the given fields replaced by the non-null parameter values.
@override
@pragma('vm:prefer-inline')
$MessageContentCopyWith<$Res> get content {
  
  return $MessageContentCopyWith<$Res>(_self.content, (value) {
    return _then(_self.copyWith(content: value));
  });
}
}

/// @nodoc
mixin _$MessageContent {





@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is MessageContent);
}


@override
int get hashCode => runtimeType.hashCode;

@override
String toString() {
  return 'MessageContent()';
}


}

/// @nodoc
class $MessageContentCopyWith<$Res>  {
$MessageContentCopyWith(MessageContent _, $Res Function(MessageContent) __);
}


/// Adds pattern-matching-related methods to [MessageContent].
extension MessageContentPatterns on MessageContent {
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

@optionalTypeArgs TResult maybeMap<TResult extends Object?>({TResult Function( MessageContent_Text value)?  text,TResult Function( MessageContent_Parts value)?  parts,TResult Function( MessageContent_Json value)?  json,required TResult orElse(),}){
final _that = this;
switch (_that) {
case MessageContent_Text() when text != null:
return text(_that);case MessageContent_Parts() when parts != null:
return parts(_that);case MessageContent_Json() when json != null:
return json(_that);case _:
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

@optionalTypeArgs TResult map<TResult extends Object?>({required TResult Function( MessageContent_Text value)  text,required TResult Function( MessageContent_Parts value)  parts,required TResult Function( MessageContent_Json value)  json,}){
final _that = this;
switch (_that) {
case MessageContent_Text():
return text(_that);case MessageContent_Parts():
return parts(_that);case MessageContent_Json():
return json(_that);}
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

@optionalTypeArgs TResult? mapOrNull<TResult extends Object?>({TResult? Function( MessageContent_Text value)?  text,TResult? Function( MessageContent_Parts value)?  parts,TResult? Function( MessageContent_Json value)?  json,}){
final _that = this;
switch (_that) {
case MessageContent_Text() when text != null:
return text(_that);case MessageContent_Parts() when parts != null:
return parts(_that);case MessageContent_Json() when json != null:
return json(_that);case _:
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

@optionalTypeArgs TResult maybeWhen<TResult extends Object?>({TResult Function( String text)?  text,TResult Function( List<ContentPart> parts)?  parts,TResult Function( String json)?  json,required TResult orElse(),}) {final _that = this;
switch (_that) {
case MessageContent_Text() when text != null:
return text(_that.text);case MessageContent_Parts() when parts != null:
return parts(_that.parts);case MessageContent_Json() when json != null:
return json(_that.json);case _:
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

@optionalTypeArgs TResult when<TResult extends Object?>({required TResult Function( String text)  text,required TResult Function( List<ContentPart> parts)  parts,required TResult Function( String json)  json,}) {final _that = this;
switch (_that) {
case MessageContent_Text():
return text(_that.text);case MessageContent_Parts():
return parts(_that.parts);case MessageContent_Json():
return json(_that.json);}
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

@optionalTypeArgs TResult? whenOrNull<TResult extends Object?>({TResult? Function( String text)?  text,TResult? Function( List<ContentPart> parts)?  parts,TResult? Function( String json)?  json,}) {final _that = this;
switch (_that) {
case MessageContent_Text() when text != null:
return text(_that.text);case MessageContent_Parts() when parts != null:
return parts(_that.parts);case MessageContent_Json() when json != null:
return json(_that.json);case _:
  return null;

}
}

}

/// @nodoc


class MessageContent_Text extends MessageContent {
  const MessageContent_Text({required this.text}): super._();
  

 final  String text;

/// Create a copy of MessageContent
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$MessageContent_TextCopyWith<MessageContent_Text> get copyWith => _$MessageContent_TextCopyWithImpl<MessageContent_Text>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is MessageContent_Text&&(identical(other.text, text) || other.text == text));
}


@override
int get hashCode => Object.hash(runtimeType,text);

@override
String toString() {
  return 'MessageContent.text(text: $text)';
}


}

/// @nodoc
abstract mixin class $MessageContent_TextCopyWith<$Res> implements $MessageContentCopyWith<$Res> {
  factory $MessageContent_TextCopyWith(MessageContent_Text value, $Res Function(MessageContent_Text) _then) = _$MessageContent_TextCopyWithImpl;
@useResult
$Res call({
 String text
});




}
/// @nodoc
class _$MessageContent_TextCopyWithImpl<$Res>
    implements $MessageContent_TextCopyWith<$Res> {
  _$MessageContent_TextCopyWithImpl(this._self, this._then);

  final MessageContent_Text _self;
  final $Res Function(MessageContent_Text) _then;

/// Create a copy of MessageContent
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? text = null,}) {
  return _then(MessageContent_Text(
text: null == text ? _self.text : text // ignore: cast_nullable_to_non_nullable
as String,
  ));
}


}

/// @nodoc


class MessageContent_Parts extends MessageContent {
  const MessageContent_Parts({required final  List<ContentPart> parts}): _parts = parts,super._();
  

 final  List<ContentPart> _parts;
 List<ContentPart> get parts {
  if (_parts is EqualUnmodifiableListView) return _parts;
  // ignore: implicit_dynamic_type
  return EqualUnmodifiableListView(_parts);
}


/// Create a copy of MessageContent
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$MessageContent_PartsCopyWith<MessageContent_Parts> get copyWith => _$MessageContent_PartsCopyWithImpl<MessageContent_Parts>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is MessageContent_Parts&&const DeepCollectionEquality().equals(other._parts, _parts));
}


@override
int get hashCode => Object.hash(runtimeType,const DeepCollectionEquality().hash(_parts));

@override
String toString() {
  return 'MessageContent.parts(parts: $parts)';
}


}

/// @nodoc
abstract mixin class $MessageContent_PartsCopyWith<$Res> implements $MessageContentCopyWith<$Res> {
  factory $MessageContent_PartsCopyWith(MessageContent_Parts value, $Res Function(MessageContent_Parts) _then) = _$MessageContent_PartsCopyWithImpl;
@useResult
$Res call({
 List<ContentPart> parts
});




}
/// @nodoc
class _$MessageContent_PartsCopyWithImpl<$Res>
    implements $MessageContent_PartsCopyWith<$Res> {
  _$MessageContent_PartsCopyWithImpl(this._self, this._then);

  final MessageContent_Parts _self;
  final $Res Function(MessageContent_Parts) _then;

/// Create a copy of MessageContent
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? parts = null,}) {
  return _then(MessageContent_Parts(
parts: null == parts ? _self._parts : parts // ignore: cast_nullable_to_non_nullable
as List<ContentPart>,
  ));
}


}

/// @nodoc


class MessageContent_Json extends MessageContent {
  const MessageContent_Json({required this.json}): super._();
  

 final  String json;

/// Create a copy of MessageContent
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$MessageContent_JsonCopyWith<MessageContent_Json> get copyWith => _$MessageContent_JsonCopyWithImpl<MessageContent_Json>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is MessageContent_Json&&(identical(other.json, json) || other.json == json));
}


@override
int get hashCode => Object.hash(runtimeType,json);

@override
String toString() {
  return 'MessageContent.json(json: $json)';
}


}

/// @nodoc
abstract mixin class $MessageContent_JsonCopyWith<$Res> implements $MessageContentCopyWith<$Res> {
  factory $MessageContent_JsonCopyWith(MessageContent_Json value, $Res Function(MessageContent_Json) _then) = _$MessageContent_JsonCopyWithImpl;
@useResult
$Res call({
 String json
});




}
/// @nodoc
class _$MessageContent_JsonCopyWithImpl<$Res>
    implements $MessageContent_JsonCopyWith<$Res> {
  _$MessageContent_JsonCopyWithImpl(this._self, this._then);

  final MessageContent_Json _self;
  final $Res Function(MessageContent_Json) _then;

/// Create a copy of MessageContent
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? json = null,}) {
  return _then(MessageContent_Json(
json: null == json ? _self.json : json // ignore: cast_nullable_to_non_nullable
as String,
  ));
}


}

// dart format on

// coverage:ignore-file
// GENERATED CODE - DO NOT MODIFY BY HAND
// ignore_for_file: type=lint
// ignore_for_file: unused_element, deprecated_member_use, deprecated_member_use_from_same_package, use_function_type_syntax_for_parameters, unnecessary_const, avoid_init_to_null, invalid_override_different_default_values_named, prefer_expression_function_bodies, annotate_overrides, invalid_annotation_target, unnecessary_question_mark

part of 'lib.dart';

// **************************************************************************
// FreezedGenerator
// **************************************************************************

T _$identity<T>(T value) => value;

final _privateConstructorUsedError = UnsupportedError(
  'It seems like you constructed your class using `MyClass._()`. This constructor is only meant to be used by freezed and you are not supposed to need it nor use it.\nPlease check the documentation here for more information: https://github.com/rrousselGit/freezed#adding-getters-and-methods-to-our-models',
);

/// @nodoc
mixin _$Message {
  Role get role => throw _privateConstructorUsedError;
  String get content => throw _privateConstructorUsedError;
  @optionalTypeArgs
  TResult when<TResult extends Object?>({
    required TResult Function(Role role, String content, List<String> assetIds)
    message,
    required TResult Function(
      Role role,
      String content,
      List<ToolCall> toolCalls,
    )
    toolCalls,
    required TResult Function(Role role, String name, String content) toolResp,
  }) => throw _privateConstructorUsedError;
  @optionalTypeArgs
  TResult? whenOrNull<TResult extends Object?>({
    TResult? Function(Role role, String content, List<String> assetIds)?
    message,
    TResult? Function(Role role, String content, List<ToolCall> toolCalls)?
    toolCalls,
    TResult? Function(Role role, String name, String content)? toolResp,
  }) => throw _privateConstructorUsedError;
  @optionalTypeArgs
  TResult maybeWhen<TResult extends Object?>({
    TResult Function(Role role, String content, List<String> assetIds)? message,
    TResult Function(Role role, String content, List<ToolCall> toolCalls)?
    toolCalls,
    TResult Function(Role role, String name, String content)? toolResp,
    required TResult orElse(),
  }) => throw _privateConstructorUsedError;
  @optionalTypeArgs
  TResult map<TResult extends Object?>({
    required TResult Function(Message_Message value) message,
    required TResult Function(Message_ToolCalls value) toolCalls,
    required TResult Function(Message_ToolResp value) toolResp,
  }) => throw _privateConstructorUsedError;
  @optionalTypeArgs
  TResult? mapOrNull<TResult extends Object?>({
    TResult? Function(Message_Message value)? message,
    TResult? Function(Message_ToolCalls value)? toolCalls,
    TResult? Function(Message_ToolResp value)? toolResp,
  }) => throw _privateConstructorUsedError;
  @optionalTypeArgs
  TResult maybeMap<TResult extends Object?>({
    TResult Function(Message_Message value)? message,
    TResult Function(Message_ToolCalls value)? toolCalls,
    TResult Function(Message_ToolResp value)? toolResp,
    required TResult orElse(),
  }) => throw _privateConstructorUsedError;

  /// Create a copy of Message
  /// with the given fields replaced by the non-null parameter values.
  @JsonKey(includeFromJson: false, includeToJson: false)
  $MessageCopyWith<Message> get copyWith => throw _privateConstructorUsedError;
}

/// @nodoc
abstract class $MessageCopyWith<$Res> {
  factory $MessageCopyWith(Message value, $Res Function(Message) then) =
      _$MessageCopyWithImpl<$Res, Message>;
  @useResult
  $Res call({Role role, String content});
}

/// @nodoc
class _$MessageCopyWithImpl<$Res, $Val extends Message>
    implements $MessageCopyWith<$Res> {
  _$MessageCopyWithImpl(this._value, this._then);

  // ignore: unused_field
  final $Val _value;
  // ignore: unused_field
  final $Res Function($Val) _then;

  /// Create a copy of Message
  /// with the given fields replaced by the non-null parameter values.
  @pragma('vm:prefer-inline')
  @override
  $Res call({Object? role = null, Object? content = null}) {
    return _then(
      _value.copyWith(
            role: null == role
                ? _value.role
                : role // ignore: cast_nullable_to_non_nullable
                      as Role,
            content: null == content
                ? _value.content
                : content // ignore: cast_nullable_to_non_nullable
                      as String,
          )
          as $Val,
    );
  }
}

/// @nodoc
abstract class _$$Message_MessageImplCopyWith<$Res>
    implements $MessageCopyWith<$Res> {
  factory _$$Message_MessageImplCopyWith(
    _$Message_MessageImpl value,
    $Res Function(_$Message_MessageImpl) then,
  ) = __$$Message_MessageImplCopyWithImpl<$Res>;
  @override
  @useResult
  $Res call({Role role, String content, List<String> assetIds});
}

/// @nodoc
class __$$Message_MessageImplCopyWithImpl<$Res>
    extends _$MessageCopyWithImpl<$Res, _$Message_MessageImpl>
    implements _$$Message_MessageImplCopyWith<$Res> {
  __$$Message_MessageImplCopyWithImpl(
    _$Message_MessageImpl _value,
    $Res Function(_$Message_MessageImpl) _then,
  ) : super(_value, _then);

  /// Create a copy of Message
  /// with the given fields replaced by the non-null parameter values.
  @pragma('vm:prefer-inline')
  @override
  $Res call({
    Object? role = null,
    Object? content = null,
    Object? assetIds = null,
  }) {
    return _then(
      _$Message_MessageImpl(
        role: null == role
            ? _value.role
            : role // ignore: cast_nullable_to_non_nullable
                  as Role,
        content: null == content
            ? _value.content
            : content // ignore: cast_nullable_to_non_nullable
                  as String,
        assetIds: null == assetIds
            ? _value._assetIds
            : assetIds // ignore: cast_nullable_to_non_nullable
                  as List<String>,
      ),
    );
  }
}

/// @nodoc

class _$Message_MessageImpl extends Message_Message {
  const _$Message_MessageImpl({
    required this.role,
    required this.content,
    required final List<String> assetIds,
  }) : _assetIds = assetIds,
       super._();

  @override
  final Role role;
  @override
  final String content;
  final List<String> _assetIds;
  @override
  List<String> get assetIds {
    if (_assetIds is EqualUnmodifiableListView) return _assetIds;
    // ignore: implicit_dynamic_type
    return EqualUnmodifiableListView(_assetIds);
  }

  @override
  String toString() {
    return 'Message.message(role: $role, content: $content, assetIds: $assetIds)';
  }

  @override
  bool operator ==(Object other) {
    return identical(this, other) ||
        (other.runtimeType == runtimeType &&
            other is _$Message_MessageImpl &&
            (identical(other.role, role) || other.role == role) &&
            (identical(other.content, content) || other.content == content) &&
            const DeepCollectionEquality().equals(other._assetIds, _assetIds));
  }

  @override
  int get hashCode => Object.hash(
    runtimeType,
    role,
    content,
    const DeepCollectionEquality().hash(_assetIds),
  );

  /// Create a copy of Message
  /// with the given fields replaced by the non-null parameter values.
  @JsonKey(includeFromJson: false, includeToJson: false)
  @override
  @pragma('vm:prefer-inline')
  _$$Message_MessageImplCopyWith<_$Message_MessageImpl> get copyWith =>
      __$$Message_MessageImplCopyWithImpl<_$Message_MessageImpl>(
        this,
        _$identity,
      );

  @override
  @optionalTypeArgs
  TResult when<TResult extends Object?>({
    required TResult Function(Role role, String content, List<String> assetIds)
    message,
    required TResult Function(
      Role role,
      String content,
      List<ToolCall> toolCalls,
    )
    toolCalls,
    required TResult Function(Role role, String name, String content) toolResp,
  }) {
    return message(role, content, assetIds);
  }

  @override
  @optionalTypeArgs
  TResult? whenOrNull<TResult extends Object?>({
    TResult? Function(Role role, String content, List<String> assetIds)?
    message,
    TResult? Function(Role role, String content, List<ToolCall> toolCalls)?
    toolCalls,
    TResult? Function(Role role, String name, String content)? toolResp,
  }) {
    return message?.call(role, content, assetIds);
  }

  @override
  @optionalTypeArgs
  TResult maybeWhen<TResult extends Object?>({
    TResult Function(Role role, String content, List<String> assetIds)? message,
    TResult Function(Role role, String content, List<ToolCall> toolCalls)?
    toolCalls,
    TResult Function(Role role, String name, String content)? toolResp,
    required TResult orElse(),
  }) {
    if (message != null) {
      return message(role, content, assetIds);
    }
    return orElse();
  }

  @override
  @optionalTypeArgs
  TResult map<TResult extends Object?>({
    required TResult Function(Message_Message value) message,
    required TResult Function(Message_ToolCalls value) toolCalls,
    required TResult Function(Message_ToolResp value) toolResp,
  }) {
    return message(this);
  }

  @override
  @optionalTypeArgs
  TResult? mapOrNull<TResult extends Object?>({
    TResult? Function(Message_Message value)? message,
    TResult? Function(Message_ToolCalls value)? toolCalls,
    TResult? Function(Message_ToolResp value)? toolResp,
  }) {
    return message?.call(this);
  }

  @override
  @optionalTypeArgs
  TResult maybeMap<TResult extends Object?>({
    TResult Function(Message_Message value)? message,
    TResult Function(Message_ToolCalls value)? toolCalls,
    TResult Function(Message_ToolResp value)? toolResp,
    required TResult orElse(),
  }) {
    if (message != null) {
      return message(this);
    }
    return orElse();
  }
}

abstract class Message_Message extends Message {
  const factory Message_Message({
    required final Role role,
    required final String content,
    required final List<String> assetIds,
  }) = _$Message_MessageImpl;
  const Message_Message._() : super._();

  @override
  Role get role;
  @override
  String get content;
  List<String> get assetIds;

  /// Create a copy of Message
  /// with the given fields replaced by the non-null parameter values.
  @override
  @JsonKey(includeFromJson: false, includeToJson: false)
  _$$Message_MessageImplCopyWith<_$Message_MessageImpl> get copyWith =>
      throw _privateConstructorUsedError;
}

/// @nodoc
abstract class _$$Message_ToolCallsImplCopyWith<$Res>
    implements $MessageCopyWith<$Res> {
  factory _$$Message_ToolCallsImplCopyWith(
    _$Message_ToolCallsImpl value,
    $Res Function(_$Message_ToolCallsImpl) then,
  ) = __$$Message_ToolCallsImplCopyWithImpl<$Res>;
  @override
  @useResult
  $Res call({Role role, String content, List<ToolCall> toolCalls});
}

/// @nodoc
class __$$Message_ToolCallsImplCopyWithImpl<$Res>
    extends _$MessageCopyWithImpl<$Res, _$Message_ToolCallsImpl>
    implements _$$Message_ToolCallsImplCopyWith<$Res> {
  __$$Message_ToolCallsImplCopyWithImpl(
    _$Message_ToolCallsImpl _value,
    $Res Function(_$Message_ToolCallsImpl) _then,
  ) : super(_value, _then);

  /// Create a copy of Message
  /// with the given fields replaced by the non-null parameter values.
  @pragma('vm:prefer-inline')
  @override
  $Res call({
    Object? role = null,
    Object? content = null,
    Object? toolCalls = null,
  }) {
    return _then(
      _$Message_ToolCallsImpl(
        role: null == role
            ? _value.role
            : role // ignore: cast_nullable_to_non_nullable
                  as Role,
        content: null == content
            ? _value.content
            : content // ignore: cast_nullable_to_non_nullable
                  as String,
        toolCalls: null == toolCalls
            ? _value._toolCalls
            : toolCalls // ignore: cast_nullable_to_non_nullable
                  as List<ToolCall>,
      ),
    );
  }
}

/// @nodoc

class _$Message_ToolCallsImpl extends Message_ToolCalls {
  const _$Message_ToolCallsImpl({
    required this.role,
    required this.content,
    required final List<ToolCall> toolCalls,
  }) : _toolCalls = toolCalls,
       super._();

  @override
  final Role role;
  @override
  final String content;
  final List<ToolCall> _toolCalls;
  @override
  List<ToolCall> get toolCalls {
    if (_toolCalls is EqualUnmodifiableListView) return _toolCalls;
    // ignore: implicit_dynamic_type
    return EqualUnmodifiableListView(_toolCalls);
  }

  @override
  String toString() {
    return 'Message.toolCalls(role: $role, content: $content, toolCalls: $toolCalls)';
  }

  @override
  bool operator ==(Object other) {
    return identical(this, other) ||
        (other.runtimeType == runtimeType &&
            other is _$Message_ToolCallsImpl &&
            (identical(other.role, role) || other.role == role) &&
            (identical(other.content, content) || other.content == content) &&
            const DeepCollectionEquality().equals(
              other._toolCalls,
              _toolCalls,
            ));
  }

  @override
  int get hashCode => Object.hash(
    runtimeType,
    role,
    content,
    const DeepCollectionEquality().hash(_toolCalls),
  );

  /// Create a copy of Message
  /// with the given fields replaced by the non-null parameter values.
  @JsonKey(includeFromJson: false, includeToJson: false)
  @override
  @pragma('vm:prefer-inline')
  _$$Message_ToolCallsImplCopyWith<_$Message_ToolCallsImpl> get copyWith =>
      __$$Message_ToolCallsImplCopyWithImpl<_$Message_ToolCallsImpl>(
        this,
        _$identity,
      );

  @override
  @optionalTypeArgs
  TResult when<TResult extends Object?>({
    required TResult Function(Role role, String content, List<String> assetIds)
    message,
    required TResult Function(
      Role role,
      String content,
      List<ToolCall> toolCalls,
    )
    toolCalls,
    required TResult Function(Role role, String name, String content) toolResp,
  }) {
    return toolCalls(role, content, this.toolCalls);
  }

  @override
  @optionalTypeArgs
  TResult? whenOrNull<TResult extends Object?>({
    TResult? Function(Role role, String content, List<String> assetIds)?
    message,
    TResult? Function(Role role, String content, List<ToolCall> toolCalls)?
    toolCalls,
    TResult? Function(Role role, String name, String content)? toolResp,
  }) {
    return toolCalls?.call(role, content, this.toolCalls);
  }

  @override
  @optionalTypeArgs
  TResult maybeWhen<TResult extends Object?>({
    TResult Function(Role role, String content, List<String> assetIds)? message,
    TResult Function(Role role, String content, List<ToolCall> toolCalls)?
    toolCalls,
    TResult Function(Role role, String name, String content)? toolResp,
    required TResult orElse(),
  }) {
    if (toolCalls != null) {
      return toolCalls(role, content, this.toolCalls);
    }
    return orElse();
  }

  @override
  @optionalTypeArgs
  TResult map<TResult extends Object?>({
    required TResult Function(Message_Message value) message,
    required TResult Function(Message_ToolCalls value) toolCalls,
    required TResult Function(Message_ToolResp value) toolResp,
  }) {
    return toolCalls(this);
  }

  @override
  @optionalTypeArgs
  TResult? mapOrNull<TResult extends Object?>({
    TResult? Function(Message_Message value)? message,
    TResult? Function(Message_ToolCalls value)? toolCalls,
    TResult? Function(Message_ToolResp value)? toolResp,
  }) {
    return toolCalls?.call(this);
  }

  @override
  @optionalTypeArgs
  TResult maybeMap<TResult extends Object?>({
    TResult Function(Message_Message value)? message,
    TResult Function(Message_ToolCalls value)? toolCalls,
    TResult Function(Message_ToolResp value)? toolResp,
    required TResult orElse(),
  }) {
    if (toolCalls != null) {
      return toolCalls(this);
    }
    return orElse();
  }
}

abstract class Message_ToolCalls extends Message {
  const factory Message_ToolCalls({
    required final Role role,
    required final String content,
    required final List<ToolCall> toolCalls,
  }) = _$Message_ToolCallsImpl;
  const Message_ToolCalls._() : super._();

  @override
  Role get role;
  @override
  String get content;
  List<ToolCall> get toolCalls;

  /// Create a copy of Message
  /// with the given fields replaced by the non-null parameter values.
  @override
  @JsonKey(includeFromJson: false, includeToJson: false)
  _$$Message_ToolCallsImplCopyWith<_$Message_ToolCallsImpl> get copyWith =>
      throw _privateConstructorUsedError;
}

/// @nodoc
abstract class _$$Message_ToolRespImplCopyWith<$Res>
    implements $MessageCopyWith<$Res> {
  factory _$$Message_ToolRespImplCopyWith(
    _$Message_ToolRespImpl value,
    $Res Function(_$Message_ToolRespImpl) then,
  ) = __$$Message_ToolRespImplCopyWithImpl<$Res>;
  @override
  @useResult
  $Res call({Role role, String name, String content});
}

/// @nodoc
class __$$Message_ToolRespImplCopyWithImpl<$Res>
    extends _$MessageCopyWithImpl<$Res, _$Message_ToolRespImpl>
    implements _$$Message_ToolRespImplCopyWith<$Res> {
  __$$Message_ToolRespImplCopyWithImpl(
    _$Message_ToolRespImpl _value,
    $Res Function(_$Message_ToolRespImpl) _then,
  ) : super(_value, _then);

  /// Create a copy of Message
  /// with the given fields replaced by the non-null parameter values.
  @pragma('vm:prefer-inline')
  @override
  $Res call({
    Object? role = null,
    Object? name = null,
    Object? content = null,
  }) {
    return _then(
      _$Message_ToolRespImpl(
        role: null == role
            ? _value.role
            : role // ignore: cast_nullable_to_non_nullable
                  as Role,
        name: null == name
            ? _value.name
            : name // ignore: cast_nullable_to_non_nullable
                  as String,
        content: null == content
            ? _value.content
            : content // ignore: cast_nullable_to_non_nullable
                  as String,
      ),
    );
  }
}

/// @nodoc

class _$Message_ToolRespImpl extends Message_ToolResp {
  const _$Message_ToolRespImpl({
    required this.role,
    required this.name,
    required this.content,
  }) : super._();

  @override
  final Role role;
  @override
  final String name;
  @override
  final String content;

  @override
  String toString() {
    return 'Message.toolResp(role: $role, name: $name, content: $content)';
  }

  @override
  bool operator ==(Object other) {
    return identical(this, other) ||
        (other.runtimeType == runtimeType &&
            other is _$Message_ToolRespImpl &&
            (identical(other.role, role) || other.role == role) &&
            (identical(other.name, name) || other.name == name) &&
            (identical(other.content, content) || other.content == content));
  }

  @override
  int get hashCode => Object.hash(runtimeType, role, name, content);

  /// Create a copy of Message
  /// with the given fields replaced by the non-null parameter values.
  @JsonKey(includeFromJson: false, includeToJson: false)
  @override
  @pragma('vm:prefer-inline')
  _$$Message_ToolRespImplCopyWith<_$Message_ToolRespImpl> get copyWith =>
      __$$Message_ToolRespImplCopyWithImpl<_$Message_ToolRespImpl>(
        this,
        _$identity,
      );

  @override
  @optionalTypeArgs
  TResult when<TResult extends Object?>({
    required TResult Function(Role role, String content, List<String> assetIds)
    message,
    required TResult Function(
      Role role,
      String content,
      List<ToolCall> toolCalls,
    )
    toolCalls,
    required TResult Function(Role role, String name, String content) toolResp,
  }) {
    return toolResp(role, name, content);
  }

  @override
  @optionalTypeArgs
  TResult? whenOrNull<TResult extends Object?>({
    TResult? Function(Role role, String content, List<String> assetIds)?
    message,
    TResult? Function(Role role, String content, List<ToolCall> toolCalls)?
    toolCalls,
    TResult? Function(Role role, String name, String content)? toolResp,
  }) {
    return toolResp?.call(role, name, content);
  }

  @override
  @optionalTypeArgs
  TResult maybeWhen<TResult extends Object?>({
    TResult Function(Role role, String content, List<String> assetIds)? message,
    TResult Function(Role role, String content, List<ToolCall> toolCalls)?
    toolCalls,
    TResult Function(Role role, String name, String content)? toolResp,
    required TResult orElse(),
  }) {
    if (toolResp != null) {
      return toolResp(role, name, content);
    }
    return orElse();
  }

  @override
  @optionalTypeArgs
  TResult map<TResult extends Object?>({
    required TResult Function(Message_Message value) message,
    required TResult Function(Message_ToolCalls value) toolCalls,
    required TResult Function(Message_ToolResp value) toolResp,
  }) {
    return toolResp(this);
  }

  @override
  @optionalTypeArgs
  TResult? mapOrNull<TResult extends Object?>({
    TResult? Function(Message_Message value)? message,
    TResult? Function(Message_ToolCalls value)? toolCalls,
    TResult? Function(Message_ToolResp value)? toolResp,
  }) {
    return toolResp?.call(this);
  }

  @override
  @optionalTypeArgs
  TResult maybeMap<TResult extends Object?>({
    TResult Function(Message_Message value)? message,
    TResult Function(Message_ToolCalls value)? toolCalls,
    TResult Function(Message_ToolResp value)? toolResp,
    required TResult orElse(),
  }) {
    if (toolResp != null) {
      return toolResp(this);
    }
    return orElse();
  }
}

abstract class Message_ToolResp extends Message {
  const factory Message_ToolResp({
    required final Role role,
    required final String name,
    required final String content,
  }) = _$Message_ToolRespImpl;
  const Message_ToolResp._() : super._();

  @override
  Role get role;
  String get name;
  @override
  String get content;

  /// Create a copy of Message
  /// with the given fields replaced by the non-null parameter values.
  @override
  @JsonKey(includeFromJson: false, includeToJson: false)
  _$$Message_ToolRespImplCopyWith<_$Message_ToolRespImpl> get copyWith =>
      throw _privateConstructorUsedError;
}

/// @nodoc
mixin _$PromptPart {
  @optionalTypeArgs
  TResult when<TResult extends Object?>({
    required TResult Function(String content) text,
    required TResult Function(String path) image,
  }) => throw _privateConstructorUsedError;
  @optionalTypeArgs
  TResult? whenOrNull<TResult extends Object?>({
    TResult? Function(String content)? text,
    TResult? Function(String path)? image,
  }) => throw _privateConstructorUsedError;
  @optionalTypeArgs
  TResult maybeWhen<TResult extends Object?>({
    TResult Function(String content)? text,
    TResult Function(String path)? image,
    required TResult orElse(),
  }) => throw _privateConstructorUsedError;
  @optionalTypeArgs
  TResult map<TResult extends Object?>({
    required TResult Function(PromptPart_Text value) text,
    required TResult Function(PromptPart_Image value) image,
  }) => throw _privateConstructorUsedError;
  @optionalTypeArgs
  TResult? mapOrNull<TResult extends Object?>({
    TResult? Function(PromptPart_Text value)? text,
    TResult? Function(PromptPart_Image value)? image,
  }) => throw _privateConstructorUsedError;
  @optionalTypeArgs
  TResult maybeMap<TResult extends Object?>({
    TResult Function(PromptPart_Text value)? text,
    TResult Function(PromptPart_Image value)? image,
    required TResult orElse(),
  }) => throw _privateConstructorUsedError;
}

/// @nodoc
abstract class $PromptPartCopyWith<$Res> {
  factory $PromptPartCopyWith(
    PromptPart value,
    $Res Function(PromptPart) then,
  ) = _$PromptPartCopyWithImpl<$Res, PromptPart>;
}

/// @nodoc
class _$PromptPartCopyWithImpl<$Res, $Val extends PromptPart>
    implements $PromptPartCopyWith<$Res> {
  _$PromptPartCopyWithImpl(this._value, this._then);

  // ignore: unused_field
  final $Val _value;
  // ignore: unused_field
  final $Res Function($Val) _then;

  /// Create a copy of PromptPart
  /// with the given fields replaced by the non-null parameter values.
}

/// @nodoc
abstract class _$$PromptPart_TextImplCopyWith<$Res> {
  factory _$$PromptPart_TextImplCopyWith(
    _$PromptPart_TextImpl value,
    $Res Function(_$PromptPart_TextImpl) then,
  ) = __$$PromptPart_TextImplCopyWithImpl<$Res>;
  @useResult
  $Res call({String content});
}

/// @nodoc
class __$$PromptPart_TextImplCopyWithImpl<$Res>
    extends _$PromptPartCopyWithImpl<$Res, _$PromptPart_TextImpl>
    implements _$$PromptPart_TextImplCopyWith<$Res> {
  __$$PromptPart_TextImplCopyWithImpl(
    _$PromptPart_TextImpl _value,
    $Res Function(_$PromptPart_TextImpl) _then,
  ) : super(_value, _then);

  /// Create a copy of PromptPart
  /// with the given fields replaced by the non-null parameter values.
  @pragma('vm:prefer-inline')
  @override
  $Res call({Object? content = null}) {
    return _then(
      _$PromptPart_TextImpl(
        content: null == content
            ? _value.content
            : content // ignore: cast_nullable_to_non_nullable
                  as String,
      ),
    );
  }
}

/// @nodoc

class _$PromptPart_TextImpl extends PromptPart_Text {
  const _$PromptPart_TextImpl({required this.content}) : super._();

  @override
  final String content;

  @override
  String toString() {
    return 'PromptPart.text(content: $content)';
  }

  @override
  bool operator ==(Object other) {
    return identical(this, other) ||
        (other.runtimeType == runtimeType &&
            other is _$PromptPart_TextImpl &&
            (identical(other.content, content) || other.content == content));
  }

  @override
  int get hashCode => Object.hash(runtimeType, content);

  /// Create a copy of PromptPart
  /// with the given fields replaced by the non-null parameter values.
  @JsonKey(includeFromJson: false, includeToJson: false)
  @override
  @pragma('vm:prefer-inline')
  _$$PromptPart_TextImplCopyWith<_$PromptPart_TextImpl> get copyWith =>
      __$$PromptPart_TextImplCopyWithImpl<_$PromptPart_TextImpl>(
        this,
        _$identity,
      );

  @override
  @optionalTypeArgs
  TResult when<TResult extends Object?>({
    required TResult Function(String content) text,
    required TResult Function(String path) image,
  }) {
    return text(content);
  }

  @override
  @optionalTypeArgs
  TResult? whenOrNull<TResult extends Object?>({
    TResult? Function(String content)? text,
    TResult? Function(String path)? image,
  }) {
    return text?.call(content);
  }

  @override
  @optionalTypeArgs
  TResult maybeWhen<TResult extends Object?>({
    TResult Function(String content)? text,
    TResult Function(String path)? image,
    required TResult orElse(),
  }) {
    if (text != null) {
      return text(content);
    }
    return orElse();
  }

  @override
  @optionalTypeArgs
  TResult map<TResult extends Object?>({
    required TResult Function(PromptPart_Text value) text,
    required TResult Function(PromptPart_Image value) image,
  }) {
    return text(this);
  }

  @override
  @optionalTypeArgs
  TResult? mapOrNull<TResult extends Object?>({
    TResult? Function(PromptPart_Text value)? text,
    TResult? Function(PromptPart_Image value)? image,
  }) {
    return text?.call(this);
  }

  @override
  @optionalTypeArgs
  TResult maybeMap<TResult extends Object?>({
    TResult Function(PromptPart_Text value)? text,
    TResult Function(PromptPart_Image value)? image,
    required TResult orElse(),
  }) {
    if (text != null) {
      return text(this);
    }
    return orElse();
  }
}

abstract class PromptPart_Text extends PromptPart {
  const factory PromptPart_Text({required final String content}) =
      _$PromptPart_TextImpl;
  const PromptPart_Text._() : super._();

  String get content;

  /// Create a copy of PromptPart
  /// with the given fields replaced by the non-null parameter values.
  @JsonKey(includeFromJson: false, includeToJson: false)
  _$$PromptPart_TextImplCopyWith<_$PromptPart_TextImpl> get copyWith =>
      throw _privateConstructorUsedError;
}

/// @nodoc
abstract class _$$PromptPart_ImageImplCopyWith<$Res> {
  factory _$$PromptPart_ImageImplCopyWith(
    _$PromptPart_ImageImpl value,
    $Res Function(_$PromptPart_ImageImpl) then,
  ) = __$$PromptPart_ImageImplCopyWithImpl<$Res>;
  @useResult
  $Res call({String path});
}

/// @nodoc
class __$$PromptPart_ImageImplCopyWithImpl<$Res>
    extends _$PromptPartCopyWithImpl<$Res, _$PromptPart_ImageImpl>
    implements _$$PromptPart_ImageImplCopyWith<$Res> {
  __$$PromptPart_ImageImplCopyWithImpl(
    _$PromptPart_ImageImpl _value,
    $Res Function(_$PromptPart_ImageImpl) _then,
  ) : super(_value, _then);

  /// Create a copy of PromptPart
  /// with the given fields replaced by the non-null parameter values.
  @pragma('vm:prefer-inline')
  @override
  $Res call({Object? path = null}) {
    return _then(
      _$PromptPart_ImageImpl(
        path: null == path
            ? _value.path
            : path // ignore: cast_nullable_to_non_nullable
                  as String,
      ),
    );
  }
}

/// @nodoc

class _$PromptPart_ImageImpl extends PromptPart_Image {
  const _$PromptPart_ImageImpl({required this.path}) : super._();

  @override
  final String path;

  @override
  String toString() {
    return 'PromptPart.image(path: $path)';
  }

  @override
  bool operator ==(Object other) {
    return identical(this, other) ||
        (other.runtimeType == runtimeType &&
            other is _$PromptPart_ImageImpl &&
            (identical(other.path, path) || other.path == path));
  }

  @override
  int get hashCode => Object.hash(runtimeType, path);

  /// Create a copy of PromptPart
  /// with the given fields replaced by the non-null parameter values.
  @JsonKey(includeFromJson: false, includeToJson: false)
  @override
  @pragma('vm:prefer-inline')
  _$$PromptPart_ImageImplCopyWith<_$PromptPart_ImageImpl> get copyWith =>
      __$$PromptPart_ImageImplCopyWithImpl<_$PromptPart_ImageImpl>(
        this,
        _$identity,
      );

  @override
  @optionalTypeArgs
  TResult when<TResult extends Object?>({
    required TResult Function(String content) text,
    required TResult Function(String path) image,
  }) {
    return image(path);
  }

  @override
  @optionalTypeArgs
  TResult? whenOrNull<TResult extends Object?>({
    TResult? Function(String content)? text,
    TResult? Function(String path)? image,
  }) {
    return image?.call(path);
  }

  @override
  @optionalTypeArgs
  TResult maybeWhen<TResult extends Object?>({
    TResult Function(String content)? text,
    TResult Function(String path)? image,
    required TResult orElse(),
  }) {
    if (image != null) {
      return image(path);
    }
    return orElse();
  }

  @override
  @optionalTypeArgs
  TResult map<TResult extends Object?>({
    required TResult Function(PromptPart_Text value) text,
    required TResult Function(PromptPart_Image value) image,
  }) {
    return image(this);
  }

  @override
  @optionalTypeArgs
  TResult? mapOrNull<TResult extends Object?>({
    TResult? Function(PromptPart_Text value)? text,
    TResult? Function(PromptPart_Image value)? image,
  }) {
    return image?.call(this);
  }

  @override
  @optionalTypeArgs
  TResult maybeMap<TResult extends Object?>({
    TResult Function(PromptPart_Text value)? text,
    TResult Function(PromptPart_Image value)? image,
    required TResult orElse(),
  }) {
    if (image != null) {
      return image(this);
    }
    return orElse();
  }
}

abstract class PromptPart_Image extends PromptPart {
  const factory PromptPart_Image({required final String path}) =
      _$PromptPart_ImageImpl;
  const PromptPart_Image._() : super._();

  String get path;

  /// Create a copy of PromptPart
  /// with the given fields replaced by the non-null parameter values.
  @JsonKey(includeFromJson: false, includeToJson: false)
  _$$PromptPart_ImageImplCopyWith<_$PromptPart_ImageImpl> get copyWith =>
      throw _privateConstructorUsedError;
}

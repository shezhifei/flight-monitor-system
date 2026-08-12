// GENERATED CODE - DO NOT MODIFY BY HAND
// coverage:ignore-file
// ignore_for_file: type=lint
// ignore_for_file: unused_element, deprecated_member_use, deprecated_member_use_from_same_package, use_function_type_syntax_for_parameters, unnecessary_const, avoid_init_to_null, invalid_override_different_default_values_named, prefer_expression_function_bodies, annotate_overrides, invalid_annotation_target, unnecessary_question_mark

part of 'api.dart';

// **************************************************************************
// FreezedGenerator
// **************************************************************************

// dart format off
T _$identity<T>(T value) => value;
/// @nodoc
mixin _$SseConnectionState {





@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is SseConnectionState);
}


@override
int get hashCode => runtimeType.hashCode;

@override
String toString() {
  return 'SseConnectionState()';
}


}

/// @nodoc
class $SseConnectionStateCopyWith<$Res>  {
$SseConnectionStateCopyWith(SseConnectionState _, $Res Function(SseConnectionState) __);
}


/// Adds pattern-matching-related methods to [SseConnectionState].
extension SseConnectionStatePatterns on SseConnectionState {
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

@optionalTypeArgs TResult maybeMap<TResult extends Object?>({TResult Function( SseConnectionState_Connecting value)?  connecting,TResult Function( SseConnectionState_Connected value)?  connected,TResult Function( SseConnectionState_Disconnected value)?  disconnected,required TResult orElse(),}){
final _that = this;
switch (_that) {
case SseConnectionState_Connecting() when connecting != null:
return connecting(_that);case SseConnectionState_Connected() when connected != null:
return connected(_that);case SseConnectionState_Disconnected() when disconnected != null:
return disconnected(_that);case _:
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

@optionalTypeArgs TResult map<TResult extends Object?>({required TResult Function( SseConnectionState_Connecting value)  connecting,required TResult Function( SseConnectionState_Connected value)  connected,required TResult Function( SseConnectionState_Disconnected value)  disconnected,}){
final _that = this;
switch (_that) {
case SseConnectionState_Connecting():
return connecting(_that);case SseConnectionState_Connected():
return connected(_that);case SseConnectionState_Disconnected():
return disconnected(_that);}
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

@optionalTypeArgs TResult? mapOrNull<TResult extends Object?>({TResult? Function( SseConnectionState_Connecting value)?  connecting,TResult? Function( SseConnectionState_Connected value)?  connected,TResult? Function( SseConnectionState_Disconnected value)?  disconnected,}){
final _that = this;
switch (_that) {
case SseConnectionState_Connecting() when connecting != null:
return connecting(_that);case SseConnectionState_Connected() when connected != null:
return connected(_that);case SseConnectionState_Disconnected() when disconnected != null:
return disconnected(_that);case _:
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

@optionalTypeArgs TResult maybeWhen<TResult extends Object?>({TResult Function()?  connecting,TResult Function()?  connected,TResult Function( String reason)?  disconnected,required TResult orElse(),}) {final _that = this;
switch (_that) {
case SseConnectionState_Connecting() when connecting != null:
return connecting();case SseConnectionState_Connected() when connected != null:
return connected();case SseConnectionState_Disconnected() when disconnected != null:
return disconnected(_that.reason);case _:
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

@optionalTypeArgs TResult when<TResult extends Object?>({required TResult Function()  connecting,required TResult Function()  connected,required TResult Function( String reason)  disconnected,}) {final _that = this;
switch (_that) {
case SseConnectionState_Connecting():
return connecting();case SseConnectionState_Connected():
return connected();case SseConnectionState_Disconnected():
return disconnected(_that.reason);}
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

@optionalTypeArgs TResult? whenOrNull<TResult extends Object?>({TResult? Function()?  connecting,TResult? Function()?  connected,TResult? Function( String reason)?  disconnected,}) {final _that = this;
switch (_that) {
case SseConnectionState_Connecting() when connecting != null:
return connecting();case SseConnectionState_Connected() when connected != null:
return connected();case SseConnectionState_Disconnected() when disconnected != null:
return disconnected(_that.reason);case _:
  return null;

}
}

}

/// @nodoc


class SseConnectionState_Connecting extends SseConnectionState {
  const SseConnectionState_Connecting(): super._();
  






@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is SseConnectionState_Connecting);
}


@override
int get hashCode => runtimeType.hashCode;

@override
String toString() {
  return 'SseConnectionState.connecting()';
}


}




/// @nodoc


class SseConnectionState_Connected extends SseConnectionState {
  const SseConnectionState_Connected(): super._();
  






@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is SseConnectionState_Connected);
}


@override
int get hashCode => runtimeType.hashCode;

@override
String toString() {
  return 'SseConnectionState.connected()';
}


}




/// @nodoc


class SseConnectionState_Disconnected extends SseConnectionState {
  const SseConnectionState_Disconnected({required this.reason}): super._();
  

 final  String reason;

/// Create a copy of SseConnectionState
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$SseConnectionState_DisconnectedCopyWith<SseConnectionState_Disconnected> get copyWith => _$SseConnectionState_DisconnectedCopyWithImpl<SseConnectionState_Disconnected>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is SseConnectionState_Disconnected&&(identical(other.reason, reason) || other.reason == reason));
}


@override
int get hashCode => Object.hash(runtimeType,reason);

@override
String toString() {
  return 'SseConnectionState.disconnected(reason: $reason)';
}


}

/// @nodoc
abstract mixin class $SseConnectionState_DisconnectedCopyWith<$Res> implements $SseConnectionStateCopyWith<$Res> {
  factory $SseConnectionState_DisconnectedCopyWith(SseConnectionState_Disconnected value, $Res Function(SseConnectionState_Disconnected) _then) = _$SseConnectionState_DisconnectedCopyWithImpl;
@useResult
$Res call({
 String reason
});




}
/// @nodoc
class _$SseConnectionState_DisconnectedCopyWithImpl<$Res>
    implements $SseConnectionState_DisconnectedCopyWith<$Res> {
  _$SseConnectionState_DisconnectedCopyWithImpl(this._self, this._then);

  final SseConnectionState_Disconnected _self;
  final $Res Function(SseConnectionState_Disconnected) _then;

/// Create a copy of SseConnectionState
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? reason = null,}) {
  return _then(SseConnectionState_Disconnected(
reason: null == reason ? _self.reason : reason // ignore: cast_nullable_to_non_nullable
as String,
  ));
}


}

/// @nodoc
mixin _$SseUpdate {

 Object get field0;



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is SseUpdate&&const DeepCollectionEquality().equals(other.field0, field0));
}


@override
int get hashCode => Object.hash(runtimeType,const DeepCollectionEquality().hash(field0));

@override
String toString() {
  return 'SseUpdate(field0: $field0)';
}


}

/// @nodoc
class $SseUpdateCopyWith<$Res>  {
$SseUpdateCopyWith(SseUpdate _, $Res Function(SseUpdate) __);
}


/// Adds pattern-matching-related methods to [SseUpdate].
extension SseUpdatePatterns on SseUpdate {
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

@optionalTypeArgs TResult maybeMap<TResult extends Object?>({TResult Function( SseUpdate_State value)?  state,TResult Function( SseUpdate_Event value)?  event,required TResult orElse(),}){
final _that = this;
switch (_that) {
case SseUpdate_State() when state != null:
return state(_that);case SseUpdate_Event() when event != null:
return event(_that);case _:
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

@optionalTypeArgs TResult map<TResult extends Object?>({required TResult Function( SseUpdate_State value)  state,required TResult Function( SseUpdate_Event value)  event,}){
final _that = this;
switch (_that) {
case SseUpdate_State():
return state(_that);case SseUpdate_Event():
return event(_that);}
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

@optionalTypeArgs TResult? mapOrNull<TResult extends Object?>({TResult? Function( SseUpdate_State value)?  state,TResult? Function( SseUpdate_Event value)?  event,}){
final _that = this;
switch (_that) {
case SseUpdate_State() when state != null:
return state(_that);case SseUpdate_Event() when event != null:
return event(_that);case _:
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

@optionalTypeArgs TResult maybeWhen<TResult extends Object?>({TResult Function( SseConnectionState field0)?  state,TResult Function( SseEvent field0)?  event,required TResult orElse(),}) {final _that = this;
switch (_that) {
case SseUpdate_State() when state != null:
return state(_that.field0);case SseUpdate_Event() when event != null:
return event(_that.field0);case _:
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

@optionalTypeArgs TResult when<TResult extends Object?>({required TResult Function( SseConnectionState field0)  state,required TResult Function( SseEvent field0)  event,}) {final _that = this;
switch (_that) {
case SseUpdate_State():
return state(_that.field0);case SseUpdate_Event():
return event(_that.field0);}
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

@optionalTypeArgs TResult? whenOrNull<TResult extends Object?>({TResult? Function( SseConnectionState field0)?  state,TResult? Function( SseEvent field0)?  event,}) {final _that = this;
switch (_that) {
case SseUpdate_State() when state != null:
return state(_that.field0);case SseUpdate_Event() when event != null:
return event(_that.field0);case _:
  return null;

}
}

}

/// @nodoc


class SseUpdate_State extends SseUpdate {
  const SseUpdate_State(this.field0): super._();
  

@override final  SseConnectionState field0;

/// Create a copy of SseUpdate
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$SseUpdate_StateCopyWith<SseUpdate_State> get copyWith => _$SseUpdate_StateCopyWithImpl<SseUpdate_State>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is SseUpdate_State&&(identical(other.field0, field0) || other.field0 == field0));
}


@override
int get hashCode => Object.hash(runtimeType,field0);

@override
String toString() {
  return 'SseUpdate.state(field0: $field0)';
}


}

/// @nodoc
abstract mixin class $SseUpdate_StateCopyWith<$Res> implements $SseUpdateCopyWith<$Res> {
  factory $SseUpdate_StateCopyWith(SseUpdate_State value, $Res Function(SseUpdate_State) _then) = _$SseUpdate_StateCopyWithImpl;
@useResult
$Res call({
 SseConnectionState field0
});


$SseConnectionStateCopyWith<$Res> get field0;

}
/// @nodoc
class _$SseUpdate_StateCopyWithImpl<$Res>
    implements $SseUpdate_StateCopyWith<$Res> {
  _$SseUpdate_StateCopyWithImpl(this._self, this._then);

  final SseUpdate_State _self;
  final $Res Function(SseUpdate_State) _then;

/// Create a copy of SseUpdate
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? field0 = null,}) {
  return _then(SseUpdate_State(
null == field0 ? _self.field0 : field0 // ignore: cast_nullable_to_non_nullable
as SseConnectionState,
  ));
}

/// Create a copy of SseUpdate
/// with the given fields replaced by the non-null parameter values.
@override
@pragma('vm:prefer-inline')
$SseConnectionStateCopyWith<$Res> get field0 {
  
  return $SseConnectionStateCopyWith<$Res>(_self.field0, (value) {
    return _then(_self.copyWith(field0: value));
  });
}
}

/// @nodoc


class SseUpdate_Event extends SseUpdate {
  const SseUpdate_Event(this.field0): super._();
  

@override final  SseEvent field0;

/// Create a copy of SseUpdate
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$SseUpdate_EventCopyWith<SseUpdate_Event> get copyWith => _$SseUpdate_EventCopyWithImpl<SseUpdate_Event>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is SseUpdate_Event&&(identical(other.field0, field0) || other.field0 == field0));
}


@override
int get hashCode => Object.hash(runtimeType,field0);

@override
String toString() {
  return 'SseUpdate.event(field0: $field0)';
}


}

/// @nodoc
abstract mixin class $SseUpdate_EventCopyWith<$Res> implements $SseUpdateCopyWith<$Res> {
  factory $SseUpdate_EventCopyWith(SseUpdate_Event value, $Res Function(SseUpdate_Event) _then) = _$SseUpdate_EventCopyWithImpl;
@useResult
$Res call({
 SseEvent field0
});




}
/// @nodoc
class _$SseUpdate_EventCopyWithImpl<$Res>
    implements $SseUpdate_EventCopyWith<$Res> {
  _$SseUpdate_EventCopyWithImpl(this._self, this._then);

  final SseUpdate_Event _self;
  final $Res Function(SseUpdate_Event) _then;

/// Create a copy of SseUpdate
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? field0 = null,}) {
  return _then(SseUpdate_Event(
null == field0 ? _self.field0 : field0 // ignore: cast_nullable_to_non_nullable
as SseEvent,
  ));
}


}

// dart format on

// GENERATED CODE - DO NOT MODIFY BY HAND
// coverage:ignore-file
// ignore_for_file: type=lint
// ignore_for_file: unused_element, deprecated_member_use, deprecated_member_use_from_same_package, use_function_type_syntax_for_parameters, unnecessary_const, avoid_init_to_null, invalid_override_different_default_values_named, prefer_expression_function_bodies, annotate_overrides, invalid_annotation_target, unnecessary_question_mark

part of 'session.dart';

// **************************************************************************
// FreezedGenerator
// **************************************************************************

// dart format off
T _$identity<T>(T value) => value;
/// @nodoc
mixin _$SessionState {





@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is SessionState);
}


@override
int get hashCode => runtimeType.hashCode;

@override
String toString() {
  return 'SessionState()';
}


}

/// @nodoc
class $SessionStateCopyWith<$Res>  {
$SessionStateCopyWith(SessionState _, $Res Function(SessionState) __);
}


/// Adds pattern-matching-related methods to [SessionState].
extension SessionStatePatterns on SessionState {
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

@optionalTypeArgs TResult maybeMap<TResult extends Object?>({TResult Function( SessionState_Anonymous value)?  anonymous,TResult Function( SessionState_Active value)?  active,required TResult orElse(),}){
final _that = this;
switch (_that) {
case SessionState_Anonymous() when anonymous != null:
return anonymous(_that);case SessionState_Active() when active != null:
return active(_that);case _:
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

@optionalTypeArgs TResult map<TResult extends Object?>({required TResult Function( SessionState_Anonymous value)  anonymous,required TResult Function( SessionState_Active value)  active,}){
final _that = this;
switch (_that) {
case SessionState_Anonymous():
return anonymous(_that);case SessionState_Active():
return active(_that);}
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

@optionalTypeArgs TResult? mapOrNull<TResult extends Object?>({TResult? Function( SessionState_Anonymous value)?  anonymous,TResult? Function( SessionState_Active value)?  active,}){
final _that = this;
switch (_that) {
case SessionState_Anonymous() when anonymous != null:
return anonymous(_that);case SessionState_Active() when active != null:
return active(_that);case _:
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

@optionalTypeArgs TResult maybeWhen<TResult extends Object?>({TResult Function()?  anonymous,TResult Function( PlatformInt64 accessExpireAt)?  active,required TResult orElse(),}) {final _that = this;
switch (_that) {
case SessionState_Anonymous() when anonymous != null:
return anonymous();case SessionState_Active() when active != null:
return active(_that.accessExpireAt);case _:
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

@optionalTypeArgs TResult when<TResult extends Object?>({required TResult Function()  anonymous,required TResult Function( PlatformInt64 accessExpireAt)  active,}) {final _that = this;
switch (_that) {
case SessionState_Anonymous():
return anonymous();case SessionState_Active():
return active(_that.accessExpireAt);}
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

@optionalTypeArgs TResult? whenOrNull<TResult extends Object?>({TResult? Function()?  anonymous,TResult? Function( PlatformInt64 accessExpireAt)?  active,}) {final _that = this;
switch (_that) {
case SessionState_Anonymous() when anonymous != null:
return anonymous();case SessionState_Active() when active != null:
return active(_that.accessExpireAt);case _:
  return null;

}
}

}

/// @nodoc


class SessionState_Anonymous extends SessionState {
  const SessionState_Anonymous(): super._();
  






@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is SessionState_Anonymous);
}


@override
int get hashCode => runtimeType.hashCode;

@override
String toString() {
  return 'SessionState.anonymous()';
}


}




/// @nodoc


class SessionState_Active extends SessionState {
  const SessionState_Active({required this.accessExpireAt}): super._();
  

 final  PlatformInt64 accessExpireAt;

/// Create a copy of SessionState
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$SessionState_ActiveCopyWith<SessionState_Active> get copyWith => _$SessionState_ActiveCopyWithImpl<SessionState_Active>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is SessionState_Active&&(identical(other.accessExpireAt, accessExpireAt) || other.accessExpireAt == accessExpireAt));
}


@override
int get hashCode => Object.hash(runtimeType,accessExpireAt);

@override
String toString() {
  return 'SessionState.active(accessExpireAt: $accessExpireAt)';
}


}

/// @nodoc
abstract mixin class $SessionState_ActiveCopyWith<$Res> implements $SessionStateCopyWith<$Res> {
  factory $SessionState_ActiveCopyWith(SessionState_Active value, $Res Function(SessionState_Active) _then) = _$SessionState_ActiveCopyWithImpl;
@useResult
$Res call({
 PlatformInt64 accessExpireAt
});




}
/// @nodoc
class _$SessionState_ActiveCopyWithImpl<$Res>
    implements $SessionState_ActiveCopyWith<$Res> {
  _$SessionState_ActiveCopyWithImpl(this._self, this._then);

  final SessionState_Active _self;
  final $Res Function(SessionState_Active) _then;

/// Create a copy of SessionState
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? accessExpireAt = null,}) {
  return _then(SessionState_Active(
accessExpireAt: null == accessExpireAt ? _self.accessExpireAt : accessExpireAt // ignore: cast_nullable_to_non_nullable
as PlatformInt64,
  ));
}


}

// dart format on

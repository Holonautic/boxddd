//! FFI-compatible math value types used throughout `boxddd`.

use super::*;
use crate::Foundation;
use crate::core::{callback_state, validation};
use crate::error::InvalidValueReason;

/// Returns whether a scalar is finite and accepted by Box3D validation.
#[inline]
pub fn is_valid_float(value: f32) -> bool {
    value.is_finite()
}

/// Deterministic cosine/sine pair returned by Box3D's trigonometry helpers.
#[repr(C)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Copy, Clone, Debug, Default, PartialEq)]
pub struct CosSin {
    /// Cosine component.
    pub cosine: f32,
    /// Sine component.
    pub sine: f32,
}

impl CosSin {
    /// Converts a raw Box3D cosine/sine pair into the Rust value type.
    #[inline]
    pub const fn from_raw(raw: ffi::b3CosSin) -> Self {
        Self {
            cosine: raw.cosine,
            sine: raw.sine,
        }
    }

    /// Converts this value into the raw Box3D representation.
    #[inline]
    pub const fn into_raw(self) -> ffi::b3CosSin {
        ffi::b3CosSin {
            cosine: self.cosine,
            sine: self.sine,
        }
    }

    #[inline]
    fn validate(self) -> Result<Self> {
        if is_valid_float(self.cosine) && is_valid_float(self.sine) {
            Ok(self)
        } else {
            Err(Error::NativeFailure)
        }
    }
}

/// Computes Box3D's deterministic `atan2` for valid scalar inputs.
pub fn deterministic_atan2(y: f32, x: f32) -> Result<f32> {
    callback_state::check_not_in_callback()?;
    validation::finite("deterministic_atan2.y", y)?;
    validation::finite("deterministic_atan2.x", x)?;
    let _call = Foundation::enter_transient_call()?;
    let value = unsafe { ffi::b3Atan2(y, x) };
    if is_valid_float(value) {
        Ok(value)
    } else {
        Err(Error::NativeFailure)
    }
}

/// Computes Box3D's deterministic cosine and sine for an angle in radians.
pub fn compute_cos_sin(radians: f32) -> Result<CosSin> {
    callback_state::check_not_in_callback()?;
    validation::finite("compute_cos_sin.radians", radians)?;
    let _call = Foundation::enter_transient_call()?;
    CosSin::from_raw(unsafe { ffi::b3ComputeCosSin(radians) }).validate()
}

/// Two-dimensional vector with the same layout as `b3Vec2`.
#[repr(C)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Copy, Clone, Debug, Default, PartialEq)]
pub struct Vec2 {
    /// X component.
    pub x: f32,
    /// Y component.
    pub y: f32,
}

impl Vec2 {
    /// Zero vector.
    pub const ZERO: Self = Self::new(0.0, 0.0);

    /// Creates a vector from components.
    #[inline]
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }

    /// Converts a raw Box3D vector into the Rust value type.
    #[inline]
    pub const fn from_raw(raw: ffi::b3Vec2) -> Self {
        Self { x: raw.x, y: raw.y }
    }

    /// Converts this value into the raw Box3D representation.
    #[inline]
    pub const fn into_raw(self) -> ffi::b3Vec2 {
        ffi::b3Vec2 {
            x: self.x,
            y: self.y,
        }
    }
}

impl From<[f32; 2]> for Vec2 {
    #[inline]
    fn from(value: [f32; 2]) -> Self {
        Self::new(value[0], value[1])
    }
}

impl From<(f32, f32)> for Vec2 {
    #[inline]
    fn from(value: (f32, f32)) -> Self {
        Self::new(value.0, value.1)
    }
}

/// Three-dimensional vector with the same layout as `b3Vec3`.
#[repr(C)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Copy, Clone, Debug, Default, PartialEq)]
pub struct Vec3 {
    /// X component.
    pub x: f32,
    /// Y component.
    pub y: f32,
    /// Z component.
    pub z: f32,
}

impl Vec3 {
    /// Zero vector.
    pub const ZERO: Self = Self::new(0.0, 0.0, 0.0);
    /// Unit vector along the positive X axis.
    pub const X: Self = Self::new(1.0, 0.0, 0.0);
    /// Unit vector along the positive Y axis.
    pub const Y: Self = Self::new(0.0, 1.0, 0.0);
    /// Unit vector along the positive Z axis.
    pub const Z: Self = Self::new(0.0, 0.0, 1.0);

    /// Creates a vector from components.
    #[inline]
    pub const fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }

    /// Converts a raw Box3D vector into the Rust value type.
    #[inline]
    pub const fn from_raw(raw: ffi::b3Vec3) -> Self {
        Self {
            x: raw.x,
            y: raw.y,
            z: raw.z,
        }
    }

    /// Converts this value into the raw Box3D representation.
    #[inline]
    pub const fn into_raw(self) -> ffi::b3Vec3 {
        ffi::b3Vec3 {
            x: self.x,
            y: self.y,
            z: self.z,
        }
    }

    /// Returns whether all components are valid Box3D scalars.
    #[inline]
    pub fn is_valid(self) -> bool {
        self.x.is_finite() && self.y.is_finite() && self.z.is_finite()
    }

    /// Returns this vector or [`Error::InvalidValue`] if any component is non-finite.
    #[inline]
    pub fn validate(self) -> crate::error::Result<Self> {
        validation::vec3("vec3", self)?;
        Ok(self)
    }
}

/// Closest-point result for two lines or segments.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Copy, Clone, Debug, Default, PartialEq)]
pub struct SegmentDistanceResult {
    /// Closest point on the first line or segment.
    pub point1: Vec3,
    /// Fraction along the first segment when the query is segment-based.
    pub fraction1: f32,
    /// Closest point on the second line or segment.
    pub point2: Vec3,
    /// Fraction along the second segment when the query is segment-based.
    pub fraction2: f32,
}

impl SegmentDistanceResult {
    /// Returns the squared distance between `point1` and `point2`.
    #[inline]
    pub fn distance_squared(self) -> f32 {
        let delta = Vec3::new(
            self.point2.x - self.point1.x,
            self.point2.y - self.point1.y,
            self.point2.z - self.point1.z,
        );
        vec3_length_squared(delta)
    }

    /// Returns the distance between `point1` and `point2`.
    #[inline]
    pub fn distance(self) -> f32 {
        self.distance_squared().sqrt()
    }

    #[inline]
    fn from_raw(raw: ffi::b3SegmentDistanceResult) -> Self {
        Self {
            point1: Vec3::from_raw(raw.point1),
            fraction1: raw.fraction1,
            point2: Vec3::from_raw(raw.point2),
            fraction2: raw.fraction2,
        }
    }

    #[inline]
    fn validate(self) -> crate::error::Result<Self> {
        if self.point1.is_valid()
            && self.point2.is_valid()
            && is_valid_float(self.fraction1)
            && is_valid_float(self.fraction2)
            && is_valid_float(self.distance_squared())
        {
            Ok(self)
        } else {
            Err(crate::error::Error::NativeFailure)
        }
    }
}

/// Returns the closest point on segment `a`-`b` to query point `q`.
pub fn closest_point_on_segment(
    a: impl Into<Vec3>,
    b: impl Into<Vec3>,
    q: impl Into<Vec3>,
) -> crate::error::Result<Vec3> {
    callback_state::check_not_in_callback()?;
    let a = a.into();
    let b = b.into();
    let q = q.into();
    validation::vec3("closest_point_on_segment.a", a)?;
    validation::vec3("closest_point_on_segment.b", b)?;
    validation::vec3("closest_point_on_segment.q", q)?;
    let _call = Foundation::enter_transient_call()?;
    Vec3::from_raw(unsafe {
        ffi::b3PointToSegmentDistance(a.into_raw(), b.into_raw(), q.into_raw())
    })
    .validate()
    .map_err(|_| Error::NativeFailure)
}

/// Computes closest points between two infinite lines.
///
/// Direction vectors `d1` and `d2` must be finite and non-zero.
pub fn line_distance(
    p1: impl Into<Vec3>,
    d1: impl Into<Vec3>,
    p2: impl Into<Vec3>,
    d2: impl Into<Vec3>,
) -> crate::error::Result<SegmentDistanceResult> {
    callback_state::check_not_in_callback()?;
    let p1 = p1.into();
    let d1 = d1.into();
    let p2 = p2.into();
    let d2 = d2.into();
    validation::vec3("line_distance.p1", p1)?;
    validation::vec3("line_distance.d1", d1)?;
    validation::vec3("line_distance.p2", p2)?;
    validation::vec3("line_distance.d2", d2)?;
    let d1_length_squared = vec3_length_squared(d1);
    validation::finite("line_distance.d1.length_squared", d1_length_squared)?;
    if d1_length_squared <= f32::EPSILON {
        return Err(validation::invalid(
            "line_distance.d1",
            InvalidValueReason::Malformed,
        ));
    }
    let d2_length_squared = vec3_length_squared(d2);
    validation::finite("line_distance.d2.length_squared", d2_length_squared)?;
    if d2_length_squared <= f32::EPSILON {
        return Err(validation::invalid(
            "line_distance.d2",
            InvalidValueReason::Malformed,
        ));
    }
    let _call = Foundation::enter_transient_call()?;
    SegmentDistanceResult::from_raw(unsafe {
        ffi::b3LineDistance(p1.into_raw(), d1.into_raw(), p2.into_raw(), d2.into_raw())
    })
    .validate()
}

/// Computes closest points between two finite segments.
pub fn segment_distance(
    p1: impl Into<Vec3>,
    q1: impl Into<Vec3>,
    p2: impl Into<Vec3>,
    q2: impl Into<Vec3>,
) -> crate::error::Result<SegmentDistanceResult> {
    callback_state::check_not_in_callback()?;
    let p1 = p1.into();
    let q1 = q1.into();
    let p2 = p2.into();
    let q2 = q2.into();
    validation::vec3("segment_distance.p1", p1)?;
    validation::vec3("segment_distance.q1", q1)?;
    validation::vec3("segment_distance.p2", p2)?;
    validation::vec3("segment_distance.q2", q2)?;
    let _call = Foundation::enter_transient_call()?;
    SegmentDistanceResult::from_raw(unsafe {
        ffi::b3SegmentDistance(p1.into_raw(), q1.into_raw(), p2.into_raw(), q2.into_raw())
    })
    .validate()
}

#[inline]
fn vec3_length_squared(value: Vec3) -> f32 {
    value.x * value.x + value.y * value.y + value.z * value.z
}

fn validate_unit_vec3(context: &'static str, value: Vec3) -> Result<Vec3> {
    validation::vec3(context, value)?;
    let length_squared = vec3_length_squared(value);
    validation::finite(context, length_squared)?;
    if (length_squared - 1.0).abs() < 100.0 * f32::EPSILON {
        Ok(value)
    } else {
        Err(validation::invalid(context, InvalidValueReason::Malformed))
    }
}

fn validate_rotation_matrix(matrix: Matrix3) -> Result<Matrix3> {
    let matrix = matrix.validate()?;
    validate_unit_vec3("quaternion.from_matrix.cx", matrix.cx)?;
    validate_unit_vec3("quaternion.from_matrix.cy", matrix.cy)?;
    validate_unit_vec3("quaternion.from_matrix.cz", matrix.cz)?;

    let xy = vec3_dot(matrix.cx, matrix.cy);
    let xz = vec3_dot(matrix.cx, matrix.cz);
    let yz = vec3_dot(matrix.cy, matrix.cz);
    let determinant = vec3_dot(matrix.cx, vec3_cross(matrix.cy, matrix.cz));
    let tolerance = 100.0 * f32::EPSILON;
    if xy.abs() >= tolerance
        || xz.abs() >= tolerance
        || yz.abs() >= tolerance
        || (determinant - 1.0).abs() >= tolerance
    {
        return Err(validation::invalid(
            "quaternion.from_matrix.matrix",
            InvalidValueReason::Malformed,
        ));
    }
    Ok(matrix)
}

#[inline]
fn vec3_dot(a: Vec3, b: Vec3) -> f32 {
    a.x * b.x + a.y * b.y + a.z * b.z
}

#[inline]
fn vec3_cross(a: Vec3, b: Vec3) -> Vec3 {
    Vec3::new(
        a.y * b.z - a.z * b.y,
        a.z * b.x - a.x * b.z,
        a.x * b.y - a.y * b.x,
    )
}

impl From<[f32; 3]> for Vec3 {
    #[inline]
    fn from(value: [f32; 3]) -> Self {
        Self::new(value[0], value[1], value[2])
    }
}

impl From<(f32, f32, f32)> for Vec3 {
    #[inline]
    fn from(value: (f32, f32, f32)) -> Self {
        Self::new(value.0, value.1, value.2)
    }
}

/// Quaternion with vector part `v` and scalar part `s`.
#[repr(C)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Quat {
    /// Vector component.
    pub v: Vec3,
    /// Scalar component.
    pub s: f32,
}

impl Quat {
    /// Identity rotation.
    pub const IDENTITY: Self = Self {
        v: Vec3::ZERO,
        s: 1.0,
    };

    /// Creates a quaternion from vector and scalar parts.
    #[inline]
    pub const fn new(v: Vec3, s: f32) -> Self {
        Self { v, s }
    }

    /// Converts a raw Box3D quaternion into the Rust value type.
    #[inline]
    pub const fn from_raw(raw: ffi::b3Quat) -> Self {
        Self {
            v: Vec3::from_raw(raw.v),
            s: raw.s,
        }
    }

    /// Converts this value into the raw Box3D representation.
    #[inline]
    pub const fn into_raw(self) -> ffi::b3Quat {
        ffi::b3Quat {
            v: self.v.into_raw(),
            s: self.s,
        }
    }

    /// Returns whether this quaternion is valid according to Box3D.
    #[inline]
    pub fn is_valid(self) -> bool {
        let length_squared = vec3_length_squared(self.v) + self.s * self.s;
        self.v.is_valid()
            && self.s.is_finite()
            && 1.0 - 20.0 * f32::EPSILON < length_squared
            && length_squared < 1.0 + 20.0 * f32::EPSILON
    }

    /// Returns this quaternion or [`Error::InvalidValue`] if it is non-finite or malformed.
    #[inline]
    pub fn validate(self) -> crate::error::Result<Self> {
        validation::quaternion("quaternion", self)?;
        Ok(self)
    }

    /// Builds a quaternion from a valid 3x3 rotation matrix.
    pub fn from_matrix(matrix: Matrix3) -> Result<Self> {
        callback_state::check_not_in_callback()?;
        let matrix = validate_rotation_matrix(matrix)?;
        let _call = Foundation::enter_transient_call()?;
        Self::from_raw(unsafe { ffi::b3MakeQuatFromMatrix(&matrix.into_raw()) })
            .validate()
            .map_err(|_| Error::NativeFailure)
    }

    /// Computes the rotation between two unit vectors.
    pub fn between_unit_vectors(v1: impl Into<Vec3>, v2: impl Into<Vec3>) -> Result<Self> {
        callback_state::check_not_in_callback()?;
        let v1 = validate_unit_vec3("quaternion.between_unit_vectors.v1", v1.into())?;
        let v2 = validate_unit_vec3("quaternion.between_unit_vectors.v2", v2.into())?;
        let _call = Foundation::enter_transient_call()?;
        Self::from_raw(unsafe {
            ffi::b3ComputeQuatBetweenUnitVectors(v1.into_raw(), v2.into_raw())
        })
        .validate()
        .map_err(|_| Error::NativeFailure)
    }
}

impl Default for Quat {
    fn default() -> Self {
        Self::IDENTITY
    }
}

/// Local transform using `Vec3` translation and `Quat` rotation.
#[repr(C)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Copy, Clone, Debug, Default, PartialEq)]
pub struct Transform {
    /// Local translation.
    pub p: Vec3,
    /// Local rotation.
    pub q: Quat,
}

impl Transform {
    /// Identity transform.
    pub const IDENTITY: Self = Self {
        p: Vec3::ZERO,
        q: Quat::IDENTITY,
    };

    /// Creates a transform from translation and rotation.
    #[inline]
    pub const fn new(p: Vec3, q: Quat) -> Self {
        Self { p, q }
    }

    /// Converts a raw Box3D transform into the Rust value type.
    #[inline]
    pub const fn from_raw(raw: ffi::b3Transform) -> Self {
        Self {
            p: Vec3::from_raw(raw.p),
            q: Quat::from_raw(raw.q),
        }
    }

    /// Converts this value into the raw Box3D representation.
    #[inline]
    pub const fn into_raw(self) -> ffi::b3Transform {
        ffi::b3Transform {
            p: self.p.into_raw(),
            q: self.q.into_raw(),
        }
    }

    /// Returns whether this transform is valid according to Box3D.
    #[inline]
    pub fn is_valid(self) -> bool {
        self.p.is_valid() && self.q.is_valid()
    }

    /// Returns this transform or [`Error::InvalidValue`] if it is non-finite or malformed.
    #[inline]
    pub fn validate(self) -> crate::error::Result<Self> {
        validation::transform("transform", self)?;
        Ok(self)
    }
}

/// Position scalar type used by [`Pos`].
#[cfg(not(feature = "double-precision"))]
pub type PosScalar = f32;
/// Position scalar type used by [`Pos`] in double-precision builds.
#[cfg(feature = "double-precision")]
pub type PosScalar = f64;

/// World-space position with precision selected by the `double-precision` feature.
#[repr(C)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Copy, Clone, Debug, Default, PartialEq)]
pub struct Pos {
    /// X coordinate.
    pub x: PosScalar,
    /// Y coordinate.
    pub y: PosScalar,
    /// Z coordinate.
    pub z: PosScalar,
}

impl Pos {
    /// Zero position.
    pub const ZERO: Self = Self::new(0.0, 0.0, 0.0);

    /// Creates a position from coordinates.
    #[inline]
    pub const fn new(x: PosScalar, y: PosScalar, z: PosScalar) -> Self {
        Self { x, y, z }
    }

    /// Converts a raw Box3D position into the Rust value type.
    #[inline]
    pub const fn from_raw(raw: ffi::b3Pos) -> Self {
        Self {
            x: raw.x,
            y: raw.y,
            z: raw.z,
        }
    }

    /// Converts this value into the raw Box3D representation.
    #[inline]
    pub const fn into_raw(self) -> ffi::b3Pos {
        ffi::b3Pos {
            x: self.x,
            y: self.y,
            z: self.z,
        }
    }

    /// Returns whether this position is valid according to Box3D.
    #[inline]
    pub fn is_valid(self) -> bool {
        self.x.is_finite() && self.y.is_finite() && self.z.is_finite()
    }

    /// Returns this position or [`Error::InvalidValue`] if any coordinate is non-finite.
    #[inline]
    pub fn validate(self) -> crate::error::Result<Self> {
        validation::position("position", self)?;
        Ok(self)
    }
}

impl From<Vec3> for Pos {
    #[inline]
    fn from(value: Vec3) -> Self {
        #[cfg(not(feature = "double-precision"))]
        {
            Self::new(value.x, value.y, value.z)
        }
        #[cfg(feature = "double-precision")]
        {
            Self::new(value.x.into(), value.y.into(), value.z.into())
        }
    }
}

impl From<[f32; 3]> for Pos {
    #[inline]
    fn from(value: [f32; 3]) -> Self {
        Self::from(Vec3::from(value))
    }
}

impl From<(f32, f32, f32)> for Pos {
    #[inline]
    fn from(value: (f32, f32, f32)) -> Self {
        Self::from(Vec3::from(value))
    }
}

#[cfg(feature = "double-precision")]
impl From<[f64; 3]> for Pos {
    #[inline]
    fn from(value: [f64; 3]) -> Self {
        Self::new(value[0], value[1], value[2])
    }
}

#[cfg(feature = "double-precision")]
impl From<(f64, f64, f64)> for Pos {
    #[inline]
    fn from(value: (f64, f64, f64)) -> Self {
        Self::new(value.0, value.1, value.2)
    }
}

/// World-space transform using [`Pos`] for translation and [`Quat`] for rotation.
#[repr(C)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Copy, Clone, Debug, Default, PartialEq)]
pub struct WorldTransform {
    /// World-space position.
    pub p: Pos,
    /// World-space rotation.
    pub q: Quat,
}

impl WorldTransform {
    /// Identity world transform.
    pub const IDENTITY: Self = Self {
        p: Pos::ZERO,
        q: Quat::IDENTITY,
    };

    /// Creates a world transform from position and rotation.
    #[inline]
    pub const fn new(p: Pos, q: Quat) -> Self {
        Self { p, q }
    }

    /// Converts a raw Box3D world transform into the Rust value type.
    #[inline]
    pub const fn from_raw(raw: ffi::b3WorldTransform) -> Self {
        Self {
            p: Pos::from_raw(raw.p),
            q: Quat::from_raw(raw.q),
        }
    }

    /// Converts this value into the raw Box3D representation.
    #[inline]
    pub const fn into_raw(self) -> ffi::b3WorldTransform {
        ffi::b3WorldTransform {
            p: self.p.into_raw(),
            q: self.q.into_raw(),
        }
    }

    /// Returns whether this world transform is valid according to Box3D.
    #[inline]
    pub fn is_valid(self) -> bool {
        self.p.is_valid() && self.q.is_valid()
    }
}

/// 3x3 matrix stored as column vectors.
#[repr(C)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Copy, Clone, Debug, Default, PartialEq)]
pub struct Matrix3 {
    /// First column.
    pub cx: Vec3,
    /// Second column.
    pub cy: Vec3,
    /// Third column.
    pub cz: Vec3,
}

impl Matrix3 {
    /// Converts a raw Box3D matrix into the Rust value type.
    #[inline]
    pub const fn from_raw(raw: ffi::b3Matrix3) -> Self {
        Self {
            cx: Vec3::from_raw(raw.cx),
            cy: Vec3::from_raw(raw.cy),
            cz: Vec3::from_raw(raw.cz),
        }
    }

    /// Converts this value into the raw Box3D representation.
    #[inline]
    pub const fn into_raw(self) -> ffi::b3Matrix3 {
        ffi::b3Matrix3 {
            cx: self.cx.into_raw(),
            cy: self.cy.into_raw(),
            cz: self.cz.into_raw(),
        }
    }

    /// Returns whether this matrix is valid according to Box3D.
    #[inline]
    pub fn is_valid(self) -> bool {
        self.cx.is_valid() && self.cy.is_valid() && self.cz.is_valid()
    }

    /// Returns this matrix or [`Error::InvalidValue`] if any component is non-finite.
    #[inline]
    pub fn validate(self) -> Result<Self> {
        validation::vec3("matrix3.cx", self.cx)?;
        validation::vec3("matrix3.cy", self.cy)?;
        validation::vec3("matrix3.cz", self.cz)?;
        Ok(self)
    }
}

/// Axis-aligned bounding box.
#[repr(C)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Copy, Clone, Debug, Default, PartialEq)]
pub struct Aabb {
    /// Minimum corner.
    pub lower_bound: Vec3,
    /// Maximum corner.
    pub upper_bound: Vec3,
}

impl Aabb {
    /// Converts a raw Box3D AABB into the Rust value type.
    #[inline]
    pub const fn from_raw(raw: ffi::b3AABB) -> Self {
        Self {
            lower_bound: Vec3::from_raw(raw.lowerBound),
            upper_bound: Vec3::from_raw(raw.upperBound),
        }
    }

    /// Converts this value into the raw Box3D representation.
    #[inline]
    pub const fn into_raw(self) -> ffi::b3AABB {
        ffi::b3AABB {
            lowerBound: self.lower_bound.into_raw(),
            upperBound: self.upper_bound.into_raw(),
        }
    }

    /// Returns whether this AABB is valid according to Box3D.
    #[inline]
    pub fn is_valid(self) -> bool {
        self.lower_bound.is_valid()
            && self.upper_bound.is_valid()
            && self.lower_bound.x <= self.upper_bound.x
            && self.lower_bound.y <= self.upper_bound.y
            && self.lower_bound.z <= self.upper_bound.z
    }

    /// Returns this AABB or [`Error::InvalidValue`] if its bounds are invalid.
    #[inline]
    pub fn validate(self) -> Result<Self> {
        validation::vec3("aabb.lower_bound", self.lower_bound)?;
        validation::vec3("aabb.upper_bound", self.upper_bound)?;
        if self.lower_bound.x > self.upper_bound.x
            || self.lower_bound.y > self.upper_bound.y
            || self.lower_bound.z > self.upper_bound.z
        {
            return Err(validation::invalid(
                "aabb.bounds",
                InvalidValueReason::InvalidCombination,
            ));
        }
        Ok(self)
    }

    /// Returns whether this AABB is valid and finite-bounded.
    #[inline]
    pub fn is_bounded(self) -> Result<bool> {
        callback_state::check_not_in_callback()?;
        let limit = aabb_huge_limit()?;
        Ok(self.is_valid()
            && self.lower_bound.x >= -limit
            && self.lower_bound.y >= -limit
            && self.lower_bound.z >= -limit
            && self.upper_bound.x <= limit
            && self.upper_bound.y <= limit
            && self.upper_bound.z <= limit)
    }

    /// Returns whether this AABB passes Box3D's broader sanity checks.
    #[inline]
    pub fn is_sane(self) -> Result<bool> {
        self.is_bounded()
    }
}

fn aabb_huge_limit() -> Result<f32> {
    #[cfg(feature = "double-precision")]
    const HUGE_MULTIPLIER: f32 = 1.0e9;
    #[cfg(not(feature = "double-precision"))]
    const HUGE_MULTIPLIER: f32 = 1.0e5;

    Ok(HUGE_MULTIPLIER * Foundation::get()?.config().length_units_per_meter)
}

/// Applies the Steiner theorem to compute inertia for a point mass at `origin`.
pub fn steiner_inertia(mass: f32, origin: impl Into<Vec3>) -> Result<Matrix3> {
    callback_state::check_not_in_callback()?;
    let origin = origin.into();
    validation::vec3("steiner_inertia.origin", origin)?;
    validation::nonnegative("steiner_inertia.mass", mass)?;
    let _call = Foundation::enter_transient_call()?;
    Matrix3::from_raw(unsafe { ffi::b3Steiner(mass, origin.into_raw()) })
        .validate()
        .map_err(|_| Error::NativeFailure)
}

/// Plane represented by a normal and offset.
#[repr(C)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Copy, Clone, Debug, Default, PartialEq)]
pub struct Plane {
    /// Plane normal.
    pub normal: Vec3,
    /// Plane offset.
    pub offset: f32,
}

impl Plane {
    /// Converts a raw Box3D plane into the Rust value type.
    #[inline]
    pub const fn from_raw(raw: ffi::b3Plane) -> Self {
        Self {
            normal: Vec3::from_raw(raw.normal),
            offset: raw.offset,
        }
    }

    /// Converts this value into the raw Box3D representation.
    #[inline]
    pub const fn into_raw(self) -> ffi::b3Plane {
        ffi::b3Plane {
            normal: self.normal.into_raw(),
            offset: self.offset,
        }
    }

    /// Returns whether this plane is valid according to Box3D.
    #[inline]
    pub fn is_valid(self) -> bool {
        let length_squared = vec3_length_squared(self.normal);
        self.normal.is_valid()
            && (1.0 - length_squared).abs() < 100.0 * f32::EPSILON
            && self.offset.is_finite()
    }

    /// Returns this plane or [`Error::InvalidValue`] if it is non-finite or malformed.
    #[inline]
    pub fn validate(self) -> Result<Self> {
        validate_unit_vec3("plane.normal", self.normal)?;
        validation::finite("plane.offset", self.offset)?;
        Ok(self)
    }
}

/// Collision filter data used by shapes and queries.
#[repr(C)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Filter {
    /// Category bits assigned to the shape.
    pub category_bits: u64,
    /// Mask bits accepted by the shape.
    pub mask_bits: u64,
    /// Group override index. Matching non-zero groups override category/mask filtering.
    pub group_index: i32,
}

impl Filter {
    /// Converts a raw Box3D filter into the Rust value type.
    #[inline]
    pub const fn from_raw(raw: ffi::b3Filter) -> Self {
        Self {
            category_bits: raw.categoryBits,
            mask_bits: raw.maskBits,
            group_index: raw.groupIndex,
        }
    }

    /// Converts this value into the raw Box3D representation.
    #[inline]
    pub const fn into_raw(self) -> ffi::b3Filter {
        ffi::b3Filter {
            categoryBits: self.category_bits,
            maskBits: self.mask_bits,
            groupIndex: self.group_index,
        }
    }
}

impl Default for Filter {
    fn default() -> Self {
        Self {
            category_bits: u64::MAX,
            mask_bits: u64::MAX,
            group_index: 0,
        }
    }
}

/// Mass properties for a shape or body.
#[repr(C)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Copy, Clone, Debug, Default, PartialEq)]
pub struct MassData {
    /// Mass.
    pub mass: f32,
    /// Local center of mass.
    pub center: Vec3,
    /// Inertia tensor.
    pub inertia: Matrix3,
}

impl MassData {
    /// Converts raw Box3D mass data into the Rust value type.
    #[inline]
    pub const fn from_raw(raw: ffi::b3MassData) -> Self {
        Self {
            mass: raw.mass,
            center: Vec3::from_raw(raw.center),
            inertia: Matrix3::from_raw(raw.inertia),
        }
    }

    /// Converts this value into the raw Box3D representation.
    #[inline]
    pub const fn into_raw(self) -> ffi::b3MassData {
        ffi::b3MassData {
            mass: self.mass,
            center: self.center.into_raw(),
            inertia: self.inertia.into_raw(),
        }
    }
}

/// Per-axis motion locks for a body.
#[repr(C)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct MotionLocks {
    /// Locks linear motion along the X axis.
    pub linear_x: bool,
    /// Locks linear motion along the Y axis.
    pub linear_y: bool,
    /// Locks linear motion along the Z axis.
    pub linear_z: bool,
    /// Locks angular motion around the X axis.
    pub angular_x: bool,
    /// Locks angular motion around the Y axis.
    pub angular_y: bool,
    /// Locks angular motion around the Z axis.
    pub angular_z: bool,
}

impl MotionLocks {
    /// Creates motion locks from individual axis flags.
    #[inline]
    pub const fn new(
        linear_x: bool,
        linear_y: bool,
        linear_z: bool,
        angular_x: bool,
        angular_y: bool,
        angular_z: bool,
    ) -> Self {
        Self {
            linear_x,
            linear_y,
            linear_z,
            angular_x,
            angular_y,
            angular_z,
        }
    }

    /// Converts raw Box3D motion locks into the Rust value type.
    #[inline]
    pub const fn from_raw(raw: ffi::b3MotionLocks) -> Self {
        Self {
            linear_x: raw.linearX,
            linear_y: raw.linearY,
            linear_z: raw.linearZ,
            angular_x: raw.angularX,
            angular_y: raw.angularY,
            angular_z: raw.angularZ,
        }
    }

    /// Converts this value into the raw Box3D representation.
    #[inline]
    pub const fn into_raw(self) -> ffi::b3MotionLocks {
        ffi::b3MotionLocks {
            linearX: self.linear_x,
            linearY: self.linear_y,
            linearZ: self.linear_z,
            angularX: self.angular_x,
            angularY: self.angular_y,
            angularZ: self.angular_z,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn value_validation_preserves_context_and_reason() {
        assert_eq!(
            Vec3::new(f32::NAN, 0.0, 0.0).validate(),
            Err(Error::InvalidValue {
                context: "vec3",
                reason: InvalidValueReason::NonFinite,
            })
        );
        assert_eq!(
            Aabb {
                lower_bound: Vec3::new(1.0, 0.0, 0.0),
                upper_bound: Vec3::ZERO,
            }
            .validate(),
            Err(Error::InvalidValue {
                context: "aabb.bounds",
                reason: InvalidValueReason::InvalidCombination,
            })
        );
    }

    #[test]
    fn quaternion_matrix_conversion_rejects_a_reflection_before_native_code() {
        let reflection = Matrix3 {
            cx: Vec3::X,
            cy: Vec3::Y,
            cz: Vec3::new(0.0, 0.0, -1.0),
        };

        assert_eq!(
            Quat::from_matrix(reflection),
            Err(Error::InvalidValue {
                context: "quaternion.from_matrix.matrix",
                reason: InvalidValueReason::Malformed,
            })
        );
    }
}

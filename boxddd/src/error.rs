use thiserror::Error;

/// Safe handle family associated with a provenance error.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HandleKind {
    /// Body owned by a world.
    Body,
    /// Shape owned by a world.
    Shape,
    /// Joint owned by a world.
    Joint,
    /// Contact observed from a world.
    Contact,
    /// Proxy owned by a standalone dynamic tree.
    DynamicTreeProxy,
    /// World owned by a replay player.
    ReplayWorld,
}

impl std::fmt::Display for HandleKind {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Body => "body",
            Self::Shape => "shape",
            Self::Joint => "joint",
            Self::Contact => "contact",
            Self::DynamicTreeProxy => "dynamic-tree proxy",
            Self::ReplayWorld => "replay world",
        })
    }
}

/// Stable reason category for a rejected public value.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InvalidValueReason {
    /// A floating-point value is NaN or infinite.
    NonFinite,
    /// A value lies outside the accepted range.
    OutOfRange,
    /// A string contains an interior NUL byte.
    InteriorNul,
    /// Individually valid values form an unsupported combination.
    InvalidCombination,
    /// Structured input is malformed.
    Malformed,
}

impl std::fmt::Display for InvalidValueReason {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::NonFinite => "value must be finite",
            Self::OutOfRange => "value is outside the accepted range",
            Self::InteriorNul => "string contains an interior NUL byte",
            Self::InvalidCombination => "values form an invalid combination",
            Self::Malformed => "value is malformed",
        })
    }
}

/// Result type used by the safe Box3D wrapper.
pub type Result<T> = std::result::Result<T, Error>;

/// Public alias for APIs that return a boxddd error.
pub type ApiResult<T> = Result<T>;

/// Public alias for the boxddd error type.
pub type ApiError = Error;

/// Errors returned by safe wrapper operations.
#[non_exhaustive]
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum Error {
    /// A public value failed validation before native code was called.
    #[error("invalid value for {context}: {reason}")]
    InvalidValue {
        /// Stable field or operation context.
        context: &'static str,
        /// Machine-matchable validation reason.
        reason: InvalidValueReason,
    },

    /// A safe handle belongs to a different native owner.
    #[error("foreign {kind} handle")]
    ForeignHandle {
        /// Family of the rejected handle.
        kind: HandleKind,
    },

    /// A safe handle once belonged to this owner but is no longer live.
    #[error("stale {kind} handle")]
    StaleHandle {
        /// Family of the rejected handle.
        kind: HandleKind,
    },

    /// The operation was attempted while Box3D was executing a callback.
    #[error("boxddd API called from a Box3D callback; Box3D world is locked")]
    InCallback,

    /// Box3D did not return a valid world id.
    #[error("failed to create Box3D world")]
    CreateWorldFailed,

    /// Box3D did not return a valid body id.
    #[error("failed to create Box3D body")]
    CreateBodyFailed,

    /// Box3D did not return a valid shape id.
    #[error("failed to create Box3D shape")]
    CreateShapeFailed,

    /// Box3D did not return a valid recording handle.
    #[error("failed to create Box3D recording")]
    CreateRecordingFailed,

    /// Box3D did not return a valid dynamic tree handle.
    #[error("failed to create Box3D dynamic tree")]
    CreateDynamicTreeFailed,

    /// Box3D did not return a valid replay player handle.
    #[error("failed to create Box3D replay player")]
    CreateRecPlayerFailed,

    /// Native code or a native-provider protocol failed without an input error.
    #[error("native Box3D operation failed")]
    NativeFailure,

    /// Rust could not reserve bookkeeping storage before a native mutation.
    #[error("failed to reserve Rust bookkeeping storage")]
    AllocationFailed,

    /// A native recording file operation failed.
    #[error("failed to load or save a Box3D recording")]
    RecordingIoFailed,

    /// An input value failed validation.
    #[error("invalid argument")]
    InvalidArgument,

    /// The world handle failed Box3D id validation.
    #[error("invalid WorldId")]
    InvalidWorldId,

    /// The body handle failed Box3D id validation.
    #[error("invalid BodyId")]
    InvalidBodyId,

    /// The shape handle failed Box3D id validation.
    #[error("invalid ShapeId")]
    InvalidShapeId,

    /// The joint handle failed Box3D id validation.
    #[error("invalid JointId")]
    InvalidJointId,

    /// The contact handle failed Box3D id validation.
    #[error("invalid ContactId")]
    InvalidContactId,

    /// A joint-specific API was called with a different joint type.
    #[error("wrong joint type for this API")]
    WrongJointType,

    /// The provided index is outside the range accepted by the native API.
    #[error("index out of range for this API")]
    IndexOutOfRange,

    /// A string passed to Box3D contained an interior NUL byte.
    #[error("string contains an interior NUL byte")]
    NulByteInString,

    /// A native resource would outlive the resource it depends on.
    #[error("native resource lifetime would be violated")]
    ResourceLifetimeViolation,

    /// The operation is unavailable for the current WebAssembly backend.
    #[error("this API is not supported on the current WASM target")]
    UnsupportedOnWasm,

    /// A Rust callback panicked and boxddd stopped the native traversal safely.
    #[error("Rust callback panicked and native traversal was stopped")]
    CallbackPanicked,

    /// All global callback slots are currently in use.
    #[error("no callback slot is available")]
    CallbackSlotsExhausted,

    /// The process-wide provenance token space has been consumed permanently.
    #[error("provenance token space is exhausted")]
    ProvenanceExhausted,

    /// A provider-mode callback bridge failed while collecting debug draw data.
    #[error("provider callback bridge failed")]
    ProviderCallbackFailed,
}

#[cfg(test)]
mod tests {
    use super::{Error, HandleKind, InvalidValueReason};

    #[test]
    fn error_is_thread_safe_and_standard() {
        fn assert_error<T: std::error::Error + Send + Sync + 'static>() {}
        assert_error::<Error>();
    }

    #[test]
    fn canonical_errors_preserve_recovery_categories() {
        let invalid = Error::InvalidValue {
            context: "body.linear_damping",
            reason: InvalidValueReason::OutOfRange,
        };
        let foreign = Error::ForeignHandle {
            kind: HandleKind::Body,
        };
        let stale = Error::StaleHandle {
            kind: HandleKind::Body,
        };

        assert!(invalid.to_string().contains("body.linear_damping"));
        assert_ne!(foreign, stale);
        assert_eq!(
            Error::NativeFailure.to_string(),
            "native Box3D operation failed"
        );
    }
}

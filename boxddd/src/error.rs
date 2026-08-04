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
    #[error("boxddd API called from a Box3D callback; reentrant native entry is not allowed")]
    InCallback,

    /// Safe native work was requested before explicit process initialization.
    #[error("the Box3D foundation has not been initialized")]
    FoundationUninitialized,

    /// The process foundation was already initialized with different settings.
    #[error("the Box3D foundation is already initialized with a different configuration")]
    FoundationConflict,

    /// Ordinary and exclusive Foundation activity cannot overlap.
    #[error("the Box3D foundation is busy with conflicting native activity")]
    FoundationBusy,

    /// The process Foundation can no longer prove its native invariants.
    #[error("the Box3D foundation is poisoned")]
    FoundationPoisoned,

    /// A Foundation activity counter cannot represent another lease.
    #[error("the Box3D foundation activity capacity is exhausted")]
    FoundationActivityExhausted,

    /// A native creation call could not allocate a new identity.
    #[error("Box3D native identity capacity is exhausted")]
    ObjectIdentityExhausted,

    /// A native owner can no longer prove its Rust/native correspondence.
    #[error("the Box3D owner is poisoned")]
    OwnerPoisoned,

    /// Native code or a native-provider protocol failed without an input error.
    #[error("native Box3D operation failed")]
    NativeFailure,

    /// Rust could not reserve bookkeeping storage before a native mutation.
    #[error("failed to reserve Rust bookkeeping storage")]
    AllocationFailed,

    /// A native recording file operation failed.
    #[error("failed to load or save a Box3D recording")]
    RecordingIoFailed,

    /// A recording is still attached to a live world.
    #[error("Box3D recording is already in use by a world")]
    RecordingInUse,

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

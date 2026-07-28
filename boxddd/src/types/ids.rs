//! Opaque ids for native Box3D resources.

use super::*;
use crate::core::provenance::{ContactEpoch, OwnerToken, ResourceToken};

macro_rules! world_resource_id {
    (
        $(#[$meta:meta])*
        $name:ident,
        $key:ident,
        $raw:path
    ) => {
        #[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
        pub(crate) struct $key {
            index1: i32,
            world0: u16,
            generation: u16,
        }

        impl $key {
            #[inline]
            pub(crate) const fn from_raw(raw: $raw) -> Self {
                Self {
                    index1: raw.index1,
                    world0: raw.world0,
                    generation: raw.generation,
                }
            }

            #[inline]
            pub(crate) const fn into_raw(self) -> $raw {
                $raw {
                    index1: self.index1,
                    world0: self.world0,
                    generation: self.generation,
                }
            }
        }

        $(#[$meta])*
        #[derive(Copy, Clone, PartialEq, Eq, Hash)]
        pub struct $name {
            raw: $key,
            owner: OwnerToken,
            resource: ResourceToken,
        }

        impl $name {
            #[inline]
            pub(crate) const fn from_parts(
                raw: $raw,
                owner: OwnerToken,
                resource: ResourceToken,
            ) -> Self {
                Self {
                    raw: $key::from_raw(raw),
                    owner,
                    resource,
                }
            }

            #[inline]
            pub(crate) const fn into_raw(self) -> $raw {
                self.raw.into_raw()
            }

            #[inline]
            pub(crate) const fn key(self) -> $key {
                self.raw
            }

            #[inline]
            pub(crate) const fn owner_token(self) -> OwnerToken {
                self.owner
            }

            #[inline]
            pub(crate) const fn resource_token(self) -> ResourceToken {
                self.resource
            }
        }

        impl ::core::fmt::Debug for $name {
            fn fmt(&self, formatter: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                formatter.write_str(concat!(stringify!($name), "(..)"))
            }
        }
    };
}

world_resource_id!(
    /// Opaque handle for a body owned by a Box3D world.
    BodyId,
    BodyKey,
    ffi::b3BodyId
);

world_resource_id!(
    /// Opaque handle for a shape owned by a Box3D world.
    ShapeId,
    ShapeKey,
    ffi::b3ShapeId
);

world_resource_id!(
    /// Opaque handle for a joint owned by a Box3D world.
    JointId,
    JointKey,
    ffi::b3JointId
);

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct ContactKey {
    index1: i32,
    world0: u16,
    generation: u32,
}

impl ContactKey {
    #[inline]
    pub(crate) const fn from_raw(raw: ffi::b3ContactId) -> Self {
        Self {
            index1: raw.index1,
            world0: raw.world0,
            generation: raw.generation,
        }
    }

    #[inline]
    pub(crate) const fn into_raw(self) -> ffi::b3ContactId {
        ffi::b3ContactId {
            index1: self.index1,
            world0: self.world0,
            padding: 0,
            generation: self.generation,
        }
    }
}

/// Opaque handle for a contact observed from a Box3D world.
#[derive(Copy, Clone, PartialEq, Eq, Hash)]
pub struct ContactId {
    raw: ContactKey,
    owner: OwnerToken,
    resource: ResourceToken,
    epoch: ContactEpoch,
}

impl ::core::fmt::Debug for ContactId {
    fn fmt(&self, formatter: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
        formatter.write_str("ContactId(..)")
    }
}

impl ContactId {
    #[inline]
    pub(crate) const fn from_parts(
        raw: ffi::b3ContactId,
        owner: OwnerToken,
        resource: ResourceToken,
        epoch: ContactEpoch,
    ) -> Self {
        Self {
            raw: ContactKey::from_raw(raw),
            owner,
            resource,
            epoch,
        }
    }

    #[inline]
    pub(crate) const fn into_raw(self) -> ffi::b3ContactId {
        self.raw.into_raw()
    }

    #[inline]
    pub(crate) const fn owner_token(self) -> OwnerToken {
        self.owner
    }

    #[inline]
    pub(crate) const fn resource_token(self) -> ResourceToken {
        self.resource
    }

    #[inline]
    pub(crate) const fn epoch(self) -> ContactEpoch {
        self.epoch
    }
}

const _: () = {
    assert!(::core::mem::size_of::<Vec2>() == ::core::mem::size_of::<ffi::b3Vec2>());
    assert!(::core::mem::align_of::<Vec2>() == ::core::mem::align_of::<ffi::b3Vec2>());
    assert!(::core::mem::size_of::<Vec3>() == ::core::mem::size_of::<ffi::b3Vec3>());
    assert!(::core::mem::align_of::<Vec3>() == ::core::mem::align_of::<ffi::b3Vec3>());
    assert!(::core::mem::size_of::<Quat>() == ::core::mem::size_of::<ffi::b3Quat>());
    assert!(::core::mem::align_of::<Quat>() == ::core::mem::align_of::<ffi::b3Quat>());
    assert!(::core::mem::size_of::<Transform>() == ::core::mem::size_of::<ffi::b3Transform>());
    assert!(::core::mem::align_of::<Transform>() == ::core::mem::align_of::<ffi::b3Transform>());
    assert!(::core::mem::size_of::<Pos>() == ::core::mem::size_of::<ffi::b3Pos>());
    assert!(::core::mem::align_of::<Pos>() == ::core::mem::align_of::<ffi::b3Pos>());
    assert!(
        ::core::mem::size_of::<WorldTransform>() == ::core::mem::size_of::<ffi::b3WorldTransform>()
    );
    assert!(
        ::core::mem::align_of::<WorldTransform>()
            == ::core::mem::align_of::<ffi::b3WorldTransform>()
    );
    assert!(::core::mem::size_of::<Matrix3>() == ::core::mem::size_of::<ffi::b3Matrix3>());
    assert!(::core::mem::align_of::<Matrix3>() == ::core::mem::align_of::<ffi::b3Matrix3>());
    assert!(::core::mem::size_of::<Aabb>() == ::core::mem::size_of::<ffi::b3AABB>());
    assert!(::core::mem::align_of::<Aabb>() == ::core::mem::align_of::<ffi::b3AABB>());
    assert!(::core::mem::size_of::<Plane>() == ::core::mem::size_of::<ffi::b3Plane>());
    assert!(::core::mem::align_of::<Plane>() == ::core::mem::align_of::<ffi::b3Plane>());
    assert!(::core::mem::size_of::<Filter>() == ::core::mem::size_of::<ffi::b3Filter>());
    assert!(::core::mem::align_of::<Filter>() == ::core::mem::align_of::<ffi::b3Filter>());
    assert!(::core::mem::size_of::<MassData>() == ::core::mem::size_of::<ffi::b3MassData>());
    assert!(::core::mem::align_of::<MassData>() == ::core::mem::align_of::<ffi::b3MassData>());
    assert!(::core::mem::size_of::<MotionLocks>() == ::core::mem::size_of::<ffi::b3MotionLocks>());
    assert!(
        ::core::mem::align_of::<MotionLocks>() == ::core::mem::align_of::<ffi::b3MotionLocks>()
    );
    assert!(::core::mem::size_of::<Capacity>() == ::core::mem::size_of::<ffi::b3Capacity>());
    assert!(::core::mem::align_of::<Capacity>() == ::core::mem::align_of::<ffi::b3Capacity>());
};

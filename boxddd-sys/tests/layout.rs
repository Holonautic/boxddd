use std::ffi::c_void;
use std::mem::{align_of, offset_of, size_of};

use boxddd_sys::ffi;

#[cfg(target_pointer_width = "32")]
const _: () = {
    assert!(size_of::<ffi::b3SurfaceMaterial>() == 40);
    assert!(offset_of!(ffi::b3SurfaceMaterial, padding) == 36);
    assert!(size_of::<ffi::b3HullData>() == 144);
    assert!(size_of::<ffi::b3BoxHull>() == 648);
    assert!(offset_of!(ffi::b3BoxHull, vx) == 456);
    assert!(offset_of!(ffi::b3BoxHull, nz) == 616);
    assert!(size_of::<ffi::b3ShapeDef>() == 104);
    assert!(offset_of!(ffi::b3ShapeDef, baseMaterial) == 16);
    assert!(offset_of!(ffi::b3ShapeDef, enableSpeculativeContact) == 96);
    assert!(offset_of!(ffi::b3ShapeDef, internalValue) == 100);
    assert!(size_of::<ffi::b3DebugDraw>() == 88);
    assert!(offset_of!(ffi::b3DebugDraw, drawingBounds) == 36);
    assert!(offset_of!(ffi::b3DebugDraw, drawSleep) == 73);
    assert!(offset_of!(ffi::b3DebugDraw, drawAnchorA) == 76);
    assert!(offset_of!(ffi::b3DebugDraw, context) == 84);
};

#[test]
fn representative_public_api_symbols_are_bound() {
    let _world_step: unsafe extern "C" fn(ffi::b3WorldId, f32, i32) = ffi::b3World_Step;
    let _body_transform: unsafe extern "C" fn(ffi::b3BodyId) -> ffi::b3WorldTransform =
        ffi::b3Body_GetTransform;
    let _sphere_shape: unsafe extern "C" fn(
        ffi::b3BodyId,
        *const ffi::b3ShapeDef,
        *const ffi::b3Sphere,
    ) -> ffi::b3ShapeId = ffi::b3CreateSphereShape;
    let _distance_joint: unsafe extern "C" fn() -> ffi::b3DistanceJointDef =
        ffi::b3DefaultDistanceJointDef;
    let _query_filter: unsafe extern "C" fn() -> ffi::b3QueryFilter = ffi::b3DefaultQueryFilter;
    let _recording: unsafe extern "C" fn(ffi::b3WorldId, *mut ffi::b3Recording) =
        ffi::b3World_StartRecording;
    let _mover: unsafe extern "C" fn(
        ffi::b3WorldId,
        ffi::b3Pos,
        *const ffi::b3Capsule,
        ffi::b3QueryFilter,
        ffi::b3PlaneResultFcn,
        *mut std::ffi::c_void,
    ) = ffi::b3World_CollideMover;
    let _collision: unsafe extern "C" fn(
        *mut ffi::b3LocalManifold,
        i32,
        *const ffi::b3Sphere,
        *const ffi::b3Sphere,
        ffi::b3Transform,
    ) = ffi::b3CollideSpheres;
    let _triangle_hull: unsafe extern "C" fn(
        *mut ffi::b3LocalManifold,
        i32,
        ffi::b3Vec3,
        ffi::b3Vec3,
        ffi::b3Vec3,
        i32,
        *const ffi::b3HullData,
        *mut ffi::b3SATCache,
        bool,
    ) = ffi::b3CollideTriangleAndHull;
    let _baked_compound: unsafe extern "C" fn(
        ffi::b3BodyId,
        *mut ffi::b3ShapeDef,
        *const ffi::b3CompoundData,
    ) -> ffi::b3ShapeId = ffi::b3CreateBakedCompoundShape;
    let _local_center: unsafe extern "C" fn(ffi::b3BodyId) -> ffi::b3Vec3 =
        ffi::b3Body_GetLocalCenter;
    let _world_center: unsafe extern "C" fn(ffi::b3BodyId) -> ffi::b3Pos =
        ffi::b3Body_GetWorldCenter;
    let _fast_rotation: unsafe extern "C" fn(ffi::b3BodyId, bool) = ffi::b3Body_AllowFastRotation;
    let _shape_name: unsafe extern "C" fn(ffi::b3ShapeId) -> *const std::ffi::c_char =
        ffi::b3Shape_GetName;
    let _sub_step: unsafe extern "C" fn(*mut ffi::b3RecPlayer) = ffi::b3RecPlayer_SubStepFrame;
}

#[test]
fn default_filter_bits_preserve_the_upstream_u64_contract() {
    let category_bits: u64 = ffi::B3_DEFAULT_CATEGORY_BITS;
    let mask_bits: u64 = ffi::B3_DEFAULT_MASK_BITS;

    assert_eq!(category_bits, u64::MAX);
    assert_eq!(mask_bits, u64::MAX);
}

#[test]
fn selected_abi_mode_matches_runtime_library() {
    let is_double = unsafe { ffi::b3IsDoublePrecision() };

    #[cfg(feature = "double-precision")]
    {
        assert!(is_double);
        assert_eq!(size_of::<ffi::b3Pos>(), 24);
        assert_eq!(align_of::<ffi::b3Pos>(), 8);
        assert_eq!(size_of::<ffi::b3WorldTransform>(), 40);
    }

    #[cfg(not(feature = "double-precision"))]
    {
        assert!(!is_double);
        assert_eq!(size_of::<ffi::b3Pos>(), size_of::<ffi::b3Vec3>());
        assert_eq!(align_of::<ffi::b3Pos>(), align_of::<ffi::b3Vec3>());
        assert_eq!(
            size_of::<ffi::b3WorldTransform>(),
            size_of::<ffi::b3Transform>()
        );
    }
}

#[test]
fn representative_layouts_match_the_pinned_headers() {
    assert_eq!(size_of::<ffi::b3WorldId>(), 4);
    assert_eq!(size_of::<ffi::b3BodyId>(), 8);
    assert_eq!(size_of::<ffi::b3ShapeId>(), 8);
    assert_eq!(size_of::<ffi::b3JointId>(), 8);
    assert_eq!(size_of::<ffi::b3Vec3>(), 12);
    assert_eq!(size_of::<ffi::b3Quat>(), 16);
    assert_eq!(size_of::<ffi::b3SurfaceMaterial>(), 40);
    assert_eq!(align_of::<ffi::b3SurfaceMaterial>(), 8);
    assert_eq!(offset_of!(ffi::b3SurfaceMaterial, padding), 36);

    assert_eq!(size_of::<ffi::b3HullData>(), 144);
    assert_eq!(align_of::<ffi::b3HullData>(), 8);
    assert_eq!(offset_of!(ffi::b3HullData, planeOffset), 124);
    assert_eq!(offset_of!(ffi::b3HullData, faceOffset), 128);
    assert_eq!(offset_of!(ffi::b3HullData, soaVertexOffset), 132);
    assert_eq!(offset_of!(ffi::b3HullData, soaNormalOffset), 136);
    assert_eq!(offset_of!(ffi::b3HullData, padding), 140);

    assert_eq!(size_of::<ffi::b3BoxHull>(), 648);
    assert_eq!(align_of::<ffi::b3BoxHull>(), 8);
    assert_eq!(offset_of!(ffi::b3BoxHull, base), 0);
    assert_eq!(offset_of!(ffi::b3BoxHull, boxVertices), 144);
    assert_eq!(offset_of!(ffi::b3BoxHull, boxPoints), 152);
    assert_eq!(offset_of!(ffi::b3BoxHull, boxEdges), 248);
    assert_eq!(offset_of!(ffi::b3BoxHull, boxPlanes), 344);
    assert_eq!(offset_of!(ffi::b3BoxHull, boxFaces), 440);
    assert_eq!(offset_of!(ffi::b3BoxHull, padding), 446);
    assert_eq!(offset_of!(ffi::b3BoxHull, vx), 456);
    assert_eq!(offset_of!(ffi::b3BoxHull, vy), 488);
    assert_eq!(offset_of!(ffi::b3BoxHull, vz), 520);
    assert_eq!(offset_of!(ffi::b3BoxHull, nx), 552);
    assert_eq!(offset_of!(ffi::b3BoxHull, ny), 584);
    assert_eq!(offset_of!(ffi::b3BoxHull, nz), 616);

    #[cfg(target_pointer_width = "64")]
    {
        assert_eq!(size_of::<ffi::b3ShapeDef>(), 120);
        assert_eq!(align_of::<ffi::b3ShapeDef>(), 8);
        assert_eq!(offset_of!(ffi::b3ShapeDef, name), 0);
        assert_eq!(offset_of!(ffi::b3ShapeDef, baseMaterial), 32);
        assert_eq!(offset_of!(ffi::b3ShapeDef, enableSpeculativeContact), 112);
        assert_eq!(offset_of!(ffi::b3ShapeDef, internalValue), 116);

        assert_eq!(size_of::<ffi::b3DebugDraw>(), 128);
        assert_eq!(align_of::<ffi::b3DebugDraw>(), 8);
        assert_eq!(offset_of!(ffi::b3DebugDraw, drawingBounds), 72);
        assert_eq!(offset_of!(ffi::b3DebugDraw, drawMass), 108);
        assert_eq!(offset_of!(ffi::b3DebugDraw, drawSleep), 109);
        assert_eq!(offset_of!(ffi::b3DebugDraw, drawAnchorA), 112);
        assert_eq!(offset_of!(ffi::b3DebugDraw, drawIslands), 117);
        assert_eq!(offset_of!(ffi::b3DebugDraw, context), 120);
    }

    #[cfg(target_pointer_width = "32")]
    {
        assert_eq!(size_of::<ffi::b3ShapeDef>(), 104);
        assert_eq!(align_of::<ffi::b3ShapeDef>(), 8);
        assert_eq!(offset_of!(ffi::b3ShapeDef, name), 0);
        assert_eq!(offset_of!(ffi::b3ShapeDef, baseMaterial), 16);
        assert_eq!(offset_of!(ffi::b3ShapeDef, enableSpeculativeContact), 96);
        assert_eq!(offset_of!(ffi::b3ShapeDef, internalValue), 100);

        assert_eq!(size_of::<ffi::b3DebugDraw>(), 88);
        assert_eq!(align_of::<ffi::b3DebugDraw>(), 4);
        assert_eq!(offset_of!(ffi::b3DebugDraw, drawingBounds), 36);
        assert_eq!(offset_of!(ffi::b3DebugDraw, drawMass), 72);
        assert_eq!(offset_of!(ffi::b3DebugDraw, drawSleep), 73);
        assert_eq!(offset_of!(ffi::b3DebugDraw, drawAnchorA), 76);
        assert_eq!(offset_of!(ffi::b3DebugDraw, drawIslands), 81);
        assert_eq!(offset_of!(ffi::b3DebugDraw, context), 84);
    }
}

#[test]
fn changed_defaults_and_by_value_returns_match_the_pinned_runtime() {
    let material = unsafe { ffi::b3DefaultSurfaceMaterial() };
    assert_eq!(material.friction, 0.6);
    assert_eq!(material.padding, 0);

    let shape = unsafe { ffi::b3DefaultShapeDef() };
    assert!(shape.name.is_null());
    assert!(shape.userData.is_null());
    assert!(shape.materials.is_null());
    assert_eq!(shape.materialCount, 0);
    assert_eq!(shape.baseMaterial.padding, 0);
    assert!(shape.invokeContactCreation);
    assert!(shape.updateBodyMass);
    assert!(shape.enableSpeculativeContact);

    let draw = unsafe { ffi::b3DefaultDebugDraw() };
    let draw_shape: Option<
        unsafe extern "C" fn(*mut c_void, ffi::b3WorldTransform, ffi::b3HexColor, *mut c_void),
    > = draw.DrawShapeFcn;
    assert!(draw_shape.is_some());
    assert!(!draw.drawSleep);
    assert!(!draw.drawAnchorA);
    assert!(draw.context.is_null());

    let hull = unsafe { ffi::b3MakeCubeHull(1.0) };
    assert_eq!(hull.base.vertexCount, 8);
    assert_eq!(hull.base.edgeCount, 24);
    assert_eq!(hull.base.faceCount, 6);
    assert_eq!(
        hull.base.vertexOffset as usize,
        offset_of!(ffi::b3BoxHull, boxVertices)
    );
    assert_eq!(
        hull.base.pointOffset as usize,
        offset_of!(ffi::b3BoxHull, boxPoints)
    );
    assert_eq!(
        hull.base.edgeOffset as usize,
        offset_of!(ffi::b3BoxHull, boxEdges)
    );
    assert_eq!(
        hull.base.planeOffset as usize,
        offset_of!(ffi::b3BoxHull, boxPlanes)
    );
    assert_eq!(
        hull.base.faceOffset as usize,
        offset_of!(ffi::b3BoxHull, boxFaces)
    );
    assert_eq!(
        hull.base.soaVertexOffset as usize,
        offset_of!(ffi::b3BoxHull, vx)
    );
    assert_eq!(
        hull.base.soaNormalOffset as usize,
        offset_of!(ffi::b3BoxHull, nx)
    );
    assert_eq!(hull.base.padding, 0);
    assert_eq!(hull.padding, [0; 10]);
}

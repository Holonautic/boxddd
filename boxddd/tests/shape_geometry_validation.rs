use boxddd::error::InvalidValueReason;
use boxddd::{
    BoxHull, Capsule, Compound, Error, HeightField, Hull, MeshData, MeshDataOptions, Sphere,
    SurfaceMaterial, Transform, Vec3,
};

fn foundation() -> &'static boxddd::Foundation {
    boxddd::Foundation::initialize_default().unwrap()
}

#[test]
fn surface_material_default_matches_box3d_default() {
    let material = SurfaceMaterial::default();
    assert_eq!(material.friction, 0.6);
    assert_eq!(material.restitution, 0.0);
    assert_eq!(material.rolling_resistance, 0.0);
    assert_eq!(material.tangent_velocity, Vec3::ZERO);
}

#[test]
fn value_and_resource_geometry_reject_invalid_inputs() {
    let foundation = foundation();
    assert_eq!(
        Capsule::new(Vec3::ZERO, Vec3::X, f32::NAN).validate(),
        Err(Error::InvalidValue {
            context: "capsule.radius",
            reason: InvalidValueReason::NonFinite,
        })
    );
    assert_eq!(
        Sphere::new(Vec3::ZERO, 0.0).validate(),
        Err(Error::InvalidValue {
            context: "sphere.radius",
            reason: InvalidValueReason::OutOfRange,
        })
    );
    assert!(Hull::from_points([Vec3::ZERO, Vec3::X, Vec3::Y], 8).is_err());
    assert!(Hull::cylinder(1.0, 1.0, 0.0, 2).is_err());
    assert!(Hull::cylinder(1.0, 1.0, 0.0, 33).is_err());
    assert_eq!(
        Hull::cone(1.0, 0.0, 1.0, 4).unwrap_err(),
        Error::InvalidValue {
            context: "hull.cone.radius1",
            reason: InvalidValueReason::OutOfRange,
        }
    );
    assert_eq!(
        Hull::cone(1.0, 1.0, 0.0, 4).unwrap_err(),
        Error::InvalidValue {
            context: "hull.cone.radius2",
            reason: InvalidValueReason::OutOfRange,
        }
    );
    for slices in [3, 33] {
        assert_eq!(
            Hull::cone(1.0, 1.0, 1.0, slices).unwrap_err(),
            Error::InvalidValue {
                context: "hull.cone.slices",
                reason: InvalidValueReason::OutOfRange,
            }
        );
    }
    assert_eq!(
        BoxHull::cube(0.0).unwrap_err(),
        Error::InvalidValue {
            context: "box_hull.half_width",
            reason: InvalidValueReason::OutOfRange,
        }
    );
    assert_eq!(
        BoxHull::new(1.0, f32::NAN, 1.0).unwrap_err(),
        Error::InvalidValue {
            context: "box_hull.half_widths",
            reason: InvalidValueReason::NonFinite,
        }
    );
    assert_eq!(
        BoxHull::offset(1.0, 1.0, 1.0, [f32::INFINITY, 0.0, 0.0]).unwrap_err(),
        Error::InvalidValue {
            context: "box_hull.offset",
            reason: InvalidValueReason::NonFinite,
        }
    );
    assert_eq!(
        BoxHull::transformed(
            1.0,
            1.0,
            1.0,
            Transform::new(Vec3::ZERO, boxddd::Quat::new(Vec3::ZERO, 0.0)),
        )
        .unwrap_err(),
        Error::InvalidValue {
            context: "box_hull.transform",
            reason: InvalidValueReason::Malformed,
        }
    );
    assert_eq!(
        BoxHull::scaled([1.0, 1.0, 1.0], Transform::IDENTITY, [1.0, f32::NAN, 1.0],).unwrap_err(),
        Error::InvalidValue {
            context: "box_hull.post_scale",
            reason: InvalidValueReason::NonFinite,
        }
    );
    assert!(MeshData::box_mesh(Vec3::ZERO, [1.0, 0.0, 1.0], true).is_err());
    assert!(
        MeshData::from_triangles(
            vec![Vec3::ZERO, Vec3::X, Vec3::Y],
            vec![0, 1, 3],
            None,
            MeshDataOptions::default(),
        )
        .is_err()
    );
    assert!(
        MeshData::from_triangles(
            vec![Vec3::ZERO, Vec3::X, Vec3::Y],
            vec![0, 1, 2],
            Some(&[0, 1]),
            MeshDataOptions::default(),
        )
        .is_err()
    );
    assert!(
        MeshData::from_triangles(
            vec![Vec3::ZERO, Vec3::X, Vec3::new(2.0, 0.0, 0.0)],
            vec![0, 1, 2],
            None,
            MeshDataOptions::default(),
        )
        .is_err()
    );
    assert!(
        MeshData::from_triangles(
            vec![Vec3::ZERO, Vec3::X, Vec3::Y],
            vec![0, 1, 2],
            None,
            MeshDataOptions::default().weld_tolerance(f32::NAN),
        )
        .is_err()
    );
    assert!(MeshData::wave_mesh(2, 2, 1.0, f32::NAN, 1.0, 1.0).is_err());
    assert!(MeshData::torus_mesh(2, 8, 1.0, 0.25).is_err());
    assert!(MeshData::hollow_box_mesh(Vec3::ZERO, [1.0, 0.0, 1.0]).is_err());
    assert!(MeshData::platform_mesh(Vec3::ZERO, 1.0, 0.0, 2.0).is_err());
    assert!(HeightField::grid(1, 2, [1.0, 1.0, 1.0], false).is_err());
    assert!(
        HeightField::from_samples(2, 2, vec![0.0, 1.0, 2.0], [1.0, 1.0, 1.0], None, false).is_err()
    );
    assert!(
        HeightField::from_samples(
            2,
            2,
            vec![0.0, 1.0, 2.0, f32::NAN],
            [1.0, 1.0, 1.0],
            None,
            false,
        )
        .is_err()
    );
    assert!(
        HeightField::from_samples(
            2,
            2,
            vec![0.0, 1.0, 2.0, 3.0],
            [1.0, 1.0, 1.0],
            Some(&[0, 1]),
            false,
        )
        .is_err()
    );
    assert!(HeightField::wave(2, 2, [1.0, 1.0, 1.0], f32::NAN, 1.0, false).is_err());
    assert!(
        Compound::single_sphere(
            Sphere::new(Vec3::ZERO, f32::INFINITY),
            SurfaceMaterial::default()
        )
        .is_err()
    );
    assert!(Compound::builder().build().is_err());
    let mesh = MeshData::box_mesh(Vec3::ZERO, [1.0, 1.0, 1.0], true).unwrap();
    let mut compound_builder = Compound::builder();
    compound_builder
        .add_sphere(Sphere::new(Vec3::ZERO, 0.25), SurfaceMaterial::default())
        .unwrap();
    assert!(
        compound_builder
            .add_mesh(
                &mesh,
                Transform::IDENTITY,
                [0.0, 1.0, 1.0],
                [SurfaceMaterial::default()],
            )
            .is_err()
    );
    assert_eq!(
        SurfaceMaterial {
            friction: -1.0,
            ..Default::default()
        }
        .validate(),
        Err(Error::InvalidValue {
            context: "surface_material.friction",
            reason: InvalidValueReason::OutOfRange,
        })
    );
    assert_eq!(
        SurfaceMaterial {
            restitution: -1.0,
            ..Default::default()
        }
        .validate(),
        Err(Error::InvalidValue {
            context: "surface_material.restitution",
            reason: InvalidValueReason::OutOfRange,
        })
    );
    assert_eq!(
        SurfaceMaterial {
            rolling_resistance: -1.0,
            ..Default::default()
        }
        .validate(),
        Err(Error::InvalidValue {
            context: "surface_material.rolling_resistance",
            reason: InvalidValueReason::OutOfRange,
        })
    );
    assert_eq!(
        foundation
            .shape_def_builder()
            .friction(-1.0)
            .build()
            .unwrap_err(),
        Error::InvalidValue {
            context: "surface_material.friction",
            reason: InvalidValueReason::OutOfRange,
        }
    );
    assert_eq!(
        foundation
            .shape_def_builder()
            .restitution(-1.0)
            .build()
            .unwrap_err(),
        Error::InvalidValue {
            context: "surface_material.restitution",
            reason: InvalidValueReason::OutOfRange,
        }
    );
}

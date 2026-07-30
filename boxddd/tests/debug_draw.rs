use boxddd::error::InvalidValueReason;
use boxddd::{
    Aabb, BodyType, BoxHull, Capsule, Compound, DebugDrawCommand, DebugDrawFrame, DebugDrawOptions,
    DebugShapeEvent, DebugShapeGeometry, Error, HeightField, HexColor, MeshData, Sphere,
    SurfaceMaterial, Vec3, World,
};

fn foundation() -> &'static boxddd::Foundation {
    boxddd::Foundation::initialize_default().unwrap()
}

fn debug_world() -> World {
    let foundation = foundation();
    let mut world = foundation.create_world(foundation.world_def()).unwrap();
    let body = world.create_body(foundation.body_def()).unwrap();
    world
        .create_hull_shape(body, &foundation.shape_def(), &BoxHull::cube(1.0).unwrap())
        .unwrap();
    world
}

#[test]
fn debug_draw_collects_shape_commands_and_reuses_buffer() {
    let mut world = debug_world();
    let mut frame = DebugDrawFrame {
        commands: vec![DebugDrawCommand::Transform(Default::default())],
        ..DebugDrawFrame::default()
    };
    let initial_capacity = frame.commands.capacity();

    world
        .debug_draw_frame_into(&mut frame, DebugDrawOptions::default())
        .unwrap();

    assert!(frame.commands.capacity() >= initial_capacity);
    assert!(frame.commands.iter().any(|command| matches!(
        command,
        DebugDrawCommand::Shape {
            handle: Some(_),
            ..
        }
    )));

    let first_len = frame.commands.len();
    world
        .debug_draw_frame_into(&mut frame, DebugDrawOptions::default())
        .unwrap();
    assert_eq!(frame.commands.len(), first_len);
}

#[test]
fn debug_draw_frame_emits_shape_asset_events_and_reuses_handles() {
    let mut world = debug_world();

    let first = world.debug_draw_frame(DebugDrawOptions::default()).unwrap();
    let created: Vec<_> = first
        .events
        .iter()
        .filter_map(|event| match event {
            DebugShapeEvent::Created(asset) => Some(asset),
            _ => None,
        })
        .collect();
    assert_eq!(created.len(), 1);
    assert!(matches!(
        created[0].geometry,
        DebugShapeGeometry::Hull { .. }
    ));
    let handle = created[0].handle;
    assert!(first.commands.iter().any(|command| {
        matches!(command, DebugDrawCommand::Shape { handle: Some(seen), .. } if *seen == handle)
    }));
    let second = world.debug_draw_frame(DebugDrawOptions::default()).unwrap();
    assert!(
        second
            .events
            .iter()
            .all(|event| !matches!(event, DebugShapeEvent::Created(_)))
    );
    assert!(second.commands.iter().any(|command| {
        matches!(command, DebugDrawCommand::Shape { handle: Some(seen), .. } if *seen == handle)
    }));
}

#[test]
fn debug_draw_frame_queues_destroy_events_for_drawn_shapes() {
    let mut world = debug_world();
    let first = world.debug_draw_frame(DebugDrawOptions::default()).unwrap();
    let handle = first
        .events
        .iter()
        .find_map(|event| match event {
            DebugShapeEvent::Created(asset) => Some(asset.handle),
            _ => None,
        })
        .expect("first frame should create a debug shape");
    let shape_id = first
        .events
        .iter()
        .find_map(|event| match event {
            DebugShapeEvent::Created(asset) => Some(asset.shape_id),
            _ => None,
        })
        .expect("first frame should include source shape id");

    world.destroy_shape(shape_id, true).unwrap();
    let second = world.debug_draw_frame(DebugDrawOptions::default()).unwrap();

    assert!(second.events.iter().any(|event| {
        matches!(event, DebugShapeEvent::Destroyed { handle: seen } if *seen == handle)
    }));
    assert!(second.commands.iter().all(|command| {
        !matches!(command, DebugDrawCommand::Shape { handle: Some(seen), .. } if *seen == handle)
    }));
}

#[test]
fn debug_draw_frame_copies_sphere_geometry() {
    let foundation = foundation();
    let mut world = foundation.create_world(foundation.world_def()).unwrap();
    let body = world.create_body(foundation.body_def()).unwrap();
    let sphere = Sphere::new([1.0, 2.0, 3.0], 0.75);
    world
        .create_sphere_shape(body, &foundation.shape_def(), &sphere)
        .unwrap();

    let frame = world.debug_draw_frame(DebugDrawOptions::default()).unwrap();
    let asset = frame
        .events
        .iter()
        .find_map(|event| match event {
            DebugShapeEvent::Created(asset) => Some(asset),
            _ => None,
        })
        .expect("sphere debug shape asset should be created");

    assert!(matches!(
        asset.geometry,
        DebugShapeGeometry::Sphere {
            center,
            radius
        } if center == [1.0, 2.0, 3.0].into() && radius == 0.75
    ));
}

#[test]
fn debug_draw_frame_copies_capsule_geometry() {
    let foundation = foundation();
    let mut world = foundation.create_world(foundation.world_def()).unwrap();
    let body = world.create_body(foundation.body_def()).unwrap();
    let capsule = Capsule::new([-1.0, 0.0, 0.0], [1.0, 0.0, 0.0], 0.25);
    world
        .create_capsule_shape(body, &foundation.shape_def(), &capsule)
        .unwrap();

    let frame = world.debug_draw_frame(DebugDrawOptions::default()).unwrap();
    let asset = frame
        .events
        .iter()
        .find_map(|event| match event {
            DebugShapeEvent::Created(asset) => Some(asset),
            _ => None,
        })
        .expect("capsule debug shape asset should be created");

    assert!(matches!(
        asset.geometry,
        DebugShapeGeometry::Capsule {
            center1,
            center2,
            radius
        } if center1 == [-1.0, 0.0, 0.0].into()
            && center2 == [1.0, 0.0, 0.0].into()
            && radius == 0.25
    ));
}

#[test]
fn debug_draw_frame_copies_mesh_geometry() {
    let foundation = foundation();
    let mut world = foundation.create_world(foundation.world_def()).unwrap();
    let body = world
        .create_body(
            foundation
                .body_def_builder()
                .body_type(BodyType::Static)
                .build()
                .unwrap(),
        )
        .unwrap();
    let mesh = MeshData::box_mesh(Vec3::ZERO, [0.5, 0.5, 0.5], true).unwrap();
    world
        .create_mesh_shape(body, &foundation.shape_def(), mesh, [1.0, 2.0, 1.0])
        .unwrap();

    let frame = world.debug_draw_frame(DebugDrawOptions::default()).unwrap();
    let asset = frame
        .events
        .iter()
        .find_map(|event| match event {
            DebugShapeEvent::Created(asset) => Some(asset),
            _ => None,
        })
        .expect("mesh debug shape asset should be created");

    assert!(matches!(
        &asset.geometry,
        DebugShapeGeometry::Mesh { mesh, scale }
            if !mesh.vertices.is_empty()
            && !mesh.triangles.is_empty()
            && *scale == [1.0, 2.0, 1.0].into()
    ));
}

#[test]
fn debug_draw_frame_copies_height_field_geometry() {
    let foundation = foundation();
    let mut world = foundation.create_world(foundation.world_def()).unwrap();
    let body = world
        .create_body(
            foundation
                .body_def_builder()
                .body_type(BodyType::Static)
                .build()
                .unwrap(),
        )
        .unwrap();
    let height_field = HeightField::grid(3, 3, [1.0, 1.0, 1.0], false).unwrap();
    world
        .create_height_field_shape(body, &foundation.shape_def(), height_field)
        .unwrap();

    let frame = world.debug_draw_frame(DebugDrawOptions::default()).unwrap();
    let asset = frame
        .events
        .iter()
        .find_map(|event| match event {
            DebugShapeEvent::Created(asset) => Some(asset),
            _ => None,
        })
        .expect("height-field debug shape asset should be created");

    assert!(matches!(
        &asset.geometry,
        DebugShapeGeometry::HeightField { mesh }
            if mesh.vertices.len() == 9 && mesh.triangles.len() == 8
    ));
}

#[test]
fn debug_draw_frame_flattens_compound_children() {
    let foundation = foundation();
    let mut world = foundation.create_world(foundation.world_def()).unwrap();
    let body = world
        .create_body(
            foundation
                .body_def_builder()
                .body_type(BodyType::Static)
                .build()
                .unwrap(),
        )
        .unwrap();
    let compound = Compound::builder()
        .sphere(
            Sphere::new([0.0, 0.0, 0.0], 0.5),
            SurfaceMaterial::default(),
        )
        .capsule(
            Capsule::new([0.0, -0.5, 0.0], [0.0, 0.5, 0.0], 0.1),
            SurfaceMaterial::default(),
        )
        .build()
        .unwrap();
    world
        .create_compound_shape(body, &foundation.shape_def(), compound)
        .unwrap();

    let frame = world.debug_draw_frame(DebugDrawOptions::default()).unwrap();
    let asset = frame
        .events
        .iter()
        .find_map(|event| match event {
            DebugShapeEvent::Created(asset) => Some(asset),
            _ => None,
        })
        .expect("compound debug shape asset should be created");

    match &asset.geometry {
        DebugShapeGeometry::Compound { children } => {
            assert_eq!(children.len(), 2);
            assert!(
                children
                    .iter()
                    .any(|child| matches!(child.geometry, DebugShapeGeometry::Sphere { .. }))
            );
            assert!(
                children
                    .iter()
                    .any(|child| matches!(child.geometry, DebugShapeGeometry::Capsule { .. }))
            );
        }
        other => panic!("expected compound debug geometry, got {other:?}"),
    }
}

#[test]
fn hex_color_preserves_box3d_material_payload() {
    let raw = 0x05_ab_cd_ef_u32;
    let color = HexColor::from_raw(raw);

    assert_eq!(color.raw_u32(), raw);
    assert_eq!(color.rgb_u32(), 0x00_ab_cd_ef);
    assert_eq!(color.into_raw(), raw);
    assert_eq!(HexColor::from_rgb_u32(raw).raw_u32(), 0x00_ab_cd_ef);
}

#[test]
fn debug_draw_options_can_collect_bounds_commands() {
    let mut world = debug_world();
    let options = DebugDrawOptions {
        draw_bounds: true,
        ..Default::default()
    };

    let frame = world.debug_draw_frame(options).unwrap();

    assert!(
        frame
            .commands
            .iter()
            .any(|command| matches!(command, DebugDrawCommand::Bounds { .. }))
    );
}

#[test]
fn debug_draw_shape_callback_visits_shape_commands() {
    let foundation = foundation();
    let mut world = foundation.create_world(foundation.world_def()).unwrap();
    for offset in [-2.0, 0.0, 2.0] {
        let body = world
            .create_body(
                foundation
                    .body_def_builder()
                    .position([offset, 0.0, 0.0])
                    .build()
                    .unwrap(),
            )
            .unwrap();
        world
            .create_hull_shape(body, &foundation.shape_def(), &BoxHull::cube(0.2).unwrap())
            .unwrap();
    }

    let baseline_shape_count = world
        .debug_draw_frame(DebugDrawOptions::default())
        .unwrap()
        .commands
        .iter()
        .filter(|command| matches!(command, DebugDrawCommand::Shape { .. }))
        .count();
    assert!(baseline_shape_count > 1);

    let frame = world.debug_draw_frame(DebugDrawOptions::default()).unwrap();

    assert_eq!(
        frame
            .commands
            .iter()
            .filter(|command| matches!(command, DebugDrawCommand::Shape { .. }))
            .count(),
        baseline_shape_count
    );
}

#[test]
fn debug_draw_respects_callback_guard() {
    let mut world = debug_world();
    let _guard = boxddd::__private::enter_callback_guard_for_test();

    assert_eq!(
        world
            .debug_draw_frame(DebugDrawOptions::default())
            .unwrap_err(),
        Error::InCallback
    );
}

#[test]
fn debug_draw_rejects_invalid_bounds() {
    let mut world = debug_world();
    let options = DebugDrawOptions {
        drawing_bounds: Aabb {
            lower_bound: [2.0, 0.0, 0.0].into(),
            upper_bound: [1.0, 1.0, 1.0].into(),
        },
        ..Default::default()
    };

    assert_eq!(
        world.debug_draw_frame(options).unwrap_err(),
        Error::InvalidValue {
            context: "aabb.bounds",
            reason: InvalidValueReason::InvalidCombination,
        }
    );
}

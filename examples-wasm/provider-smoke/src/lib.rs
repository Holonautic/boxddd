use boxddd::error::InvalidValueReason;
use boxddd::{
    BodyType, BoxHull, DebugDrawOptions, DistanceInput, DistanceJointDef, Error, Foundation,
    FoundationConfig, Quat, QueryFilter, Recording, ShapeCastPairInput, ShapeProxy, Sphere,
    StallThreshold, Transform, Vec3, World, shape_cast_pair, shape_distance,
};

#[cfg(target_arch = "wasm32")]
use boxddd::{
    Aabb, BoxCastInput, Compound, DynamicTree, DynamicTreeCastControl, DynamicTreeFilter,
    HeightField, MeshData, RayCastInput, SurfaceMaterial, TaskSystem,
};

const OK: i32 = 0;
const ERR_WORLD: i32 = -1;
const ERR_SHAPE: i32 = -2;
const ERR_STEP: i32 = -3;
#[cfg(target_arch = "wasm32")]
const ERR_GUARDRAIL: i32 = -5;
const ERR_MOTION: i32 = -6;
const ERR_QUERY: i32 = -7;
#[cfg(target_arch = "wasm32")]
const ERR_CALLBACK_GUARDRAIL: i32 = -8;
const ERR_COLLISION: i32 = -9;
const ERR_JOINT: i32 = -10;
const ERR_EVENT_PROVENANCE: i32 = -11;
const ERR_RECORDING: i32 = -12;
const ERR_REPLAY: i32 = -13;
const ERR_TEARDOWN: i32 = -14;
const ERR_FOUNDATION: i32 = -15;
const ERR_LIFECYCLE: i32 = -16;

#[unsafe(no_mangle)]
pub extern "C" fn boxddd_provider_smoke() -> i32 {
    match run_smoke() {
        Ok(()) => OK,
        Err(code) => code,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn boxddd_provider_drop_millimeters() -> i32 {
    run_drop_millimeters().unwrap_or_else(|code| code)
}

#[unsafe(no_mangle)]
pub extern "C" fn boxddd_provider_ray_hit_millimeters() -> i32 {
    run_ray_hit_millimeters().unwrap_or_else(|code| code)
}

#[unsafe(no_mangle)]
pub extern "C" fn boxddd_provider_shape_cast_permyriad() -> i32 {
    run_shape_cast_permyriad().unwrap_or_else(|code| code)
}

#[unsafe(no_mangle)]
pub extern "C" fn boxddd_provider_joint_error_millimeters() -> i32 {
    run_joint_error_millimeters().unwrap_or_else(|code| code)
}

#[unsafe(no_mangle)]
pub extern "C" fn boxddd_provider_event_provenance_mask() -> i32 {
    run_event_provenance_mask().unwrap_or_else(|code| code)
}

#[unsafe(no_mangle)]
pub extern "C" fn boxddd_provider_foundation_lifecycle_mask() -> i32 {
    run_foundation_lifecycle_mask().unwrap_or_else(|code| code)
}

#[unsafe(no_mangle)]
pub extern "C" fn boxddd_provider_teardown_debug_shape_count() -> i32 {
    run_teardown_debug_shape_count().unwrap_or_else(|code| code)
}

fn foundation() -> Result<&'static Foundation, i32> {
    Foundation::initialize_default().map_err(|_| ERR_FOUNDATION)
}

fn create_world(
    foundation: &'static Foundation,
    gravity: Vec3,
    worker_count: u32,
    error: i32,
) -> Result<World, i32> {
    foundation
        .create_world(
            foundation
                .world_def_builder()
                .gravity(gravity)
                .worker_count(worker_count)
                .build()
                .map_err(|_| error)?,
        )
        .map_err(|_| error)
}

fn run_smoke() -> Result<(), i32> {
    let foundation = foundation()?;
    assert_wasm_guardrails(foundation)?;

    let mut world = create_world(foundation, Vec3::new(0.0, -10.0, 0.0), 1, ERR_WORLD)?;

    let ground = world
        .create_body(
            foundation
                .body_def_builder()
                .position([0.0, -1.0, 0.0])
                .build()
                .map_err(|_| ERR_WORLD)?,
        )
        .map_err(|_| ERR_WORLD)?;
    world
        .create_hull_shape(
            ground,
            &foundation.shape_def(),
            &BoxHull::new(8.0, 0.5, 8.0).map_err(|_| ERR_SHAPE)?,
        )
        .map_err(|_| ERR_SHAPE)?;

    let body = world
        .create_body(
            foundation
                .body_def_builder()
                .body_type(BodyType::Dynamic)
                .position([0.0, 4.0, 0.0])
                .build()
                .map_err(|_| ERR_WORLD)?,
        )
        .map_err(|_| ERR_WORLD)?;
    let shape = world
        .create_hull_shape(
            body,
            &foundation
                .shape_def_builder()
                .density(1.0)
                .friction(0.3)
                .build()
                .map_err(|_| ERR_SHAPE)?,
            &BoxHull::cube(0.5).map_err(|_| ERR_SHAPE)?,
        )
        .map_err(|_| ERR_SHAPE)?;
    if !world.contains_shape(shape).map_err(|_| ERR_SHAPE)? {
        return Err(ERR_SHAPE);
    }

    let start_y = world.body_position(body).map_err(|_| ERR_WORLD)?.y;
    for _ in 0..60 {
        world.step(1.0 / 60.0, 4).map_err(|_| ERR_STEP)?;
    }
    let end_y = world.body_position(body).map_err(|_| ERR_WORLD)?.y;
    if end_y >= start_y - 0.1 {
        return Err(ERR_MOTION);
    }

    let closest = world
        .cast_ray_closest([0.0, 8.0, 0.0], [0.0, -16.0, 0.0], QueryFilter::default())
        .map_err(|_| ERR_QUERY)?;
    if closest.is_none() {
        return Err(ERR_QUERY);
    }

    run_ray_hit_millimeters_with(foundation)?;
    run_shape_cast_permyriad_with(foundation)?;
    run_joint_error_millimeters_with(foundation)?;

    assert_provider_callback_guardrails(&mut world)?;

    Ok(())
}

fn run_drop_millimeters() -> Result<i32, i32> {
    let foundation = foundation()?;
    let mut world = create_world(foundation, Vec3::new(0.0, -10.0, 0.0), 1, ERR_WORLD)?;

    let ground = world
        .create_body(
            foundation
                .body_def_builder()
                .position([0.0, -1.0, 0.0])
                .build()
                .map_err(|_| ERR_WORLD)?,
        )
        .map_err(|_| ERR_WORLD)?;
    world
        .create_hull_shape(
            ground,
            &foundation.shape_def(),
            &BoxHull::new(8.0, 0.5, 8.0).map_err(|_| ERR_SHAPE)?,
        )
        .map_err(|_| ERR_SHAPE)?;

    let body = world
        .create_body(
            foundation
                .body_def_builder()
                .body_type(BodyType::Dynamic)
                .position([0.0, 4.0, 0.0])
                .build()
                .map_err(|_| ERR_WORLD)?,
        )
        .map_err(|_| ERR_WORLD)?;
    let shape = world
        .create_hull_shape(
            body,
            &foundation
                .shape_def_builder()
                .density(1.0)
                .friction(0.3)
                .build()
                .map_err(|_| ERR_SHAPE)?,
            &BoxHull::cube(0.5).map_err(|_| ERR_SHAPE)?,
        )
        .map_err(|_| ERR_SHAPE)?;
    if !world.contains_shape(shape).map_err(|_| ERR_SHAPE)? {
        return Err(ERR_SHAPE);
    }

    let start_y = world.body_position(body).map_err(|_| ERR_WORLD)?.y;
    for _ in 0..60 {
        world.step(1.0 / 60.0, 4).map_err(|_| ERR_STEP)?;
    }
    let end_y = world.body_position(body).map_err(|_| ERR_WORLD)?.y;
    if end_y >= start_y - 0.1 {
        return Err(ERR_MOTION);
    }

    let closest = world
        .cast_ray_closest([0.0, 8.0, 0.0], [0.0, -16.0, 0.0], QueryFilter::default())
        .map_err(|_| ERR_QUERY)?;
    if closest.is_none() {
        return Err(ERR_QUERY);
    }

    Ok(((start_y - end_y).max(0.0) * 1000.0) as i32)
}

fn run_ray_hit_millimeters() -> Result<i32, i32> {
    run_ray_hit_millimeters_with(foundation()?)
}

fn run_ray_hit_millimeters_with(foundation: &'static Foundation) -> Result<i32, i32> {
    let mut world = create_world(foundation, Vec3::ZERO, 1, ERR_WORLD)?;
    let body = world
        .create_body(
            foundation
                .body_def_builder()
                .position(Vec3::ZERO)
                .build()
                .map_err(|_| ERR_WORLD)?,
        )
        .map_err(|_| ERR_WORLD)?;
    let sphere = world
        .create_sphere_shape(
            body,
            &foundation
                .shape_def_builder()
                .density(1.0)
                .build()
                .map_err(|_| ERR_SHAPE)?,
            &Sphere::new([-1.0, 0.0, 0.0], 0.5),
        )
        .map_err(|_| ERR_SHAPE)?;
    if !world.contains_shape(sphere).map_err(|_| ERR_SHAPE)? {
        return Err(ERR_SHAPE);
    }
    let cube = world
        .create_hull_shape(
            body,
            &foundation
                .shape_def_builder()
                .density(1.0)
                .build()
                .map_err(|_| ERR_SHAPE)?,
            &BoxHull::cube(0.4).map_err(|_| ERR_SHAPE)?,
        )
        .map_err(|_| ERR_SHAPE)?;
    if !world.contains_shape(cube).map_err(|_| ERR_SHAPE)? {
        return Err(ERR_SHAPE);
    }

    let hit = world
        .cast_ray_closest([-3.0, 0.0, 0.0], [5.0, 0.0, 0.0], QueryFilter::default())
        .map_err(|_| ERR_QUERY)?
        .ok_or(ERR_QUERY)?;
    if !hit.fraction.is_finite() || !(0.0..=1.0).contains(&hit.fraction) {
        return Err(ERR_QUERY);
    }

    Ok((hit.fraction * 5000.0).round() as i32)
}

fn run_shape_cast_permyriad() -> Result<i32, i32> {
    run_shape_cast_permyriad_with(foundation()?)
}

fn run_shape_cast_permyriad_with(_foundation: &'static Foundation) -> Result<i32, i32> {
    let sphere_a = ShapeProxy::sphere(0.5).map_err(|_| ERR_COLLISION)?;
    let sphere_b = ShapeProxy::sphere(0.5).map_err(|_| ERR_COLLISION)?;

    let distance = shape_distance(
        DistanceInput::new(
            sphere_a.clone(),
            sphere_b.clone(),
            Transform::new(Vec3::new(1.4, 0.0, 0.0), Quat::IDENTITY),
        )
        .map_err(|_| ERR_COLLISION)?,
    )
    .map_err(|_| ERR_COLLISION)?;
    if !distance.distance.is_finite() || !(0.35..=0.45).contains(&distance.distance) {
        return Err(ERR_COLLISION);
    }

    let cast = shape_cast_pair(
        ShapeCastPairInput::new(
            sphere_a,
            sphere_b,
            Transform::new(Vec3::new(3.0, 0.0, 0.0), Quat::IDENTITY),
            Vec3::new(-4.0, 0.0, 0.0),
        )
        .map_err(|_| ERR_COLLISION)?,
    )
    .map_err(|_| ERR_COLLISION)?;
    if !cast.hit || !cast.fraction.is_finite() || !(0.0..=1.0).contains(&cast.fraction) {
        return Err(ERR_COLLISION);
    }

    Ok((cast.fraction * 10_000.0).round() as i32)
}

fn run_joint_error_millimeters() -> Result<i32, i32> {
    run_joint_error_millimeters_with(foundation()?)
}

fn run_joint_error_millimeters_with(foundation: &'static Foundation) -> Result<i32, i32> {
    let mut world = create_world(foundation, Vec3::ZERO, 1, ERR_WORLD)?;
    let anchor = world
        .create_body(
            foundation
                .body_def_builder()
                .position([0.0, 0.0, 0.0])
                .build()
                .map_err(|_| ERR_WORLD)?,
        )
        .map_err(|_| ERR_WORLD)?;
    let body = world
        .create_body(
            foundation
                .body_def_builder()
                .body_type(BodyType::Dynamic)
                .position([1.0, 0.0, 0.0])
                .build()
                .map_err(|_| ERR_WORLD)?,
        )
        .map_err(|_| ERR_WORLD)?;
    let shape = world
        .create_sphere_shape(
            body,
            &foundation
                .shape_def_builder()
                .density(1.0)
                .build()
                .map_err(|_| ERR_SHAPE)?,
            &Sphere::new([0.0, 0.0, 0.0], 0.25),
        )
        .map_err(|_| ERR_SHAPE)?;
    if !world.contains_shape(shape).map_err(|_| ERR_SHAPE)? {
        return Err(ERR_SHAPE);
    }

    let joint = world
        .create_distance_joint(DistanceJointDef::new(anchor, body).length(1.0))
        .map_err(|_| ERR_JOINT)?;
    world
        .apply_force_to_center(body, [25.0, 0.0, 0.0], true)
        .map_err(|_| ERR_JOINT)?;
    for _ in 0..60 {
        world.step(1.0 / 60.0, 4).map_err(|_| ERR_STEP)?;
    }

    let length = world
        .distance_joint_current_length(joint)
        .map_err(|_| ERR_JOINT)?;
    if !length.is_finite() || !(0.5..=1.5).contains(&length) {
        return Err(ERR_JOINT);
    }

    Ok(((length - 1.0).abs() * 1000.0).round() as i32)
}

fn run_event_provenance_mask() -> Result<i32, i32> {
    let foundation = foundation()?;
    let mut mask = 0;

    let mut contact_world = create_world(foundation, Vec3::ZERO, 1, ERR_EVENT_PROVENANCE)?;
    let ground = contact_world
        .create_body(foundation.body_def())
        .map_err(|_| ERR_EVENT_PROVENANCE)?;
    let ground_shape = contact_world
        .create_hull_shape(
            ground,
            &foundation.shape_def(),
            &BoxHull::new(2.0, 0.5, 2.0).map_err(|_| ERR_EVENT_PROVENANCE)?,
        )
        .map_err(|_| ERR_EVENT_PROVENANCE)?;
    let body = contact_world
        .create_body(
            foundation
                .body_def_builder()
                .body_type(BodyType::Dynamic)
                .position([0.0, 0.9, 0.0])
                .gravity_scale(0.0)
                .build()
                .map_err(|_| ERR_EVENT_PROVENANCE)?,
        )
        .map_err(|_| ERR_EVENT_PROVENANCE)?;
    let shape = contact_world
        .create_sphere_shape(
            body,
            &foundation
                .shape_def_builder()
                .density(1.0)
                .enable_contact_events(true)
                .build()
                .map_err(|_| ERR_EVENT_PROVENANCE)?,
            &Sphere::new(Vec3::ZERO, 0.5),
        )
        .map_err(|_| ERR_EVENT_PROVENANCE)?;

    let mut contact_id = None;
    for _ in 0..8 {
        contact_world
            .step(1.0 / 60.0, 4)
            .map_err(|_| ERR_EVENT_PROVENANCE)?;
        contact_id = contact_world
            .body_contacts(body)
            .map_err(|_| ERR_EVENT_PROVENANCE)?
            .first()
            .map(|contact| contact.contact_id);
        if contact_id.is_some() {
            break;
        }
    }
    let contact_id = contact_id.ok_or(ERR_EVENT_PROVENANCE)?;
    mask |= 1 << 0;

    contact_world
        .destroy_shape(shape, true)
        .map_err(|_| ERR_EVENT_PROVENANCE)?;
    if !contact_world
        .contains_shape(shape)
        .map_err(|_| ERR_EVENT_PROVENANCE)?
        && contact_world
            .contains_shape(ground_shape)
            .map_err(|_| ERR_EVENT_PROVENANCE)?
    {
        mask |= 1 << 1;
    }
    contact_world
        .step(1.0 / 60.0, 4)
        .map_err(|_| ERR_EVENT_PROVENANCE)?;

    let contact_events = contact_world
        .contact_events()
        .map_err(|_| ERR_EVENT_PROVENANCE)?;
    if let Some(event) = contact_events.end.iter().find(|event| {
        [event.shape_a, event.shape_b].contains(&shape)
            && [event.shape_a, event.shape_b].contains(&ground_shape)
    }) {
        mask |= 1 << 2;
        if event.contact_id == contact_id {
            mask |= 1 << 3;
        }
    }
    let contact_view_matches = contact_world
        .with_contact_events_view(|_, end, _| {
            for event in end {
                let shape_a = event.shape_a()?;
                let shape_b = event.shape_b()?;
                if [shape_a, shape_b].contains(&shape)
                    && [shape_a, shape_b].contains(&ground_shape)
                    && event.contact_id()? == contact_id
                {
                    return Ok::<_, Error>(true);
                }
            }
            Ok::<_, Error>(false)
        })
        .map_err(|_| ERR_EVENT_PROVENANCE)?
        .map_err(|_| ERR_EVENT_PROVENANCE)?;
    if contact_view_matches {
        mask |= 1 << 4;
    }
    contact_world
        .step(1.0 / 60.0, 4)
        .map_err(|_| ERR_EVENT_PROVENANCE)?;
    if !contact_world
        .contact_events()
        .map_err(|_| ERR_EVENT_PROVENANCE)?
        .end
        .iter()
        .any(|event| [event.shape_a, event.shape_b].contains(&shape))
    {
        mask |= 1 << 5;
    }

    let mut sensor_world = create_world(foundation, Vec3::ZERO, 1, ERR_EVENT_PROVENANCE)?;
    let sensor_body = sensor_world
        .create_body(foundation.body_def())
        .map_err(|_| ERR_EVENT_PROVENANCE)?;
    let sensor_shape = sensor_world
        .create_hull_shape(
            sensor_body,
            &foundation
                .shape_def_builder()
                .sensor(true)
                .enable_sensor_events(true)
                .build()
                .map_err(|_| ERR_EVENT_PROVENANCE)?,
            &BoxHull::new(2.0, 2.0, 2.0).map_err(|_| ERR_EVENT_PROVENANCE)?,
        )
        .map_err(|_| ERR_EVENT_PROVENANCE)?;
    let visitor_body = sensor_world
        .create_body(
            foundation
                .body_def_builder()
                .body_type(BodyType::Dynamic)
                .gravity_scale(0.0)
                .build()
                .map_err(|_| ERR_EVENT_PROVENANCE)?,
        )
        .map_err(|_| ERR_EVENT_PROVENANCE)?;
    let visitor_shape = sensor_world
        .create_sphere_shape(
            visitor_body,
            &foundation
                .shape_def_builder()
                .density(1.0)
                .enable_sensor_events(true)
                .build()
                .map_err(|_| ERR_EVENT_PROVENANCE)?,
            &Sphere::new(Vec3::ZERO, 0.5),
        )
        .map_err(|_| ERR_EVENT_PROVENANCE)?;

    let mut sensor_touch_seen = false;
    for _ in 0..8 {
        sensor_world
            .step(1.0 / 60.0, 4)
            .map_err(|_| ERR_EVENT_PROVENANCE)?;
        sensor_touch_seen = sensor_world
            .sensor_events()
            .map_err(|_| ERR_EVENT_PROVENANCE)?
            .begin
            .iter()
            .any(|event| {
                event.sensor_shape == sensor_shape && event.visitor_shape == visitor_shape
            });
        if sensor_touch_seen {
            break;
        }
    }
    if !sensor_touch_seen {
        return Err(ERR_EVENT_PROVENANCE);
    }
    mask |= 1 << 6;

    sensor_world
        .destroy_body(visitor_body)
        .map_err(|_| ERR_EVENT_PROVENANCE)?;
    if !sensor_world
        .contains_body(visitor_body)
        .map_err(|_| ERR_EVENT_PROVENANCE)?
        && !sensor_world
            .contains_shape(visitor_shape)
            .map_err(|_| ERR_EVENT_PROVENANCE)?
        && sensor_world
            .contains_shape(sensor_shape)
            .map_err(|_| ERR_EVENT_PROVENANCE)?
    {
        mask |= 1 << 7;
    }
    sensor_world
        .step(1.0 / 60.0, 4)
        .map_err(|_| ERR_EVENT_PROVENANCE)?;

    let sensor_events = sensor_world
        .sensor_events()
        .map_err(|_| ERR_EVENT_PROVENANCE)?;
    if sensor_events
        .end
        .iter()
        .any(|event| event.sensor_shape == sensor_shape && event.visitor_shape == visitor_shape)
    {
        mask |= 1 << 8;
    }
    let sensor_view_matches = sensor_world
        .with_sensor_events_view(|_, end| {
            for event in end {
                if event.sensor_shape()? == sensor_shape && event.visitor_shape()? == visitor_shape
                {
                    return Ok::<_, Error>(true);
                }
            }
            Ok::<_, Error>(false)
        })
        .map_err(|_| ERR_EVENT_PROVENANCE)?
        .map_err(|_| ERR_EVENT_PROVENANCE)?;
    if sensor_view_matches {
        mask |= 1 << 9;
    }
    sensor_world
        .step(1.0 / 60.0, 4)
        .map_err(|_| ERR_EVENT_PROVENANCE)?;
    if !sensor_world
        .sensor_events()
        .map_err(|_| ERR_EVENT_PROVENANCE)?
        .end
        .iter()
        .any(|event| event.sensor_shape == sensor_shape && event.visitor_shape == visitor_shape)
    {
        mask |= 1 << 10;
    }

    Ok(mask)
}

fn record_replay_scene(foundation: &'static Foundation) -> Result<Vec<u8>, i32> {
    let mut world = create_world(foundation, Vec3::ZERO, 1, ERR_RECORDING)?;
    let mut recording = Recording::new().map_err(|_| ERR_RECORDING)?;
    let mut session = world.record(&mut recording).map_err(|_| ERR_RECORDING)?;
    let body = session
        .world()
        .create_body(
            foundation
                .body_def_builder()
                .body_type(BodyType::Dynamic)
                .gravity_scale(0.0)
                .build()
                .map_err(|_| ERR_RECORDING)?,
        )
        .map_err(|_| ERR_RECORDING)?;
    session
        .world()
        .create_sphere_shape(
            body,
            &foundation
                .shape_def_builder()
                .density(1.0)
                .build()
                .map_err(|_| ERR_RECORDING)?,
            &Sphere::new(Vec3::ZERO, 0.5),
        )
        .map_err(|_| ERR_RECORDING)?;
    for _ in 0..2 {
        session
            .world()
            .step(1.0 / 60.0, 4)
            .map_err(|_| ERR_RECORDING)?;
    }
    session.finish().map_err(|_| ERR_RECORDING)?;
    recording.to_vec().map_err(|_| ERR_RECORDING)
}

fn run_foundation_lifecycle_mask() -> Result<i32, i32> {
    let foundation = foundation()?;
    let replay_bytes = record_replay_scene(foundation)?;
    let mut mask = 0;
    if std::ptr::eq(
        foundation,
        Foundation::initialize_default().map_err(|_| ERR_LIFECYCLE)?,
    ) {
        mask |= 1 << 0;
    }
    if matches!(
        Foundation::initialize(FoundationConfig {
            length_units_per_meter: 2.0,
            stall_threshold: StallThreshold::Disabled,
        }),
        Err(Error::FoundationConflict)
    ) && foundation.config() == FoundationConfig::default()
    {
        mask |= 1 << 1;
    }

    let truncated = replay_bytes.get(..47).ok_or(ERR_RECORDING)?;
    if matches!(
        foundation.create_replay_player(truncated, 1),
        Err(Error::InvalidValue {
            context: "recording.replay_bytes",
            reason: InvalidValueReason::Malformed,
        })
    ) && matches!(
        foundation.validate_replay(truncated, 1),
        Err(Error::InvalidValue {
            context: "recording.replay_bytes",
            reason: InvalidValueReason::Malformed,
        })
    ) {
        mask |= 1 << 2;
    }

    let ordinary = create_world(foundation, Vec3::ZERO, 1, ERR_REPLAY)?;
    if matches!(
        foundation.create_replay_player(&replay_bytes, 1),
        Err(Error::FoundationBusy)
    ) {
        mask |= 1 << 3;
    }
    drop(ordinary);

    let player = foundation
        .create_replay_player(&replay_bytes, 1)
        .map_err(|_| ERR_REPLAY)?;
    if foundation.activity().replay_active {
        mask |= 1 << 4;
    }
    let blocked_world = foundation.create_world(
        foundation
            .world_def_builder()
            .gravity(Vec3::ZERO)
            .worker_count(1)
            .build()
            .map_err(|_| ERR_LIFECYCLE)?,
    );
    if matches!(blocked_world, Err(Error::FoundationBusy)) {
        mask |= 1 << 5;
    }
    if matches!(boxddd::version(), Err(Error::FoundationBusy)) {
        mask |= 1 << 6;
    }
    if matches!(
        foundation.create_replay_player(&replay_bytes, 1),
        Err(Error::FoundationBusy)
    ) {
        mask |= 1 << 7;
    }
    {
        let _callback = boxddd::__private::enter_callback_guard_for_test();
        let callback_world = foundation.create_world(
            foundation
                .world_def_builder()
                .gravity(Vec3::ZERO)
                .worker_count(1)
                .build()
                .map_err(|_| ERR_LIFECYCLE)?,
        );
        if matches!(callback_world, Err(Error::InCallback))
            && matches!(boxddd::version(), Err(Error::InCallback))
        {
            mask |= 1 << 8;
        }
        if matches!(
            foundation.create_replay_player(&replay_bytes, 1),
            Err(Error::InCallback)
        ) {
            mask |= 1 << 9;
        }
    }

    let before_drain = boxddd::__private::defer_replay_drop_for_test(player);
    if before_drain.replay_active {
        mask |= 1 << 10;
    }
    if foundation.activity() == boxddd::FoundationActivity::default() {
        let recovered = create_world(foundation, Vec3::ZERO, 1, ERR_LIFECYCLE)?;
        drop(recovered);
        if foundation.activity() == boxddd::FoundationActivity::default() {
            mask |= 1 << 11;
        }
    }

    Ok(mask)
}

fn run_teardown_debug_shape_count() -> Result<i32, i32> {
    let foundation = foundation()?;
    let mut world = foundation
        .create_world(
            foundation
                .world_def_builder()
                .gravity(Vec3::ZERO)
                .worker_count(1)
                .build()
                .map_err(|_| ERR_TEARDOWN)?,
        )
        .map_err(|_| ERR_TEARDOWN)?;
    let body = world
        .create_body(foundation.body_def())
        .map_err(|_| ERR_TEARDOWN)?;
    world
        .create_hull_shape(
            body,
            &foundation.shape_def(),
            &BoxHull::cube(0.5).map_err(|_| ERR_TEARDOWN)?,
        )
        .map_err(|_| ERR_TEARDOWN)?;
    let frame = world
        .debug_draw_frame(DebugDrawOptions::default())
        .map_err(|_| ERR_TEARDOWN)?;
    let created_count = frame
        .events
        .iter()
        .filter(|event| matches!(event, boxddd::DebugShapeEvent::Created(_)))
        .count() as i32;
    drop(world);
    Ok(created_count)
}

#[cfg(target_arch = "wasm32")]
fn assert_wasm_guardrails(foundation: &'static Foundation) -> Result<(), i32> {
    if !is_unsupported_on_wasm(TaskSystem::blocking_threads()) {
        return Err(ERR_GUARDRAIL);
    }
    let multithreaded = foundation
        .world_def_builder()
        .worker_count(2)
        .build()
        .map_err(|_| ERR_GUARDRAIL)?;
    if !is_unsupported_on_wasm(foundation.create_world(multithreaded)) {
        return Err(ERR_GUARDRAIL);
    }

    let serial = foundation
        .world_def_builder()
        .worker_count(1)
        .build()
        .map_err(|_| ERR_GUARDRAIL)?;
    let mut world = foundation.create_world(serial).map_err(|_| ERR_GUARDRAIL)?;
    if !is_unsupported_on_wasm(world.set_worker_count(2)) {
        return Err(ERR_GUARDRAIL);
    }
    if !is_unsupported_on_wasm(foundation.validate_replay(&[0], 2)) {
        return Err(ERR_GUARDRAIL);
    }
    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
fn assert_wasm_guardrails(_foundation: &'static Foundation) -> Result<(), i32> {
    Ok(())
}

#[cfg(target_arch = "wasm32")]
fn assert_provider_callback_guardrails(world: &mut World) -> Result<(), i32> {
    let aabb = Aabb {
        lower_bound: Vec3::new(-2.0, -2.0, -2.0),
        upper_bound: Vec3::new(2.0, 5.0, 2.0),
    };
    let overlaps = world
        .overlap_aabb(aabb, QueryFilter::default())
        .map_err(|_| ERR_CALLBACK_GUARDRAIL)?;
    if overlaps.is_empty() {
        return Err(ERR_CALLBACK_GUARDRAIL);
    }
    let ray_hits = world
        .cast_ray(
            Vec3::new(0.0, 8.0, 0.0),
            Vec3::new(0.0, -16.0, 0.0),
            QueryFilter::default(),
        )
        .map_err(|_| ERR_CALLBACK_GUARDRAIL)?;
    if ray_hits.is_empty() {
        return Err(ERR_CALLBACK_GUARDRAIL);
    }
    let frame = world
        .debug_draw_frame(DebugDrawOptions::default())
        .map_err(|_| ERR_CALLBACK_GUARDRAIL)?;
    if !frame
        .events
        .iter()
        .any(|event| matches!(event, boxddd::DebugShapeEvent::Created(_)))
        || !frame.commands.iter().any(|command| {
            matches!(
                command,
                boxddd::DebugDrawCommand::Shape {
                    handle: Some(_),
                    ..
                }
            )
        })
    {
        return Err(ERR_CALLBACK_GUARDRAIL);
    }
    if !is_unsupported_on_wasm(world.set_custom_filter(|_, _| true)) {
        return Err(ERR_CALLBACK_GUARDRAIL);
    }
    if !is_unsupported_on_wasm(world.set_pre_solve(|_, _, _, _| true)) {
        return Err(ERR_CALLBACK_GUARDRAIL);
    }
    if !is_unsupported_on_wasm(
        world.set_friction_callback(|a, b| (a.coefficient * b.coefficient).sqrt()),
    ) {
        return Err(ERR_CALLBACK_GUARDRAIL);
    }
    if !is_unsupported_on_wasm(
        world.set_restitution_callback(|a, b| a.coefficient.max(b.coefficient)),
    ) {
        return Err(ERR_CALLBACK_GUARDRAIL);
    }

    let tree = DynamicTree::new().map_err(|_| ERR_CALLBACK_GUARDRAIL)?;
    if !is_unsupported_on_wasm(tree.query(aabb, DynamicTreeFilter::default())) {
        return Err(ERR_CALLBACK_GUARDRAIL);
    }
    if !is_unsupported_on_wasm(tree.visit_query_closest(
        Vec3::ZERO,
        DynamicTreeFilter::default(),
        1.0,
        |_| 0.0,
    )) {
        return Err(ERR_CALLBACK_GUARDRAIL);
    }
    let ray = RayCastInput::new(Vec3::new(-1.0, 0.0, 0.0), Vec3::new(2.0, 0.0, 0.0))
        .map_err(|_| ERR_CALLBACK_GUARDRAIL)?;
    if !is_unsupported_on_wasm(tree.visit_ray_cast(ray, DynamicTreeFilter::default(), |_| {
        DynamicTreeCastControl::Continue
    })) {
        return Err(ERR_CALLBACK_GUARDRAIL);
    }
    let box_cast =
        BoxCastInput::new(aabb, Vec3::new(1.0, 0.0, 0.0)).map_err(|_| ERR_CALLBACK_GUARDRAIL)?;
    if !is_unsupported_on_wasm(
        tree.visit_box_cast(box_cast, DynamicTreeFilter::default(), |_| {
            DynamicTreeCastControl::Continue
        }),
    ) {
        return Err(ERR_CALLBACK_GUARDRAIL);
    }

    let unit_scale = Vec3::new(1.0, 1.0, 1.0);
    let mesh =
        MeshData::box_mesh(Vec3::ZERO, unit_scale, true).map_err(|_| ERR_CALLBACK_GUARDRAIL)?;
    if !is_unsupported_on_wasm(mesh.query_triangles(aabb, unit_scale)) {
        return Err(ERR_CALLBACK_GUARDRAIL);
    }

    let height_field =
        HeightField::grid(3, 3, unit_scale, false).map_err(|_| ERR_CALLBACK_GUARDRAIL)?;
    if !is_unsupported_on_wasm(height_field.query_triangles(aabb)) {
        return Err(ERR_CALLBACK_GUARDRAIL);
    }

    let compound =
        Compound::single_sphere(Sphere::new(Vec3::ZERO, 0.5), SurfaceMaterial::default())
            .map_err(|_| ERR_CALLBACK_GUARDRAIL)?;
    if !is_unsupported_on_wasm(compound.query_aabb(aabb)) {
        return Err(ERR_CALLBACK_GUARDRAIL);
    }
    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
fn assert_provider_callback_guardrails(_world: &mut World) -> Result<(), i32> {
    Ok(())
}

#[cfg(target_arch = "wasm32")]
fn is_unsupported_on_wasm<T>(result: boxddd::Result<T>) -> bool {
    matches!(result, Err(Error::UnsupportedOnWasm))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exported_metrics_cover_core_examples() {
        assert!(run_drop_millimeters().expect("drop metric") > 100);
        assert_eq!(run_ray_hit_millimeters().expect("ray metric"), 1500);
        let shape_cast = run_shape_cast_permyriad().expect("shape-cast metric");
        assert!((4000..=6000).contains(&shape_cast));
        let joint_error = run_joint_error_millimeters().expect("joint metric");
        assert!((0..=500).contains(&joint_error));
        assert_eq!(
            run_foundation_lifecycle_mask().expect("Foundation lifecycle metric"),
            4095
        );
    }
}

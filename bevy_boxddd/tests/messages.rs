use bevy_app::App;
use bevy_boxddd::prelude::*;
use bevy_ecs::message::Messages;
use bevy_math::Vec3;
use bevy_time::{TimePlugin, TimeUpdateStrategy};
use bevy_transform::components::Transform;

fn physics_app(settings: BoxdddPhysicsSettings) -> App {
    let mut app = App::new();
    app.add_plugins(TimePlugin)
        .insert_resource(TimeUpdateStrategy::FixedTimesteps(1))
        .add_plugins(
            BoxdddPhysicsPlugin::new(boxddd::FoundationConfig::default()).with_settings(settings),
        );
    app
}

#[test]
fn contact_messages_include_boxddd_ids_and_bevy_entities() {
    let mut app = physics_app(BoxdddPhysicsSettings::default());
    let ground = app
        .world_mut()
        .spawn((
            RigidBody::Static,
            Collider::cuboid(10.0, 0.5, 10.0),
            Transform::from_xyz(0.0, -0.5, 0.0),
        ))
        .id();
    let sphere = app
        .world_mut()
        .spawn((
            RigidBody::Dynamic,
            Collider::sphere(0.5),
            PhysicsMaterial {
                enable_contact_events: true,
                enable_hit_events: true,
                ..Default::default()
            },
            Transform::from_xyz(0.0, 4.0, 0.0),
        ))
        .id();

    let mut seen_contact = false;
    for _ in 0..180 {
        app.update();
        let contacts = app
            .world_mut()
            .resource_mut::<Messages<BoxdddContactBeginMessage>>()
            .drain()
            .collect::<Vec<_>>();
        let context = app.world().get_non_send::<BoxdddPhysicsContext>().unwrap();
        seen_contact |= contacts.iter().any(|message| {
            [message.entity_a, message.entity_b].contains(&Some(ground))
                && [message.entity_a, message.entity_b].contains(&Some(sphere))
                && context.shape_entity(message.shape_a) == message.entity_a
                && context.shape_entity(message.shape_b) == message.entity_b
        });
        if seen_contact {
            break;
        }
    }

    assert!(seen_contact);
}

#[test]
fn sensor_messages_include_shape_entity_mapping() {
    let settings = BoxdddPhysicsSettings {
        gravity: Vec3::ZERO,
        ..Default::default()
    };
    let mut app = physics_app(settings);
    let wall = app
        .world_mut()
        .spawn((
            RigidBody::Static,
            Collider::cuboid(0.5, 10.0, 1.0),
            PhysicsMaterial {
                enable_sensor_events: true,
                ..Default::default()
            },
            Transform::from_xyz(1.5, 0.0, 0.0),
        ))
        .id();
    let bullet = app
        .world_mut()
        .spawn((
            RigidBody::Dynamic,
            Collider::sphere(0.1),
            PhysicsMaterial {
                is_sensor: true,
                enable_sensor_events: true,
                ..Default::default()
            },
            LinearVelocity(Vec3::new(-20.0, 0.0, 0.0)),
            Transform::from_xyz(7.39814, 0.0, 0.0),
        ))
        .id();

    let mut seen_begin = false;
    for _ in 0..120 {
        app.update();
        let messages = app
            .world_mut()
            .resource_mut::<Messages<BoxdddSensorBeginMessage>>()
            .drain()
            .collect::<Vec<_>>();
        seen_begin |= messages.iter().any(|message| {
            [message.sensor_entity, message.visitor_entity].contains(&Some(wall))
                && [message.sensor_entity, message.visitor_entity].contains(&Some(bullet))
        });
        if seen_begin {
            break;
        }
    }

    assert!(seen_begin);
}

#[test]
fn advanced_step_with_callback_error_still_publishes_and_syncs() {
    let settings = BoxdddPhysicsSettings {
        gravity: Vec3::ZERO,
        ..Default::default()
    };
    let mut app = physics_app(settings);
    let ground = app
        .world_mut()
        .spawn((
            RigidBody::Static,
            Collider::cuboid(2.0, 0.5, 2.0),
            Transform::from_xyz(0.0, -0.5, 0.0),
        ))
        .id();
    let contact_body = app
        .world_mut()
        .spawn((
            RigidBody::Dynamic,
            Collider::sphere(0.5),
            PhysicsMaterial {
                enable_contact_events: true,
                ..Default::default()
            },
            Transform::from_xyz(0.0, 0.25, 0.0),
        ))
        .id();
    let moving_body = app
        .world_mut()
        .spawn((
            RigidBody::Dynamic,
            Collider::sphere(0.25),
            LinearVelocity(Vec3::new(3.0, 0.0, 0.0)),
            Transform::from_xyz(-10.0, 2.0, 0.0),
        ))
        .id();

    app.world_mut()
        .get_non_send_mut::<BoxdddPhysicsContext>()
        .unwrap()
        .world_mut()
        .unwrap()
        .set_friction_callback(|_, _| panic!("friction callback failure"))
        .unwrap();

    app.update();
    app.update();

    let errors = app
        .world_mut()
        .resource_mut::<Messages<BoxdddErrorMessage>>()
        .drain()
        .collect::<Vec<_>>();
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].operation, BoxdddOperation::StepWorld);
    assert_eq!(errors[0].error, boxddd::Error::CallbackPanicked);

    let contacts = app
        .world_mut()
        .resource_mut::<Messages<BoxdddContactBeginMessage>>()
        .drain()
        .collect::<Vec<_>>();
    assert!(contacts.iter().any(|message| {
        [message.entity_a, message.entity_b].contains(&Some(ground))
            && [message.entity_a, message.entity_b].contains(&Some(contact_body))
    }));

    let transform = app.world().entity(moving_body).get::<Transform>().unwrap();
    assert!(transform.translation.x > -10.0);
}

#[test]
fn pre_native_step_failure_skips_message_publication_and_transform_sync() {
    let settings = BoxdddPhysicsSettings {
        gravity: Vec3::ZERO,
        sub_step_count: -1,
        ..Default::default()
    };
    let mut app = physics_app(settings);
    let moving_body = app
        .world_mut()
        .spawn((
            RigidBody::Dynamic,
            Collider::sphere(0.25),
            LinearVelocity(Vec3::new(3.0, 0.0, 0.0)),
            Transform::from_xyz(-10.0, 2.0, 0.0),
        ))
        .id();

    app.update();
    app.update();

    let transform = app.world().entity(moving_body).get::<Transform>().unwrap();
    assert_eq!(transform.translation.x, -10.0);
    assert!(
        app.world_mut()
            .resource_mut::<Messages<BoxdddBodyMoveMessage>>()
            .drain()
            .next()
            .is_none()
    );
    let errors = app
        .world_mut()
        .resource_mut::<Messages<BoxdddErrorMessage>>()
        .drain()
        .collect::<Vec<_>>();
    assert_eq!(
        errors,
        vec![BoxdddErrorMessage {
            operation: BoxdddOperation::StepWorld,
            entity: None,
            error: boxddd::Error::InvalidValue {
                context: "world.step.sub_step_count",
                reason: boxddd::error::InvalidValueReason::OutOfRange,
            },
        }]
    );
}

#[test]
fn removing_overlapping_collider_publishes_contact_end_with_survivor_mapping() {
    let settings = BoxdddPhysicsSettings {
        gravity: Vec3::ZERO,
        ..Default::default()
    };
    let mut app = physics_app(settings);
    let ground = app
        .world_mut()
        .spawn((
            RigidBody::Static,
            Collider::cuboid(2.0, 0.5, 2.0),
            Transform::from_xyz(0.0, -0.5, 0.0),
        ))
        .id();
    let sphere = app
        .world_mut()
        .spawn((
            RigidBody::Dynamic,
            Collider::sphere(0.5),
            PhysicsMaterial {
                enable_contact_events: true,
                ..Default::default()
            },
            Transform::from_xyz(0.0, 0.25, 0.0),
        ))
        .id();

    let mut saw_begin = false;
    for _ in 0..4 {
        app.update();
        saw_begin |= app
            .world_mut()
            .resource_mut::<Messages<BoxdddContactBeginMessage>>()
            .drain()
            .any(|message| {
                [message.entity_a, message.entity_b].contains(&Some(ground))
                    && [message.entity_a, message.entity_b].contains(&Some(sphere))
            });
        if saw_begin {
            break;
        }
    }
    assert!(saw_begin);

    let ground_shape = app
        .world()
        .entity(ground)
        .get::<BoxdddShape>()
        .unwrap()
        .id();
    let removed_shape = app
        .world()
        .entity(sphere)
        .get::<BoxdddShape>()
        .unwrap()
        .id();
    app.world_mut()
        .resource_mut::<Messages<BoxdddErrorMessage>>()
        .clear();
    app.world_mut().entity_mut(sphere).remove::<Collider>();

    app.update();

    let end = app
        .world_mut()
        .resource_mut::<Messages<BoxdddContactEndMessage>>()
        .drain()
        .find(|message| {
            [message.shape_a, message.shape_b].contains(&ground_shape)
                && [message.shape_a, message.shape_b].contains(&removed_shape)
        })
        .expect("removing an overlapping collider should publish contact end");
    if end.shape_a == removed_shape {
        assert_eq!(end.entity_a, None);
        assert_eq!(end.entity_b, Some(ground));
    } else {
        assert_eq!(end.entity_a, Some(ground));
        assert_eq!(end.entity_b, None);
    }
    let errors = app
        .world_mut()
        .resource_mut::<Messages<BoxdddErrorMessage>>()
        .drain()
        .collect::<Vec<_>>();
    assert!(
        !errors
            .iter()
            .any(|message| message.operation == BoxdddOperation::ReadEvents)
    );
}

#[test]
fn despawning_overlapping_body_publishes_sensor_end_with_survivor_mapping() {
    let settings = BoxdddPhysicsSettings {
        gravity: Vec3::ZERO,
        ..Default::default()
    };
    let mut app = physics_app(settings);
    let sensor = app
        .world_mut()
        .spawn((
            RigidBody::Static,
            Collider::cuboid(1.0, 1.0, 1.0),
            PhysicsMaterial {
                is_sensor: true,
                enable_sensor_events: true,
                ..Default::default()
            },
            Transform::default(),
        ))
        .id();
    let visitor = app
        .world_mut()
        .spawn((
            RigidBody::Dynamic,
            Collider::sphere(0.25),
            PhysicsMaterial {
                enable_sensor_events: true,
                ..Default::default()
            },
            Transform::default(),
        ))
        .id();

    let mut saw_begin = false;
    for _ in 0..4 {
        app.update();
        saw_begin |= app
            .world_mut()
            .resource_mut::<Messages<BoxdddSensorBeginMessage>>()
            .drain()
            .any(|message| {
                message.sensor_entity == Some(sensor) && message.visitor_entity == Some(visitor)
            });
        if saw_begin {
            break;
        }
    }
    assert!(saw_begin);

    let sensor_shape = app
        .world()
        .entity(sensor)
        .get::<BoxdddShape>()
        .unwrap()
        .id();
    let visitor_shape = app
        .world()
        .entity(visitor)
        .get::<BoxdddShape>()
        .unwrap()
        .id();
    app.world_mut()
        .resource_mut::<Messages<BoxdddErrorMessage>>()
        .clear();
    app.world_mut().entity_mut(visitor).despawn();

    app.update();

    let end = app
        .world_mut()
        .resource_mut::<Messages<BoxdddSensorEndMessage>>()
        .drain()
        .find(|message| {
            message.sensor_shape == sensor_shape && message.visitor_shape == visitor_shape
        })
        .expect("despawning an overlapping body should publish sensor end");
    assert_eq!(end.sensor_entity, Some(sensor));
    assert_eq!(end.visitor_entity, None);
    let errors = app
        .world_mut()
        .resource_mut::<Messages<BoxdddErrorMessage>>()
        .drain()
        .collect::<Vec<_>>();
    assert!(
        !errors
            .iter()
            .any(|message| message.operation == BoxdddOperation::ReadEvents)
    );
}

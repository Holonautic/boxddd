use boxddd::error::HandleKind;
use boxddd::{
    BodyDef, BodyId, BodyType, BoxHull, ContactEvents, ContactId, DistanceJointDef, Error, Filter,
    SensorEvents, ShapeDef, ShapeId, Sphere, Vec3, World, WorldDef,
};

fn world_with_live_contact() -> (World, BodyId, BodyId, ShapeId, ContactId) {
    let mut world = World::new(WorldDef::builder().gravity(Vec3::ZERO).build().unwrap()).unwrap();
    let ground = world.create_body(BodyDef::default()).unwrap();
    world
        .create_hull_shape(
            ground,
            &ShapeDef::default(),
            &BoxHull::new(2.0, 0.5, 2.0).unwrap(),
        )
        .unwrap();
    let body = world
        .create_body(
            BodyDef::builder()
                .body_type(BodyType::Dynamic)
                .position([0.0, 0.9, 0.0])
                .gravity_scale(0.0)
                .build()
                .unwrap(),
        )
        .unwrap();
    let shape = world
        .create_sphere_shape(
            body,
            &ShapeDef::builder()
                .density(1.0)
                .enable_contact_events(true)
                .build()
                .unwrap(),
            &Sphere::new(Vec3::ZERO, 0.5),
        )
        .unwrap();

    for _ in 0..8 {
        world.step(1.0 / 60.0, 4).unwrap();
        if let Some(contact) = world.body_contacts(body).unwrap().into_iter().next() {
            assert!(world.contains_contact(contact.contact_id));
            return (world, ground, body, shape, contact.contact_id);
        }
    }

    panic!("expected a live contact");
}

fn world_with_live_sensor_touch() -> (World, BodyId, BodyId, ShapeId, ShapeId) {
    let mut world = World::new(WorldDef::builder().gravity(Vec3::ZERO).build().unwrap()).unwrap();
    let sensor_body = world.create_body(BodyDef::default()).unwrap();
    let sensor_shape = world
        .create_hull_shape(
            sensor_body,
            &ShapeDef::builder()
                .sensor(true)
                .enable_sensor_events(true)
                .build()
                .unwrap(),
            &BoxHull::new(2.0, 2.0, 2.0).unwrap(),
        )
        .unwrap();
    let visitor_body = world
        .create_body(
            BodyDef::builder()
                .body_type(BodyType::Dynamic)
                .gravity_scale(0.0)
                .build()
                .unwrap(),
        )
        .unwrap();
    let visitor_shape = world
        .create_sphere_shape(
            visitor_body,
            &ShapeDef::builder()
                .density(1.0)
                .enable_sensor_events(true)
                .build()
                .unwrap(),
            &Sphere::new(Vec3::ZERO, 0.5),
        )
        .unwrap();

    for _ in 0..8 {
        world.step(1.0 / 60.0, 4).unwrap();
        if world
            .sensor_events()
            .unwrap()
            .begin
            .iter()
            .any(|event| event.sensor_shape == sensor_shape && event.visitor_shape == visitor_shape)
        {
            return (
                world,
                sensor_body,
                visitor_body,
                sensor_shape,
                visitor_shape,
            );
        }
    }

    panic!("expected a live sensor touch");
}

#[test]
fn sensor_events_support_owned_into_and_view_reads() {
    let mut world = World::new(WorldDef::default()).unwrap();

    let wall = world
        .create_body(
            BodyDef::builder()
                .position([1.5, 0.0, 0.0])
                .build()
                .unwrap(),
        )
        .unwrap();
    let wall_shape = world
        .create_hull_shape(
            wall,
            &ShapeDef::builder()
                .enable_sensor_events(true)
                .build()
                .unwrap(),
            &BoxHull::new(0.5, 10.0, 1.0).unwrap(),
        )
        .unwrap();

    let bullet = world
        .create_body(
            BodyDef::builder()
                .body_type(BodyType::Dynamic)
                .position([7.39814, 0.0, 0.0])
                .linear_velocity([-20.0, 0.0, 0.0])
                .gravity_scale(0.0)
                .bullet(true)
                .build()
                .unwrap(),
        )
        .unwrap();
    let bullet_shape = world
        .create_sphere_shape(
            bullet,
            &ShapeDef::builder()
                .sensor(true)
                .enable_sensor_events(true)
                .build()
                .unwrap(),
            &Sphere::new([0.0, 0.0, 0.0], 0.1),
        )
        .unwrap();

    let mut begin_seen = false;
    let mut end_seen = false;
    let mut reusable = boxddd::SensorEvents::default();

    for _ in 0..120 {
        world.step(1.0 / 60.0, 4).unwrap();
        world.sensor_events_into(&mut reusable).unwrap();
        begin_seen |= reusable.begin.iter().any(|event| {
            [event.sensor_shape, event.visitor_shape].contains(&wall_shape)
                && [event.sensor_shape, event.visitor_shape].contains(&bullet_shape)
        });
        end_seen |= reusable.end.iter().any(|event| {
            [event.sensor_shape, event.visitor_shape].contains(&wall_shape)
                && [event.sensor_shape, event.visitor_shape].contains(&bullet_shape)
        });

        let view_count = world
            .with_sensor_events_view(|begin, end| {
                let begin = begin
                    .map(|event| {
                        event.sensor_shape()?;
                        event.visitor_shape()?;
                        Ok(())
                    })
                    .collect::<boxddd::Result<Vec<_>>>()?;
                let end = end
                    .map(|event| {
                        event.sensor_shape()?;
                        event.visitor_shape()?;
                        Ok(())
                    })
                    .collect::<boxddd::Result<Vec<_>>>()?;
                Ok::<usize, boxddd::Error>(begin.len() + end.len())
            })
            .unwrap()
            .unwrap();
        assert_eq!(view_count, reusable.begin.len() + reusable.end.len());

        if begin_seen && end_seen {
            break;
        }
    }

    assert!(begin_seen);
    assert!(end_seen);
    assert!(world.sensor_events().unwrap().begin.is_empty() || begin_seen);
}

#[test]
fn shape_sensor_data_reports_current_overlaps() {
    let mut world = World::new(WorldDef::default()).unwrap();

    let sensor_body = world
        .create_body(
            BodyDef::builder()
                .body_type(BodyType::Kinematic)
                .position([0.0, 2.0, 0.0])
                .build()
                .unwrap(),
        )
        .unwrap();
    let sensor_shape = world
        .create_hull_shape(
            sensor_body,
            &ShapeDef::builder()
                .sensor(true)
                .enable_sensor_events(true)
                .build()
                .unwrap(),
            &BoxHull::new(2.0, 2.0, 2.0).unwrap(),
        )
        .unwrap();

    let visitor_body = world
        .create_body(
            BodyDef::builder()
                .body_type(BodyType::Dynamic)
                .position([0.0, 7.0, 0.0])
                .build()
                .unwrap(),
        )
        .unwrap();
    let visitor_shape = world
        .create_sphere_shape(
            visitor_body,
            &ShapeDef::builder()
                .density(1.0)
                .enable_sensor_events(true)
                .build()
                .unwrap(),
            &Sphere::new(Vec3::ZERO, 0.5),
        )
        .unwrap();

    let mut begin_seen = false;
    for _ in 0..180 {
        world.step(1.0 / 60.0, 4).unwrap();
        let events = world.sensor_events().unwrap();
        begin_seen |= events.begin.iter().any(|event| {
            [event.sensor_shape, event.visitor_shape].contains(&sensor_shape)
                && [event.sensor_shape, event.visitor_shape].contains(&visitor_shape)
        });
        if begin_seen {
            break;
        }
    }
    assert!(begin_seen);

    let visitors = world.shape_sensor_data(sensor_shape).unwrap();
    assert!(visitors.contains(&visitor_shape), "{visitors:?}");

    let mut reusable = vec![sensor_shape];
    world
        .shape_sensor_data_into(sensor_shape, &mut reusable)
        .unwrap();
    assert!(reusable.contains(&visitor_shape));
}

#[test]
fn contact_and_hit_events_capture_ids_materials_and_reuse_buffers() {
    let mut world = World::new(
        WorldDef::builder()
            .gravity(Vec3::new(0.0, -10.0, 0.0))
            .build()
            .unwrap(),
    )
    .unwrap();
    world.set_hit_event_threshold(1.0).unwrap();

    let ground = world
        .create_body(
            BodyDef::builder()
                .position([0.0, -0.5, 0.0])
                .build()
                .unwrap(),
        )
        .unwrap();
    let ground_shape = world
        .create_hull_shape(
            ground,
            &ShapeDef::builder().user_material_id(11).build().unwrap(),
            &BoxHull::new(10.0, 0.5, 10.0).unwrap(),
        )
        .unwrap();

    let sphere = world
        .create_body(
            BodyDef::builder()
                .body_type(BodyType::Dynamic)
                .position([0.0, 4.0, 0.0])
                .build()
                .unwrap(),
        )
        .unwrap();
    let sphere_shape = world
        .create_sphere_shape(
            sphere,
            &ShapeDef::builder()
                .density(1.0)
                .restitution(0.6)
                .user_material_id(7)
                .enable_contact_events(true)
                .enable_hit_events(true)
                .build()
                .unwrap(),
            &Sphere::new([0.0, 0.0, 0.0], 0.5),
        )
        .unwrap();

    let mut events = ContactEvents {
        begin: Vec::with_capacity(8),
        end: Vec::with_capacity(8),
        hit: Vec::with_capacity(8),
    };
    let begin_capacity = events.begin.capacity();
    let mut begin_seen = false;
    let mut hit_seen = false;
    let mut contact_id = None;

    for _ in 0..160 {
        world.step(1.0 / 60.0, 4).unwrap();
        world.contact_events_into(&mut events).unwrap();
        assert!(events.begin.capacity() >= begin_capacity);

        begin_seen |= events.begin.iter().any(|event| {
            let matches = world.contains_contact(event.contact_id)
                && [event.shape_a, event.shape_b].contains(&ground_shape)
                && [event.shape_a, event.shape_b].contains(&sphere_shape);
            if matches {
                contact_id = Some(event.contact_id);
            }
            matches
        });
        hit_seen |= events.hit.iter().any(|event| {
            [event.shape_a, event.shape_b].contains(&ground_shape)
                && [event.shape_a, event.shape_b].contains(&sphere_shape)
                && [event.user_material_id_a, event.user_material_id_b].contains(&7)
                && event.approach_speed > 0.0
        });
        if let Some(event) = events.hit.iter().find(|event| {
            world.contains_contact(event.contact_id)
                && [event.shape_a, event.shape_b].contains(&ground_shape)
                && [event.shape_a, event.shape_b].contains(&sphere_shape)
        }) {
            contact_id = Some(event.contact_id);
        }

        let view_count = world
            .with_contact_events_view(|begin, end, hit| {
                let begin = begin
                    .map(|event| {
                        event.shape_a()?;
                        event.shape_b()?;
                        event.contact_id()?;
                        Ok(())
                    })
                    .collect::<boxddd::Result<Vec<_>>>()?;
                let end = end
                    .map(|event| {
                        event.shape_a()?;
                        event.shape_b()?;
                        event.contact_id()?;
                        Ok(())
                    })
                    .collect::<boxddd::Result<Vec<_>>>()?;
                let hit = hit
                    .map(|event| {
                        event.shape_a()?;
                        event.shape_b()?;
                        event.contact_id()?;
                        Ok(())
                    })
                    .collect::<boxddd::Result<Vec<_>>>()?;
                Ok::<usize, boxddd::Error>(begin.len() + end.len() + hit.len())
            })
            .unwrap()
            .unwrap();
        assert_eq!(
            view_count,
            events.begin.len() + events.end.len() + events.hit.len()
        );

        if begin_seen
            && hit_seen
            && contact_id.is_some_and(|contact_id| world.contains_contact(contact_id))
        {
            break;
        }
    }

    assert!(begin_seen);
    assert!(hit_seen);

    let contact_id = contact_id.expect("contact id");
    assert!(world.contains_contact(contact_id));
    let contact = world.contact_data(contact_id).unwrap();
    assert_eq!(contact.contact_id, contact_id);
    assert!(
        [contact.shape_id_a, contact.shape_id_b].contains(&ground_shape)
            && [contact.shape_id_a, contact.shape_id_b].contains(&sphere_shape)
    );
    assert!(!contact.manifolds.is_empty());

    let other_world = World::new(WorldDef::default()).unwrap();
    assert!(!other_world.contains_contact(contact_id));
    assert_eq!(
        other_world.contact_data(contact_id).unwrap_err(),
        Error::ForeignHandle {
            kind: HandleKind::Contact,
        }
    );

    let shape_contacts = world.shape_contacts(sphere_shape).unwrap();
    assert!(
        shape_contacts.iter().any(|contact| {
            [contact.shape_id_a, contact.shape_id_b].contains(&ground_shape)
                && [contact.shape_id_a, contact.shape_id_b].contains(&sphere_shape)
        }),
        "{shape_contacts:?}"
    );

    let mut reusable_contacts = vec![contact.clone()];
    world
        .shape_contacts_into(sphere_shape, &mut reusable_contacts)
        .unwrap();
    assert!(reusable_contacts.iter().any(|contact| {
        [contact.shape_id_a, contact.shape_id_b].contains(&ground_shape)
            && [contact.shape_id_a, contact.shape_id_b].contains(&sphere_shape)
    }));

    world.step(1.0 / 60.0, 4).unwrap();
    assert!(!world.contains_contact(contact_id));
    assert_eq!(
        world.contact_data(contact_id).unwrap_err(),
        Error::StaleHandle {
            kind: HandleKind::Contact,
        }
    );
}

#[test]
fn body_events_report_simulated_motion() {
    let mut world = World::new(
        WorldDef::builder()
            .gravity(Vec3::new(0.0, -10.0, 0.0))
            .build()
            .unwrap(),
    )
    .unwrap();
    let body = world
        .create_body(
            BodyDef::builder()
                .body_type(BodyType::Dynamic)
                .position([0.0, 2.0, 0.0])
                .build()
                .unwrap(),
        )
        .unwrap();
    world
        .create_sphere_shape(
            body,
            &ShapeDef::builder().density(1.0).build().unwrap(),
            &Sphere::new([0.0, 0.0, 0.0], 0.5),
        )
        .unwrap();

    world.step(1.0 / 60.0, 4).unwrap();
    let events = world.body_events().unwrap();
    assert!(events.iter().any(|event| event.body_id == body));

    let view_ids = world
        .with_body_events_view(|events| {
            events
                .map(|event| event.body_id())
                .collect::<boxddd::Result<Vec<_>>>()
        })
        .unwrap()
        .unwrap();
    assert!(view_ids.contains(&body));

    let mut reusable = vec![events[0].clone()];
    world.body_events_into(&mut reusable).unwrap();
    assert!(reusable.iter().any(|event| event.body_id == body));
}

#[test]
fn event_apis_respect_callback_guard() {
    let (world, _, _, _, contact_id) = world_with_live_contact();
    assert!(world.contains_contact(contact_id));
    let _guard = boxddd::__private::enter_callback_guard_for_test();

    assert_eq!(world.body_events().unwrap_err(), boxddd::Error::InCallback);
    assert_eq!(
        world.sensor_events().unwrap_err(),
        boxddd::Error::InCallback
    );
    assert_eq!(
        world.contact_events().unwrap_err(),
        boxddd::Error::InCallback
    );
    assert_eq!(
        world.contact_data(contact_id).unwrap_err(),
        boxddd::Error::InCallback
    );
    assert_eq!(world.joint_events().unwrap_err(), boxddd::Error::InCallback);
}

fn assert_contact_stale(world: &World, contact_id: ContactId) {
    assert!(!world.contains_contact(contact_id));
    assert_eq!(
        world.contact_data(contact_id).unwrap_err(),
        Error::StaleHandle {
            kind: HandleKind::Contact,
        }
    );
}

#[test]
fn destroying_shape_preserves_contact_end_event_provenance_until_next_rotation() {
    let (mut world, _, _, shape, contact_id) = world_with_live_contact();
    world.destroy_shape(shape, true).unwrap();

    assert!(!world.contains_shape(shape));
    assert_eq!(
        world.shape_type(shape).unwrap_err(),
        Error::StaleHandle {
            kind: HandleKind::Shape,
        }
    );
    assert_contact_stale(&world, contact_id);

    // Box3D publishes destruction end events from the next completed step.
    world.step(1.0 / 60.0, 4).unwrap();

    let owned = world.contact_events().unwrap();
    let end = owned
        .end
        .iter()
        .find(|event| [event.shape_a, event.shape_b].contains(&shape))
        .expect("destroyed shape contact end event");
    assert_eq!(end.contact_id, contact_id);
    assert_contact_stale(&world, end.contact_id);

    let mut reusable = ContactEvents {
        begin: Vec::with_capacity(4),
        end: Vec::with_capacity(4),
        hit: Vec::with_capacity(4),
    };
    world.contact_events_into(&mut reusable).unwrap();
    assert_eq!(reusable, owned);

    let view = world
        .with_contact_events_view(|begin, end, hit| {
            let begin = begin
                .map(|event| Ok((event.shape_a()?, event.shape_b()?, event.contact_id()?)))
                .collect::<boxddd::Result<Vec<_>>>()?;
            let end = end
                .map(|event| Ok((event.shape_a()?, event.shape_b()?, event.contact_id()?)))
                .collect::<boxddd::Result<Vec<_>>>()?;
            let hit = hit
                .map(|event| Ok((event.shape_a()?, event.shape_b()?, event.contact_id()?)))
                .collect::<boxddd::Result<Vec<_>>>()?;
            Ok::<_, Error>((begin, end, hit))
        })
        .unwrap()
        .unwrap();
    let owned_view = (
        owned
            .begin
            .iter()
            .map(|event| (event.shape_a, event.shape_b, event.contact_id))
            .collect::<Vec<_>>(),
        owned
            .end
            .iter()
            .map(|event| (event.shape_a, event.shape_b, event.contact_id))
            .collect::<Vec<_>>(),
        owned
            .hit
            .iter()
            .map(|event| (event.shape_a, event.shape_b, event.contact_id))
            .collect::<Vec<_>>(),
    );
    assert_eq!(view, owned_view);

    world.step(1.0 / 60.0, 4).unwrap();
    assert!(
        !world
            .contact_events()
            .unwrap()
            .end
            .iter()
            .any(|event| [event.shape_a, event.shape_b].contains(&shape))
    );
}

#[test]
fn destroying_body_preserves_sensor_end_event_provenance_until_next_rotation() {
    let (mut world, _, visitor_body, sensor_shape, visitor_shape) = world_with_live_sensor_touch();
    world.destroy_body(visitor_body).unwrap();

    assert!(!world.contains_body(visitor_body));
    assert!(!world.contains_shape(visitor_shape));
    assert_eq!(
        world.shape_type(visitor_shape).unwrap_err(),
        Error::StaleHandle {
            kind: HandleKind::Shape,
        }
    );

    // Box3D publishes destruction end events from the next completed step.
    world.step(1.0 / 60.0, 4).unwrap();

    let owned = world.sensor_events().unwrap();
    let end = owned
        .end
        .iter()
        .find(|event| event.sensor_shape == sensor_shape && event.visitor_shape == visitor_shape)
        .expect("destroyed visitor sensor end event");
    assert_eq!(end.sensor_shape, sensor_shape);
    assert_eq!(end.visitor_shape, visitor_shape);
    assert_eq!(
        world.shape_type(end.visitor_shape).unwrap_err(),
        Error::StaleHandle {
            kind: HandleKind::Shape,
        }
    );

    let mut reusable = SensorEvents {
        begin: Vec::with_capacity(4),
        end: Vec::with_capacity(4),
    };
    world.sensor_events_into(&mut reusable).unwrap();
    assert_eq!(reusable, owned);

    let view = world
        .with_sensor_events_view(|begin, end| {
            let begin = begin
                .map(|event| Ok((event.sensor_shape()?, event.visitor_shape()?)))
                .collect::<boxddd::Result<Vec<_>>>()?;
            let end = end
                .map(|event| Ok((event.sensor_shape()?, event.visitor_shape()?)))
                .collect::<boxddd::Result<Vec<_>>>()?;
            Ok::<_, Error>((begin, end))
        })
        .unwrap()
        .unwrap();
    let owned_view = (
        owned
            .begin
            .iter()
            .map(|event| (event.sensor_shape, event.visitor_shape))
            .collect::<Vec<_>>(),
        owned
            .end
            .iter()
            .map(|event| (event.sensor_shape, event.visitor_shape))
            .collect::<Vec<_>>(),
    );
    assert_eq!(view, owned_view);

    world.step(1.0 / 60.0, 4).unwrap();
    assert!(!world.sensor_events().unwrap().end.iter().any(|event| {
        event.sensor_shape == sensor_shape && event.visitor_shape == visitor_shape
    }));
}

#[test]
fn contact_turnover_mutations_retire_old_contact_ids() {
    let (mut world, _, body, _, contact_id) = world_with_live_contact();
    world.disable_body(body).unwrap();
    assert_contact_stale(&world, contact_id);

    let (mut world, _, _, shape, contact_id) = world_with_live_contact();
    world
        .set_shape_filter(
            shape,
            Filter {
                mask_bits: 0,
                ..Filter::default()
            },
            true,
        )
        .unwrap();
    assert_contact_stale(&world, contact_id);

    let (mut world, _, _, shape, contact_id) = world_with_live_contact();
    world
        .set_shape_sphere(shape, &Sphere::new(Vec3::ZERO, 0.4))
        .unwrap();
    assert_contact_stale(&world, contact_id);

    let (mut world, ground, body, _, contact_id) = world_with_live_contact();
    let joint = world
        .create_distance_joint(
            DistanceJointDef::new(ground, body)
                .collide_connected(true)
                .length(0.9),
        )
        .unwrap();
    assert!(world.contains_contact(contact_id));
    world.set_joint_collide_connected(joint, false).unwrap();
    assert_contact_stale(&world, contact_id);
}

//! Fixed-update systems registered by [`crate::BoxdddPhysicsPlugin`].

use crate::components::{
    AngularVelocity, BodySettings, BoxdddBody, BoxdddJoint, BoxdddShape, Collider, ExternalForce,
    ExternalImpulse, Joint, JointTarget, LinearVelocity, PhysicsMaterial, RigidBody,
    TransformSyncMode,
};
use crate::errors::report_error;
use crate::math::{apply_boxddd_transform, to_boxddd_pos, to_boxddd_quat, to_boxddd_vec3};
use crate::messages::{
    BoxdddBodyMoveMessage, BoxdddContactBeginMessage, BoxdddContactEndMessage,
    BoxdddContactHitMessage, BoxdddErrorMessage, BoxdddOperation, BoxdddSensorBeginMessage,
    BoxdddSensorEndMessage,
};
use crate::resources::{
    BoxdddPhysicsContext, BoxdddPhysicsSettings, ShapeDescriptor, ShapeLocalTransform,
};
use bevy_ecs::hierarchy::ChildOf;
use bevy_ecs::message::MessageWriter;
use bevy_ecs::prelude::{Changed, Commands, Entity, NonSendMut, Query, Res, With, Without};
use bevy_ecs::system::SystemParam;
use bevy_math::Vec3;
use bevy_time::{Fixed, Time};
use bevy_transform::components::Transform;
use boxddd::{
    BodyId, BodyType, BoxHull, Capsule as BoxdddCapsule, Compound, DistanceJointDef, HeightField,
    Hull, JointId, MeshData, PrismaticJointDef, RevoluteJointDef, ShapeDef, ShapeId,
    Sphere as BoxdddSphere, SphericalJointDef, Transform as BoxdddTransform, WeldJointDef,
    WheelJointDef,
};

/// Query used by [`create_missing_bodies`] for entities awaiting native bodies.
pub type MissingBodyQuery<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static RigidBody,
        Option<&'static BodySettings>,
        Option<&'static Transform>,
        Option<&'static LinearVelocity>,
        Option<&'static AngularVelocity>,
    ),
    Without<BoxdddBody>,
>;

/// Query used by [`create_missing_shapes`] for colliders awaiting native shapes.
pub type MissingShapeQuery<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        Option<&'static BoxdddBody>,
        Option<&'static ChildOf>,
        &'static Collider,
        Option<&'static PhysicsMaterial>,
        Option<&'static Transform>,
    ),
    Without<BoxdddShape>,
>;

/// Query used by [`cleanup_removed_colliders`] for tracked native shapes.
pub type TrackedShapeQuery<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static BoxdddShape,
        Option<&'static Collider>,
        Option<&'static PhysicsMaterial>,
        Option<&'static Transform>,
        Option<&'static BoxdddBody>,
        Option<&'static ChildOf>,
    ),
>;

/// Query used by [`apply_body_controls`] for runtime body inputs.
pub type BodyControlQuery<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static BoxdddBody,
        Option<&'static LinearVelocity>,
        Option<&'static AngularVelocity>,
        Option<&'static ExternalForce>,
        Option<&'static ExternalImpulse>,
    ),
>;

/// Query used by [`sync_bevy_transforms_to_boxddd`] for Bevy-authored transforms.
pub type BevyToPhysicsBodyQuery<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static BoxdddBody,
        &'static Transform,
        Option<&'static TransformSyncMode>,
        Option<&'static RigidBody>,
    ),
>;

/// Query used by [`sync_boxddd_transforms_to_bevy`] for physics-authored transforms.
pub type PhysicsToBevyBodyQuery<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static BoxdddBody,
        &'static mut Transform,
        Option<&'static TransformSyncMode>,
        Option<&'static RigidBody>,
    ),
>;

/// Message writers used to publish one coherent advanced physics step.
#[derive(SystemParam)]
pub struct PhysicsMessageWriters<'w> {
    errors: MessageWriter<'w, BoxdddErrorMessage>,
    body_moves: MessageWriter<'w, BoxdddBodyMoveMessage>,
    contact_begin: MessageWriter<'w, BoxdddContactBeginMessage>,
    contact_end: MessageWriter<'w, BoxdddContactEndMessage>,
    contact_hit: MessageWriter<'w, BoxdddContactHitMessage>,
    sensor_begin: MessageWriter<'w, BoxdddSensorBeginMessage>,
    sensor_end: MessageWriter<'w, BoxdddSensorEndMessage>,
}

/// Creates native Box3D bodies for entities with [`RigidBody`] but no [`BoxdddBody`].
pub fn create_missing_bodies(
    mut commands: Commands,
    mut context: NonSendMut<BoxdddPhysicsContext>,
    settings: Res<BoxdddPhysicsSettings>,
    mut errors: MessageWriter<BoxdddErrorMessage>,
    bodies: MissingBodyQuery,
) {
    if context.world().is_none() {
        return;
    }
    let foundation = context
        .foundation()
        .expect("a live physics context pairs its World with a Foundation");

    for (entity, rigid_body, body_settings, transform, linear_velocity, angular_velocity) in &bodies
    {
        let body_settings = body_settings.copied().unwrap_or_default();
        if let Err(error) = body_settings.validate() {
            report_error(
                &settings,
                &mut errors,
                BoxdddErrorMessage {
                    operation: BoxdddOperation::CreateBody,
                    entity: Some(entity),
                    error,
                },
            );
            continue;
        }

        let mut def = foundation
            .body_def_builder()
            .body_type((*rigid_body).into());
        def = def
            .gravity_scale(body_settings.gravity_scale)
            .bullet(body_settings.bullet);

        if let Some(transform) = transform {
            let rotation = match to_boxddd_quat(transform.rotation) {
                Ok(rotation) => rotation,
                Err(error) => {
                    report_error(
                        &settings,
                        &mut errors,
                        BoxdddErrorMessage {
                            operation: BoxdddOperation::CreateBody,
                            entity: Some(entity),
                            error,
                        },
                    );
                    continue;
                }
            };
            def = def
                .position(to_boxddd_pos(transform.translation))
                .rotation(rotation);
        }

        if let Some(linear_velocity) = linear_velocity {
            def = def.linear_velocity(to_boxddd_vec3(linear_velocity.0));
        }

        if let Some(angular_velocity) = angular_velocity {
            def = def.angular_velocity(to_boxddd_vec3(angular_velocity.0));
        }

        let result = def
            .build()
            .and_then(|def| context.world_mut().expect("checked above").create_body(def));

        match result {
            Ok(body_id) => {
                context.insert_body(entity, body_id);
                commands.entity(entity).insert(BoxdddBody(body_id));
            }
            Err(error) => report_error(
                &settings,
                &mut errors,
                BoxdddErrorMessage {
                    operation: BoxdddOperation::CreateBody,
                    entity: Some(entity),
                    error,
                },
            ),
        }
    }
}

/// Applies changed runtime body settings to native bodies.
pub fn apply_body_settings(
    mut context: NonSendMut<BoxdddPhysicsContext>,
    settings: Res<BoxdddPhysicsSettings>,
    mut errors: MessageWriter<BoxdddErrorMessage>,
    bodies: Query<(Entity, &BoxdddBody, &BodySettings), Changed<BodySettings>>,
) {
    if context.world().is_none() {
        return;
    }

    for (entity, body, body_settings) in &bodies {
        let result = apply_body_settings_to_world(
            context.world_mut().expect("checked above"),
            body.0,
            *body_settings,
        );
        apply_control_result(&settings, &mut errors, entity, result);
    }
}

/// Creates native Box3D shapes for entities with [`Collider`] but no [`BoxdddShape`].
///
/// Colliders may live on the body entity itself or on a child entity of a body.
pub fn create_missing_shapes(
    mut commands: Commands,
    mut context: NonSendMut<BoxdddPhysicsContext>,
    settings: Res<BoxdddPhysicsSettings>,
    mut errors: MessageWriter<BoxdddErrorMessage>,
    colliders: MissingShapeQuery,
    bodies: Query<&BoxdddBody>,
) {
    if context.world().is_none() {
        return;
    }
    let foundation = context
        .foundation()
        .expect("a live physics context pairs its World with a Foundation");

    for (entity, own_body, parent, collider, material, transform) in &colliders {
        let (body_entity, body) = match resolve_collider_body(entity, own_body, parent, &bodies) {
            Ok(body) => body,
            Err(error) => {
                report_error(
                    &settings,
                    &mut errors,
                    BoxdddErrorMessage {
                        operation: BoxdddOperation::CreateShape,
                        entity: Some(entity),
                        error,
                    },
                );
                continue;
            }
        };
        let local_transform = if own_body.is_some() {
            ShapeLocalTransform::IDENTITY
        } else {
            ShapeLocalTransform::from_transform(transform)
        };
        let descriptor = ShapeDescriptor {
            collider: *collider,
            material: material.copied().unwrap_or_default(),
            local_transform,
        };
        let result = collider
            .validate()
            .and_then(|()| descriptor.material.shape_def(foundation))
            .and_then(|shape_def| {
                create_shape(
                    context.world_mut().expect("checked above"),
                    body.0,
                    descriptor.collider,
                    descriptor.local_transform,
                    &shape_def,
                )
            });

        match result {
            Ok(shape_id) => {
                context.insert_shape(entity, body_entity, descriptor, shape_id);
                commands.entity(entity).insert(BoxdddShape(shape_id));
            }
            Err(error) => report_error(
                &settings,
                &mut errors,
                BoxdddErrorMessage {
                    operation: BoxdddOperation::CreateShape,
                    entity: Some(entity),
                    error,
                },
            ),
        }
    }
}

/// Creates native Box3D joints for entities with [`JointTarget`] and [`Joint`].
pub fn create_missing_joints(
    mut commands: Commands,
    mut context: NonSendMut<BoxdddPhysicsContext>,
    settings: Res<BoxdddPhysicsSettings>,
    mut errors: MessageWriter<BoxdddErrorMessage>,
    joints: Query<(Entity, &JointTarget, &Joint), Without<BoxdddJoint>>,
    bodies: Query<&BoxdddBody>,
) {
    if context.world().is_none() {
        return;
    }

    for (entity, target, joint) in &joints {
        let result = resolve_joint_bodies(target, &bodies).and_then(|(body_a, body_b)| {
            create_joint(
                context.world_mut().expect("checked above"),
                body_a,
                body_b,
                *joint,
            )
        });

        match result {
            Ok(joint_id) => {
                context.insert_joint(entity, target.body_a, target.body_b, *joint, joint_id);
                commands.entity(entity).insert(BoxdddJoint(joint_id));
            }
            Err(error) => report_error(
                &settings,
                &mut errors,
                BoxdddErrorMessage {
                    operation: BoxdddOperation::CreateJoint,
                    entity: Some(entity),
                    error,
                },
            ),
        }
    }
}

/// Destroys or recreates native shapes when collider entities are removed or changed.
pub fn cleanup_removed_colliders(
    mut commands: Commands,
    mut context: NonSendMut<BoxdddPhysicsContext>,
    settings: Res<BoxdddPhysicsSettings>,
    mut errors: MessageWriter<BoxdddErrorMessage>,
    shapes: TrackedShapeQuery,
    bodies: Query<&BoxdddBody>,
) {
    if context.world().is_none() {
        return;
    }

    let stale_shape_entities = context
        .entity_to_shape
        .iter()
        .filter_map(|(entity, shape_id)| {
            shapes.get(*entity).is_err().then_some((*entity, *shape_id))
        })
        .collect::<Vec<_>>();

    for (entity, shape_id) in stale_shape_entities {
        let result = context
            .world_mut()
            .expect("checked above")
            .destroy_shape(shape_id, true);

        match result {
            Ok(()) => {
                context.remove_shape(entity, shape_id);
            }
            Err(error) => report_error(
                &settings,
                &mut errors,
                BoxdddErrorMessage {
                    operation: BoxdddOperation::DestroyShape,
                    entity: Some(entity),
                    error,
                },
            ),
        }
    }

    for (entity, shape, collider, material, transform, own_body, parent) in &shapes {
        let current_body_entity = collider
            .is_some()
            .then(|| {
                resolve_collider_body(entity, own_body, parent, &bodies)
                    .ok()
                    .map(|(entity, _)| entity)
            })
            .flatten();
        let tracked_body_entity = context.shape_body_entity(entity);
        let local_transform = if own_body.is_some() {
            ShapeLocalTransform::IDENTITY
        } else {
            ShapeLocalTransform::from_transform(transform)
        };
        let current_descriptor = collider.map(|collider| ShapeDescriptor {
            collider: *collider,
            material: material.copied().unwrap_or_default(),
            local_transform,
        });
        let tracked_descriptor = context.shape_descriptor(entity);

        if collider.is_some()
            && current_body_entity.is_some()
            && current_body_entity == tracked_body_entity
            && current_descriptor == tracked_descriptor
        {
            continue;
        }

        let result = context
            .world_mut()
            .expect("checked above")
            .destroy_shape(shape.0, true);

        match result {
            Ok(()) => {
                context.remove_shape(entity, shape.0);
                commands.entity(entity).remove::<BoxdddShape>();
            }
            Err(error) => report_error(
                &settings,
                &mut errors,
                BoxdddErrorMessage {
                    operation: BoxdddOperation::DestroyShape,
                    entity: Some(entity),
                    error,
                },
            ),
        }
    }
}

/// Destroys or recreates native joints when joint entities are removed or changed.
pub fn cleanup_removed_joints(
    mut commands: Commands,
    mut context: NonSendMut<BoxdddPhysicsContext>,
    settings: Res<BoxdddPhysicsSettings>,
    mut errors: MessageWriter<BoxdddErrorMessage>,
    joints: Query<(Entity, &BoxdddJoint, Option<&JointTarget>, Option<&Joint>)>,
    bodies: Query<&BoxdddBody>,
) {
    if context.world().is_none() {
        return;
    }

    let stale_joint_entities = context
        .entity_to_joint
        .iter()
        .filter_map(|(entity, joint_id)| {
            joints.get(*entity).is_err().then_some((*entity, *joint_id))
        })
        .collect::<Vec<_>>();

    for (entity, joint_id) in stale_joint_entities {
        let result = context
            .world_mut()
            .expect("checked above")
            .destroy_joint(joint_id, true);

        match result {
            Ok(()) => {
                context.remove_joint(entity, joint_id);
            }
            Err(error) => report_error(
                &settings,
                &mut errors,
                BoxdddErrorMessage {
                    operation: BoxdddOperation::DestroyJoint,
                    entity: Some(entity),
                    error,
                },
            ),
        }
    }

    for (entity, joint_id, target, joint) in &joints {
        let current_target = target.map(|target| (target.body_a, target.body_b));
        let tracked_target = context.joint_body_entities(entity);
        let tracked_joint = context.joint_descriptor(entity);
        let endpoints_exist = target
            .map(|target| bodies.get(target.body_a).is_ok() && bodies.get(target.body_b).is_ok())
            .unwrap_or(false);

        if joint.is_some()
            && current_target.is_some()
            && current_target == tracked_target
            && joint.copied() == tracked_joint
            && endpoints_exist
        {
            continue;
        }

        let result = context
            .world_mut()
            .expect("checked above")
            .destroy_joint(joint_id.0, true);

        match result {
            Ok(()) => {
                context.remove_joint(entity, joint_id.0);
                commands.entity(entity).remove::<BoxdddJoint>();
            }
            Err(error) => report_error(
                &settings,
                &mut errors,
                BoxdddErrorMessage {
                    operation: BoxdddOperation::DestroyJoint,
                    entity: Some(entity),
                    error,
                },
            ),
        }
    }
}

/// Destroys native bodies when their Bevy body entities are removed.
///
/// Shapes and joints owned by the removed body are detached from their Bevy entities too.
pub fn cleanup_removed_bodies(
    mut commands: Commands,
    mut context: NonSendMut<BoxdddPhysicsContext>,
    settings: Res<BoxdddPhysicsSettings>,
    mut errors: MessageWriter<BoxdddErrorMessage>,
    bodies: Query<(), With<BoxdddBody>>,
    shapes: Query<Option<&BoxdddShape>>,
    joints: Query<Option<&BoxdddJoint>>,
) {
    if context.world().is_none() {
        return;
    }

    let stale = context
        .entity_to_body
        .iter()
        .filter_map(|(entity, body_id)| bodies.get(*entity).is_err().then_some((*entity, *body_id)))
        .map(|(entity, body_id)| {
            let shapes = context
                .shape_to_body_entity
                .iter()
                .filter_map(|(shape_entity, body_entity)| {
                    (*body_entity == entity)
                        .then(|| {
                            shapes
                                .get(*shape_entity)
                                .ok()
                                .flatten()
                                .copied()
                                .map(|shape| (*shape_entity, shape))
                        })
                        .flatten()
                })
                .collect::<Vec<_>>();
            let joints = context
                .joint_to_body_entities
                .iter()
                .filter_map(|(joint_entity, (body_a, body_b))| {
                    (*body_a == entity || *body_b == entity)
                        .then(|| {
                            joints
                                .get(*joint_entity)
                                .ok()
                                .flatten()
                                .copied()
                                .map(|joint| (*joint_entity, joint))
                        })
                        .flatten()
                })
                .collect::<Vec<_>>();
            (entity, body_id, shapes, joints)
        })
        .collect::<Vec<_>>();

    for (entity, body_id, shapes, joints) in stale {
        let result = context
            .world_mut()
            .expect("checked above")
            .destroy_body(body_id);

        match result {
            Ok(()) => {
                context.remove_body(entity, body_id);
                for (shape_entity, _) in shapes {
                    commands.entity(shape_entity).remove::<BoxdddShape>();
                }
                for (joint_entity, _) in joints {
                    commands.entity(joint_entity).remove::<BoxdddJoint>();
                }
            }
            Err(error) => report_error(
                &settings,
                &mut errors,
                BoxdddErrorMessage {
                    operation: BoxdddOperation::DestroyBody,
                    entity: Some(entity),
                    error,
                },
            ),
        }
    }
}

fn resolve_collider_body<'a>(
    collider_entity: Entity,
    own_body: Option<&'a BoxdddBody>,
    parent: Option<&ChildOf>,
    bodies: &'a Query<'_, '_, &BoxdddBody>,
) -> boxddd::Result<(Entity, &'a BoxdddBody)> {
    if let Some(body) = own_body {
        return Ok((collider_entity, body));
    }

    let parent = parent
        .map(ChildOf::parent)
        .ok_or_else(|| invalid_combination("collider.parent"))?;
    let body = bodies
        .get(parent)
        .map_err(|_| invalid_combination("collider.parent"))?;
    Ok((parent, body))
}

fn resolve_joint_bodies(
    target: &JointTarget,
    bodies: &Query<'_, '_, &BoxdddBody>,
) -> boxddd::Result<(BodyId, BodyId)> {
    let body_a = bodies
        .get(target.body_a)
        .map_err(|_| invalid_combination("joint.target.body_a"))?
        .0;
    let body_b = bodies
        .get(target.body_b)
        .map_err(|_| invalid_combination("joint.target.body_b"))?
        .0;
    Ok((body_a, body_b))
}

/// Applies velocity, force, and one-shot impulse components to native bodies.
pub fn apply_body_controls(
    mut commands: Commands,
    mut context: NonSendMut<BoxdddPhysicsContext>,
    settings: Res<BoxdddPhysicsSettings>,
    mut errors: MessageWriter<BoxdddErrorMessage>,
    controls: BodyControlQuery,
) {
    if context.world().is_none() {
        return;
    }

    for (entity, body, linear_velocity, angular_velocity, force, impulse) in &controls {
        if let Some(linear_velocity) = linear_velocity {
            apply_control_result(
                &settings,
                &mut errors,
                entity,
                context
                    .world_mut()
                    .expect("checked above")
                    .set_body_linear_velocity(body.0, to_boxddd_vec3(linear_velocity.0)),
            );
        }

        if let Some(angular_velocity) = angular_velocity {
            apply_control_result(
                &settings,
                &mut errors,
                entity,
                context
                    .world_mut()
                    .expect("checked above")
                    .set_body_angular_velocity(body.0, to_boxddd_vec3(angular_velocity.0)),
            );
        }

        if let Some(force) = force {
            let result = match force.point {
                Some(point) => context.world_mut().expect("checked above").apply_force(
                    body.0,
                    to_boxddd_vec3(force.force),
                    to_boxddd_pos(point),
                    force.wake,
                ),
                None => context
                    .world_mut()
                    .expect("checked above")
                    .apply_force_to_center(body.0, to_boxddd_vec3(force.force), force.wake),
            };
            apply_control_result(&settings, &mut errors, entity, result);
        }

        if let Some(impulse) = impulse {
            let result = match impulse.point {
                Some(point) => context
                    .world_mut()
                    .expect("checked above")
                    .apply_linear_impulse(
                        body.0,
                        to_boxddd_vec3(impulse.impulse),
                        to_boxddd_pos(point),
                        impulse.wake,
                    ),
                None => context
                    .world_mut()
                    .expect("checked above")
                    .apply_linear_impulse_to_center(
                        body.0,
                        to_boxddd_vec3(impulse.impulse),
                        impulse.wake,
                    ),
            };
            apply_control_result(&settings, &mut errors, entity, result);
            commands.entity(entity).remove::<ExternalImpulse>();
        }
    }
}

/// Writes Bevy transforms into Box3D for bodies using [`TransformSyncMode::BevyToPhysics`].
pub fn sync_bevy_transforms_to_boxddd(
    mut context: NonSendMut<BoxdddPhysicsContext>,
    settings: Res<BoxdddPhysicsSettings>,
    mut errors: MessageWriter<BoxdddErrorMessage>,
    bodies: BevyToPhysicsBodyQuery,
) {
    if context.world().is_none() {
        return;
    }

    for (entity, body, transform, sync_mode, rigid_body) in &bodies {
        if effective_sync_mode(sync_mode, rigid_body) != TransformSyncMode::BevyToPhysics {
            continue;
        }

        let result = to_boxddd_quat(transform.rotation).and_then(|rotation| {
            context
                .world_mut()
                .expect("checked above")
                .set_body_transform(body.0, to_boxddd_pos(transform.translation), rotation)
        });

        if let Err(error) = result {
            report_error(
                &settings,
                &mut errors,
                BoxdddErrorMessage {
                    operation: BoxdddOperation::SyncTransform,
                    entity: Some(entity),
                    error,
                },
            );
        }
    }
}

/// Advances the Box3D world by Bevy's fixed timestep.
pub fn step_world(
    mut context: NonSendMut<BoxdddPhysicsContext>,
    settings: Res<BoxdddPhysicsSettings>,
    time: Res<Time<Fixed>>,
) {
    context.pending_step_error = None;
    let Some(world) = context.world_mut() else {
        return;
    };

    match world.step_outcome(time.delta_secs(), settings.sub_step_count) {
        Ok(outcome) => {
            context.last_step_failed = false;
            context.pending_step_error = outcome.into_result().err();
        }
        Err(error) => {
            context.last_step_failed = true;
            context.pending_step_error = Some(error);
        }
    }
}

/// Publishes body, contact, and sensor messages produced by the last advanced step.
pub fn publish_physics_messages(
    context: NonSendMut<BoxdddPhysicsContext>,
    settings: Res<BoxdddPhysicsSettings>,
    mut messages: PhysicsMessageWriters,
) {
    if context.last_step_failed {
        return;
    }

    let Some(world) = context.world() else {
        return;
    };

    match world.body_events() {
        Ok(events) => {
            for event in events {
                messages.body_moves.write(BoxdddBodyMoveMessage {
                    body_id: event.body_id,
                    entity: context.body_entity(event.body_id),
                    transform: event.transform,
                    fell_asleep: event.fell_asleep,
                });
            }
        }
        Err(error) => report_error(
            &settings,
            &mut messages.errors,
            BoxdddErrorMessage {
                operation: BoxdddOperation::ReadEvents,
                entity: None,
                error,
            },
        ),
    }

    match world.contact_events() {
        Ok(events) => {
            for event in events.begin {
                messages.contact_begin.write(BoxdddContactBeginMessage {
                    shape_a: event.shape_a,
                    shape_b: event.shape_b,
                    entity_a: context.shape_entity(event.shape_a),
                    entity_b: context.shape_entity(event.shape_b),
                    contact_id: event.contact_id,
                });
            }

            for event in events.end {
                messages.contact_end.write(BoxdddContactEndMessage {
                    shape_a: event.shape_a,
                    shape_b: event.shape_b,
                    entity_a: context.shape_entity(event.shape_a),
                    entity_b: context.shape_entity(event.shape_b),
                    contact_id: event.contact_id,
                });
            }

            for event in events.hit {
                messages.contact_hit.write(BoxdddContactHitMessage {
                    shape_a: event.shape_a,
                    shape_b: event.shape_b,
                    entity_a: context.shape_entity(event.shape_a),
                    entity_b: context.shape_entity(event.shape_b),
                    contact_id: event.contact_id,
                    point: event.point,
                    normal: event.normal,
                    approach_speed: event.approach_speed,
                    user_material_id_a: event.user_material_id_a,
                    user_material_id_b: event.user_material_id_b,
                });
            }
        }
        Err(error) => report_error(
            &settings,
            &mut messages.errors,
            BoxdddErrorMessage {
                operation: BoxdddOperation::ReadEvents,
                entity: None,
                error,
            },
        ),
    }

    match world.sensor_events() {
        Ok(events) => {
            for event in events.begin {
                messages.sensor_begin.write(BoxdddSensorBeginMessage {
                    sensor_shape: event.sensor_shape,
                    visitor_shape: event.visitor_shape,
                    sensor_entity: context.shape_entity(event.sensor_shape),
                    visitor_entity: context.shape_entity(event.visitor_shape),
                });
            }

            for event in events.end {
                messages.sensor_end.write(BoxdddSensorEndMessage {
                    sensor_shape: event.sensor_shape,
                    visitor_shape: event.visitor_shape,
                    sensor_entity: context.shape_entity(event.sensor_shape),
                    visitor_entity: context.shape_entity(event.visitor_shape),
                });
            }
        }
        Err(error) => report_error(
            &settings,
            &mut messages.errors,
            BoxdddErrorMessage {
                operation: BoxdddOperation::ReadEvents,
                entity: None,
                error,
            },
        ),
    }
}

/// Writes Box3D transforms into Bevy for bodies using [`TransformSyncMode::PhysicsToBevy`].
pub fn sync_boxddd_transforms_to_bevy(
    context: NonSendMut<BoxdddPhysicsContext>,
    settings: Res<BoxdddPhysicsSettings>,
    mut errors: MessageWriter<BoxdddErrorMessage>,
    mut bodies: PhysicsToBevyBodyQuery,
) {
    if context.last_step_failed || context.world().is_none() {
        return;
    }

    for (entity, body, mut transform, sync_mode, rigid_body) in &mut bodies {
        if effective_sync_mode(sync_mode, rigid_body) != TransformSyncMode::PhysicsToBevy {
            continue;
        }

        let result = context
            .world()
            .expect("checked above")
            .body_transform(body.0);

        match result {
            Ok(boxddd_transform) => apply_boxddd_transform(&mut transform, boxddd_transform),
            Err(error) => report_error(
                &settings,
                &mut errors,
                BoxdddErrorMessage {
                    operation: BoxdddOperation::SyncTransform,
                    entity: Some(entity),
                    error,
                },
            ),
        }
    }
}

/// Reports the last step error after advanced state has been published and synchronized.
pub fn report_step_error(
    mut context: NonSendMut<BoxdddPhysicsContext>,
    settings: Res<BoxdddPhysicsSettings>,
    mut errors: MessageWriter<BoxdddErrorMessage>,
) {
    let Some(error) = context.pending_step_error.take() else {
        return;
    };
    report_error(
        &settings,
        &mut errors,
        BoxdddErrorMessage {
            operation: BoxdddOperation::StepWorld,
            entity: None,
            error,
        },
    );
}

fn create_shape(
    world: &mut boxddd::World,
    body_id: BodyId,
    collider: Collider,
    local_transform: ShapeLocalTransform,
    shape_def: &ShapeDef,
) -> boxddd::Result<ShapeId> {
    if collider.requires_static_body() && world.body_type(body_id)? != BodyType::Static {
        return Err(invalid_combination("collider.body_type"));
    }

    match collider {
        Collider::Cuboid { half_extents } => {
            let hull = if local_transform == ShapeLocalTransform::IDENTITY {
                BoxHull::new(half_extents.x, half_extents.y, half_extents.z)?
            } else {
                BoxHull::transformed(
                    half_extents.x,
                    half_extents.y,
                    half_extents.z,
                    to_boxddd_local_transform(local_transform)?,
                )?
            };
            world.create_hull_shape(body_id, shape_def, &hull)
        }
        Collider::Sphere { radius, center } => world.create_sphere_shape(
            body_id,
            shape_def,
            &BoxdddSphere::new(to_boxddd_vec3(center + local_transform.translation), radius),
        ),
        Collider::Capsule {
            point1,
            point2,
            radius,
        } => {
            let point1 = transform_local_point(local_transform, point1);
            let point2 = transform_local_point(local_transform, point2);
            world.create_capsule_shape(
                body_id,
                shape_def,
                &BoxdddCapsule::new(to_boxddd_vec3(point1), to_boxddd_vec3(point2), radius),
            )
        }
        Collider::MeshBox {
            center,
            extent,
            scale,
            identify_edges,
        } => world.create_mesh_shape(
            body_id,
            shape_def,
            MeshData::box_mesh(
                to_boxddd_vec3(center),
                to_boxddd_vec3(extent),
                identify_edges,
            )?,
            to_boxddd_vec3(scale),
        ),
        Collider::MeshGrid {
            x_count,
            z_count,
            cell_width,
            material_count,
            scale,
            identify_edges,
        } => world.create_mesh_shape(
            body_id,
            shape_def,
            MeshData::grid_mesh(x_count, z_count, cell_width, material_count, identify_edges)?,
            to_boxddd_vec3(scale),
        ),
        Collider::HeightFieldGrid {
            row_count,
            column_count,
            scale,
            make_holes,
        } => world.create_height_field_shape(
            body_id,
            shape_def,
            HeightField::grid(row_count, column_count, to_boxddd_vec3(scale), make_holes)?,
        ),
        Collider::CompoundSphere {
            center,
            radius,
            material,
        } => world.create_compound_shape(
            body_id,
            shape_def,
            Compound::single_sphere(BoxdddSphere::new(to_boxddd_vec3(center), radius), material)?,
        ),
        Collider::CreatedHull { hull } => world.create_transformed_hull_shape(
            body_id,
            shape_def,
            &create_hull(hull)?,
            to_boxddd_local_transform(local_transform)?,
            boxddd::Vec3::new(1.0, 1.0, 1.0),
        ),
        Collider::TransformedHull {
            hull,
            translation,
            rotation,
            scale,
        } => world.create_transformed_hull_shape(
            body_id,
            shape_def,
            &create_hull(hull)?,
            to_boxddd_local_transform(ShapeLocalTransform {
                translation: transform_local_point(local_transform, translation),
                rotation: local_transform.rotation * rotation,
            })?,
            to_boxddd_vec3(scale),
        ),
    }
}

fn create_hull(hull: crate::components::HullDescriptor) -> boxddd::Result<Hull> {
    match hull {
        crate::components::HullDescriptor::Rock { radius } => Hull::rock(radius),
        crate::components::HullDescriptor::Cylinder {
            height,
            radius,
            y_offset,
            sides,
        } => Hull::cylinder(height, radius, y_offset, sides),
    }
}

fn apply_control_result(
    settings: &BoxdddPhysicsSettings,
    errors: &mut MessageWriter<'_, BoxdddErrorMessage>,
    entity: Entity,
    result: boxddd::Result<()>,
) {
    if let Err(error) = result {
        report_error(
            settings,
            errors,
            BoxdddErrorMessage {
                operation: BoxdddOperation::ApplyBodyControl,
                entity: Some(entity),
                error,
            },
        );
    }
}

fn apply_body_settings_to_world(
    world: &mut boxddd::World,
    body_id: BodyId,
    settings: BodySettings,
) -> boxddd::Result<()> {
    settings.validate()?;
    world.set_body_gravity_scale(body_id, settings.gravity_scale)?;
    world.set_body_linear_damping(body_id, settings.linear_damping)?;
    world.set_body_angular_damping(body_id, settings.angular_damping)?;
    world.enable_body_sleep(body_id, settings.sleep_enabled)?;
    world.set_body_bullet(body_id, settings.bullet)?;
    world.set_body_motion_locks(body_id, settings.motion_locks)
}

fn create_joint(
    world: &mut boxddd::World,
    body_a: BodyId,
    body_b: BodyId,
    joint: Joint,
) -> boxddd::Result<JointId> {
    joint.validate()?;

    match joint {
        Joint::Distance { length } => {
            world.create_distance_joint(DistanceJointDef::new(body_a, body_b).length(length))
        }
        Joint::Revolute => world.create_revolute_joint(RevoluteJointDef::new(body_a, body_b)),
        Joint::Spherical => world.create_spherical_joint(SphericalJointDef::new(body_a, body_b)),
        Joint::Weld => world.create_weld_joint(WeldJointDef::new(body_a, body_b)),
        Joint::Prismatic => world.create_prismatic_joint(PrismaticJointDef::new(body_a, body_b)),
        Joint::Wheel => world.create_wheel_joint(WheelJointDef::new(body_a, body_b)),
    }
}

fn effective_sync_mode(
    mode: Option<&TransformSyncMode>,
    rigid_body: Option<&RigidBody>,
) -> TransformSyncMode {
    mode.copied().unwrap_or(match rigid_body.copied() {
        Some(RigidBody::Static | RigidBody::Kinematic) => TransformSyncMode::BevyToPhysics,
        Some(RigidBody::Dynamic) | None => TransformSyncMode::PhysicsToBevy,
    })
}

fn to_boxddd_local_transform(value: ShapeLocalTransform) -> boxddd::Result<BoxdddTransform> {
    Ok(BoxdddTransform::new(
        to_boxddd_vec3(value.translation),
        to_boxddd_quat(value.rotation)?,
    ))
}

fn transform_local_point(transform: ShapeLocalTransform, point: Vec3) -> Vec3 {
    transform.translation + transform.rotation * point
}

fn invalid_combination(context: &'static str) -> boxddd::Error {
    boxddd::Error::InvalidValue {
        context,
        reason: boxddd::error::InvalidValueReason::InvalidCombination,
    }
}

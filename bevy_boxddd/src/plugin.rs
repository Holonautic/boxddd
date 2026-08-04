//! Bevy plugin wiring for `boxddd` fixed-step physics.

use crate::debug_draw::{
    BoxdddDebugDrawFrame, BoxdddDebugDrawSettings, collect_debug_draw_commands,
};
use crate::messages::{
    BoxdddBodyMoveMessage, BoxdddContactBeginMessage, BoxdddContactEndMessage,
    BoxdddContactHitMessage, BoxdddErrorMessage, BoxdddOperation, BoxdddSensorBeginMessage,
    BoxdddSensorEndMessage,
};
use crate::resources::{BoxdddErrorPolicy, BoxdddPhysicsContext, BoxdddPhysicsSettings};
use crate::systems::{
    apply_body_controls, apply_body_settings, cleanup_removed_bodies, cleanup_removed_colliders,
    cleanup_removed_joints, create_missing_bodies, create_missing_joints, create_missing_shapes,
    publish_physics_messages, report_step_error, step_world, sync_bevy_transforms_to_boxddd,
    sync_boxddd_transforms_to_bevy,
};
use bevy_app::{App, FixedUpdate, Plugin};
use bevy_ecs::schedule::{ApplyDeferred, IntoScheduleConfigs};
use bevy_time::{Fixed, Time};
use boxddd::{Foundation, FoundationConfig};

/// Plugin that owns the Box3D world and registers fixed-step physics systems.
#[derive(Clone, Debug)]
pub struct BoxdddPhysicsPlugin {
    foundation_config: FoundationConfig,
    settings: BoxdddPhysicsSettings,
}

impl BoxdddPhysicsPlugin {
    /// Creates the plugin with explicit process-wide Foundation configuration and default
    /// per-App physics settings.
    pub fn new(foundation_config: FoundationConfig) -> Self {
        Self {
            foundation_config,
            settings: BoxdddPhysicsSettings::default(),
        }
    }

    /// Replaces the per-App physics settings.
    #[must_use]
    pub fn with_settings(mut self, settings: BoxdddPhysicsSettings) -> Self {
        self.settings = settings;
        self
    }

    /// Returns the immutable Foundation configuration this plugin requests.
    pub const fn foundation_config(&self) -> FoundationConfig {
        self.foundation_config
    }
}

impl Plugin for BoxdddPhysicsPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<BoxdddErrorMessage>()
            .add_message::<BoxdddBodyMoveMessage>()
            .add_message::<BoxdddContactBeginMessage>()
            .add_message::<BoxdddContactEndMessage>()
            .add_message::<BoxdddContactHitMessage>()
            .add_message::<BoxdddSensorBeginMessage>()
            .add_message::<BoxdddSensorEndMessage>();

        app.insert_resource(self.settings.clone());
        app.init_resource::<BoxdddDebugDrawSettings>()
            .init_resource::<BoxdddDebugDrawFrame>();

        if let Some(seconds) = self.settings.fixed_timestep_seconds {
            if seconds.is_finite() && seconds > 0.0 {
                app.insert_resource(Time::<Fixed>::from_seconds(seconds));
            } else {
                let message = BoxdddErrorMessage {
                    operation: BoxdddOperation::ConfigureFixedTimestep,
                    entity: None,
                    error: invalid_fixed_timestep_error(seconds),
                };
                report_startup_error(app, self.settings.error_policy, message);
                app.insert_resource(Time::<Fixed>::default());
            }
        }

        let context = match Foundation::initialize(self.foundation_config) {
            Ok(foundation) => match BoxdddPhysicsContext::new(foundation, &self.settings) {
                Ok(context) => context,
                Err(error) => {
                    let message = BoxdddErrorMessage {
                        operation: BoxdddOperation::CreateWorld,
                        entity: None,
                        error,
                    };
                    report_startup_error(app, self.settings.error_policy, message);
                    BoxdddPhysicsContext::disabled()
                }
            },
            Err(error) => {
                let message = BoxdddErrorMessage {
                    operation: BoxdddOperation::InitializeFoundation,
                    entity: None,
                    error,
                };
                report_startup_error(app, self.settings.error_policy, message);
                BoxdddPhysicsContext::disabled()
            }
        };

        app.insert_non_send(context);

        app.add_systems(
            FixedUpdate,
            (
                cleanup_removed_joints,
                cleanup_removed_colliders,
                cleanup_removed_bodies,
                create_missing_bodies,
                ApplyDeferred,
                apply_body_settings,
                create_missing_shapes,
                create_missing_joints,
                apply_body_controls,
                sync_bevy_transforms_to_boxddd,
                step_world,
                collect_debug_draw_commands,
                publish_physics_messages,
                sync_boxddd_transforms_to_bevy,
                report_step_error,
            )
                .chain(),
        );
    }
}

fn report_startup_error(app: &mut App, policy: BoxdddErrorPolicy, message: BoxdddErrorMessage) {
    if policy == BoxdddErrorPolicy::MessageAndLog {
        log::error!("{message:?}");
    }
    app.world_mut().write_message(message);
}

fn invalid_fixed_timestep_error(seconds: f64) -> boxddd::Error {
    let reason = if seconds.is_finite() {
        boxddd::error::InvalidValueReason::OutOfRange
    } else {
        boxddd::error::InvalidValueReason::NonFinite
    };
    boxddd::Error::InvalidValue {
        context: "physics.fixed_timestep_seconds",
        reason,
    }
}

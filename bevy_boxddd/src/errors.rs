//! Internal error reporting helpers for plugin systems.

use crate::messages::BoxdddErrorMessage;
use crate::resources::{BoxdddErrorPolicy, BoxdddPhysicsSettings};
use bevy_ecs::message::MessageWriter;

pub(crate) fn report_error(
    settings: &BoxdddPhysicsSettings,
    writer: &mut MessageWriter<'_, BoxdddErrorMessage>,
    message: BoxdddErrorMessage,
) {
    if settings.error_policy == BoxdddErrorPolicy::MessageAndLog {
        log::error!("{message:?}");
    }
    writer.write(message);
}

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use syn::parse::Parser;
use syn::visit::Visit;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum ExceptionKind {
    FoundationInitialization,
    CallerScopedCarrier,
    RetainedOwnerCleanup,
    AttachmentOrStorageCleanup,
    NativeCallback,
}

impl ExceptionKind {
    fn activity_label(self) -> &'static str {
        match self {
            Self::FoundationInitialization => "foundation-bootstrap",
            Self::CallerScopedCarrier => "caller-scoped",
            Self::RetainedOwnerCleanup => "retained-owner-cleanup",
            Self::AttachmentOrStorageCleanup => "attachment-or-storage-cleanup",
            Self::NativeCallback => "owner-native-callback",
        }
    }

    fn callback_label(self) -> &'static str {
        match self {
            Self::FoundationInitialization => "reject-reentry",
            Self::CallerScopedCarrier => "inherited-frame",
            Self::RetainedOwnerCleanup | Self::AttachmentOrStorageCleanup => "deferred-cleanup",
            Self::NativeCallback => "native-callback",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::FoundationInitialization => "foundation-initialization",
            Self::CallerScopedCarrier => "caller-scoped-carrier",
            Self::RetainedOwnerCleanup => "retained-owner-cleanup",
            Self::AttachmentOrStorageCleanup => "attachment-or-storage-cleanup",
            Self::NativeCallback => "native-callback",
        }
    }
}

struct Exception {
    path: &'static str,
    owners: &'static [&'static str],
    function: &'static str,
    symbols: &'static [&'static str],
    kind: ExceptionKind,
    reason: &'static str,
}

macro_rules! exception {
    ($path:literal, $function:literal, [$($symbol:literal),+ $(,)?], $kind:ident, $reason:literal) => {
        Exception {
            path: $path,
            owners: &["<free>"],
            function: $function,
            symbols: &[$($symbol),+],
            kind: ExceptionKind::$kind,
            reason: $reason,
        }
    };
    ($path:literal, $owner:literal, $function:literal, [$($symbol:literal),+ $(,)?], $kind:ident, $reason:literal) => {
        Exception {
            path: $path,
            owners: &[$owner],
            function: $function,
            symbols: &[$($symbol),+],
            kind: ExceptionKind::$kind,
            reason: $reason,
        }
    };
    ($path:literal, [$($owner:literal),+ $(,)?], $function:literal, [$($symbol:literal),+ $(,)?], $kind:ident, $reason:literal) => {
        Exception {
            path: $path,
            owners: &[$($owner),+],
            function: $function,
            symbols: &[$($symbol),+],
            kind: ExceptionKind::$kind,
            reason: $reason,
        }
    };
}

// Filled only for native-call functions whose activity is deliberately established by their
// caller or retained owner. Ordinary safe entry points must prove admission in their own body.
const EXCEPTIONS: &[Exception] = &[
    exception!(
        "body.rs",
        "PreparedBodyDef",
        "create",
        ["b3CreateBody"],
        CallerScopedCarrier,
        "PreparedBodyDef is consumed only by World::create_body after entering the World call"
    ),
    exception!(
        "body.rs",
        "PreparedBodyDef",
        "invoke",
        ["b3DefaultBodyDef"],
        CallerScopedCarrier,
        "definition lowering and its create closure execute in the same admitted World call"
    ),
    exception!(
        "callbacks.rs",
        "WorldCallbacks",
        "install_raw_callbacks",
        [
            "b3World_SetCustomFilterCallback",
            "b3World_SetPreSolveCallback",
            "boxddd_provider_install_default_pre_solve",
        ],
        CallerScopedCarrier,
        "World bootstrap calls this while ordinary activity and the World-slot guard are live"
    ),
    exception!(
        "callbacks.rs",
        "WorldCallbacks",
        "clear_raw_callbacks",
        [
            "b3World_SetCustomFilterCallback",
            "b3World_SetFrictionCallback",
            "b3World_SetPreSolveCallback",
            "b3World_SetRestitutionCallback",
        ],
        RetainedOwnerCleanup,
        "World teardown retains its ordinary lease and owner call frame through callback removal"
    ),
    exception!(
        "core/debug_checks.rs",
        "check_body_valid_raw",
        ["b3Body_IsValid"],
        CallerScopedCarrier,
        "callers enter a World owner frame before native body validation"
    ),
    exception!(
        "core/debug_checks.rs",
        "check_shape_valid_raw",
        ["b3Shape_IsValid"],
        CallerScopedCarrier,
        "callers enter a World owner frame before native shape validation"
    ),
    exception!(
        "core/debug_checks.rs",
        "check_joint_valid_raw",
        ["b3Joint_IsValid"],
        CallerScopedCarrier,
        "callers enter a World owner frame before native joint validation"
    ),
    exception!(
        "core/debug_checks.rs",
        "check_contact_valid_raw",
        ["b3Contact_IsValid"],
        CallerScopedCarrier,
        "raw contact resolution is available only through an admitted WorldRawGuard"
    ),
    exception!(
        "core/foundation.rs",
        "Foundation",
        "initialize",
        [
            "b3GetLengthUnitsPerMeter",
            "b3GetStallThreshold",
            "b3SetLengthUnitsPerMeter",
            "b3SetStallThreshold",
        ],
        FoundationInitialization,
        "Foundation initialization is the pre-admission bootstrap serialized by FOUNDATION_INIT_LOCK"
    ),
    exception!(
        "core/units.rs",
        "restore_foundation_scale",
        ["b3SetLengthUnitsPerMeter"],
        CallerScopedCarrier,
        "the required ReplayLease is already exclusive and callers retain a replay call frame"
    ),
    exception!(
        "core/units.rs",
        "verify_observed",
        ["b3GetLengthUnitsPerMeter"],
        CallerScopedCarrier,
        "the required ReplayLease is already exclusive and callers retain a replay call frame"
    ),
    exception!(
        "debug_draw.rs",
        "take_provider_debug_error",
        ["boxddd_provider_debug_take_error"],
        CallerScopedCarrier,
        "provider error polling completes an admitted World or replay debug call"
    ),
    exception!(
        "debug_draw.rs",
        "snapshot_compound_ptr",
        ["b3GetCompoundChild"],
        NativeCallback,
        "the debug-shape callback executes synchronously inside its owner's active native frame"
    ),
    exception!(
        "debug_draw.rs",
        "with_debug_draw_context",
        ["b3DefaultDebugDraw"],
        CallerScopedCarrier,
        "World and replay wrappers enter their owner call before constructing debug draw state"
    ),
    exception!(
        "dynamic_tree.rs",
        "BoundProxyCreation",
        "verify_postflight",
        ["b3DynamicTree_GetProxyCount"],
        CallerScopedCarrier,
        "the typed proxy transaction cannot escape DynamicTree::create_proxy's owner call"
    ),
    exception!(
        "dynamic_tree.rs",
        "ProxyCreationTransaction as Drop",
        "drop",
        ["b3DynamicTree_DestroyProxy"],
        RetainedOwnerCleanup,
        "the armed proxy transaction drops before its creating owner call frame"
    ),
    exception!(
        "dynamic_tree.rs",
        "proxy_compensation_is_verified",
        ["b3DynamicTree_GetProxyCount"],
        CallerScopedCarrier,
        "proxy compensation verification runs synchronously from the armed transaction Drop"
    ),
    exception!(
        "dynamic_tree.rs",
        "DynamicTreeInner",
        "destroy",
        ["b3DynamicTree_Destroy"],
        RetainedOwnerCleanup,
        "DynamicTreeInner owns the ordinary lease through native destruction"
    ),
    exception!(
        "joints/defs.rs",
        [
            "DistanceJointDef",
            "FilterJointDef",
            "MotorJointDef",
            "ParallelJointDef",
            "PrismaticJointDef",
            "RevoluteJointDef",
            "SphericalJointDef",
            "WeldJointDef",
            "WheelJointDef",
        ],
        "lower",
        [
            "b3DefaultDistanceJointDef",
            "b3DefaultFilterJointDef",
            "b3DefaultMotorJointDef",
            "b3DefaultParallelJointDef",
            "b3DefaultPrismaticJointDef",
            "b3DefaultRevoluteJointDef",
            "b3DefaultSphericalJointDef",
            "b3DefaultWeldJointDef",
            "b3DefaultWheelJointDef",
        ],
        CallerScopedCarrier,
        "joint definitions lower only inside World::create_joint's admitted transaction"
    ),
    exception!(
        "joints/defs.rs",
        [
            "DistanceJointDef as JointDefinition",
            "FilterJointDef as JointDefinition",
            "MotorJointDef as JointDefinition",
            "ParallelJointDef as JointDefinition",
            "PrismaticJointDef as JointDefinition",
            "RevoluteJointDef as JointDefinition",
            "SphericalJointDef as JointDefinition",
            "WeldJointDef as JointDefinition",
            "WheelJointDef as JointDefinition",
        ],
        "create_native",
        [
            "b3CreateDistanceJoint",
            "b3CreateFilterJoint",
            "b3CreateMotorJoint",
            "b3CreateParallelJoint",
            "b3CreatePrismaticJoint",
            "b3CreateRevoluteJoint",
            "b3CreateSphericalJoint",
            "b3CreateWeldJoint",
            "b3CreateWheelJoint",
        ],
        CallerScopedCarrier,
        "the macro-generated carrier is invoked only by admitted World::create_joint before its frame ends"
    ),
    exception!(
        "query.rs",
        "poll_provider_query_bridge_error",
        ["boxddd_provider_query_take_error"],
        CallerScopedCarrier,
        "provider error polling completes the surrounding admitted World query"
    ),
    exception!(
        "recording.rs",
        "Recording",
        "native_bytes",
        ["b3Recording_GetData", "b3Recording_GetSize"],
        CallerScopedCarrier,
        "byte access callers retain transient admission and first prove the buffer detached"
    ),
    exception!(
        "recording.rs",
        "destroy_recording",
        [
            "b3DestroyRecording",
            "b3World_IsValid",
            "b3World_StopRecording",
        ],
        AttachmentOrStorageCleanup,
        "an attachment proves a live ordinary World lease; detached storage destruction is scale-independent"
    ),
    exception!(
        "recording.rs",
        "ReplayTransactionGuard",
        "restore",
        ["b3RecPlayer_Destroy"],
        RetainedOwnerCleanup,
        "ReplayTransactionGuard runs while its exclusive lease, replay call, and slot guard are live"
    ),
    exception!(
        "recording.rs",
        "ReplayTransactionGuard as Drop",
        "drop",
        ["b3RecPlayer_Destroy"],
        RetainedOwnerCleanup,
        "ReplayTransactionGuard retains the exclusive lease through rollback destruction"
    ),
    exception!(
        "recording.rs",
        "destroy_rec_player",
        ["b3RecPlayer_Destroy"],
        RetainedOwnerCleanup,
        "RecPlayerOwner retains exclusivity and this function establishes the teardown frame and slot guard"
    ),
    exception!(
        "recording.rs",
        "World",
        "defer_stop_recording",
        ["b3World_IsValid", "b3World_StopRecording"],
        RetainedOwnerCleanup,
        "the closure is queued on and drained by the surrounding World owner call frame"
    ),
    exception!(
        "recording.rs",
        "stop_recording_owner",
        ["b3World_IsValid", "b3World_StopRecording"],
        CallerScopedCarrier,
        "session finish and World teardown both retain the World ordinary lease and call frame"
    ),
    exception!(
        "shapes.rs",
        "PreparedShapeDef",
        "create_sphere",
        ["b3CreateSphereShape"],
        CallerScopedCarrier,
        "the prepared shape carrier is invoked only inside an admitted World creation transaction"
    ),
    exception!(
        "shapes.rs",
        "PreparedShapeDef",
        "create_box_hull",
        ["b3CreateHullShape"],
        CallerScopedCarrier,
        "the prepared shape carrier is invoked only inside an admitted World creation transaction"
    ),
    exception!(
        "shapes.rs",
        "PreparedShapeDef",
        "create_capsule",
        ["b3CreateCapsuleShape"],
        CallerScopedCarrier,
        "the prepared shape carrier is invoked only inside an admitted World creation transaction"
    ),
    exception!(
        "shapes.rs",
        "PreparedShapeDef",
        "create_hull",
        ["b3CreateHullShape"],
        CallerScopedCarrier,
        "the prepared shape carrier is invoked only inside an admitted World creation transaction"
    ),
    exception!(
        "shapes.rs",
        "PreparedShapeDef",
        "create_transformed_hull",
        ["b3CreateTransformedHullShape"],
        CallerScopedCarrier,
        "the prepared shape carrier is invoked only inside an admitted World creation transaction"
    ),
    exception!(
        "shapes.rs",
        "PreparedShapeDef",
        "create_mesh",
        ["b3CreateMeshShape"],
        CallerScopedCarrier,
        "the prepared shape carrier and retained mesh backing share one admitted World transaction"
    ),
    exception!(
        "shapes.rs",
        "PreparedShapeDef",
        "create_height_field",
        ["b3CreateHeightFieldShape"],
        CallerScopedCarrier,
        "the prepared shape carrier and retained height-field backing share one admitted World transaction"
    ),
    exception!(
        "shapes.rs",
        "PreparedShapeDef",
        "create_compound",
        ["b3CreateBakedCompoundShape"],
        CallerScopedCarrier,
        "the prepared shape carrier and retained compound backing share one admitted World transaction"
    ),
    exception!(
        "shapes.rs",
        "PreparedShapeDef",
        "invoke",
        ["b3DefaultShapeDef"],
        CallerScopedCarrier,
        "shape definition lowering and its create closure share the admitted World transaction"
    ),
    exception!(
        "shapes.rs",
        [
            "CompoundBytesInner",
            "CompoundInner",
            "HeightFieldInner",
            "HullInner",
            "MeshDataInner",
        ],
        "destroy",
        [
            "b3DestroyCompound",
            "b3DestroyHeightField",
            "b3DestroyHull",
            "b3DestroyMesh",
        ],
        RetainedOwnerCleanup,
        "each native geometry inner owns its ordinary lease through exact native destruction"
    ),
    exception!(
        "shapes.rs",
        "compound_query_trampoline",
        ["b3GetCompoundChild"],
        NativeCallback,
        "the query trampoline executes synchronously inside Compound's admitted owner call"
    ),
    exception!(
        "world/creation.rs",
        "World",
        "finish_body_creation",
        ["b3Body_GetType"],
        CallerScopedCarrier,
        "the strict body transaction finishes before World::create_body releases its call frame"
    ),
    exception!(
        "world/creation.rs",
        "finish_shape_creation",
        ["b3Shape_GetType"],
        CallerScopedCarrier,
        "the strict shape transaction finishes before its World call frame can end"
    ),
    exception!(
        "world/creation_transaction.rs",
        [
            "BodyNative as NativeResource",
            "JointNative as NativeResource",
            "ShapeNative as NativeResource",
        ],
        "is_valid",
        ["b3Body_IsValid", "b3Joint_IsValid", "b3Shape_IsValid"],
        CallerScopedCarrier,
        "typed creation transactions are constructed and consumed entirely inside an owner call"
    ),
    exception!(
        "world/creation_transaction.rs",
        [
            "BodyNative as NativeResource",
            "JointNative as NativeResource",
            "ShapeNative as NativeResource",
        ],
        "world",
        ["b3Body_GetWorld", "b3Joint_GetWorld", "b3Shape_GetWorld"],
        CallerScopedCarrier,
        "typed creation transactions are constructed and consumed entirely inside an owner call"
    ),
    exception!(
        "world/creation_transaction.rs",
        [
            "BodyNative as NativeResource",
            "JointNative as NativeResource",
            "ShapeNative as NativeResource",
        ],
        "destroy",
        ["b3DestroyBody", "b3DestroyJoint", "b3DestroyShape"],
        RetainedOwnerCleanup,
        "armed compensation drops before the creating World call frame and backing guard"
    ),
    exception!(
        "world/creation_transaction.rs",
        "observed_shape_parent",
        ["b3Shape_GetBody"],
        CallerScopedCarrier,
        "shape parent postflight runs inside the strict admitted creation transaction"
    ),
    exception!(
        "world/creation_transaction.rs",
        "observed_joint_endpoints",
        ["b3Joint_GetBodyA", "b3Joint_GetBodyB"],
        CallerScopedCarrier,
        "joint endpoint postflight runs inside the strict admitted creation transaction"
    ),
    exception!(
        "world/shape_api.rs",
        "ensure_shape_type",
        ["b3Shape_GetType"],
        CallerScopedCarrier,
        "callers retain the frame returned by enter_shape_call through family validation"
    ),
    exception!(
        "world.rs",
        "WorldDef",
        "create_native",
        [
            "b3DefaultWorldDef",
            "boxddd_provider_debug_install_world_def"
        ],
        CallerScopedCarrier,
        "World bootstrap owns ordinary activity and the slot guard for the complete native create"
    ),
    exception!(
        "world.rs",
        "WorldBootstrapGuard",
        "verify_postflight",
        ["b3GetWorldCount", "b3World_IsValid"],
        CallerScopedCarrier,
        "World bootstrap postflight cannot escape its ordinary activity and slot transaction"
    ),
    exception!(
        "world.rs",
        "WorldBootstrapGuard as Drop",
        "drop",
        ["b3DestroyWorld", "b3GetWorldCount", "b3World_IsValid"],
        RetainedOwnerCleanup,
        "an armed World bootstrap guard retains ordinary activity and the slot guard through rollback"
    ),
    exception!(
        "world.rs",
        "World",
        "check_world_valid",
        ["b3World_IsValid"],
        CallerScopedCarrier,
        "every caller first establishes the World owner frame and Foundation health check"
    ),
    exception!(
        "world.rs",
        "create_world_raw",
        ["b3CreateWorld", "b3CreateWorldDoublePrecision"],
        CallerScopedCarrier,
        "precision-specific native adapters are private to the admitted World bootstrap"
    ),
    exception!(
        "world.rs",
        "shutdown_world",
        ["b3DestroyWorld", "b3GetWorldCount", "b3World_IsValid"],
        RetainedOwnerCleanup,
        "WorldOwner retains ordinary activity while teardown establishes its frame and slot guard"
    ),
];

const GENERATED_JOINT_CREATE_CALLS: &[&str] = &[
    "b3CreateDistanceJoint",
    "b3CreateFilterJoint",
    "b3CreateMotorJoint",
    "b3CreateParallelJoint",
    "b3CreatePrismaticJoint",
    "b3CreateRevoluteJoint",
    "b3CreateSphericalJoint",
    "b3CreateWeldJoint",
    "b3CreateWheelJoint",
];

const DIRECT_WORLD_SLOT_ACQUIRERS: &[(&str, &str, &str)] = &[
    ("recording.rs", "Foundation", "create_replay_player"),
    ("recording.rs", "Foundation", "validate_replay"),
    ("recording.rs", "<free>", "destroy_rec_player"),
    ("world.rs", "World", "create"),
    ("world.rs", "<free>", "shutdown_world"),
];

const RETAINED_ACTIVITY_SLOT_TEARDOWNS: &[(&str, &str, &str)] = &[
    ("recording.rs", "<free>", "destroy_rec_player"),
    ("world.rs", "<free>", "shutdown_world"),
];

const INHERITED_WORLD_SLOT_SCOPES: &[(&str, &str, &str)] = &[
    ("callbacks.rs", "WorldCallbacks", "clear_raw_callbacks"),
    ("callbacks.rs", "WorldCallbacks", "install_raw_callbacks"),
    ("core/units.rs", "<free>", "restore_foundation_scale"),
    ("core/units.rs", "<free>", "verify_observed"),
    ("recording.rs", "ReplayTransactionGuard", "restore"),
    ("recording.rs", "ReplayTransactionGuard as Drop", "drop"),
    ("world.rs", "<free>", "create_world_raw"),
    ("world.rs", "WorldBootstrapGuard", "verify_postflight"),
    ("world.rs", "WorldBootstrapGuard as Drop", "drop"),
    ("world.rs", "WorldDef", "create_native"),
];

const REVIEWED_INVENTORY: &str = include_str!("fixtures/ffi_lifetime_inventory.txt");

#[derive(Debug)]
struct NativeCall {
    order: usize,
    scope: Vec<usize>,
    symbol: String,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum Activity {
    Transient,
    OrdinaryOwner,
    WorldOwner,
    ReplayExclusive,
}

impl Activity {
    fn label(self) -> &'static str {
        match self {
            Self::Transient => "transient",
            Self::OrdinaryOwner => "ordinary-owner",
            Self::WorldOwner => "world-owner",
            Self::ReplayExclusive => "replay-exclusive",
        }
    }
}

#[derive(Debug)]
struct Admission {
    order: usize,
    scope: Vec<usize>,
    boundary: String,
    activity: Activity,
}

#[derive(Debug)]
struct Release {
    order: usize,
    scope: Vec<usize>,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum Exposure {
    Safe,
    UnsafeRaw,
    Internal,
}

impl Exposure {
    fn label(self) -> &'static str {
        match self {
            Self::Safe => "safe",
            Self::UnsafeRaw => "unsafe-raw",
            Self::Internal => "internal",
        }
    }
}

#[derive(Debug)]
struct FunctionAudit {
    path: String,
    owner: String,
    name: String,
    exposure: Exposure,
    native_calls: Vec<NativeCall>,
    admissions: Vec<Admission>,
    releases: Vec<Release>,
    slot_acquires: Vec<Admission>,
    slot_releases: Vec<Release>,
}

struct InventoryVisitor {
    path: String,
    functions: Vec<FunctionAudit>,
    function_stack: Vec<usize>,
    owner_stack: Vec<String>,
    scope_stack: Vec<usize>,
    next_scope: usize,
    next_order: usize,
    module_scope_calls: Vec<String>,
}

impl InventoryVisitor {
    fn new(path: String) -> Self {
        Self {
            path,
            functions: Vec::new(),
            function_stack: Vec::new(),
            owner_stack: Vec::new(),
            scope_stack: Vec::new(),
            next_scope: 0,
            next_order: 0,
            module_scope_calls: Vec::new(),
        }
    }

    fn visit_function(
        &mut self,
        name: &syn::Ident,
        visibility: Option<&syn::Visibility>,
        signature: &syn::Signature,
        attrs: &[syn::Attribute],
        block: &syn::Block,
    ) {
        if is_test_only(attrs) {
            return;
        }

        let index = self.functions.len();
        self.functions.push(FunctionAudit {
            path: self.path.clone(),
            owner: self
                .owner_stack
                .last()
                .cloned()
                .unwrap_or_else(|| "<free>".to_owned()),
            name: name.to_string(),
            exposure: function_exposure(visibility, signature),
            native_calls: Vec::new(),
            admissions: Vec::new(),
            releases: Vec::new(),
            slot_acquires: Vec::new(),
            slot_releases: Vec::new(),
        });
        self.function_stack.push(index);
        let previous_order = self.next_order;
        self.next_order = 0;
        syn::visit::visit_block(self, block);
        self.next_order = previous_order;
        self.function_stack.pop();
    }

    fn record_native_call(&mut self, symbol: impl Into<String>) {
        let symbol = symbol.into();
        if let Some(index) = self.function_stack.last().copied() {
            let order = self.next_order;
            let scope = self.current_scope();
            self.next_order += 1;
            self.functions[index].native_calls.push(NativeCall {
                order,
                scope,
                symbol,
            });
        } else {
            self.module_scope_calls
                .push(format!("{}::{symbol}", self.path));
        }
    }

    fn record_admission(&mut self, boundary: impl Into<String>, activity: Activity) {
        let Some(index) = self.function_stack.last().copied() else {
            return;
        };
        let order = self.next_order;
        let scope = self.current_scope();
        self.next_order += 1;
        self.functions[index].admissions.push(Admission {
            order,
            scope,
            boundary: boundary.into(),
            activity,
        });
    }

    fn record_release(&mut self) {
        let Some(index) = self.function_stack.last().copied() else {
            return;
        };
        let order = self.next_order;
        let scope = self.current_scope();
        self.next_order += 1;
        self.functions[index]
            .releases
            .push(Release { order, scope });
    }

    fn record_slot_acquire(&mut self) {
        let Some(index) = self.function_stack.last().copied() else {
            return;
        };
        let order = self.next_order;
        let scope = self.current_scope();
        self.next_order += 1;
        self.functions[index].slot_acquires.push(Admission {
            order,
            scope,
            boundary: "world_slot_mutation_lock()".to_owned(),
            activity: Activity::WorldOwner,
        });
    }

    fn record_slot_release(&mut self) {
        let Some(index) = self.function_stack.last().copied() else {
            return;
        };
        let order = self.next_order;
        let scope = self.current_scope();
        self.next_order += 1;
        self.functions[index]
            .slot_releases
            .push(Release { order, scope });
    }

    fn current_scope(&self) -> Vec<usize> {
        self.scope_stack.clone()
    }

    fn push_synthetic_scope(&mut self) {
        let scope = self.next_scope;
        self.next_scope += 1;
        self.scope_stack.push(scope);
    }

    fn pop_synthetic_scope(&mut self) {
        self.scope_stack
            .pop()
            .expect("a synthetic inventory scope must be active");
    }

    fn add_generated_joint_calls(&mut self, mac: &syn::Macro) {
        let tokens = lexical_tokens(&mac.tokens.to_string());
        let owner = tokens.first().cloned().unwrap_or_default();
        let (_, paths) = ffi_symbols(&tokens);
        for symbol in paths {
            self.functions.push(FunctionAudit {
                path: self.path.clone(),
                owner: format!("{owner} as JointDefinition"),
                name: "create_native".to_owned(),
                exposure: Exposure::Internal,
                native_calls: vec![NativeCall {
                    order: 0,
                    scope: Vec::new(),
                    symbol,
                }],
                admissions: Vec::new(),
                releases: Vec::new(),
                slot_acquires: Vec::new(),
                slot_releases: Vec::new(),
            });
        }
    }

    fn inspect_macro(&mut self, mac: &syn::Macro, module_scope: bool) {
        let macro_name = mac
            .path
            .segments
            .last()
            .map(|segment| segment.ident.to_string());
        let scoped_admission = macro_name.as_deref() == Some("family_method");
        if scoped_admission {
            self.push_synthetic_scope();
            self.record_admission("family_method!", Activity::WorldOwner);
        }

        let tokens = lexical_tokens(&mac.tokens.to_string());
        let (calls, paths) = ffi_symbols(&tokens);
        for call in calls {
            self.record_native_call(call);
        }
        if !module_scope {
            for path in paths {
                self.record_native_call(path);
            }
        }
        if scoped_admission {
            self.pop_synthetic_scope();
        }
    }
}

impl<'ast> Visit<'ast> for InventoryVisitor {
    fn visit_item_fn(&mut self, node: &'ast syn::ItemFn) {
        self.visit_function(
            &node.sig.ident,
            Some(&node.vis),
            &node.sig,
            &node.attrs,
            &node.block,
        );
    }

    fn visit_impl_item_fn(&mut self, node: &'ast syn::ImplItemFn) {
        self.visit_function(
            &node.sig.ident,
            Some(&node.vis),
            &node.sig,
            &node.attrs,
            &node.block,
        );
    }

    fn visit_trait_item_fn(&mut self, node: &'ast syn::TraitItemFn) {
        if let Some(block) = &node.default {
            self.visit_function(&node.sig.ident, None, &node.sig, &node.attrs, block);
        }
    }

    fn visit_item_mod(&mut self, node: &'ast syn::ItemMod) {
        if !is_test_only(&node.attrs) {
            syn::visit::visit_item_mod(self, node);
        }
    }

    fn visit_item_impl(&mut self, node: &'ast syn::ItemImpl) {
        if !is_test_only(&node.attrs) {
            self.owner_stack.push(impl_identity(node));
            syn::visit::visit_item_impl(self, node);
            self.owner_stack.pop();
        }
    }

    fn visit_item_trait(&mut self, node: &'ast syn::ItemTrait) {
        if !is_test_only(&node.attrs) {
            self.owner_stack.push(format!("trait {}", node.ident));
            syn::visit::visit_item_trait(self, node);
            self.owner_stack.pop();
        }
    }

    fn visit_item_macro(&mut self, node: &'ast syn::ItemMacro) {
        if !is_test_only(&node.attrs) {
            let module_scope = self.function_stack.is_empty();
            if module_scope
                && node
                    .mac
                    .path
                    .segments
                    .last()
                    .is_some_and(|segment| segment.ident == "impl_joint_definition")
            {
                self.add_generated_joint_calls(&node.mac);
            } else {
                self.inspect_macro(&node.mac, module_scope);
            }
        }
    }

    fn visit_block(&mut self, node: &'ast syn::Block) {
        let scope = self.next_scope;
        self.next_scope += 1;
        self.scope_stack.push(scope);
        for statement in &node.stmts {
            self.visit_stmt(statement);
        }
        self.scope_stack.pop();
    }

    fn visit_expr_call(&mut self, node: &'ast syn::ExprCall) {
        let path = expression_path(&node.func);
        let admission = path.and_then(|path| {
            admission_function(
                path,
                &self.path,
                self.owner_stack.last().map(String::as_str),
            )
        });
        if let Some((boundary, activity)) = admission.as_ref()
            && is_closure_scoped_admission(boundary)
            && node.args.iter().any(is_closure_expression)
        {
            self.visit_expr(&node.func);
            for argument in node
                .args
                .iter()
                .filter(|argument| !is_closure_expression(argument))
            {
                self.visit_expr(argument);
            }
            self.push_synthetic_scope();
            self.record_admission(boundary.clone(), *activity);
            for argument in node
                .args
                .iter()
                .filter(|argument| is_closure_expression(argument))
            {
                self.visit_expr(argument);
            }
            self.pop_synthetic_scope();
            return;
        }

        syn::visit::visit_expr_call(self, node);
        if let Some(path) = path {
            if is_guard_release(path, &node.args) {
                self.record_release();
            }
            if is_slot_release(path, &node.args) {
                self.record_slot_release();
            }
            if let Some(symbol) = ffi_function(path) {
                self.record_native_call(symbol);
            }
            if path
                .segments
                .last()
                .is_some_and(|segment| segment.ident == "world_slot_mutation_lock")
            {
                self.record_slot_acquire();
            }
            if let Some((boundary, activity)) = admission {
                self.record_admission(boundary, activity);
            }
        }
    }

    fn visit_expr_method_call(&mut self, node: &'ast syn::ExprMethodCall) {
        let admission = admission_method(
            node,
            &self.path,
            self.owner_stack.last().map(String::as_str),
        );
        if let Some((boundary, activity)) = admission.as_ref()
            && is_closure_scoped_admission(boundary)
            && node.args.iter().any(is_closure_expression)
        {
            self.visit_expr(&node.receiver);
            for argument in node
                .args
                .iter()
                .filter(|argument| !is_closure_expression(argument))
            {
                self.visit_expr(argument);
            }
            self.push_synthetic_scope();
            self.record_admission(boundary.clone(), *activity);
            for argument in node
                .args
                .iter()
                .filter(|argument| is_closure_expression(argument))
            {
                self.visit_expr(argument);
            }
            self.pop_synthetic_scope();
            return;
        }

        syn::visit::visit_expr_method_call(self, node);
        if let Some((boundary, activity)) = admission {
            self.record_admission(boundary, activity);
        }
    }

    fn visit_expr_macro(&mut self, node: &'ast syn::ExprMacro) {
        self.inspect_macro(&node.mac, false);
    }

    fn visit_stmt_macro(&mut self, node: &'ast syn::StmtMacro) {
        self.inspect_macro(&node.mac, false);
    }
}

#[test]
fn every_production_native_call_has_function_level_activity() {
    let (source_root, functions, module_scope_calls) = collect_inventory();

    assert!(
        module_scope_calls.is_empty(),
        "native calls outside an auditable function: {module_scope_calls:#?}"
    );
    assert_eq!(
        functions
            .iter()
            .filter(|function| function.owner.ends_with(" as JointDefinition"))
            .flat_map(|function| function.native_calls.iter())
            .map(|call| call.symbol.clone())
            .collect::<BTreeSet<_>>(),
        GENERATED_JOINT_CREATE_CALLS
            .iter()
            .map(|symbol| (*symbol).to_owned())
            .collect(),
        "macro-generated native function paths changed without review"
    );
    let joint_parent = functions
        .iter()
        .find(|function| {
            function.path == "joints/world_api.rs"
                && function.owner == "World"
                && function.name == "create_joint"
        })
        .expect("World::create_joint must remain the generated joint-create parent");
    assert!(
        !joint_parent.admissions.is_empty(),
        "generated joint creation lost its admitted World parent"
    );
    let joint_source = fs::read_to_string(source_root.join("joints/world_api.rs")).unwrap();
    assert!(
        joint_source.contains("def.create_native(self.raw())"),
        "generated joint creation is no longer called by the reviewed World transaction"
    );

    let mut used_exceptions = BTreeSet::new();
    let mut used_exception_owners = BTreeSet::new();
    let mut used_exception_symbols = BTreeSet::new();
    let mut failures = Vec::new();
    let mut audited_call_count = 0;
    for function in functions
        .iter()
        .filter(|function| !function.native_calls.is_empty())
    {
        audited_call_count += function.native_calls.len();
        let uncovered: Vec<_> = function
            .native_calls
            .iter()
            .filter(|call| !call_has_live_admission(function, call))
            .collect();
        if uncovered.is_empty() {
            continue;
        }

        let symbols: BTreeSet<_> = uncovered.iter().map(|call| call.symbol.as_str()).collect();
        let exception = EXCEPTIONS.iter().enumerate().find(|(_, exception)| {
            exception.path == function.path
                && exception.owners.contains(&function.owner.as_str())
                && exception.function == function.name
                && symbols
                    .iter()
                    .all(|symbol| exception.symbols.contains(symbol))
        });
        if let Some((index, exception)) = exception {
            assert!(
                !exception.reason.trim().is_empty(),
                "{}::{}::{} has an unexplained {:?} exception",
                function.path,
                function.owner,
                function.name,
                exception.kind
            );
            used_exceptions.insert(index);
            used_exception_owners.insert((index, function.owner.clone()));
            for symbol in symbols {
                used_exception_symbols.insert((index, symbol.to_owned()));
            }
            continue;
        }

        failures.push(format!(
            "{}::{}::{} has uncovered calls {:?}; observed boundaries: {:?}",
            function.path,
            function.owner,
            function.name,
            symbols,
            function
                .admissions
                .iter()
                .map(|admission| admission.boundary.as_str())
                .collect::<Vec<_>>()
        ));
    }

    assert!(
        audited_call_count > 600,
        "native inventory unexpectedly shrank"
    );
    assert!(
        failures.is_empty(),
        "unclassified production native calls:\n{}",
        failures.join("\n")
    );
    assert_eq!(
        used_exceptions.len(),
        EXCEPTIONS.len(),
        "an activity exception is stale or too broad"
    );
    for (index, exception) in EXCEPTIONS.iter().enumerate() {
        let observed_owners: BTreeSet<_> = used_exception_owners
            .iter()
            .filter_map(|(used_index, owner)| (*used_index == index).then_some(owner.as_str()))
            .collect();
        let declared_owners: BTreeSet<_> = exception.owners.iter().copied().collect();
        assert_eq!(
            observed_owners, declared_owners,
            "{}::{:?}::{} has a stale owner exception",
            exception.path, exception.owners, exception.function
        );
        let observed: BTreeSet<_> = used_exception_symbols
            .iter()
            .filter_map(|(used_index, symbol)| (*used_index == index).then_some(symbol.as_str()))
            .collect();
        let declared: BTreeSet<_> = exception.symbols.iter().copied().collect();
        assert_eq!(
            observed, declared,
            "{}::{:?}::{} has a stale native-call exception",
            exception.path, exception.owners, exception.function
        );
    }
}

#[test]
fn reviewed_inventory_matches_every_function_and_native_symbol() {
    let (_, functions, module_scope_calls) = collect_inventory();
    assert!(
        module_scope_calls.is_empty(),
        "native calls outside an auditable function: {module_scope_calls:#?}"
    );
    let actual = render_inventory_snapshot(&functions);
    if std::env::var_os("BOXDDD_PRINT_FFI_INVENTORY").is_some() {
        println!("BOXDDD_FFI_INVENTORY_BEGIN\n{actual}BOXDDD_FFI_INVENTORY_END");
        return;
    }
    assert_eq!(
        REVIEWED_INVENTORY.trim_end(),
        actual.trim_end(),
        "the function-level FFI contract changed; review the activity, callback, slot, exposure, admission, and exact symbol columns"
    );
}

fn render_inventory_snapshot(functions: &[FunctionAudit]) -> String {
    let mut grouped: BTreeMap<(&str, &str, &str), Vec<&FunctionAudit>> = BTreeMap::new();
    for function in functions
        .iter()
        .filter(|function| !function.native_calls.is_empty())
    {
        grouped
            .entry((&function.path, &function.owner, &function.name))
            .or_default()
            .push(function);
    }

    let mut snapshot = String::new();
    for ((path, owner, name), definitions) in grouped {
        let mut exposures = BTreeSet::new();
        let mut activities = BTreeSet::new();
        let mut callback_policies = BTreeSet::new();
        let mut admitted_by = BTreeSet::new();
        let mut symbols = BTreeMap::<&str, usize>::new();
        let mut direct_slot = false;

        for function in definitions {
            exposures.insert(function.exposure);
            direct_slot |= !function.slot_acquires.is_empty();
            for call in &function.native_calls {
                *symbols.entry(&call.symbol).or_default() += 1;
                let live: Vec<_> = live_admissions(function, call).collect();
                if live.is_empty() {
                    let candidates: Vec<_> = EXCEPTIONS
                        .iter()
                        .filter(|exception| {
                            exception.path == function.path
                                && exception.owners.contains(&function.owner.as_str())
                                && exception.function == function.name
                                && exception.symbols.contains(&call.symbol.as_str())
                        })
                        .collect();
                    assert_eq!(
                        candidates.len(),
                        1,
                        "{}::{}::{}::{} must have exactly one reviewed exception",
                        function.path,
                        function.owner,
                        function.name,
                        call.symbol
                    );
                    let kind = candidates[0].kind;
                    activities.insert(kind.activity_label());
                    callback_policies.insert(kind.callback_label());
                    admitted_by.insert(kind.label().to_owned());
                } else {
                    callback_policies.insert("reject-reentry");
                    for admission in live {
                        activities.insert(admission.activity.label());
                        admitted_by.insert(admission.boundary.clone());
                    }
                }
            }
        }

        assert_eq!(
            exposures.len(),
            1,
            "{path}::{owner}::{name} has conflicting public exposure across cfg variants"
        );
        let exposure = exposures.first().expect("one exposure").label();
        let activity = activities.into_iter().collect::<Vec<_>>().join(",");
        let callback = callback_policies.into_iter().collect::<Vec<_>>().join(",");
        let admission = admitted_by.into_iter().collect::<Vec<_>>().join(",");
        let symbols = symbols
            .into_iter()
            .map(|(symbol, count)| {
                if count == 1 {
                    symbol.to_owned()
                } else {
                    format!("{symbol}*{count}")
                }
            })
            .collect::<Vec<_>>()
            .join(",");
        let slot_policy = if direct_slot {
            "direct"
        } else if INHERITED_WORLD_SLOT_SCOPES.contains(&(path, owner, name)) {
            "inherited"
        } else {
            "none"
        };
        writeln!(
            snapshot,
            "{path} | {owner} | {name} | exposure={exposure} | activity={activity} | callback={callback} | slot={slot_policy} | admitted-by={admission} | symbols={symbols}",
        )
        .expect("writing a String cannot fail");
    }
    snapshot
}

fn call_has_live_admission(function: &FunctionAudit, call: &NativeCall) -> bool {
    live_admissions(function, call).next().is_some()
}

fn live_admissions<'a>(
    function: &'a FunctionAudit,
    call: &'a NativeCall,
) -> impl Iterator<Item = &'a Admission> {
    function.admissions.iter().filter(|admission| {
        admission.order < call.order
            && scope_contains(&admission.scope, &call.scope)
            && !function.releases.iter().any(|release| {
                admission.order < release.order
                    && release.order < call.order
                    && scope_contains(&admission.scope, &release.scope)
            })
    })
}

fn scope_contains(parent: &[usize], child: &[usize]) -> bool {
    child.starts_with(parent)
}

#[test]
fn world_slot_mutation_is_narrow_ordered_and_exact() {
    let (_, functions, _) = collect_inventory();
    let observed: BTreeSet<_> = functions
        .iter()
        .filter(|function| !function.slot_acquires.is_empty())
        .map(function_key)
        .collect();
    let expected: BTreeSet<_> = DIRECT_WORLD_SLOT_ACQUIRERS
        .iter()
        .map(|(path, owner, function)| {
            (
                (*path).to_owned(),
                (*owner).to_owned(),
                (*function).to_owned(),
            )
        })
        .collect();
    assert_eq!(
        observed, expected,
        "direct World-slot mutation boundaries changed without review"
    );

    let retained_teardowns: BTreeSet<_> = RETAINED_ACTIVITY_SLOT_TEARDOWNS
        .iter()
        .map(|(path, owner, function)| {
            (
                (*path).to_owned(),
                (*owner).to_owned(),
                (*function).to_owned(),
            )
        })
        .collect();
    for (path, owner, name) in INHERITED_WORLD_SLOT_SCOPES {
        let function = functions
            .iter()
            .find(|function| {
                function.path == *path && function.owner == *owner && function.name == *name
            })
            .unwrap_or_else(|| panic!("{path}::{owner}::{name} must remain in the slot inventory"));
        assert!(
            function.slot_acquires.is_empty(),
            "{path}::{owner}::{name} unexpectedly changed from inherited to direct slot admission"
        );
        assert!(
            !function.native_calls.is_empty(),
            "{path}::{owner}::{name} no longer carries a native slot transaction"
        );
    }
    for function in functions
        .iter()
        .filter(|function| !function.slot_acquires.is_empty())
    {
        assert_eq!(
            function.slot_acquires.len(),
            1,
            "{}::{}::{} must acquire the World-slot mutex exactly once",
            function.path,
            function.owner,
            function.name
        );
        let slot = &function.slot_acquires[0];
        let key = function_key(function);
        if !retained_teardowns.contains(&key) {
            assert!(
                function.admissions.iter().any(|admission| {
                    admission.order < slot.order
                        && scope_contains(&admission.scope, &slot.scope)
                        && !function.releases.iter().any(|release| {
                            admission.order < release.order
                                && release.order < slot.order
                                && scope_contains(&admission.scope, &release.scope)
                        })
                }),
                "{}::{}::{} must acquire Foundation activity before the World-slot mutex",
                function.path,
                function.owner,
                function.name
            );
        }

        for call in &function.native_calls {
            assert!(
                slot.order < call.order
                    && scope_contains(&slot.scope, &call.scope)
                    && !function.slot_releases.iter().any(|release| {
                        slot.order < release.order
                            && release.order < call.order
                            && scope_contains(&slot.scope, &release.scope)
                    }),
                "{}::{}::{} calls {} outside its World-slot mutation guard",
                function.path,
                function.owner,
                function.name,
                call.symbol
            );
        }
    }
}

fn function_key(function: &FunctionAudit) -> (String, String, String) {
    (
        function.path.clone(),
        function.owner.clone(),
        function.name.clone(),
    )
}

#[test]
fn scanner_is_fail_closed_for_scopes_paths_evaluation_order_and_cfg() {
    let functions = audit_source_snippet(
        "recording.rs",
        r#"
            struct RecPlayer;

            impl RecPlayer {
                pub fn scoped_wrapper(&self) {
                    self.with_native(|raw| unsafe {
                        boxddd_sys::ffi::b3InsideWrapper(raw)
                    });
                    unsafe { b3AfterWrapper() };
                }
            }

            #[cfg(not(test))]
            fn production_cfg() {
                unsafe { crate::ffi::b3QualifiedPath() };
            }

            #[cfg(any(test, feature = "provider"))]
            fn mixed_cfg() {
                unsafe { ffi::boxddd_provider_mixed_cfg() };
            }

            #[cfg(test)]
            fn excluded_test_only() {
                unsafe { ffi::b3ExcludedTestOnly() };
            }

            fn argument_evaluation() {
                Foundation::enter_transient_call(unsafe { ffi::b3BeforeAdmission() });
                unsafe { ffi::b3AfterAdmission() };
            }
        "#,
    );

    let symbols: BTreeSet<_> = functions
        .iter()
        .flat_map(|function| function.native_calls.iter())
        .map(|call| call.symbol.as_str())
        .collect();
    assert_eq!(
        symbols,
        BTreeSet::from([
            "b3AfterAdmission",
            "b3AfterWrapper",
            "b3BeforeAdmission",
            "b3InsideWrapper",
            "b3QualifiedPath",
            "boxddd_provider_mixed_cfg",
        ])
    );

    let scoped = functions
        .iter()
        .find(|function| function.name == "scoped_wrapper")
        .unwrap();
    assert!(
        call_has_live_admission(scoped, native_call(scoped, "b3InsideWrapper")),
        "the reviewed wrapper must admit only its closure body"
    );
    assert!(
        !call_has_live_admission(scoped, native_call(scoped, "b3AfterWrapper")),
        "closure-scoped admission must not bleed into later calls"
    );

    let evaluation = functions
        .iter()
        .find(|function| function.name == "argument_evaluation")
        .unwrap();
    assert!(
        !call_has_live_admission(evaluation, native_call(evaluation, "b3BeforeAdmission")),
        "call arguments execute before the admission function returns"
    );
    assert!(call_has_live_admission(
        evaluation,
        native_call(evaluation, "b3AfterAdmission")
    ));
}

fn audit_source_snippet(path: &str, source: &str) -> Vec<FunctionAudit> {
    let syntax = syn::parse_file(source).expect("inventory regression source must parse");
    let mut visitor = InventoryVisitor::new(path.to_owned());
    visitor.visit_file(&syntax);
    assert!(visitor.module_scope_calls.is_empty());
    visitor.functions
}

fn native_call<'a>(function: &'a FunctionAudit, symbol: &str) -> &'a NativeCall {
    function
        .native_calls
        .iter()
        .find(|call| call.symbol == symbol)
        .unwrap_or_else(|| panic!("{symbol} must be present in the synthetic inventory"))
}

#[test]
fn only_narrow_foundation_mutexes_and_exclusive_raw_borrowing_remain() {
    let source_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut source_files = Vec::new();
    collect_rust_sources(&source_root, &mut source_files);

    let mut all_source = String::new();
    for path in source_files {
        all_source.push_str(&fs::read_to_string(path).expect("crate source should be readable"));
    }
    assert!(!all_source.contains("box3d_lock"));
    assert!(!all_source.contains("BOX3D_GLOBAL_LOCK"));
    assert!(!all_source.contains("MutexGuard<'static, ()>"));

    let foundation = fs::read_to_string(source_root.join("core/foundation.rs")).unwrap();
    assert_eq!(
        foundation.matches("static FOUNDATION_INIT_LOCK:").count(),
        1
    );
    assert_eq!(
        foundation
            .matches("static WORLD_SLOT_MUTATION_LOCK:")
            .count(),
        1
    );

    let raw = fs::read_to_string(source_root.join("raw.rs")).unwrap();
    assert!(raw.contains("world: &'a mut World"));
    assert!(raw.contains("_call: callback_state::OwnerCallFrame"));
    assert!(!raw.contains("Mutex"));
}

fn expression_path(expression: &syn::Expr) -> Option<&syn::Path> {
    match expression {
        syn::Expr::Path(path) => Some(&path.path),
        syn::Expr::Paren(paren) => expression_path(&paren.expr),
        _ => None,
    }
}

fn ffi_function(path: &syn::Path) -> Option<String> {
    path.segments
        .last()
        .map(|segment| segment.ident.to_string())
        .filter(|symbol| looks_like_native_function(symbol))
}

fn is_closure_scoped_admission(boundary: &str) -> bool {
    matches!(
        boundary,
        "create_native_owner"
            | "from_native"
            | "overlap"
            | "ray_cast"
            | "shape_cast"
            | "collide"
            | "with_replay_debug_draw"
            | ".from_native()"
            | ".with_native()"
    )
}

fn is_closure_expression(expression: &syn::Expr) -> bool {
    match expression {
        syn::Expr::Closure(_) => true,
        syn::Expr::Group(group) => is_closure_expression(&group.expr),
        syn::Expr::Paren(paren) => is_closure_expression(&paren.expr),
        _ => false,
    }
}

fn admission_function(
    path: &syn::Path,
    file: &str,
    owner: Option<&str>,
) -> Option<(String, Activity)> {
    let symbol = path.segments.last()?.ident.to_string();
    let activity = match symbol.as_str() {
        "enter_transient_call"
            if path
                .segments
                .iter()
                .rev()
                .nth(1)
                .is_some_and(|segment| segment.ident == "Foundation") =>
        {
            Some(Activity::Transient)
        }
        "enter_world_call"
        | "enter_body_call"
        | "enter_shape_call"
        | "enter_joint_call"
        | "enter_typed_joint_call" => Some(Activity::WorldOwner),
        "create_native_owner" if file == "shapes.rs" => Some(Activity::OrdinaryOwner),
        "from_native" => {
            let receiver = path
                .segments
                .iter()
                .rev()
                .nth(1)
                .map(|segment| segment.ident.to_string());
            (matches!(
                receiver.as_deref(),
                Some("Self" | "Hull" | "MeshData" | "HeightField" | "Compound")
            ) && owner.is_some_and(|owner| {
                [
                    "Hull",
                    "MeshData",
                    "MeshDataBuilder",
                    "HeightField",
                    "HeightFieldBuilder",
                    "Compound",
                    "CompoundBuilder",
                ]
                .contains(&owner)
            }))
            .then_some(Activity::OrdinaryOwner)
        }
        "overlap" | "ray_cast" | "shape_cast" | "collide" if file == "collision.rs" => {
            Some(Activity::Transient)
        }
        "with_replay_debug_draw" if matches!(file, "debug_draw.rs" | "recording.rs") => {
            Some(Activity::ReplayExclusive)
        }
        _ => None,
    };
    activity.map(|activity| (symbol, activity))
}

fn admission_method(
    node: &syn::ExprMethodCall,
    file: &str,
    owner: Option<&str>,
) -> Option<(String, Activity)> {
    let method = node.method.to_string();
    let activity = match method.as_str() {
        "acquire_ordinary"
            if owner == Some("Foundation") || expression_mentions_foundation(&node.receiver) =>
        {
            Some(if owner == Some("World") {
                Activity::WorldOwner
            } else {
                Activity::OrdinaryOwner
            })
        }
        "acquire_replay"
            if owner == Some("Foundation") || expression_mentions_foundation(&node.receiver) =>
        {
            Some(Activity::ReplayExclusive)
        }
        "enter_call" => {
            if expression_is_named(&node.receiver, "world")
                || (expression_is_named(&node.receiver, "self") && owner == Some("World"))
            {
                Some(Activity::WorldOwner)
            } else if expression_contains_lease(&node.receiver) {
                if matches!(file, "recording.rs" | "debug_draw.rs") {
                    Some(Activity::ReplayExclusive)
                } else {
                    Some(Activity::OrdinaryOwner)
                }
            } else if expression_is_named(&node.receiver, "self") && owner == Some("DynamicTree") {
                Some(Activity::OrdinaryOwner)
            } else {
                None
            }
        }
        "enter_world_call" | "enter_body_call" | "enter_shape_call" | "enter_joint_call" => {
            (expression_is_named(&node.receiver, "world")
                || (expression_is_named(&node.receiver, "self") && owner == Some("World")))
            .then_some(Activity::WorldOwner)
        }
        "from_native"
            if matches!(
                owner,
                Some(
                    "Hull"
                        | "MeshData"
                        | "MeshDataBuilder"
                        | "HeightField"
                        | "HeightFieldBuilder"
                        | "Compound"
                        | "CompoundBuilder"
                )
            ) =>
        {
            Some(Activity::OrdinaryOwner)
        }
        "with_native"
            if expression_is_named(&node.receiver, "self") && owner == Some("RecPlayer") =>
        {
            Some(Activity::ReplayExclusive)
        }
        _ => None,
    };
    activity.map(|activity| (format!(".{method}()"), activity))
}

fn function_exposure(visibility: Option<&syn::Visibility>, signature: &syn::Signature) -> Exposure {
    if matches!(visibility, Some(syn::Visibility::Public(_))) {
        if signature.unsafety.is_some() {
            Exposure::UnsafeRaw
        } else {
            Exposure::Safe
        }
    } else {
        Exposure::Internal
    }
}

fn expression_is_named(expression: &syn::Expr, expected: &str) -> bool {
    matches!(
        expression,
        syn::Expr::Path(path)
            if path.path.segments.len() == 1 && path.path.segments[0].ident == expected
    )
}

fn expression_contains_lease(expression: &syn::Expr) -> bool {
    match expression {
        syn::Expr::Path(path) => path.path.segments.last().is_some_and(|segment| {
            matches!(
                segment.ident.to_string().as_str(),
                "lease" | "foundation_lease" | "_foundation_lease"
            )
        }),
        syn::Expr::Field(field) => {
            let member = match &field.member {
                syn::Member::Named(member) => member.to_string(),
                syn::Member::Unnamed(_) => String::new(),
            };
            matches!(
                member.as_str(),
                "lease" | "foundation_lease" | "_foundation_lease"
            ) || expression_contains_lease(&field.base)
        }
        syn::Expr::MethodCall(call) => expression_contains_lease(&call.receiver),
        syn::Expr::Paren(paren) => expression_contains_lease(&paren.expr),
        syn::Expr::Reference(reference) => expression_contains_lease(&reference.expr),
        _ => false,
    }
}

fn expression_mentions_foundation(expression: &syn::Expr) -> bool {
    match expression {
        syn::Expr::Path(path) => path.path.segments.iter().any(|segment| {
            matches!(
                segment.ident.to_string().as_str(),
                "Foundation" | "foundation"
            )
        }),
        syn::Expr::MethodCall(call) => expression_mentions_foundation(&call.receiver),
        syn::Expr::Try(try_expression) => expression_mentions_foundation(&try_expression.expr),
        syn::Expr::Call(call) => expression_mentions_foundation(&call.func),
        syn::Expr::Paren(paren) => expression_mentions_foundation(&paren.expr),
        _ => false,
    }
}

fn is_guard_release(
    path: &syn::Path,
    arguments: &syn::punctuated::Punctuated<syn::Expr, syn::Token![,]>,
) -> bool {
    dropped_binding_name(path, arguments).is_some_and(|name| {
        name == "_call" || name.contains("call_frame") || name.ends_with("_lease")
    })
}

fn is_slot_release(
    path: &syn::Path,
    arguments: &syn::punctuated::Punctuated<syn::Expr, syn::Token![,]>,
) -> bool {
    dropped_binding_name(path, arguments)
        .is_some_and(|name| matches!(name.as_str(), "_slot_guard" | "slot_guard"))
}

fn dropped_binding_name(
    path: &syn::Path,
    arguments: &syn::punctuated::Punctuated<syn::Expr, syn::Token![,]>,
) -> Option<String> {
    if !path.is_ident("drop") || arguments.len() != 1 {
        return None;
    }
    let syn::Expr::Path(argument) = arguments.first()? else {
        return None;
    };
    let segment = argument.path.segments.last()?;
    Some(segment.ident.to_string())
}

fn impl_identity(item: &syn::ItemImpl) -> String {
    let owner = type_identity(&item.self_ty);
    if let Some((_, trait_path, _)) = &item.trait_ {
        let trait_name = trait_path
            .segments
            .last()
            .map(|segment| segment.ident.to_string())
            .unwrap_or_else(|| "<trait>".to_owned());
        format!("{owner} as {trait_name}")
    } else {
        owner
    }
}

fn type_identity(ty: &syn::Type) -> String {
    match ty {
        syn::Type::Path(path) => path
            .path
            .segments
            .last()
            .map(|segment| segment.ident.to_string())
            .unwrap_or_else(|| "<type>".to_owned()),
        syn::Type::Reference(reference) => type_identity(&reference.elem),
        syn::Type::Paren(paren) => type_identity(&paren.elem),
        syn::Type::Group(group) => type_identity(&group.elem),
        _ => "<complex-type>".to_owned(),
    }
}

fn is_test_only(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|attr| {
        if attr.path().is_ident("test") {
            return true;
        }
        if !attr.path().is_ident("cfg") {
            return false;
        }
        let syn::Meta::List(list) = &attr.meta else {
            return false;
        };
        parse_meta_list(list).is_some_and(|metas| metas.iter().any(cfg_requires_test))
    })
}

fn cfg_requires_test(meta: &syn::Meta) -> bool {
    match meta {
        syn::Meta::Path(path) => path.is_ident("test"),
        syn::Meta::NameValue(_) => false,
        syn::Meta::List(list) if list.path.is_ident("all") => {
            parse_meta_list(list).is_some_and(|metas| metas.iter().any(cfg_requires_test))
        }
        syn::Meta::List(list) if list.path.is_ident("any") => parse_meta_list(list)
            .is_some_and(|metas| !metas.is_empty() && metas.iter().all(cfg_requires_test)),
        // Conservatively include `not(...)` and unknown predicates in the production inventory.
        syn::Meta::List(_) => false,
    }
}

fn parse_meta_list(
    list: &syn::MetaList,
) -> Option<syn::punctuated::Punctuated<syn::Meta, syn::Token![,]>> {
    syn::punctuated::Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated
        .parse2(list.tokens.clone())
        .ok()
}

fn lexical_tokens(source: &str) -> Vec<String> {
    let bytes = source.as_bytes();
    let mut tokens = Vec::new();
    let mut cursor = 0;
    while cursor < bytes.len() {
        if bytes[cursor].is_ascii_alphabetic() || bytes[cursor] == b'_' {
            let start = cursor;
            cursor += 1;
            while cursor < bytes.len()
                && (bytes[cursor].is_ascii_alphanumeric() || bytes[cursor] == b'_')
            {
                cursor += 1;
            }
            tokens.push(source[start..cursor].to_owned());
        } else if matches!(bytes[cursor], b':' | b'(' | b'!' | b',' | b'<' | b'>') {
            tokens.push(char::from(bytes[cursor]).to_string());
            cursor += 1;
        } else {
            cursor += 1;
        }
    }
    tokens
}

fn ffi_symbols(tokens: &[String]) -> (Vec<String>, Vec<String>) {
    let mut calls = Vec::new();
    let mut paths = Vec::new();
    for (index, symbol) in tokens.iter().enumerate() {
        if !looks_like_native_function(symbol) {
            continue;
        }
        if tokens.get(index + 1).is_some_and(|token| token == "(") {
            calls.push(symbol.clone());
        } else if symbol.starts_with("b3Create") && symbol.ends_with("Joint") {
            paths.push(symbol.clone());
        }
    }
    (calls, paths)
}

fn looks_like_native_function(symbol: &str) -> bool {
    symbol.starts_with("b3") || symbol.starts_with("boxddd_provider_")
}

fn collect_inventory() -> (PathBuf, Vec<FunctionAudit>, Vec<String>) {
    let source_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut source_files = Vec::new();
    collect_rust_sources(&source_root, &mut source_files);
    source_files.sort();

    let mut functions = Vec::new();
    let mut module_scope_calls = Vec::new();
    for path in source_files {
        let source = fs::read_to_string(&path).expect("crate source should be readable");
        let syntax = syn::parse_file(&source)
            .unwrap_or_else(|error| panic!("failed to parse {}: {error}", path.display()));
        let relative = path
            .strip_prefix(&source_root)
            .expect("source file should be below crate source root")
            .to_string_lossy()
            .replace('\\', "/");
        let mut visitor = InventoryVisitor::new(relative);
        visitor.visit_file(&syntax);
        functions.extend(visitor.functions);
        module_scope_calls.extend(visitor.module_scope_calls);
    }
    (source_root, functions, module_scope_calls)
}

fn collect_rust_sources(directory: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(directory).expect("crate source directory should be readable") {
        let path = entry
            .expect("source directory entry should be readable")
            .path();
        if path.is_dir() {
            collect_rust_sources(&path, out);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            out.push(path);
        }
    }
}

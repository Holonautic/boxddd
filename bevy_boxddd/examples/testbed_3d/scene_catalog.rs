#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum TestbedScene {
    FallingStack,
    AdvancedColliders,
    BodyControls,
    ContinuousCollision,
    CharacterMover,
    Materials,
    Joints,
    Contacts,
    RayPicking,
    DebugDraw,
    QueryLab,
    DebugDrawInspector,
    MaterialLab,
    StatsDashboard,
    DominoRun,
    ArchStack,
    WindField,
    RagdollChain,
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct TestbedCamera {
    pub position: [f32; 3],
    pub target: [f32; 3],
}

impl TestbedCamera {
    pub const fn new(position: [f32; 3], target: [f32; 3]) -> Self {
        Self { position, target }
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum ParityMode {
    FaithfulPort,
    TeachingAdaptation,
}

impl ParityMode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::FaithfulPort => "FaithfulPort",
            Self::TeachingAdaptation => "TeachingAdaptation",
        }
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct UpstreamSampleRef {
    pub category: &'static str,
    pub name: &'static str,
    pub mode: ParityMode,
}

#[derive(Copy, Clone)]
pub struct SceneCatalogEntry {
    pub scene: TestbedScene,
    pub id: &'static str,
    pub category: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub upstream: &'static [UpstreamSampleRef],
    pub showcase_lesson: Option<&'static str>,
    pub camera: TestbedCamera,
}

impl SceneCatalogEntry {
    pub const fn source_label(self) -> &'static str {
        if self.showcase_lesson.is_some() {
            "boxddd showcase"
        } else {
            "official Box3D sample"
        }
    }
}

pub const SCENE_CATALOG: [SceneCatalogEntry; 18] = [
    SceneCatalogEntry {
        scene: TestbedScene::FallingStack,
        id: "falling-stack",
        category: "Basics",
        name: "Falling Stack",
        description: "Box, sphere, capsule, and cylinder stacks falling onto a static floor.",
        upstream: &[
            UpstreamSampleRef {
                category: "Stacking",
                name: "Single Box",
                mode: ParityMode::FaithfulPort,
            },
            UpstreamSampleRef {
                category: "Stacking",
                name: "Box Stack",
                mode: ParityMode::TeachingAdaptation,
            },
            UpstreamSampleRef {
                category: "Stacking",
                name: "Pyramid2D",
                mode: ParityMode::TeachingAdaptation,
            },
            UpstreamSampleRef {
                category: "Stacking",
                name: "Sphere Stack",
                mode: ParityMode::TeachingAdaptation,
            },
            UpstreamSampleRef {
                category: "Stacking",
                name: "Capsule Stack",
                mode: ParityMode::TeachingAdaptation,
            },
            UpstreamSampleRef {
                category: "Stacking",
                name: "Cylinder",
                mode: ParityMode::TeachingAdaptation,
            },
            UpstreamSampleRef {
                category: "Stacking",
                name: "Cylinder Stack",
                mode: ParityMode::TeachingAdaptation,
            },
        ],
        showcase_lesson: None,
        camera: TestbedCamera::new([-8.2, 5.7, 9.6], [0.0, 1.5, 0.0]),
    },
    SceneCatalogEntry {
        scene: TestbedScene::AdvancedColliders,
        id: "advanced-colliders",
        category: "Colliders",
        name: "Advanced Colliders",
        description: "Mesh, height-field, compound, sphere, and hull colliders.",
        upstream: &[
            UpstreamSampleRef {
                category: "Compound",
                name: "Simple",
                mode: ParityMode::TeachingAdaptation,
            },
            UpstreamSampleRef {
                category: "Compound",
                name: "Mesh Tile",
                mode: ParityMode::TeachingAdaptation,
            },
            UpstreamSampleRef {
                category: "Mesh",
                name: "Grid",
                mode: ParityMode::TeachingAdaptation,
            },
            UpstreamSampleRef {
                category: "Mesh",
                name: "Height Field",
                mode: ParityMode::TeachingAdaptation,
            },
            UpstreamSampleRef {
                category: "Geometry",
                name: "Hull",
                mode: ParityMode::TeachingAdaptation,
            },
        ],
        showcase_lesson: None,
        camera: TestbedCamera::new([-7.0, 5.0, 9.0], [1.0, 1.2, 0.0]),
    },
    SceneCatalogEntry {
        scene: TestbedScene::BodyControls,
        id: "body-controls",
        category: "Bodies",
        name: "Body Controls",
        description: "Body settings, force, impulse, kinematic motion, and gravity scale.",
        upstream: &[
            UpstreamSampleRef {
                category: "Bodies",
                name: "Body Type",
                mode: ParityMode::TeachingAdaptation,
            },
            UpstreamSampleRef {
                category: "Bodies",
                name: "Disable",
                mode: ParityMode::TeachingAdaptation,
            },
            UpstreamSampleRef {
                category: "Bodies",
                name: "Kinematic",
                mode: ParityMode::TeachingAdaptation,
            },
            UpstreamSampleRef {
                category: "Bodies",
                name: "Fixed Rotation",
                mode: ParityMode::TeachingAdaptation,
            },
            UpstreamSampleRef {
                category: "Joints",
                name: "Motion Locks",
                mode: ParityMode::TeachingAdaptation,
            },
        ],
        showcase_lesson: None,
        camera: TestbedCamera::new([-7.0, 5.0, 9.0], [0.0, 1.2, 0.0]),
    },
    SceneCatalogEntry {
        scene: TestbedScene::ContinuousCollision,
        id: "continuous-collision",
        category: "Collision",
        name: "Continuous Collision",
        description: "Bullet-style fast bodies colliding with thin obstacles.",
        upstream: &[
            UpstreamSampleRef {
                category: "Continuous",
                name: "Thin Wall",
                mode: ParityMode::TeachingAdaptation,
            },
            UpstreamSampleRef {
                category: "Continuous",
                name: "Bullet vs Stack",
                mode: ParityMode::TeachingAdaptation,
            },
            UpstreamSampleRef {
                category: "Collision",
                name: "Time of Impact",
                mode: ParityMode::TeachingAdaptation,
            },
        ],
        showcase_lesson: None,
        camera: TestbedCamera::new([-7.0, 5.0, 9.0], [0.0, 1.2, 0.0]),
    },
    SceneCatalogEntry {
        scene: TestbedScene::CharacterMover,
        id: "character-mover",
        category: "Character",
        name: "Character Mover",
        description: "Capsule mover casts and obstacle probes.",
        upstream: &[
            UpstreamSampleRef {
                category: "Character",
                name: "Mover",
                mode: ParityMode::TeachingAdaptation,
            },
            UpstreamSampleRef {
                category: "Character",
                name: "Rigid Body",
                mode: ParityMode::TeachingAdaptation,
            },
        ],
        showcase_lesson: None,
        camera: TestbedCamera::new([-7.0, 5.0, 9.0], [0.0, 1.2, 0.0]),
    },
    SceneCatalogEntry {
        scene: TestbedScene::Materials,
        id: "materials",
        category: "Materials",
        name: "Materials",
        description: "Friction and restitution variants shown with dynamic shapes.",
        upstream: &[
            UpstreamSampleRef {
                category: "Shapes",
                name: "Inclined Plane",
                mode: ParityMode::TeachingAdaptation,
            },
            UpstreamSampleRef {
                category: "Shapes",
                name: "Rolling Resistance",
                mode: ParityMode::TeachingAdaptation,
            },
            UpstreamSampleRef {
                category: "Shapes",
                name: "High Resistance",
                mode: ParityMode::TeachingAdaptation,
            },
            UpstreamSampleRef {
                category: "Shapes",
                name: "Restitution",
                mode: ParityMode::TeachingAdaptation,
            },
        ],
        showcase_lesson: None,
        camera: TestbedCamera::new([-7.0, 5.0, 9.0], [0.0, 1.2, 0.0]),
    },
    SceneCatalogEntry {
        scene: TestbedScene::Joints,
        id: "joints",
        category: "Joints",
        name: "Joints",
        description: "Public joint variants authored as Bevy entities.",
        upstream: &[
            UpstreamSampleRef {
                category: "Joints",
                name: "Distance Joint",
                mode: ParityMode::TeachingAdaptation,
            },
            UpstreamSampleRef {
                category: "Joints",
                name: "Prismatic",
                mode: ParityMode::TeachingAdaptation,
            },
            UpstreamSampleRef {
                category: "Joints",
                name: "Spherical",
                mode: ParityMode::TeachingAdaptation,
            },
            UpstreamSampleRef {
                category: "Joints",
                name: "Revolute",
                mode: ParityMode::TeachingAdaptation,
            },
            UpstreamSampleRef {
                category: "Joints",
                name: "Weld",
                mode: ParityMode::TeachingAdaptation,
            },
            UpstreamSampleRef {
                category: "Joints",
                name: "Wheel",
                mode: ParityMode::TeachingAdaptation,
            },
        ],
        showcase_lesson: None,
        camera: TestbedCamera::new([-7.0, 5.0, 9.0], [0.0, 1.2, 0.0]),
    },
    SceneCatalogEntry {
        scene: TestbedScene::Contacts,
        id: "contacts",
        category: "Events",
        name: "Contacts And Sensors",
        description: "Contact and sensor messages emitted from the physics step.",
        upstream: &[
            UpstreamSampleRef {
                category: "Events",
                name: "Hit",
                mode: ParityMode::TeachingAdaptation,
            },
            UpstreamSampleRef {
                category: "Events",
                name: "Persistent Contact",
                mode: ParityMode::TeachingAdaptation,
            },
            UpstreamSampleRef {
                category: "Events",
                name: "Sensor Hits",
                mode: ParityMode::TeachingAdaptation,
            },
        ],
        showcase_lesson: None,
        camera: TestbedCamera::new([-7.0, 5.0, 9.0], [0.0, 1.2, 0.0]),
    },
    SceneCatalogEntry {
        scene: TestbedScene::RayPicking,
        id: "ray-picking",
        category: "Queries",
        name: "Ray Picking",
        description: "Camera rays resolved through Box3D world queries.",
        upstream: &[
            UpstreamSampleRef {
                category: "Collision",
                name: "Ray Curtain",
                mode: ParityMode::TeachingAdaptation,
            },
            UpstreamSampleRef {
                category: "Collision",
                name: "Cast World",
                mode: ParityMode::TeachingAdaptation,
            },
            UpstreamSampleRef {
                category: "Collision",
                name: "Overlap World",
                mode: ParityMode::TeachingAdaptation,
            },
        ],
        showcase_lesson: None,
        camera: TestbedCamera::new([-7.0, 5.0, 9.0], [0.0, 1.2, 0.0]),
    },
    SceneCatalogEntry {
        scene: TestbedScene::DebugDraw,
        id: "debug-draw",
        category: "Debug",
        name: "Debug Draw",
        description: "Native Box3D debug draw commands rendered through Bevy gizmos.",
        upstream: &[
            UpstreamSampleRef {
                category: "Collision",
                name: "Shape Cast Debug",
                mode: ParityMode::TeachingAdaptation,
            },
            UpstreamSampleRef {
                category: "Collision",
                name: "Distance Debug",
                mode: ParityMode::TeachingAdaptation,
            },
        ],
        showcase_lesson: None,
        camera: TestbedCamera::new([-7.0, 5.0, 9.0], [0.0, 1.2, 0.0]),
    },
    SceneCatalogEntry {
        scene: TestbedScene::QueryLab,
        id: "query-lab",
        category: "Showcase",
        name: "Query Lab",
        description: "Editor-style picking bodies driven by Box3D world queries.",
        upstream: &[],
        showcase_lesson: Some("Use Box3D query results as the authority for Bevy tool selection."),
        camera: TestbedCamera::new([-7.0, 5.0, 9.0], [0.0, 1.2, 0.0]),
    },
    SceneCatalogEntry {
        scene: TestbedScene::DebugDrawInspector,
        id: "debug-draw-inspector",
        category: "Showcase",
        name: "Debug Draw Inspector",
        description: "Debug draw frame assets rendered through the Bevy testbed overlay.",
        upstream: &[],
        showcase_lesson: Some("Inspect persistent debug assets without borrowing native memory."),
        camera: TestbedCamera::new([-7.0, 5.0, 9.0], [0.0, 1.2, 0.0]),
    },
    SceneCatalogEntry {
        scene: TestbedScene::MaterialLab,
        id: "material-lab",
        category: "Showcase",
        name: "Material Lab",
        description: "Friction and restitution variants arranged for side-by-side comparison.",
        upstream: &[],
        showcase_lesson: Some(
            "Compare material coefficients in a Bevy scene before building custom tooling.",
        ),
        camera: TestbedCamera::new([-7.0, 5.0, 9.0], [0.0, 1.2, 0.0]),
    },
    SceneCatalogEntry {
        scene: TestbedScene::StatsDashboard,
        id: "stats-dashboard",
        category: "Showcase",
        name: "Stats Dashboard",
        description: "World counters, awake body counts, and profile timings surfaced in egui.",
        upstream: &[],
        showcase_lesson: Some(
            "Surface Box3D counters and per-step profile snapshots as ordinary Bevy diagnostics.",
        ),
        camera: TestbedCamera::new([-7.4, 5.2, 8.8], [0.0, 1.4, 0.0]),
    },
    SceneCatalogEntry {
        scene: TestbedScene::DominoRun,
        id: "domino-run",
        category: "Stacking",
        name: "Domino Run",
        description: "A curved line of dynamic dominoes started by a moving sphere.",
        upstream: &[
            UpstreamSampleRef {
                category: "Stacking",
                name: "Dominoes",
                mode: ParityMode::TeachingAdaptation,
            },
            UpstreamSampleRef {
                category: "Stacking",
                name: "Double Domino",
                mode: ParityMode::TeachingAdaptation,
            },
        ],
        showcase_lesson: None,
        camera: TestbedCamera::new([-6.5, 5.3, 8.4], [0.3, 0.9, 0.1]),
    },
    SceneCatalogEntry {
        scene: TestbedScene::ArchStack,
        id: "arch-stack",
        category: "Stacking",
        name: "Arch Stack",
        description: "Dynamic blocks arranged as a simple arch over static pillars.",
        upstream: &[
            UpstreamSampleRef {
                category: "Stacking",
                name: "Arch",
                mode: ParityMode::TeachingAdaptation,
            },
            UpstreamSampleRef {
                category: "Stacking",
                name: "Wedge",
                mode: ParityMode::TeachingAdaptation,
            },
            UpstreamSampleRef {
                category: "Stacking",
                name: "Card House",
                mode: ParityMode::TeachingAdaptation,
            },
        ],
        showcase_lesson: None,
        camera: TestbedCamera::new([-7.2, 5.4, 8.8], [0.0, 1.6, 0.0]),
    },
    SceneCatalogEntry {
        scene: TestbedScene::WindField,
        id: "wind-field",
        category: "Forces",
        name: "Wind Field",
        description: "Continuous external forces push light bodies through obstacles.",
        upstream: &[
            UpstreamSampleRef {
                category: "Shapes",
                name: "Wind",
                mode: ParityMode::TeachingAdaptation,
            },
            UpstreamSampleRef {
                category: "Shapes",
                name: "Wind Drop",
                mode: ParityMode::TeachingAdaptation,
            },
            UpstreamSampleRef {
                category: "Shapes",
                name: "Wind Flap",
                mode: ParityMode::TeachingAdaptation,
            },
        ],
        showcase_lesson: None,
        camera: TestbedCamera::new([-7.0, 4.8, 8.6], [0.3, 1.2, 0.0]),
    },
    SceneCatalogEntry {
        scene: TestbedScene::RagdollChain,
        id: "ragdoll-chain",
        category: "Joints",
        name: "Ragdoll Chain",
        description: "A lightweight joint chain made from capsule bodies.",
        upstream: &[
            UpstreamSampleRef {
                category: "Ragdoll",
                name: "Box",
                mode: ParityMode::TeachingAdaptation,
            },
            UpstreamSampleRef {
                category: "Ragdoll",
                name: "Pile",
                mode: ParityMode::TeachingAdaptation,
            },
            UpstreamSampleRef {
                category: "Ragdoll",
                name: "Incline",
                mode: ParityMode::TeachingAdaptation,
            },
            UpstreamSampleRef {
                category: "Joints",
                name: "Ball and Chain",
                mode: ParityMode::TeachingAdaptation,
            },
        ],
        showcase_lesson: None,
        camera: TestbedCamera::new([-6.4, 5.6, 8.0], [0.0, 1.9, 0.0]),
    },
];

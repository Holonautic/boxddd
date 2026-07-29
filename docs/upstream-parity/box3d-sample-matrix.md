# Box3D Official Sample Parity Matrix

This matrix maps the official samples registered by Box3D commit `781673be801569a94be7942848755fc0baad3653` to `boxddd` and `bevy_boxddd` teaching material.
It is a case-level parity map, not a promise to port every C++ sample host feature line-for-line.

The source of truth is the generated inventory in `docs/upstream-parity/box3d-sample-inventory.json`.
The inventory is derived from the pinned upstream commit, so parity checking does not depend on retaining the upstream sample host in the vendored runtime tree.
Run the parity check whenever the upstream manifest, generated inventory, or matrix changes:

```bash
cargo run -p xtask -- sample-parity --check
```

## Parity Modes

- **FaithfulPort**: the Rust example or Bevy scene closely follows the official sample's user-visible behavior.
- **TeachingAdaptation**: the Rust or Bevy artifact teaches the same Box3D concept with idiomatic `boxddd` structure.
- **TestOnly**: the case is better proven through a focused test, core example, or deterministic smoke than a visual scene.
- **Deferred**: the case is intentionally not part of the current public teaching surface; the note records the trigger for future work.
- **UpstreamReference**: the case is tied to the official sample host or renderer and remains reference material.

## Category Summary

| Category | Cases | Current Rust strategy |
|---|---:|---|
| Benchmark | 19 | Deferred until a Rust benchmark harness has explicit measurement goals. |
| Bodies | 10 | Common controls are covered; measured gyroscopic precession remains deferred. |
| Character | 4 | Covered through mover examples and the Bevy character mover scene. |
| Collision | 12 | Covered through query/collision examples, tests, and picking scenes. |
| Compound | 6 | Covered through compound, mesh, height-field, and advanced collider examples. |
| Continuous | 9 | Covered for the common bullet/TOI path; mesh stress cases stay test/deferred. |
| Determinism | 4 | Covered at contract level through recording/replay plus query and collision tests; exact upstream scenario hashes are not claimed. |
| Events | 6 | Covered through event examples, Bevy messages, and focused tests. |
| Geometry | 5 | Covered through collision geometry tests and collider examples. |
| Issues | 10 | Deferred until a corresponding wrapper regression is reproduced. |
| Joints | 16 | Covered through joint gallery/testbed scenes and joint runtime tests. |
| Manifold | 9 | Covered through headless manifold collision tests. |
| Mesh | 9 | Covered through mesh/height-field examples; renderer viewer and creation benchmark are deferred/reference. |
| Ragdoll | 5 | Covered as ragdoll-chain teaching adaptation except pose editing and mesh rig details. |
| Replay | 1 | Covered through recording/replay example and tests, not the upstream ImGui viewer. |
| Robustness | 4 | Deferred until a wrapper regression or robustness target appears. |
| Shapes | 12 | Covered through materials, wind, collision, and collider scenes; conveyor authoring remains deferred. |
| Stacking | 14 | Common stacks are covered through falling-stack, domino, and arch scenes; edge-crossing stress remains deferred. |
| Tree | 1 | Covered through dynamic tree example and tests. |
| World | 4 | Deferred until far-origin precision behavior becomes user-facing. |

## Deferred Route Summary

The matrix contains 43 Deferred rows. They are intentional routing decisions, not untriaged backlog.
Use this table when deciding whether a future change should remain deferred, become a benchmark, become a regression test, or become a visual scene.

| Bucket | Deferred rows | Route |
|---|---:|---|
| Benchmark stress scenes | 19 | Convert only when `boxddd/benches` has a measured throughput or allocation goal for the specific scenario. Do not turn these into visual examples unless they teach a user-facing workflow. |
| Upstream issue repros | 10 | Convert to a focused regression test only after the upstream behavior reproduces through safe `boxddd` wrappers. Until then they remain upstream references. |
| Robustness scenes | 4 | Convert to release-risk tests when scale, mass-ratio, recovery, or debug-color behavior exposes a Rust API contract or bug. |
| Conveyor scenes | 2 | Convert to a Bevy material/conveyor showcase after tangent velocity or equivalent conveyor authoring is available safely in `bevy_boxddd`. `material-lab` covers material coefficients today, not conveyor motion. |
| Far-world scenes | 4 | Convert after `boxddd` documents a far-origin precision contract and has a visual or headless assertion worth teaching. |
| Ragdoll pose | 1 | Convert only when interactive pose editing or rig import becomes part of the Bevy teaching surface; `ragdoll-chain` remains the current lightweight joint-chain adaptation. |
| Mesh creation benchmark | 1 | Convert with the benchmark bucket when mesh construction throughput has explicit measurement goals. |
| Body/collision stress references | 2 | Convert gyroscopic precession or edge crossing only when a focused teaching artifact or stable regression assertion exists. |

## Official Case Matrix

| Category | Official sample | Source location | Parity mode | Target | Notes |
|---|---|---|---|---|---|
| Benchmark | Candy Cups | `sample_benchmark.cpp:371` | Deferred | Upstream benchmark reference | Convex stress scene; port when hull benchmark coverage is needed. |
| Benchmark | Chains | `sample_benchmark.cpp:1222` | Deferred | Upstream benchmark reference | Chain stress benchmark; joint teaching coverage lives in Bevy scenes. |
| Benchmark | Convex Pile | `sample_benchmark.cpp:1483` | Deferred | Upstream benchmark reference | Convex-pile stress benchmark; port when hull/contact throughput has a measured target. |
| Benchmark | Destruction | `sample_benchmark.cpp:1416` | Deferred | Upstream benchmark reference | Destruction stress benchmark; port when destruction throughput is measured. |
| Benchmark | Explosion | `sample_benchmark.cpp:488` | Deferred | Upstream benchmark reference | High-force stress case; port when force-field benchmark coverage is needed. |
| Benchmark | Falling Boxes | `sample_benchmark.cpp:293` | Deferred | Upstream benchmark reference | Box throughput benchmark; visible stack scenes cover teaching only. |
| Benchmark | Falling Trees | `sample_benchmark.cpp:730` | Deferred | Upstream benchmark reference | Broad dynamic stress case; port when scenario has a benchmark target. |
| Benchmark | Height Field | `sample_benchmark.cpp:658` | Deferred | Upstream benchmark reference | Height-field throughput benchmark; teaching covered elsewhere. |
| Benchmark | Hull | `sample_benchmark.cpp:1110` | Deferred | Upstream benchmark reference | Hull creation/query benchmark; safe hull API is covered by tests/examples. |
| Benchmark | Joint Grid | `sample_benchmark.cpp:248` | Deferred | Upstream benchmark reference | Joint throughput benchmark; current joint examples are teaching coverage. |
| Benchmark | Junkyard | `sample_benchmark.cpp:1455` | Deferred | Upstream benchmark reference | Large mixed stress scene; not a first-release teaching target. |
| Benchmark | Large Pyramid | `sample_benchmark.cpp:44` | Deferred | Upstream benchmark reference | Stress benchmark; port when `boxddd` has benchmark harness goals. |
| Benchmark | Large World | `sample_benchmark.cpp:1022` | Deferred | Upstream benchmark reference | Second upstream large-world registration; source location disambiguates it. |
| Benchmark | Large World | `sample_benchmark.cpp:203` | Deferred | Upstream benchmark reference | Large-world stress case; port with a precision/performance benchmark. |
| Benchmark | Many Pyramids | `sample_benchmark.cpp:101` | Deferred | Upstream benchmark reference | Stress benchmark; port when multi-island throughput is measured. |
| Benchmark | Rain | `sample_benchmark.cpp:154` | Deferred | Upstream benchmark reference | Stress benchmark; port when spawn-rate performance is measured. |
| Benchmark | Sensor | `sample_benchmark.cpp:963` | Deferred | Upstream benchmark reference | Sensor throughput benchmark; event semantics are covered by examples/tests. |
| Benchmark | Washer | `sample_benchmark.cpp:990` | Deferred | Upstream benchmark reference | Stress scene; port only with a measured benchmark story. |
| Benchmark | Wide Pyramid | `sample_benchmark.cpp:68` | Deferred | Upstream benchmark reference | Stress benchmark; port when pyramid breadth is a measured target. |
| Bodies | Body Type | `sample_bodies.cpp:269` | TeachingAdaptation | `boxddd/examples/body_controls.rs`, `bevy_boxddd/examples/testbed_3d/scenes.rs#body-controls` | Teaches static/dynamic/kinematic body authoring through Rust and Bevy. |
| Bodies | Cast | `sample_bodies.cpp:975` | TestOnly | `boxddd/examples/shape_queries.rs`, `boxddd/tests/world_and_queries.rs` | Body and shape query APIs are covered headlessly. |
| Bodies | Disable | `sample_bodies.cpp:788` | TeachingAdaptation | `boxddd/examples/body_controls.rs`, `bevy_boxddd/examples/testbed_3d/scenes.rs#body-controls` | Safe enable/disable lifecycle is covered by the body-controls example. |
| Bodies | Fixed Rotation | `sample_bodies.cpp:1182` | TeachingAdaptation | `boxddd/examples/body_controls.rs`, `bevy_boxddd/examples/testbed_3d/scenes.rs#body-controls` | Fixed rotation maps to the safe motion-lock API. |
| Bodies | Gyroscopic Precession | `sample_bodies.cpp:578` | Deferred | Upstream body-dynamics reference | Port when precession rate or gyroscopic stability has an explicit Rust teaching or regression target. |
| Bodies | Gyroscopic Torque | `sample_bodies.cpp:370` | TeachingAdaptation | `bevy_boxddd/examples/testbed_3d/scenes.rs#body-controls` | Bevy body-control scene is the current visible angular-torque teaching path. |
| Bodies | Kinematic | `sample_bodies.cpp:1051` | TeachingAdaptation | `boxddd/examples/body_controls.rs`, `bevy_boxddd/examples/testbed_3d/scenes.rs#body-controls` | Kinematic body motion is part of the body-controls teaching path. |
| Bodies | Lock Mixing | `sample_bodies.cpp:1133` | TeachingAdaptation | `boxddd/examples/body_controls.rs`, `bevy_boxddd/examples/testbed_3d/scenes.rs#body-controls` | Motion locks are covered in core and Bevy body-control examples. |
| Bodies | Spinning Book | `sample_bodies.cpp:317` | TeachingAdaptation | `bevy_boxddd/examples/testbed_3d/scenes.rs#body-controls` | Visual body-control scene covers angular motion teaching. |
| Bodies | Weeble | `sample_bodies.cpp:679` | TeachingAdaptation | `bevy_boxddd/examples/testbed_3d/scenes.rs#body-controls` | Body-control scene covers center-of-mass and stability behavior at teaching level. |
| Character | CapsulePlane | `sample_character.cpp:146` | TestOnly | `boxddd/tests/mover_api.rs`, `boxddd/examples/character_mover.rs` | Capsule mover plane behavior is covered through the mover API test/example. |
| Character | Mover | `sample_character.cpp:584` | TeachingAdaptation | `boxddd/examples/character_mover.rs`, `bevy_boxddd/examples/testbed_3d/scenes.rs#character-mover` | Core and Bevy examples teach mover casts and obstacle probes. |
| Character | MoverOverlap | `sample_character.cpp:313` | TestOnly | `boxddd/tests/mover_api.rs`, `boxddd/examples/character_mover.rs` | Mover overlap behavior is covered by focused core mover coverage. |
| Character | Rigid Body | `sample_character.cpp:1667` | TeachingAdaptation | `bevy_boxddd/examples/testbed_3d/scenes.rs#character-mover` | Bevy scene shows rigid-body character-style interaction without a framework promise. |
| Collision | Capsule Cast Ray | `sample_collision.cpp:2798` | TestOnly | `boxddd/examples/advanced_collision.rs`, `boxddd/tests/collision_validation.rs` | Capsule ray casting is covered in core collision diagnostics. |
| Collision | Cast World | `sample_collision.cpp:772` | TestOnly | `boxddd/examples/shape_queries.rs`, `boxddd/tests/world_and_queries.rs` | World casts are covered through safe query APIs. |
| Collision | Distance Debug | `sample_collision.cpp:2048` | TestOnly | `boxddd/examples/advanced_collision.rs`, `boxddd/tests/collision_validation.rs` | Distance diagnostics are covered headlessly. |
| Collision | Initial Overlap | `sample_collision.cpp:1680` | TestOnly | `boxddd/examples/advanced_collision.rs`, `boxddd/tests/collision_validation.rs` | Initial-overlap diagnostics belong in deterministic collision tests. |
| Collision | Long Ray Cast | `sample_collision.cpp:1573` | TestOnly | `boxddd/examples/shape_queries.rs`, `boxddd/tests/world_and_queries.rs` | Long ray behavior is part of query coverage rather than a separate scene. |
| Collision | Mesh Scale | `sample_collision.cpp:888` | TeachingAdaptation | `boxddd/examples/mesh_height_field_query.rs`, `bevy_boxddd/examples/testbed_3d/scenes.rs#advanced-colliders` | Mesh/height-field query and Bevy collider scenes teach the concept. |
| Collision | Overlap World | `sample_collision.cpp:1329` | TestOnly | `boxddd/examples/shape_queries.rs`, `boxddd/tests/world_and_queries.rs` | Overlap queries are covered by query examples and tests. |
| Collision | Ray Curtain | `sample_collision.cpp:118` | TestOnly | `boxddd/examples/shape_queries.rs`, `boxddd/tests/world_and_queries.rs` | Ray casting is covered through query examples and tests. |
| Collision | Shape Cast | `sample_collision.cpp:1144` | TestOnly | `boxddd/examples/advanced_collision.rs`, `boxddd/tests/collision_validation.rs` | Standalone shape-cast coverage is headless and deterministic. |
| Collision | Shape Cast Debug | `sample_collision.cpp:1791` | TestOnly | `boxddd/examples/advanced_collision.rs`, `boxddd/tests/collision_validation.rs` | Debug-style shape-cast data is covered by the core collision example/test. |
| Collision | Shape Distance | `sample_collision.cpp:2496` | TestOnly | `boxddd/examples/advanced_collision.rs`, `boxddd/tests/collision_validation.rs` | Shape distance maps to the standalone collision helper coverage. |
| Collision | Time of Impact | `sample_collision.cpp:2724` | TestOnly | `boxddd/examples/continuous_collision.rs`, `boxddd/tests/collision_validation.rs` | TOI is covered by continuous collision diagnostics. |
| Compound | Hulls | `sample_compound.cpp:243` | TeachingAdaptation | `bevy_boxddd/examples/testbed_3d/scenes.rs#advanced-colliders` | Hull compound behavior is covered as a visual collider adaptation. |
| Compound | Mesh Tile | `sample_compound.cpp:477` | TeachingAdaptation | `boxddd/examples/mesh_height_field_query.rs`, `bevy_boxddd/examples/testbed_3d/scenes.rs#advanced-colliders` | Mesh tile behavior is covered through mesh/height-field examples. |
| Compound | Simple | `sample_compound.cpp:108` | TeachingAdaptation | `boxddd/examples/compound_query.rs`, `bevy_boxddd/examples/testbed_3d/scenes.rs#advanced-colliders` | Compound creation/query is covered in core and Bevy examples. |
| Compound | Spheres | `sample_compound.cpp:169` | TeachingAdaptation | `bevy_boxddd/examples/testbed_3d/scenes.rs#advanced-colliders` | Advanced collider scene includes sphere-backed collider cases. |
| Compound | Tile Floor | `sample_compound.cpp:361` | TeachingAdaptation | `boxddd/examples/compound_query.rs`, `bevy_boxddd/examples/testbed_3d/scenes.rs#advanced-colliders` | Static compound/tile concepts map to the advanced colliders scene. |
| Compound | Village | `sample_compound.cpp:806` | TeachingAdaptation | `bevy_boxddd/examples/testbed_3d/scenes.rs#advanced-colliders` | Large compound village is represented by smaller collider teaching cases. |
| Continuous | Bounce House | `sample_continuous.cpp:145` | TeachingAdaptation | `bevy_boxddd/examples/testbed_3d/scenes.rs#continuous-collision` | Bevy continuous scene teaches repeated fast-body collision. |
| Continuous | Bullet vs Stack | `sample_continuous.cpp:277` | TeachingAdaptation | `bevy_boxddd/examples/testbed_3d/scenes.rs#continuous-collision` | Bullet-vs-stack behavior is a Bevy scene target. |
| Continuous | Hump Mesh | `sample_continuous.cpp:861` | TestOnly | `boxddd/examples/continuous_collision.rs`, `boxddd/tests/collision_validation.rs` | Mesh CCD stress behavior is covered headlessly. |
| Continuous | Is Fast | `sample_continuous.cpp:945` | TestOnly | `boxddd/examples/continuous_collision.rs`, `boxddd/tests/collision_validation.rs` | Fast-body classification belongs in core continuous collision coverage. |
| Continuous | Mesh Drop | `sample_continuous.cpp:748` | TeachingAdaptation | `boxddd/examples/continuous_collision.rs`, `bevy_boxddd/examples/testbed_3d/scenes.rs#advanced-colliders` | Mesh drop is represented by mesh collider plus CCD teaching paths. |
| Continuous | Needle Mesh | `sample_continuous.cpp:388` | TestOnly | `boxddd/examples/continuous_collision.rs`, `boxddd/tests/collision_validation.rs` | Mesh CCD edge case is kept deterministic in core coverage. |
| Continuous | Spinning Stick | `sample_continuous.cpp:187` | TeachingAdaptation | `bevy_boxddd/examples/testbed_3d/scenes.rs#continuous-collision` | Fast angular motion is represented in the continuous collision scene. |
| Continuous | Stall | `sample_continuous.cpp:1029` | TestOnly | `boxddd/tests/collision_validation.rs` | Stall regression coverage is headless, not a visual teaching scene. |
| Continuous | Thin Wall | `sample_continuous.cpp:71` | TeachingAdaptation | `boxddd/examples/continuous_collision.rs`, `bevy_boxddd/examples/testbed_3d/scenes.rs#continuous-collision` | Bullet bodies against thin obstacles are covered in core and Bevy. |
| Determinism | Falling Ragdolls | `sample_determinism.cpp:62` | TestOnly | `boxddd/examples/determinism.rs`, `boxddd/tests/determinism.rs` | Rust teaches deterministic replay instead of cloning the exact ragdoll scene. |
| Determinism | Mesh Drop | `sample_determinism.cpp:266` | TestOnly | `boxddd/tests/determinism.rs`, `boxddd/tests/collision_validation.rs` | Upstream moved the former continuous mesh-drop unit sample into the determinism suite; Rust proves replay determinism and mesh collision validation separately. |
| Determinism | Query Spawn | `sample_determinism.cpp:213` | TestOnly | `boxddd/tests/determinism.rs`, `boxddd/tests/world_and_queries.rs` | Rust covers replay determinism and query correctness separately; the exact combined upstream hash remains reference behavior. |
| Determinism | Wave Pile | `sample_determinism.cpp:115` | TestOnly | `boxddd/tests/determinism.rs` | Rust validates deterministic recording/replay; the exact wave-pile sleep hash remains an upstream scenario. |
| Events | Hit | `sample_events.cpp:246` | TeachingAdaptation | `boxddd/examples/events.rs`, `bevy_boxddd/examples/contact_messages_3d.rs` | Hit events are shown through core and Bevy message examples. |
| Events | Joint | `sample_events.cpp:572` | TestOnly | `boxddd/tests/events_and_sensors.rs`, `boxddd/tests/joint_runtime.rs` | Joint event semantics stay test-focused because generation can be threshold-sensitive. |
| Events | Move | `sample_events.cpp:329` | TestOnly | `boxddd/examples/events.rs`, `boxddd/tests/events_and_sensors.rs` | Body move events are covered by core event tests/examples. |
| Events | Persistent Contact | `sample_events.cpp:671` | TeachingAdaptation | `boxddd/examples/events.rs`, `bevy_boxddd/examples/contact_messages_3d.rs` | Persistent contact teaching is covered through contact message flow. |
| Events | Sensor Hits | `sample_events.cpp:902` | TeachingAdaptation | `boxddd/examples/events.rs`, `bevy_boxddd/examples/contact_messages_3d.rs` | Sensor hit teaching is represented by event examples and Bevy messages. |
| Events | Sensor Visit | `sample_events.cpp:81` | TestOnly | `boxddd/examples/events.rs`, `boxddd/tests/events_and_sensors.rs` | Sensor visit semantics are covered by safe event snapshots/tests. |
| Geometry | Box Hull | `sample_geometry.cpp:136` | TestOnly | `boxddd/tests/shape_geometry_validation.rs`, `boxddd/examples/advanced_collision.rs` | Hull construction is covered by shape geometry validation. |
| Geometry | Capsule Mass | `sample_geometry.cpp:648` | TestOnly | `boxddd/tests/shape_geometry_validation.rs` | Capsule mass is a deterministic shape geometry check. |
| Geometry | Hull | `sample_geometry.cpp:231` | TestOnly | `boxddd/tests/shape_geometry_validation.rs`, `bevy_boxddd/examples/testbed_3d/scenes.rs#advanced-colliders` | Hull geometry is covered by validation and collider scenes. |
| Geometry | Hull Reduction | `sample_geometry.cpp:360` | TestOnly | `boxddd/tests/shape_geometry_validation.rs` | Hull reduction is an algorithmic geometry validation case. |
| Geometry | Hull Transform | `sample_geometry.cpp:488` | TestOnly | `boxddd/tests/shape_geometry_validation.rs`, `boxddd/examples/advanced_collision.rs` | Transformed hull behavior is covered through geometry/collision tests. |
| Issues | Capsule Mesh | `sample_issues.cpp:493` | Deferred | Upstream issue reference | Port when capsule-mesh behavior becomes a wrapper regression. |
| Issues | Convex Jitter | `sample_issues.cpp:343` | Deferred | Upstream issue reference | Port when jitter becomes a `boxddd` regression target. |
| Issues | Crash | `sample_issues.cpp:77` | Deferred | Upstream issue reference | Port when this crash reproduces in `boxddd`. |
| Issues | GMod Wheel Stack | `sample_issues.cpp:1147` | Deferred | Upstream issue reference | Port when stacked wheel-hull stability reproduces as a safe-wrapper regression. |
| Issues | Hull Crash | `sample_issues.cpp:234` | Deferred | Upstream issue reference | Port when this hull crash maps to Rust API behavior. |
| Issues | Multiple Prismatic | `sample_issues.cpp:133` | Deferred | Upstream issue reference | Port when a prismatic wrapper regression appears. |
| Issues | Restitution Overshoot | `sample_issues.cpp:1254` | Deferred | Upstream issue reference | Port when restitution overshoot becomes a wrapper regression with a stable assertion. |
| Issues | Slide Twist Off Center Shape | `sample_issues.cpp:1304` | Deferred | Upstream issue reference | Port when off-center slide/twist behavior becomes a material or inertia regression target. |
| Issues | s&box Ghost Collisions | `sample_issues.cpp:925` | Deferred | Upstream issue reference | Port when the flat-floor character ghost-collision scenario reproduces through safe APIs. |
| Issues | s&box mover | `sample_issues.cpp:421` | Deferred | Upstream issue reference | Port when the mover issue reproduces through safe APIs. |
| Joints | Ball and Chain | `sample_joint.cpp:1602` | TeachingAdaptation | `bevy_boxddd/examples/testbed_3d/scenes.rs#ragdoll-chain`, `boxddd/tests/joints.rs` | Chain-style joint teaching is covered by ragdoll/chain examples. |
| Joints | Bridge | `sample_joint.cpp:1954` | TeachingAdaptation | `bevy_boxddd/examples/testbed_3d/scenes.rs#joints` | Bridge-style connected bodies are represented by joint-gallery teaching. |
| Joints | Distance Joint | `sample_joint.cpp:236` | TeachingAdaptation | `boxddd/examples/joints.rs`, `bevy_boxddd/examples/testbed_3d/scenes.rs#joints` | Distance joints are covered by core and Bevy joint examples. |
| Joints | Door | `sample_joint.cpp:1831` | TeachingAdaptation | `bevy_boxddd/examples/testbed_3d/scenes.rs#joints` | Door-style revolute behavior is represented in joint gallery scenes. |
| Joints | Driving | `sample_joint.cpp:2624` | TeachingAdaptation | `bevy_boxddd/examples/testbed_3d/scenes.rs#joints` | Driving behavior maps to visible joint-gallery interaction. |
| Joints | Filter | `sample_joint.cpp:276` | TestOnly | `boxddd/tests/joints.rs`, `bevy_boxddd/tests/joints.rs` | Filter joint semantics are test-backed. |
| Joints | Gear Lift | `sample_joint.cpp:3100` | TeachingAdaptation | `bevy_boxddd/examples/testbed_3d/scenes.rs#joints` | Gear lift is represented as advanced joint-gallery teaching, not a host clone. |
| Joints | Motion Locks | `sample_joint.cpp:2235` | TeachingAdaptation | `boxddd/examples/body_controls.rs`, `bevy_boxddd/examples/testbed_3d/scenes.rs#body-controls` | Motion locks are covered through body controls. |
| Joints | Motor Joint | `sample_joint.cpp:445` | TeachingAdaptation | `bevy_boxddd/examples/joint_gallery_3d.rs`, `bevy_boxddd/examples/testbed_3d/scenes.rs#joints` | Motor-style joint behavior is covered through Bevy joint gallery. |
| Joints | Parallel Spring | `sample_joint.cpp:1013` | TeachingAdaptation | `bevy_boxddd/examples/joint_gallery_3d.rs`, `boxddd/tests/joint_new_apis.rs` | Parallel spring API is covered by joint examples/tests. |
| Joints | Prismatic | `sample_joint.cpp:718` | TeachingAdaptation | `bevy_boxddd/examples/joint_gallery_3d.rs`, `bevy_boxddd/examples/testbed_3d/scenes.rs#joints` | Prismatic joints are part of the public Bevy joint gallery. |
| Joints | Revolute | `sample_joint.cpp:1191` | TeachingAdaptation | `bevy_boxddd/examples/joint_gallery_3d.rs`, `bevy_boxddd/examples/testbed_3d/scenes.rs#joints` | Revolute joints are covered visually. |
| Joints | Spherical | `sample_joint.cpp:897` | TeachingAdaptation | `bevy_boxddd/examples/joint_gallery_3d.rs`, `bevy_boxddd/examples/testbed_3d/scenes.rs#joints` | Spherical joints are covered in the joint gallery and testbed. |
| Joints | Top Down Friction | `sample_joint.cpp:550` | TeachingAdaptation | `bevy_boxddd/examples/testbed_3d/scenes.rs#joints` | Top-down friction maps to joint/material teaching in the Bevy testbed. |
| Joints | Weld | `sample_joint.cpp:1290` | TeachingAdaptation | `bevy_boxddd/examples/joint_gallery_3d.rs`, `bevy_boxddd/examples/testbed_3d/scenes.rs#joints` | Weld joints are covered visually. |
| Joints | Wheel | `sample_joint.cpp:1536` | TeachingAdaptation | `bevy_boxddd/examples/joint_gallery_3d.rs`, `bevy_boxddd/examples/testbed_3d/scenes.rs#joints` | Wheel joints are covered visually. |
| Manifold | Capsule vs Capsule | `sample_manifold.cpp:562` | TestOnly | `boxddd/tests/manifold_collision.rs` | Manifold outputs are better asserted headlessly. |
| Manifold | Capsule vs Hull | `sample_manifold.cpp:621` | TestOnly | `boxddd/tests/manifold_collision.rs` | Manifold outputs are better asserted headlessly. |
| Manifold | Capsule vs Sphere | `sample_manifold.cpp:430` | TestOnly | `boxddd/tests/manifold_collision.rs` | Manifold outputs are better asserted headlessly. |
| Manifold | Hull vs Hull | `sample_manifold.cpp:810` | TestOnly | `boxddd/tests/manifold_collision.rs` | Manifold outputs are better asserted headlessly. |
| Manifold | Hull vs Sphere | `sample_manifold.cpp:473` | TestOnly | `boxddd/tests/manifold_collision.rs` | Manifold outputs are better asserted headlessly. |
| Manifold | Sphere vs Sphere | `sample_manifold.cpp:392` | TestOnly | `boxddd/tests/manifold_collision.rs` | Manifold outputs are better asserted headlessly. |
| Manifold | Triangle vs Capsule | `sample_manifold.cpp:677` | TestOnly | `boxddd/tests/manifold_collision.rs` | Manifold outputs are better asserted headlessly. |
| Manifold | Triangle vs Hull | `sample_manifold.cpp:926` | TestOnly | `boxddd/tests/manifold_collision.rs` | Manifold outputs are better asserted headlessly. |
| Manifold | Triangle vs Sphere | `sample_manifold.cpp:526` | TestOnly | `boxddd/tests/manifold_collision.rs` | Manifold outputs are better asserted headlessly. |
| Mesh | Big Box | `sample_mesh.cpp:382` | TeachingAdaptation | `bevy_boxddd/examples/testbed_3d/scenes.rs#advanced-colliders` | Large mesh collider behavior is represented by advanced colliders. |
| Mesh | Box | `sample_mesh.cpp:551` | TeachingAdaptation | `bevy_boxddd/examples/testbed_3d/scenes.rs#advanced-colliders` | Box mesh teaching is included in advanced colliders. |
| Mesh | Creation Benchmark | `sample_mesh.cpp:1413` | Deferred | Upstream benchmark reference | Port when mesh creation throughput is measured. |
| Mesh | Grid | `sample_mesh.cpp:208` | TeachingAdaptation | `boxddd/examples/mesh_height_field_query.rs`, `bevy_boxddd/examples/testbed_3d/scenes.rs#advanced-colliders` | Mesh grid behavior is covered by mesh and collider teaching paths. |
| Mesh | Height Field | `sample_mesh.cpp:1003` | TeachingAdaptation | `boxddd/examples/mesh_height_field_query.rs`, `bevy_boxddd/examples/testbed_3d/scenes.rs#advanced-colliders` | Height fields are covered in core and Bevy examples. |
| Mesh | Hollow Box | `sample_mesh.cpp:1623` | TeachingAdaptation | `bevy_boxddd/examples/testbed_3d/scenes.rs#advanced-colliders` | Hollow mesh concepts are represented by advanced collider teaching. |
| Mesh | Reflection | `sample_mesh.cpp:748` | TestOnly | `boxddd/examples/mesh_height_field_query.rs`, `boxddd/tests/shape_resources.rs` | Mesh resource behavior is covered by core examples/tests. |
| Mesh | Viewer | `sample_mesh.cpp:1327` | UpstreamReference | Official renderer viewer | The upstream viewer is host UI; Rust mesh teaching lives in examples. |
| Mesh | Voxel | `sample_mesh.cpp:1532` | TeachingAdaptation | `bevy_boxddd/examples/testbed_3d/scenes.rs#advanced-colliders` | Voxel-style mesh teaching is represented by advanced colliders. |
| Ragdoll | Box | `sample_ragdoll.cpp:80` | TeachingAdaptation | `bevy_boxddd/examples/testbed_3d/scenes.rs#ragdoll-chain` | Ragdoll box behavior maps to a lightweight capsule joint chain. |
| Ragdoll | Incline | `sample_ragdoll.cpp:335` | TeachingAdaptation | `bevy_boxddd/examples/testbed_3d/scenes.rs#ragdoll-chain` | Inclined ragdoll behavior is represented at teaching level. |
| Ragdoll | Mesh | `sample_ragdoll.cpp:206` | TeachingAdaptation | `bevy_boxddd/examples/testbed_3d/scenes.rs#ragdoll-chain` | Mesh rig details are simplified into a visible joint-chain teaching scene. |
| Ragdoll | Pile | `sample_ragdoll.cpp:260` | TeachingAdaptation | `bevy_boxddd/examples/testbed_3d/scenes.rs#ragdoll-chain` | Pile behavior is represented by the ragdoll-chain scene. |
| Ragdoll | Pose | `sample_ragdoll.cpp:461` | Deferred | Upstream ragdoll pose reference | Pose editing and rig import are outside current teaching scope. |
| Replay | Viewer | `sample_replay.cpp:1782` | TestOnly | `boxddd/examples/recording_replay.rs`, `boxddd/tests/recording.rs` | Rust covers deterministic recording/replay, not the upstream ImGui viewer. |
| Robustness | HighMassRatio1 | `sample_robustness.cpp:70` | Deferred | Upstream robustness reference | Port when high-mass-ratio behavior becomes a wrapper regression target. |
| Robustness | Overflow Color Pile | `sample_robustness.cpp:281` | Deferred | Upstream robustness reference | Port when debug color overflow or pile robustness becomes release-relevant. |
| Robustness | Overlap Recovery | `sample_robustness.cpp:241` | Deferred | Upstream robustness reference | Port when overlap recovery exposes a Rust API bug. |
| Robustness | Tiny Pyramid | `sample_robustness.cpp:129` | Deferred | Upstream robustness reference | Port when tiny-scale stability needs release proof. |
| Shapes | Conveyor Belt | `sample_shapes.cpp:486` | Deferred | Upstream conveyor reference | Port when tangent-velocity/conveyor authoring is added to `bevy_boxddd`. |
| Shapes | Conveyor Mesh | `sample_shapes.cpp:681` | Deferred | Upstream conveyor mesh reference | Port with conveyor authoring and mesh-material teaching support. |
| Shapes | High Resistance | `sample_shapes.cpp:149` | TeachingAdaptation | `bevy_boxddd/examples/testbed_3d/scenes.rs#materials` | High friction/resistance maps to the materials scene. |
| Shapes | Inclined Plane | `sample_shapes.cpp:54` | TeachingAdaptation | `bevy_boxddd/examples/testbed_3d/scenes.rs#materials` | Inclined-plane material behavior is represented by material scenes. |
| Shapes | Isotropic Friction | `sample_shapes.cpp:193` | TeachingAdaptation | `bevy_boxddd/examples/testbed_3d/scenes.rs#materials` | Friction behavior is covered by material variants. |
| Shapes | Restitution | `sample_shapes.cpp:337` | TeachingAdaptation | `bevy_boxddd/examples/testbed_3d/scenes.rs#materials` | Restitution variants are visible in the materials scene. |
| Shapes | Rolling Resistance | `sample_shapes.cpp:110` | TeachingAdaptation | `bevy_boxddd/examples/testbed_3d/scenes.rs#materials` | Rolling/friction behavior is taught through material variants. |
| Shapes | Slide Twist | `sample_shapes.cpp:239` | TeachingAdaptation | `bevy_boxddd/examples/testbed_3d/scenes.rs#materials` | Sliding and twisting are represented by Bevy material demonstrations. |
| Shapes | Static Invoke | `sample_shapes.cpp:436` | TestOnly | `boxddd/tests/world_runtime.rs`, `boxddd/examples/body_controls.rs` | Static-body invocation behavior is covered through runtime/body tests. |
| Shapes | Wind | `sample_shapes.cpp:848` | TeachingAdaptation | `bevy_boxddd/examples/testbed_3d/scenes.rs#wind-field` | Wind force field is represented by the Bevy wind scene. |
| Shapes | Wind Drop | `sample_shapes.cpp:909` | TeachingAdaptation | `bevy_boxddd/examples/testbed_3d/scenes.rs#wind-field` | Wind drop maps to the wind-field scene. |
| Shapes | Wind Flap | `sample_shapes.cpp:1025` | TeachingAdaptation | `bevy_boxddd/examples/testbed_3d/scenes.rs#wind-field` | Wind flap is represented by force-field teaching, not cloth/soft-body behavior. |
| Stacking | Arch | `sample_stacking.cpp:758` | TeachingAdaptation | `bevy_boxddd/examples/testbed_3d/scenes.rs#arch-stack` | Arch behavior is directly represented by the arch-stack scene. |
| Stacking | Box Stack | `sample_stacking.cpp:402` | TeachingAdaptation | `bevy_boxddd/examples/falling_stack_3d.rs`, `bevy_boxddd/examples/testbed_3d/scenes.rs#falling-stack` | Box stacks are covered in standalone and testbed Bevy examples. |
| Stacking | Capsule Stack | `sample_stacking.cpp:192` | TeachingAdaptation | `bevy_boxddd/examples/testbed_3d/scenes.rs#falling-stack` | Capsule-style stacks are represented by falling stack teaching. |
| Stacking | Card House | `sample_stacking.cpp:87` | TeachingAdaptation | `bevy_boxddd/examples/testbed_3d/scenes.rs#arch-stack` | Card-house stability maps to the arch/stack teaching scenes. |
| Stacking | Cylinder | `sample_stacking.cpp:288` | TeachingAdaptation | `bevy_boxddd/examples/testbed_3d/scenes.rs#falling-stack` | Cylinder stack behavior is represented by dynamic-shape stack teaching. |
| Stacking | Cylinder Stack | `sample_stacking.cpp:346` | TeachingAdaptation | `bevy_boxddd/examples/testbed_3d/scenes.rs#falling-stack` | Cylinder-stack teaching maps to falling stack variants. |
| Stacking | Dominoes | `sample_stacking.cpp:557` | TeachingAdaptation | `bevy_boxddd/examples/testbed_3d/scenes.rs#domino-run` | Domino behavior is directly represented by the domino-run scene. |
| Stacking | Double Domino | `sample_stacking.cpp:804` | TeachingAdaptation | `bevy_boxddd/examples/testbed_3d/scenes.rs#domino-run` | Double-domino behavior is represented by domino-run teaching. |
| Stacking | Edge Crossing | `sample_stacking.cpp:939` | Deferred | Upstream edge-crossing reference | Edge-edge manifold stress sweep; port as a focused regression when safe hull collision exhibits the issue. |
| Stacking | Jenga Stack | `sample_stacking.cpp:491` | TeachingAdaptation | `bevy_boxddd/examples/testbed_3d/scenes.rs#falling-stack` | Jenga-style instability maps to the falling-stack teaching scene. |
| Stacking | Pyramid2D | `sample_stacking.cpp:849` | TeachingAdaptation | `bevy_boxddd/examples/falling_stack_3d.rs`, `bevy_boxddd/examples/testbed_3d/scenes.rs#falling-stack` | Pyramid stack behavior maps to the falling-stack scene. |
| Stacking | Single Box | `sample_stacking.cpp:237` | FaithfulPort | `bevy_boxddd/examples/falling_stack_3d.rs`, `bevy_boxddd/examples/testbed_3d/scenes.rs#falling-stack` | The simplest official stack case is directly represented by falling box examples. |
| Stacking | Sphere Stack | `sample_stacking.cpp:148` | TeachingAdaptation | `bevy_boxddd/examples/falling_stack_3d.rs`, `bevy_boxddd/examples/testbed_3d/scenes.rs#falling-stack` | Falling stack scenes cover stacked dynamic shapes. |
| Stacking | Wedge | `sample_stacking.cpp:602` | TeachingAdaptation | `bevy_boxddd/examples/testbed_3d/scenes.rs#arch-stack` | Wedge/arch stability is represented by arch-stack teaching. |
| Tree | Benchmark | `sample_tree.cpp:663` | TestOnly | `boxddd/examples/dynamic_tree.rs`, `boxddd/tests/dynamic_tree.rs` | Dynamic tree lifecycle and query behavior are covered headlessly. |
| World | Far Mesh Drop | `sample_world.cpp:308` | Deferred | Upstream far-world reference | Port when far-origin mesh precision becomes user-facing. |
| World | Far Pyramid | `sample_world.cpp:182` | Deferred | Upstream far-world reference | Port when far-origin precision becomes user-facing. |
| World | Far Ragdolls | `sample_world.cpp:245` | Deferred | Upstream far-world reference | Port when far-origin ragdoll precision becomes user-facing. |
| World | Far Stack | `sample_world.cpp:122` | Deferred | Upstream far-world reference | Port when far-origin precision becomes user-facing. |

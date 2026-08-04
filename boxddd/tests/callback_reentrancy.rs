use boxddd::{
    Aabb, BodyType, Compound, CompoundBytes, DebugDraw, DistanceJointDef, DynamicTree,
    DynamicTreeCastControl, DynamicTreeFilter, Error, Foundation, HeightField, HexColor, Hull,
    MeshData, QueryFilter, RayCastInput, Recording, Sphere, SurfaceMaterial, Vec3, World,
    WorldTransform, allocated_byte_count,
};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::process::{Child, Command};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

const CASE_ENV: &str = "BOXDDD_CALLBACK_REENTRANCY_CASE";

fn wait_with_watchdog(mut child: Child, case: &str) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(status) = child
            .try_wait()
            .expect("callback test child should be waitable")
        {
            assert!(status.success(), "callback subprocess case {case} failed");
            return;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("callback subprocess case {case} exceeded the watchdog");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn run_case(case: &str) {
    let child = Command::new(std::env::current_exe().expect("current test executable"))
        .args([
            "--exact",
            "callback_reentrancy_subprocess_case",
            "--nocapture",
        ])
        .env(CASE_ENV, case)
        .spawn()
        .expect("callback subprocess should start");
    wait_with_watchdog(child, case);
}

#[test]
fn callback_reentrancy_contract() {
    for case in [
        "reentry_rejected",
        "drop_dynamic_tree",
        "drop_captured_dynamic_tree",
        "capture_before_cleanup",
        "drop_world",
        "drop_world_raw_parts",
        "drop_shape_owners",
        "cleanup_then_panic",
        "recording_stop_pending",
        "retain_without_frame",
        "debug_draw_drop_world",
        "replace_callback_capture",
        "clear_callback_capture",
        "drop_world_callback_capture",
        "panicking_callback_capture",
        "panicking_world_callback_capture",
        "cleanup_during_capture_unwind",
        "strict_entry_order",
    ] {
        run_case(case);
    }
}

struct CallbackCaptureDropProbe {
    drops: Arc<AtomicUsize>,
    panic_after_reentry: bool,
}

impl CallbackCaptureDropProbe {
    fn retain(&self) {}
}

impl Drop for CallbackCaptureDropProbe {
    fn drop(&mut self) {
        boxddd::version().expect("callback capture must be released after the native call frame");
        self.drops.fetch_add(1, Ordering::Release);
        assert!(!self.panic_after_reentry, "injected callback capture panic");
    }
}

fn query_bounds() -> Aabb {
    Aabb {
        lower_bound: Vec3::new(-2.0, -2.0, -2.0),
        upper_bound: Vec3::new(2.0, 2.0, 2.0),
    }
}

fn query_world(foundation: &'static Foundation) -> World {
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
    world
        .create_sphere_shape(body, &foundation.shape_def(), &Sphere::new(Vec3::ZERO, 0.5))
        .unwrap();
    world
}

fn populated_tree() -> DynamicTree {
    let mut tree = DynamicTree::new().unwrap();
    tree.create_proxy(query_bounds(), 7).unwrap();
    tree
}

fn assert_native_owner_was_released(before: i32) {
    let after = allocated_byte_count().unwrap();
    assert!(
        after < before,
        "deferred owner did not release native bytes: before={before}, after={after}"
    );
}

#[test]
fn callback_reentrancy_subprocess_case() {
    let Ok(case) = std::env::var(CASE_ENV) else {
        return;
    };
    let foundation = Foundation::initialize_default().unwrap();

    match case.as_str() {
        "reentry_rejected" => {
            let world = query_world(foundation);
            world
                .visit_overlap_aabb(query_bounds(), QueryFilter::default(), |_| {
                    assert_eq!(boxddd::version().unwrap_err(), Error::InCallback);
                    true
                })
                .unwrap();
        }
        "drop_dynamic_tree" => {
            let outer = populated_tree();
            let mut victim = Some(populated_tree());
            let before = allocated_byte_count().unwrap();
            outer
                .visit_query(query_bounds(), DynamicTreeFilter::default(), |_| {
                    drop(victim.take());
                    true
                })
                .unwrap();
            assert!(victim.is_none());
            assert_native_owner_was_released(before);
        }
        "drop_captured_dynamic_tree" => {
            let outer = populated_tree();
            let victim = populated_tree();
            let before = allocated_byte_count().unwrap();
            outer
                .visit_ray_cast(
                    RayCastInput::new(Vec3::new(-4.0, -4.0, -4.0), Vec3::new(8.0, 8.0, 8.0))
                        .unwrap(),
                    DynamicTreeFilter::default(),
                    move |_| {
                        let _keep_alive_until_visitor_drop = &victim;
                        DynamicTreeCastControl::Terminate
                    },
                )
                .unwrap();
            assert_native_owner_was_released(before);
        }
        "capture_before_cleanup" => {
            struct PendingCleanupProbe {
                expected_bytes: i32,
            }

            impl Drop for PendingCleanupProbe {
                fn drop(&mut self) {
                    assert_eq!(
                        allocated_byte_count().unwrap(),
                        self.expected_bytes,
                        "outer owner cleanup ran before the visitor capture was released"
                    );
                }
            }

            struct VisitorCapture {
                victim: Option<DynamicTree>,
                _probe: PendingCleanupProbe,
            }

            impl VisitorCapture {
                fn take_victim(&mut self) -> Option<DynamicTree> {
                    self.victim.take()
                }
            }

            let outer = populated_tree();
            let victim = populated_tree();
            let before = allocated_byte_count().unwrap();
            let mut capture = VisitorCapture {
                victim: Some(victim),
                _probe: PendingCleanupProbe {
                    expected_bytes: before,
                },
            };
            outer
                .visit_query(query_bounds(), DynamicTreeFilter::default(), move |_| {
                    assert_eq!(boxddd::version().unwrap_err(), Error::InCallback);
                    drop(capture.take_victim());
                    false
                })
                .unwrap();
            assert_native_owner_was_released(before);
        }
        "drop_world" => {
            let outer = query_world(foundation);
            let mut victim = Some(query_world(foundation));
            let before = allocated_byte_count().unwrap();
            outer
                .visit_overlap_aabb(query_bounds(), QueryFilter::default(), |_| {
                    drop(victim.take());
                    true
                })
                .unwrap();
            assert!(victim.is_none());
            assert_native_owner_was_released(before);
        }
        "drop_world_raw_parts" => {
            let outer = query_world(foundation);
            let mut victim = Some(query_world(foundation).into_raw_parts().unwrap());
            let before = allocated_byte_count().unwrap();
            outer
                .visit_overlap_aabb(query_bounds(), QueryFilter::default(), |_| {
                    drop(victim.take());
                    true
                })
                .unwrap();
            assert!(victim.is_none());
            assert_native_owner_was_released(before);
        }
        "drop_shape_owners" => {
            let outer = query_world(foundation);
            let bytes: CompoundBytes =
                Compound::single_sphere(Sphere::new(Vec3::ZERO, 0.25), SurfaceMaterial::default())
                    .unwrap()
                    .into_bytes()
                    .unwrap();
            let mut victims = Some((
                Hull::rock(0.5).unwrap(),
                MeshData::box_mesh(Vec3::ZERO, [1.0, 1.0, 1.0], true).unwrap(),
                HeightField::grid(3, 3, [1.0, 1.0, 1.0], false).unwrap(),
                Compound::single_sphere(Sphere::new(Vec3::ZERO, 0.5), SurfaceMaterial::default())
                    .unwrap(),
                bytes,
                Recording::new().unwrap(),
            ));
            let before = allocated_byte_count().unwrap();
            outer
                .visit_overlap_aabb(query_bounds(), QueryFilter::default(), |_| {
                    drop(victims.take());
                    true
                })
                .unwrap();
            assert!(victims.is_none());
            assert_native_owner_was_released(before);
        }
        "cleanup_then_panic" => {
            let outer = populated_tree();
            let mut victim = Some(populated_tree());
            let before = allocated_byte_count().unwrap();
            let error = outer
                .visit_query(query_bounds(), DynamicTreeFilter::default(), |_| {
                    drop(victim.take());
                    panic!("injected callback panic after deferred cleanup");
                })
                .unwrap_err();
            assert_eq!(error, Error::CallbackPanicked);
            assert_native_owner_was_released(before);
        }
        "recording_stop_pending" => {
            let outer = query_world(foundation);
            let mut recorded_world = foundation.create_world(foundation.world_def()).unwrap();
            let mut recording = Recording::new().unwrap();
            let mut session = Some(recorded_world.record(&mut recording).unwrap());
            outer
                .visit_overlap_aabb(query_bounds(), QueryFilter::default(), |_| {
                    assert_eq!(
                        session.take().unwrap().finish().unwrap_err(),
                        Error::InCallback
                    );
                    true
                })
                .unwrap();
            assert!(session.is_none());
            drop(session);
            assert!(recording.len().is_ok());
            recorded_world
                .record(&mut recording)
                .unwrap()
                .finish()
                .unwrap();
        }
        "retain_without_frame" => {
            let tree = populated_tree();
            let before = allocated_byte_count().unwrap();
            {
                let _callback = boxddd::__private::enter_callback_guard_for_test();
                drop(tree);
            }
            assert_eq!(allocated_byte_count().unwrap(), before);
        }
        "debug_draw_drop_world" => {
            struct DropWorldSink {
                victim: Option<World>,
            }

            impl DebugDraw for DropWorldSink {
                fn draw_shape(
                    &mut self,
                    _shape: Option<boxddd::DebugShapeHandle>,
                    _transform: WorldTransform,
                    _color: HexColor,
                ) {
                    drop(self.victim.take());
                }
            }

            let mut outer = query_world(foundation);
            let victim = query_world(foundation);
            let before = allocated_byte_count().unwrap();
            let mut sink = DropWorldSink {
                victim: Some(victim),
            };
            outer
                .debug_draw(&mut sink, boxddd::DebugDrawOptions::default())
                .unwrap();
            assert!(sink.victim.is_none());
            assert_native_owner_was_released(before);
        }
        "replace_callback_capture" => {
            let mut world = query_world(foundation);
            let drops = Arc::new(AtomicUsize::new(0));
            let probe = CallbackCaptureDropProbe {
                drops: Arc::clone(&drops),
                panic_after_reentry: false,
            };
            world
                .set_custom_filter(move |_, _| {
                    probe.retain();
                    true
                })
                .unwrap();
            world.set_custom_filter(|_, _| true).unwrap();
            assert_eq!(drops.load(Ordering::Acquire), 1);
        }
        "clear_callback_capture" => {
            let mut world = query_world(foundation);
            let drops = Arc::new(AtomicUsize::new(0));
            let probe = CallbackCaptureDropProbe {
                drops: Arc::clone(&drops),
                panic_after_reentry: false,
            };
            world
                .set_friction_callback(move |left, _| {
                    probe.retain();
                    left.coefficient
                })
                .unwrap();
            world.clear_friction_callback().unwrap();
            assert_eq!(drops.load(Ordering::Acquire), 1);
        }
        "drop_world_callback_capture" => {
            let mut world = query_world(foundation);
            let drops = Arc::new(AtomicUsize::new(0));
            let probe = CallbackCaptureDropProbe {
                drops: Arc::clone(&drops),
                panic_after_reentry: false,
            };
            world
                .set_pre_solve(move |_, _, _, _| {
                    probe.retain();
                    true
                })
                .unwrap();
            drop(world);
            assert_eq!(drops.load(Ordering::Acquire), 1);
        }
        "panicking_callback_capture" => {
            let mut world = query_world(foundation);
            let drops = Arc::new(AtomicUsize::new(0));
            let probe = CallbackCaptureDropProbe {
                drops: Arc::clone(&drops),
                panic_after_reentry: true,
            };
            world
                .set_restitution_callback(move |left, _| {
                    probe.retain();
                    left.coefficient
                })
                .unwrap();
            assert_eq!(
                world.clear_restitution_callback(),
                Err(Error::CallbackPanicked)
            );
            assert_eq!(drops.load(Ordering::Acquire), 1);
            world.gravity().unwrap();
        }
        "panicking_world_callback_capture" => {
            let mut world = query_world(foundation);
            let drops = Arc::new(AtomicUsize::new(0));
            let probe = CallbackCaptureDropProbe {
                drops: Arc::clone(&drops),
                panic_after_reentry: true,
            };
            world
                .set_custom_filter(move |_, _| {
                    probe.retain();
                    true
                })
                .unwrap();
            drop(world);
            assert_eq!(drops.load(Ordering::Acquire), 1);
            assert_eq!(boxddd::version(), Err(Error::FoundationPoisoned));
        }
        "cleanup_during_capture_unwind" => {
            struct PanicOnDrop;

            impl Drop for PanicOnDrop {
                fn drop(&mut self) {
                    panic!("injected visitor capture drop panic");
                }
            }

            struct VisitorCapture {
                victim: Option<World>,
                _panic: PanicOnDrop,
            }

            impl VisitorCapture {
                fn take_victim(&mut self) -> Option<World> {
                    self.victim.take()
                }
            }

            let outer = query_world(foundation);
            let victim = query_world(foundation);
            assert_eq!(foundation.activity().ordinary_owners, 2);
            let mut capture = VisitorCapture {
                victim: Some(victim),
                _panic: PanicOnDrop,
            };

            let result = catch_unwind(AssertUnwindSafe(|| {
                outer
                    .visit_overlap_aabb(query_bounds(), QueryFilter::default(), move |_| {
                        drop(capture.take_victim());
                        false
                    })
                    .unwrap();
            }));

            assert!(result.is_err());
            assert_eq!(foundation.activity().ordinary_owners, 1);
            drop(outer);
            assert_eq!(foundation.activity().ordinary_owners, 0);
        }
        "strict_entry_order" => {
            struct PanicPath;

            impl AsRef<std::path::Path> for PanicPath {
                fn as_ref(&self) -> &std::path::Path {
                    panic!("AsRef<Path> ran before the callback admission check");
                }
            }

            let outer = query_world(foundation);
            let mut victim = query_world(foundation);
            let mut world_hits = victim
                .overlap_aabb(query_bounds(), QueryFilter::default())
                .unwrap();
            let expected_world_hits = world_hits.clone();
            let shape_id = world_hits[0].shape_id;
            let body_id = victim.shape_body(shape_id).unwrap();
            let other_body = victim.create_body(foundation.body_def()).unwrap();
            let joint_id = victim
                .create_distance_joint(DistanceJointDef::new(body_id, other_body).length(1.0))
                .unwrap();
            let tree = populated_tree();
            let mut tree_hits = tree
                .query(query_bounds(), DynamicTreeFilter::default())
                .unwrap();
            let expected_tree_hits = tree_hits.clone();
            let recording = Recording::new().unwrap();

            outer
                .visit_overlap_aabb(query_bounds(), QueryFilter::default(), |_| {
                    assert_eq!(
                        victim.overlap_aabb_into(
                            query_bounds(),
                            QueryFilter::default(),
                            &mut world_hits,
                        ),
                        Err(Error::InCallback)
                    );
                    assert_eq!(world_hits, expected_world_hits);
                    assert_eq!(
                        tree.query_into(
                            query_bounds(),
                            DynamicTreeFilter::default(),
                            &mut tree_hits,
                        ),
                        Err(Error::InCallback)
                    );
                    assert_eq!(tree_hits, expected_tree_hits);
                    assert_eq!(
                        victim.set_restitution_threshold(f32::NAN),
                        Err(Error::InCallback)
                    );
                    assert_eq!(victim.step(f32::NAN, -1), Err(Error::InCallback));
                    assert_eq!(recording.save_to_file(PanicPath), Err(Error::InCallback));
                    assert_eq!(query_bounds().is_bounded(), Err(Error::InCallback));
                    assert_eq!(query_bounds().is_sane(), Err(Error::InCallback));
                    assert_eq!(victim.contains_body(body_id), Err(Error::InCallback));
                    assert_eq!(victim.contains_shape(shape_id), Err(Error::InCallback));
                    assert_eq!(victim.contains_joint(joint_id), Err(Error::InCallback));
                    assert_eq!(
                        tree.contains_proxy(tree_hits[0].proxy_id),
                        Err(Error::InCallback)
                    );
                    assert_eq!(victim.body_shapes(body_id), Err(Error::InCallback));
                    assert_eq!(victim.shape_contacts(shape_id), Err(Error::InCallback));
                    assert!(matches!(
                        victim.debug_draw_frame(boxddd::DebugDrawOptions::default()),
                        Err(Error::InCallback)
                    ));
                    assert_eq!(
                        victim.body_collide_mover(
                            body_id,
                            [0.0, 0.0, 0.0],
                            &boxddd::Capsule::new([0.0, -0.5, 0.0], [0.0, 0.5, 0.0], 0.25,),
                            QueryFilter::default(),
                        ),
                        Err(Error::InCallback)
                    );
                    false
                })
                .unwrap();
        }
        other => panic!("unknown callback reentrancy case: {other}"),
    }
}

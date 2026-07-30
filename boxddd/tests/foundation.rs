use boxddd::error::InvalidValueReason;
use boxddd::{
    BoxHull, DynamicTree, Error, Foundation, FoundationConfig, Recording, Sphere, StallThreshold,
    Transform, Vec3, allocated_byte_count, compute_sphere_aabb, deterministic_atan2, version,
};
use boxddd_sys::ffi;
use std::process::Command;
use std::sync::{Arc, Barrier};

const CASE_ENV: &str = "BOXDDD_FOUNDATION_TEST_CASE";

fn run_case(case: &str) {
    let status = Command::new(std::env::current_exe().expect("current test executable"))
        .args(["--exact", "foundation_subprocess_case", "--nocapture"])
        .env(CASE_ENV, case)
        .status()
        .expect("foundation subprocess should start");
    assert!(status.success(), "foundation subprocess case {case} failed");
}

#[test]
fn foundation_process_contract() {
    for case in [
        "uninitialized",
        "custom_config",
        "disabled_sentinel",
        "conflicting_stall",
        "invalid_config",
        "concurrent_same_config",
        "concurrent_conflicting_config",
    ] {
        run_case(case);
    }
}

#[test]
fn foundation_subprocess_case() {
    let Ok(case) = std::env::var(CASE_ENV) else {
        return;
    };

    match case.as_str() {
        "uninitialized" => {
            let native_length_before = unsafe { ffi::b3GetLengthUnitsPerMeter() };
            let native_stall_before = unsafe { ffi::b3GetStallThreshold() };

            assert_eq!(
                Foundation::get().unwrap_err(),
                Error::FoundationUninitialized
            );
            assert_eq!(
                allocated_byte_count().unwrap_err(),
                Error::FoundationUninitialized
            );
            assert_eq!(version().unwrap_err(), Error::FoundationUninitialized);
            assert_eq!(
                deterministic_atan2(1.0, 1.0).unwrap_err(),
                Error::FoundationUninitialized
            );
            assert_eq!(
                compute_sphere_aabb(&Sphere::new(Vec3::ZERO, 1.0), Transform::IDENTITY)
                    .unwrap_err(),
                Error::FoundationUninitialized
            );
            assert_eq!(
                BoxHull::cube(1.0).unwrap_err(),
                Error::FoundationUninitialized
            );
            assert_eq!(
                DynamicTree::new().err().unwrap(),
                Error::FoundationUninitialized
            );
            assert_eq!(
                Recording::new().unwrap_err(),
                Error::FoundationUninitialized
            );
            assert_eq!(
                unsafe { ffi::b3GetLengthUnitsPerMeter() }.to_bits(),
                native_length_before.to_bits()
            );
            assert_eq!(
                unsafe { ffi::b3GetStallThreshold() }.to_bits(),
                native_stall_before.to_bits()
            );
        }
        "custom_config" => {
            let config = FoundationConfig {
                length_units_per_meter: 100.0,
                stall_threshold: StallThreshold::Seconds(0.25),
            };
            let foundation = Foundation::initialize(config).unwrap();
            let same = Foundation::initialize(config).unwrap();
            assert!(std::ptr::eq(foundation, same));
            assert_eq!(foundation.config(), config);
            assert_eq!(
                unsafe { ffi::b3GetLengthUnitsPerMeter() }.to_bits(),
                config.length_units_per_meter.to_bits()
            );
            assert_eq!(
                unsafe { ffi::b3GetStallThreshold() }.to_bits(),
                0.25_f32.to_bits()
            );

            let world_def = foundation.world_def();
            assert_eq!(world_def.restitution_threshold, 100.0);
            assert_eq!(world_def.hit_event_threshold, 100.0);
            assert_eq!(world_def.contact_speed, 300.0);
            assert_eq!(world_def.maximum_linear_speed, 40_000.0);
            assert_eq!(foundation.body_def().sleep_threshold, 5.0);
            assert!((foundation.shape_def().density - 0.001).abs() < 1.0e-9);

            let mut world = foundation
                .create_world(
                    foundation
                        .world_def_builder()
                        .gravity(Vec3::ZERO)
                        .build()
                        .unwrap(),
                )
                .unwrap();
            assert_eq!(foundation.activity().ordinary_owners, 1);
            world.step(1.0 / 60.0, 1).unwrap();
            drop(world);
            assert_eq!(foundation.activity().ordinary_owners, 0);

            assert_eq!(
                Foundation::initialize(FoundationConfig::default()).unwrap_err(),
                Error::FoundationConflict
            );
            assert_eq!(foundation.config(), config);
            assert_eq!(
                unsafe { ffi::b3GetLengthUnitsPerMeter() }.to_bits(),
                config.length_units_per_meter.to_bits()
            );
            assert_eq!(
                unsafe { ffi::b3GetStallThreshold() }.to_bits(),
                0.25_f32.to_bits()
            );
        }
        "disabled_sentinel" => {
            let foundation = Foundation::initialize(FoundationConfig {
                length_units_per_meter: 1.0,
                stall_threshold: StallThreshold::Seconds(f32::MAX),
            })
            .unwrap();
            assert_eq!(
                foundation.config().stall_threshold,
                StallThreshold::Disabled
            );
            assert_eq!(
                unsafe { ffi::b3GetStallThreshold() }.to_bits(),
                f32::MAX.to_bits()
            );
            assert!(std::ptr::eq(
                foundation,
                Foundation::initialize_default().unwrap()
            ));
        }
        "conflicting_stall" => {
            let config = FoundationConfig {
                length_units_per_meter: 2.0,
                stall_threshold: StallThreshold::Seconds(0.25),
            };
            let foundation = Foundation::initialize(config).unwrap();
            assert_eq!(
                Foundation::initialize(FoundationConfig {
                    stall_threshold: StallThreshold::Seconds(0.5),
                    ..config
                })
                .unwrap_err(),
                Error::FoundationConflict
            );
            assert_eq!(foundation.config(), config);
        }
        "invalid_config" => {
            for (config, context, reason) in [
                (
                    FoundationConfig {
                        length_units_per_meter: 0.0,
                        stall_threshold: StallThreshold::Disabled,
                    },
                    "foundation.length_units_per_meter",
                    InvalidValueReason::OutOfRange,
                ),
                (
                    FoundationConfig {
                        length_units_per_meter: f32::NAN,
                        stall_threshold: StallThreshold::Disabled,
                    },
                    "foundation.length_units_per_meter",
                    InvalidValueReason::NonFinite,
                ),
                (
                    FoundationConfig {
                        length_units_per_meter: -1.0,
                        stall_threshold: StallThreshold::Disabled,
                    },
                    "foundation.length_units_per_meter",
                    InvalidValueReason::OutOfRange,
                ),
                (
                    FoundationConfig {
                        length_units_per_meter: f32::MIN_POSITIVE,
                        stall_threshold: StallThreshold::Disabled,
                    },
                    "foundation.length_units_per_meter",
                    InvalidValueReason::OutOfRange,
                ),
                (
                    FoundationConfig {
                        length_units_per_meter: 1.0,
                        stall_threshold: StallThreshold::Seconds(0.0),
                    },
                    "foundation.stall_threshold",
                    InvalidValueReason::OutOfRange,
                ),
                (
                    FoundationConfig {
                        length_units_per_meter: 1.0,
                        stall_threshold: StallThreshold::Seconds(f32::INFINITY),
                    },
                    "foundation.stall_threshold",
                    InvalidValueReason::NonFinite,
                ),
                (
                    FoundationConfig {
                        length_units_per_meter: 1.0,
                        stall_threshold: StallThreshold::Seconds(f32::NAN),
                    },
                    "foundation.stall_threshold",
                    InvalidValueReason::NonFinite,
                ),
                (
                    FoundationConfig {
                        length_units_per_meter: 1.0,
                        stall_threshold: StallThreshold::Seconds(-1.0),
                    },
                    "foundation.stall_threshold",
                    InvalidValueReason::OutOfRange,
                ),
                (
                    FoundationConfig {
                        length_units_per_meter: f32::MAX,
                        stall_threshold: StallThreshold::Disabled,
                    },
                    "foundation.length_units_per_meter",
                    InvalidValueReason::OutOfRange,
                ),
            ] {
                assert_eq!(
                    Foundation::initialize(config).unwrap_err(),
                    Error::InvalidValue { context, reason }
                );
                assert_eq!(
                    Foundation::get().unwrap_err(),
                    Error::FoundationUninitialized
                );
            }

            let foundation = Foundation::initialize_default().unwrap();
            assert_eq!(
                foundation.config(),
                FoundationConfig {
                    length_units_per_meter: 1.0,
                    stall_threshold: StallThreshold::Disabled,
                }
            );
        }
        "concurrent_same_config" => {
            let config = FoundationConfig::default();
            let barrier = Arc::new(Barrier::new(8));
            let threads: Vec<_> = (0..8)
                .map(|_| {
                    let barrier = Arc::clone(&barrier);
                    std::thread::spawn(move || {
                        barrier.wait();
                        Foundation::initialize(config)
                            .map(|foundation| foundation as *const _ as usize)
                    })
                })
                .collect();
            let addresses: Vec<_> = threads
                .into_iter()
                .map(|thread| thread.join().unwrap().unwrap())
                .collect();
            assert!(addresses.windows(2).all(|pair| pair[0] == pair[1]));
        }
        "concurrent_conflicting_config" => {
            let barrier = Arc::new(Barrier::new(2));
            let configs = [
                FoundationConfig::default(),
                FoundationConfig {
                    length_units_per_meter: 10.0,
                    stall_threshold: StallThreshold::Disabled,
                },
            ];
            let threads: Vec<_> = configs
                .into_iter()
                .map(|config| {
                    let barrier = Arc::clone(&barrier);
                    std::thread::spawn(move || {
                        barrier.wait();
                        Foundation::initialize(config).map(|foundation| foundation.config())
                    })
                })
                .collect();
            let results: Vec<_> = threads
                .into_iter()
                .map(|thread| thread.join().unwrap())
                .collect();
            assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
            assert_eq!(
                results
                    .iter()
                    .filter(|result| matches!(result, Err(Error::FoundationConflict)))
                    .count(),
                1
            );
        }
        other => panic!("unknown foundation subprocess case: {other}"),
    }
}

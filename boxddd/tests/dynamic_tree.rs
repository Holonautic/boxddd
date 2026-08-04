use boxddd::error::{HandleKind, InvalidValueReason};
use boxddd::{
    Aabb, BoxCastInput, DynamicTree, DynamicTreeCastControl, DynamicTreeFilter, Error, Foundation,
    RayCastInput, Vec3,
};

fn aabb(lower: f32, upper: f32) -> Aabb {
    Aabb {
        lower_bound: Vec3::new(lower, lower, lower),
        upper_bound: Vec3::new(upper, upper, upper),
    }
}

fn x_sweep_box(lower_x: f32, upper_x: f32) -> Aabb {
    Aabb {
        lower_bound: Vec3::new(lower_x, -0.5, -0.5),
        upper_bound: Vec3::new(upper_x, 0.5, 0.5),
    }
}

fn foreign_proxy() -> Error {
    Error::ForeignHandle {
        kind: HandleKind::DynamicTreeProxy,
    }
}

fn stale_proxy() -> Error {
    Error::StaleHandle {
        kind: HandleKind::DynamicTreeProxy,
    }
}

#[test]
fn proxy_lifecycle_query_and_stale_ids_are_safe() -> boxddd::Result<()> {
    Foundation::initialize_default()?;
    let mut tree = DynamicTree::new()?;
    assert_eq!(tree.proxy_count()?, 0);
    assert_eq!(tree.root_bounds()?, None);

    let proxy_id = tree.create_proxy(aabb(-1.0, 1.0), 42)?;
    assert_eq!(tree.contains_proxy(proxy_id), Ok(true));
    assert_eq!(tree.proxy_count()?, 1);
    assert_eq!(tree.proxy(proxy_id)?.user_data, 42);
    assert!(tree.byte_count()? > 0);
    assert!(tree.area_ratio()? >= 0.0);
    assert!(tree.height()? >= 0);
    assert!(tree.root_bounds()?.is_some());
    tree.validate()?;
    tree.validate_no_enlarged()?;

    let hits = tree.query(aabb(-0.5, 0.5), DynamicTreeFilter::default())?;
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].proxy_id, proxy_id);
    assert_eq!(hits[0].user_data, 42);
    assert_eq!(tree.proxy(hits[0].proxy_id)?.user_data, 42);

    tree.destroy_proxy(proxy_id)?;
    assert_eq!(tree.contains_proxy(proxy_id), Ok(false));
    assert_eq!(tree.destroy_proxy(proxy_id), Err(stale_proxy()));
    assert!(
        tree.query(aabb(-0.5, 0.5), DynamicTreeFilter::default())?
            .is_empty()
    );

    let replacement = tree.create_proxy(aabb(-1.0, 1.0), 84)?;
    assert_ne!(replacement, proxy_id);
    assert_eq!(tree.contains_proxy(proxy_id), Ok(false));
    assert_eq!(tree.proxy(proxy_id), Err(stale_proxy()));
    assert_eq!(tree.category_bits(proxy_id), Err(stale_proxy()));
    assert_eq!(
        tree.move_proxy(proxy_id, aabb(2.0, 3.0)),
        Err(stale_proxy())
    );
    assert_eq!(
        tree.enlarge_proxy(proxy_id, aabb(-2.0, 2.0)),
        Err(stale_proxy())
    );
    assert_eq!(tree.set_category_bits(proxy_id, 0b0001), Err(stale_proxy()));
    assert_eq!(tree.destroy_proxy(proxy_id), Err(stale_proxy()));
    assert_eq!(tree.proxy(replacement)?.user_data, 84);
    Ok(())
}

#[test]
fn foreign_proxy_ids_are_rejected_before_mutating_another_tree() -> boxddd::Result<()> {
    Foundation::initialize_default()?;
    let mut left = DynamicTree::new()?;
    let left_proxy = left.create_proxy_with_category_bits(aabb(-1.0, 1.0), 0b0001, 10)?;

    let mut right = DynamicTree::new()?;
    let right_proxy = right.create_proxy_with_category_bits(aabb(-1.0, 1.0), 0b0010, 20)?;

    assert_ne!(left_proxy, right_proxy);
    assert_eq!(right.contains_proxy(left_proxy), Ok(false));
    assert_eq!(right.proxy(left_proxy), Err(foreign_proxy()));
    assert_eq!(right.category_bits(left_proxy), Err(foreign_proxy()));
    assert_eq!(
        right.move_proxy(left_proxy, aabb(3.0, 4.0)),
        Err(foreign_proxy())
    );
    assert_eq!(
        right.enlarge_proxy(left_proxy, aabb(-2.0, 2.0)),
        Err(foreign_proxy())
    );
    assert_eq!(
        right.set_category_bits(left_proxy, 0b0100),
        Err(foreign_proxy())
    );
    assert_eq!(right.destroy_proxy(left_proxy), Err(foreign_proxy()));

    assert_eq!(right.proxy_count()?, 1);
    assert_eq!(right.proxy(right_proxy)?.aabb, aabb(-1.0, 1.0));
    assert_eq!(right.category_bits(right_proxy)?, 0b0010);
    let hits = right.query(aabb(-0.5, 0.5), DynamicTreeFilter::default())?;
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].proxy_id, right_proxy);
    Ok(())
}

#[test]
fn proxy_ids_remain_foreign_after_their_tree_is_dropped() -> boxddd::Result<()> {
    Foundation::initialize_default()?;
    let retired_proxy = {
        let mut retired_tree = DynamicTree::new()?;
        retired_tree.create_proxy(aabb(-1.0, 1.0), 1)?
    };

    let mut current_tree = DynamicTree::new()?;
    let current_proxy = current_tree.create_proxy(aabb(-1.0, 1.0), 2)?;

    assert_ne!(retired_proxy, current_proxy);
    assert_eq!(
        current_tree.destroy_proxy(retired_proxy),
        Err(foreign_proxy())
    );
    assert_eq!(current_tree.proxy(current_proxy)?.user_data, 2);
    Ok(())
}

#[test]
fn moving_enlarging_and_rebuilding_update_queries() -> boxddd::Result<()> {
    Foundation::initialize_default()?;
    let mut tree = DynamicTree::new()?;
    let proxy_id = tree.create_proxy(aabb(-1.0, 1.0), 7)?;

    assert!(
        tree.query(aabb(4.0, 5.0), DynamicTreeFilter::default())?
            .is_empty()
    );

    tree.move_proxy(proxy_id, aabb(4.0, 5.0))?;
    tree.validate_no_enlarged()?;
    let moved_hits = tree.query(aabb(4.25, 4.75), DynamicTreeFilter::default())?;
    assert_eq!(moved_hits[0].proxy_id, proxy_id);

    assert_eq!(
        tree.enlarge_proxy(proxy_id, aabb(4.1, 4.9)),
        Err(Error::InvalidValue {
            context: "dynamic_tree.enlarge_proxy.aabb",
            reason: InvalidValueReason::InvalidCombination,
        })
    );
    tree.enlarge_proxy(proxy_id, aabb(3.0, 6.0))?;
    assert_eq!(
        tree.validate_no_enlarged(),
        Err(Error::InvalidValue {
            context: "dynamic_tree.enlarged_nodes",
            reason: InvalidValueReason::InvalidCombination,
        })
    );

    let enlarged_hits = tree.query(aabb(3.1, 3.2), DynamicTreeFilter::default())?;
    assert_eq!(enlarged_hits[0].proxy_id, proxy_id);

    tree.rebuild(false)?;
    tree.validate_no_enlarged()?;
    Ok(())
}

#[test]
fn category_masks_and_require_all_bits_filter_proxies() -> boxddd::Result<()> {
    Foundation::initialize_default()?;
    let mut tree = DynamicTree::new()?;
    let a = tree.create_proxy_with_category_bits(aabb(-1.0, 1.0), 0b0011, 1)?;
    let b = tree.create_proxy_with_category_bits(aabb(-1.0, 1.0), 0b0101, 2)?;

    let any_bit = tree.query(aabb(-0.5, 0.5), DynamicTreeFilter::new(0b0001))?;
    assert_eq!(any_bit.len(), 2);

    let mut stopped_after_first = Vec::new();
    tree.visit_query(aabb(-0.5, 0.5), DynamicTreeFilter::new(0b0001), |hit| {
        stopped_after_first.push(hit.proxy_id);
        false
    })?;
    assert_eq!(stopped_after_first.len(), 1);

    let require_all = DynamicTreeFilter::new(0b0011).require_all_bits(true);
    let all_bits = tree.query(aabb(-0.5, 0.5), require_all)?;
    assert_eq!(all_bits.len(), 1);
    assert_eq!(all_bits[0].proxy_id, a);

    tree.set_category_bits(b, 0b0011)?;
    assert_eq!(tree.category_bits(b)?, 0b0011);
    let all_bits = tree.query(aabb(-0.5, 0.5), require_all)?;
    assert_eq!(all_bits.len(), 2);
    assert!(all_bits.iter().any(|hit| hit.proxy_id == a));
    assert!(all_bits.iter().any(|hit| hit.proxy_id == b));
    Ok(())
}

#[test]
fn closest_ray_and_box_cast_callbacks_return_owned_ids() -> boxddd::Result<()> {
    Foundation::initialize_default()?;
    let mut tree = DynamicTree::new()?;
    let near = tree.create_proxy(aabb(-1.0, 1.0), 10)?;
    let far = tree.create_proxy(aabb(5.0, 6.0), 20)?;

    let mut closest_seen = Vec::new();
    let closest = tree.visit_query_closest(
        Vec3::ZERO,
        DynamicTreeFilter::default(),
        1_000_000.0,
        |hit| {
            closest_seen.push(hit.proxy_id);
            if hit.proxy_id == near { 0.0 } else { 100.0 }
        },
    )?;
    assert!(closest.stats.leaf_visits >= 1);
    assert_eq!(closest.min_distance_squared, 0.0);
    assert!(closest_seen.contains(&near));
    assert!(
        closest_seen
            .iter()
            .copied()
            .all(|proxy_id| tree.proxy(proxy_id).is_ok())
    );

    let mut ray_hits = Vec::new();
    let ray_stats = tree.visit_ray_cast(
        RayCastInput::new(Vec3::new(-5.0, 0.0, 0.0), Vec3::new(20.0, 0.0, 0.0))?,
        DynamicTreeFilter::default(),
        |hit| {
            ray_hits.push(hit.proxy_id);
            DynamicTreeCastControl::Clip(0.4)
        },
    )?;
    assert!(ray_stats.leaf_visits >= 1);
    assert_eq!(ray_hits, vec![near]);
    assert!(!ray_hits.contains(&far));
    assert!(
        ray_hits
            .iter()
            .copied()
            .all(|proxy_id| tree.proxy(proxy_id).is_ok())
    );

    let mut box_hits = Vec::new();
    let box_stats = tree.visit_box_cast(
        BoxCastInput::new(x_sweep_box(-5.0, -4.5), Vec3::new(20.0, 0.0, 0.0))?,
        DynamicTreeFilter::default(),
        |hit| {
            box_hits.push(hit.proxy_id);
            DynamicTreeCastControl::Clip(0.4)
        },
    )?;
    assert!(box_stats.leaf_visits >= 1);
    assert_eq!(box_hits, vec![near]);
    assert!(!box_hits.contains(&far));
    assert!(
        box_hits
            .iter()
            .copied()
            .all(|proxy_id| tree.proxy(proxy_id).is_ok())
    );
    Ok(())
}

#[test]
fn dynamic_tree_callback_panics_are_reported() -> boxddd::Result<()> {
    Foundation::initialize_default()?;
    let mut tree = DynamicTree::new()?;
    let proxy_id = tree.create_proxy(aabb(-1.0, 1.0), 1)?;

    let mut reentrant_error = None;
    tree.visit_query(aabb(-0.5, 0.5), DynamicTreeFilter::default(), |hit| {
        assert_eq!(hit.proxy_id, proxy_id);
        reentrant_error = Some(tree.proxy(hit.proxy_id).unwrap_err());
        true
    })?;
    assert_eq!(reentrant_error, Some(Error::InCallback));

    assert_eq!(
        tree.visit_query(aabb(-0.5, 0.5), DynamicTreeFilter::default(), |_| {
            panic!("query panic");
        }),
        Err(Error::CallbackPanicked)
    );
    assert_eq!(
        tree.visit_query_closest(Vec3::ZERO, DynamicTreeFilter::default(), 100.0, |_| {
            panic!("closest panic");
        }),
        Err(Error::CallbackPanicked)
    );
    assert_eq!(
        tree.visit_ray_cast(
            RayCastInput::new(Vec3::new(-5.0, 0.0, 0.0), Vec3::new(10.0, 0.0, 0.0))?,
            DynamicTreeFilter::default(),
            |_| panic!("ray panic"),
        ),
        Err(Error::CallbackPanicked)
    );
    assert_eq!(
        tree.visit_box_cast(
            BoxCastInput::new(x_sweep_box(-5.0, -4.5), Vec3::new(10.0, 0.0, 0.0))?,
            DynamicTreeFilter::default(),
            |_| panic!("box panic"),
        ),
        Err(Error::CallbackPanicked)
    );
    Ok(())
}

#[test]
fn invalid_dynamic_tree_inputs_return_errors() -> boxddd::Result<()> {
    Foundation::initialize_default()?;
    let mut tree = DynamicTree::new()?;
    let invalid_aabb = Aabb {
        lower_bound: Vec3::new(1.0, 1.0, 1.0),
        upper_bound: Vec3::new(-1.0, -1.0, -1.0),
    };
    assert_eq!(
        tree.create_proxy(invalid_aabb, 0),
        Err(Error::InvalidValue {
            context: "aabb.bounds",
            reason: InvalidValueReason::InvalidCombination,
        })
    );

    let proxy_id = tree.create_proxy(aabb(-1.0, 1.0), 1)?;
    assert_eq!(
        tree.move_proxy(proxy_id, invalid_aabb),
        Err(Error::InvalidValue {
            context: "aabb.bounds",
            reason: InvalidValueReason::InvalidCombination,
        })
    );
    assert_eq!(
        tree.enlarge_proxy(proxy_id, aabb(-0.5, 0.5)),
        Err(Error::InvalidValue {
            context: "dynamic_tree.enlarge_proxy.aabb",
            reason: InvalidValueReason::InvalidCombination,
        })
    );
    tree.destroy_proxy(proxy_id)?;
    assert_eq!(
        tree.move_proxy(proxy_id, aabb(2.0, 3.0)),
        Err(stale_proxy())
    );

    assert_eq!(
        RayCastInput::with_max_fraction(Vec3::ZERO, Vec3::X, -0.1),
        Err(Error::InvalidValue {
            context: "ray_cast.max_fraction",
            reason: InvalidValueReason::OutOfRange,
        })
    );
    assert_eq!(
        BoxCastInput::new(aabb(-1.0, 1.0), Vec3::new(f32::NAN, 0.0, 0.0)),
        Err(Error::InvalidValue {
            context: "box_cast.translation",
            reason: InvalidValueReason::NonFinite,
        })
    );
    assert_eq!(
        tree.query(invalid_aabb, DynamicTreeFilter::default()),
        Err(Error::InvalidValue {
            context: "aabb.bounds",
            reason: InvalidValueReason::InvalidCombination,
        })
    );
    assert_eq!(
        tree.visit_query_closest(Vec3::ZERO, DynamicTreeFilter::default(), -1.0, |_| 0.0),
        Err(Error::InvalidValue {
            context: "dynamic_tree.query_closest.min_distance_squared",
            reason: InvalidValueReason::OutOfRange,
        })
    );
    tree.create_proxy(aabb(-1.0, 1.0), 2)?;
    assert_eq!(
        tree.visit_ray_cast(
            RayCastInput::new(Vec3::new(-5.0, 0.0, 0.0), Vec3::new(10.0, 0.0, 0.0))?,
            DynamicTreeFilter::default(),
            |_| DynamicTreeCastControl::Clip(f32::NAN),
        ),
        Err(Error::InvalidValue {
            context: "dynamic_tree.cast.clip_fraction",
            reason: InvalidValueReason::NonFinite,
        })
    );
    Ok(())
}

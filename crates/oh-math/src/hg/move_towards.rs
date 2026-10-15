//! Port of `include/SSVOpenHexagon/Utils/MoveTowards.hpp` — verbatim.

/// `hg::Utils::getMoveTowards` — moves `value` towards `target` by at most
/// `step` (`SSVOH_ASSERT(step >= 0)` in C++).
#[must_use]
pub fn get_move_towards(mut value: f32, target: f32, step: f32) -> f32 {
    debug_assert!(step >= 0.0f32);

    if value < target {
        value += step;
        if value > target {
            value = target;
        }
    } else if value > target {
        value -= step;
        if value < target {
            value = target;
        }
    }

    value
}

/// `hg::Utils::getMoveTowardsZero` — `getMoveTowards(value, 0.f, step)`.
#[must_use]
pub fn get_move_towards_zero(value: f32, step: f32) -> f32 {
    get_move_towards(value, 0.0f32, step)
}

/// `hg::Utils::moveTowards` — in-place form.
pub fn move_towards(value: &mut f32, target: f32, step: f32) {
    *value = get_move_towards(*value, target, step);
}

/// `hg::Utils::moveTowardsZero` — in-place form.
pub fn move_towards_zero(value: &mut f32, step: f32) {
    *value = get_move_towards_zero(*value, step);
}

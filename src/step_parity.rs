use std::sync::OnceLock;

use crate::timing::{ROWS_PER_BEAT, TimingData, beat_to_note_row_f32, get_time_for_beat_f32, is_fake_at_beat};

const INVALID_COLUMN: i8 = -1;
const CLM_SECOND_INVALID: f32 = -1.0;
const HOLD_END_NONE: f32 = -1.0;
const MAX_NOTE_ROW: i32 = 1 << 30;
const MISSING_HOLD_LENGTH_BEATS: f32 = MAX_NOTE_ROW as f32 / ROWS_PER_BEAT as f32;

// Weights and thresholds
const DOUBLESTEP_WEIGHT: f32 = 850.0;
const BRACKETJACK_WEIGHT: f32 = 20.0;
const JACK_WEIGHT: f32 = 30.0;
const SLOW_BRACKET_WEIGHT: f32 = 300.0;
const TWISTED_FOOT_WEIGHT: f32 = 100_000.0;
const BRACKETTAP_WEIGHT: f32 = 400.0;
const HOLDSWITCH_WEIGHT: f32 = 55.0;
const MINE_WEIGHT: f32 = 10000.0;
const FOOTSWITCH_WEIGHT: f32 = 325.0;
const MISSED_FOOTSWITCH_WEIGHT: f32 = 500.0;
const FACING_WEIGHT: f32 = 2.0;
const DISTANCE_WEIGHT: f32 = 6.0;
const SPIN_WEIGHT: f32 = 1000.0;
const SIDESWITCH_WEIGHT: f32 = 130.0;

const JACK_THRESHOLD: f32 = 0.1;
const SLOW_BRACKET_THRESHOLD: f32 = 0.15;
const SLOW_FOOTSWITCH_THRESHOLD: f32 = 0.2;
const SLOW_FOOTSWITCH_IGNORE: f32 = 0.4;
const JACK_CUTOFF: f32 = 0.176;
const FOOTSWITCH_CUTOFF: f32 = 0.3;
const DOUBLESTEP_CUTOFF: f32 = 0.235;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Ord, PartialOrd, Default)]
#[repr(u8)]
pub enum Foot {
    #[default]
    None = 0,
    LeftHeel = 1,
    LeftToe = 2,
    RightHeel = 3,
    RightToe = 4,
}

#[inline(always)]
const fn foot_idx(f: Foot) -> usize {
    f as usize
}

#[inline(always)]
const fn foot_is_left(f: Foot) -> bool {
    matches!(f, Foot::LeftHeel | Foot::LeftToe)
}

#[inline(always)]
const fn foot_is_right(f: Foot) -> bool {
    matches!(f, Foot::RightHeel | Foot::RightToe)
}

const NUM_FEET: usize = 5;
const MAX_COLUMNS: usize = 8;
const PAIR_STRIDE: usize = MAX_COLUMNS + 1;
const PAIR_LEN: usize = PAIR_STRIDE * PAIR_STRIDE;
const DIST_LEN: usize = MAX_COLUMNS * MAX_COLUMNS;
const FEET: [Foot; 4] = [
    Foot::LeftHeel,
    Foot::LeftToe,
    Foot::RightHeel,
    Foot::RightToe,
];
const FOOT_MASKS: [u8; NUM_FEET] = [0, 1, 2, 4, 8];
const LEFT_FOOT_MASK: u8 = FOOT_MASKS[1] | FOOT_MASKS[2];
const RIGHT_FOOT_MASK: u8 = FOOT_MASKS[3] | FOOT_MASKS[4];
const PERM_CAP: [usize; 9] = [1, 4, 12, 24, 24, 0, 0, 0, 0];
const OTHER_PART_OF_FOOT: [Foot; NUM_FEET] = [
    Foot::None,
    Foot::LeftToe,
    Foot::LeftHeel,
    Foot::RightToe,
    Foot::RightHeel,
];

// Foot pair for symmetric operations
struct FootPair {
    heel: Foot,
    toe: Foot,
}

const LEFT_PAIR: FootPair = FootPair {
    heel: Foot::LeftHeel,
    toe: Foot::LeftToe,
};
const RIGHT_PAIR: FootPair = FootPair {
    heel: Foot::RightHeel,
    toe: Foot::RightToe,
};

const ROW_MAP_MIN_CAP: usize = 16;

struct RowStateMap {
    entries: Vec<RowMapEntry>,
    epoch: u32,
    mask: usize,
}

#[derive(Clone, Copy, Default)]
struct RowMapEntry {
    key: u32,
    val: usize,
    mark: u32,
}

const fn row_map_hash(x: u32) -> usize {
    // 0x9E3779B9 is the 32-bit golden ratio prime
    x.wrapping_mul(0x9E3779B9) as usize
}

const fn row_map_new() -> RowStateMap {
    RowStateMap {
        entries: Vec::new(),
        epoch: 1,
        mask: 0,
    }
}

fn row_map_cap(expected: usize) -> usize {
    let target = expected.saturating_mul(2).max(ROW_MAP_MIN_CAP);
    let mut cap = ROW_MAP_MIN_CAP;
    while cap < target && cap <= (usize::MAX >> 1) {
        cap <<= 1;
    }
    cap
}

fn row_map_reset(map: &mut RowStateMap, expected: usize) {
    let need = row_map_cap(expected);
    if need > map.entries.len() {
        map.entries.resize(need, RowMapEntry::default());
        map.mask = need - 1;
    }
    map.epoch = map.epoch.wrapping_add(1);
    if map.epoch == 0 {
        for entry in &mut map.entries {
            entry.mark = 0;
        }
        map.epoch = 1;
    }
}

#[inline(always)]
fn row_map_get(map: &RowStateMap, key: u32) -> Option<usize> {
    if map.mask == 0 {
        return None;
    }
    let mut idx = row_map_hash(key) & map.mask;
    loop {
        let entry = map.entries[idx];
        if entry.mark != map.epoch {
            return None;
        }
        if entry.key == key {
            return Some(entry.val);
        }
        idx = (idx + 1) & map.mask;
    }
}

#[inline(always)]
fn row_map_insert(map: &mut RowStateMap, key: u32, val: usize) {
    let mut idx = row_map_hash(key) & map.mask;
    loop {
        let entry = &mut map.entries[idx];
        if entry.mark != map.epoch {
            entry.mark = map.epoch;
            entry.key = key;
            entry.val = val;
            return;
        }
        if entry.key == key {
            entry.val = val;
            return;
        }
        idx = (idx + 1) & map.mask;
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct StagePoint {
    x: f32,
    y: f32,
}

#[derive(Debug, Clone)]
struct StageLayout {
    cols: u8,
    columns: [StagePoint; MAX_COLUMNS],
    up_mask: u8,
    down_mask: u8,
    side_mask: u8,
    avg_points: [StagePoint; PAIR_LEN],
    facing_x_penalty: [f32; PAIR_LEN],
    facing_y_penalty: [f32; PAIR_LEN],
    bracket_ok: [bool; DIST_LEN],
    hold_switch_cost: [f32; DIST_LEN],
    dist_weighted: [f64; DIST_LEN],
}

fn dance_single_layout() -> StageLayout {
    layout_new(
        &[
            StagePoint { x: 0.0, y: 1.0 },
            StagePoint { x: 1.0, y: 0.0 },
            StagePoint { x: 1.0, y: 2.0 },
            StagePoint { x: 2.0, y: 1.0 },
        ],
        1u8 << 2,
        1u8 << 1,
        (1u8 << 0) | (1u8 << 3),
    )
}

fn dance_double_layout() -> StageLayout {
    layout_new(
        &[
            StagePoint { x: 0.0, y: 1.0 },
            StagePoint { x: 1.0, y: 0.0 },
            StagePoint { x: 1.0, y: 2.0 },
            StagePoint { x: 2.0, y: 1.0 },
            StagePoint { x: 3.0, y: 1.0 },
            StagePoint { x: 4.0, y: 0.0 },
            StagePoint { x: 4.0, y: 2.0 },
            StagePoint { x: 5.0, y: 1.0 },
        ],
        (1u8 << 2) | (1u8 << 6),
        (1u8 << 1) | (1u8 << 5),
        (1u8 << 0) | (1u8 << 3) | (1u8 << 4) | (1u8 << 7),
    )
}

fn layout_new(points: &[StagePoint], up_mask: u8, down_mask: u8, side_mask: u8) -> StageLayout {
    let cols = points.len();
    debug_assert!(cols <= MAX_COLUMNS);
    let mut columns = [StagePoint::default(); MAX_COLUMNS];
    columns[..cols].copy_from_slice(points);

    let pair_stride = cols + 1;
    let invalid = cols;

    let mut avg_points = [StagePoint::default(); PAIR_LEN];
    let mut facing_x_penalty = [0.0f32; PAIR_LEN];
    let mut facing_y_penalty = [0.0f32; PAIR_LEN];

    let facing_penalty = |v: f32| -> f32 {
        let base = -(v.min(0.0));
        if base > 0.0 {
            f64::from(base).powf(1.8) as f32 * 100.0 * FACING_WEIGHT
        } else {
            0.0
        }
    };

    for left in 0..pair_stride {
        for right in 0..pair_stride {
            let idx = left * PAIR_STRIDE + right;
            let lp = if left == invalid {
                None
            } else {
                Some(columns[left])
            };
            let rp = if right == invalid {
                None
            } else {
                Some(columns[right])
            };

            avg_points[idx] = match (lp, rp) {
                (None, None) => StagePoint::default(),
                (None, Some(r)) => r,
                (Some(l), None) => l,
                (Some(l), Some(r)) => StagePoint {
                    x: f32::midpoint(l.x, r.x),
                    y: f32::midpoint(l.y, r.y),
                },
            };

            if left == right || left == invalid || right == invalid {
                continue;
            }

            let (dx, dy) = (
                f64::from(columns[right].x - columns[left].x),
                f64::from(columns[right].y - columns[left].y),
            );
            let dist = dx.hypot(dy);
            if dist == 0.0 {
                continue;
            }

            let (ndx, ndy) = (dx / dist, dy / dist);
            let (adx, ady) = (ndx.abs(), ndy.abs());
            let (adx2, ady2) = (adx * adx, ady * ady);
            let (mut xm, mut ym) = ((adx2 * adx2) as f32, (ady2 * ady2) as f32);
            if ndx <= 0.0 {
                xm = -xm;
            }
            if ndy <= 0.0 {
                ym = -ym;
            }
            facing_x_penalty[idx] = facing_penalty(xm);
            facing_y_penalty[idx] = facing_penalty(ym);
        }
    }

    let mut bracket_ok = [false; DIST_LEN];
    let mut hold_switch_cost = [0.0f32; DIST_LEN];
    let mut dist_weighted = [0.0f64; DIST_LEN];

    for l in 0..cols {
        for r in 0..cols {
            let (dx, dy) = (columns[l].x - columns[r].x, columns[l].y - columns[r].y);
            let sq = dx.mul_add(dx, dy * dy);
            let idx = l * MAX_COLUMNS + r;
            bracket_ok[idx] = sq <= 2.0;
            let dist = f64::from(sq).sqrt();
            hold_switch_cost[idx] = dist as f32 * HOLDSWITCH_WEIGHT;
            dist_weighted[idx] = dist * f64::from(DISTANCE_WEIGHT);
        }
    }

    StageLayout {
        cols: cols as u8,
        columns,
        up_mask,
        down_mask,
        side_mask,
        avg_points,
        facing_x_penalty,
        facing_y_penalty,
        bracket_ok,
        hold_switch_cost,
        dist_weighted,
    }
}

#[inline(always)]
const fn layout_cols(layout: &StageLayout) -> usize {
    layout.cols as usize
}

#[inline(always)]
const fn layout_bracket_ok(layout: &StageLayout, c1: usize, c2: usize) -> bool {
    layout.bracket_ok[c1 * MAX_COLUMNS + c2]
}

#[inline(always)]
const fn layout_hold_switch_cost(layout: &StageLayout, c1: usize, c2: usize) -> f32 {
    layout.hold_switch_cost[c1 * MAX_COLUMNS + c2]
}

#[inline(always)]
const fn layout_dist_weighted(layout: &StageLayout, c1: usize, c2: usize) -> f64 {
    layout.dist_weighted[c1 * MAX_COLUMNS + c2]
}

#[inline(always)]
const fn layout_pair_idx(layout: &StageLayout, left: i8, right: i8) -> usize {
    let max = layout.cols as usize;
    let l = if left == INVALID_COLUMN {
        max
    } else {
        left as usize
    };
    let r = if right == INVALID_COLUMN {
        max
    } else {
        right as usize
    };
    l * PAIR_STRIDE + r
}

#[inline(always)]
const fn layout_facing_x(layout: &StageLayout, l: i8, r: i8) -> f32 {
    layout.facing_x_penalty[layout_pair_idx(layout, l, r)]
}

#[inline(always)]
const fn layout_facing_y(layout: &StageLayout, l: i8, r: i8) -> f32 {
    layout.facing_y_penalty[layout_pair_idx(layout, l, r)]
}

#[inline(always)]
const fn layout_avg_point(layout: &StageLayout, l: i8, r: i8) -> StagePoint {
    layout.avg_points[layout_pair_idx(layout, l, r)]
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum TapNoteType {
    #[default]
    Empty,
    Tap,
    HoldHead,
    Mine,
    Fake,
}

#[derive(Debug, Clone, Copy, Default)]
struct IntermediateNoteData {
    note_type: TapNoteType,
    col: usize,
    beat: f32,
    hold_length: f32,
    fake: bool,
    second: f32,
}

#[derive(Debug, Clone)]
struct Row {
    second: f32,
    beat: f32,
    note_count: u8,
    note_mask: u8,
    hold_mask: u8,
    hold_ends: [f32; MAX_COLUMNS],
    mine_mask: u8,
    mine_i32_mask: u8,
    fake_mine_mask: u8,
}

const fn row_new() -> Row {
    Row {
        second: 0.0,
        beat: 0.0,
        note_count: 0,
        note_mask: 0,
        hold_mask: 0,
        hold_ends: [HOLD_END_NONE; MAX_COLUMNS],
        mine_mask: 0,
        mine_i32_mask: 0,
        fake_mine_mask: 0,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct State {
    combined_columns: [Foot; MAX_COLUMNS],
    where_the_feet_are: [i8; NUM_FEET],
    occupied_mask: u8,
    moved_mask: u8,
    holding_mask: u8,
}

const fn state_new() -> State {
    State {
        combined_columns: [Foot::None; MAX_COLUMNS],
        where_the_feet_are: [INVALID_COLUMN; NUM_FEET],
        occupied_mask: 0,
        moved_mask: 0,
        holding_mask: 0,
    }
}

#[inline(always)]
const fn foot_moved(s: &State, pair: &FootPair) -> bool {
    let mask = FOOT_MASKS[foot_idx(pair.heel)] | FOOT_MASKS[foot_idx(pair.toe)];
    (s.moved_mask & mask) != 0
}

#[inline(always)]
const fn foot_moved_not_holding(s: &State, pair: &FootPair) -> bool {
    let mask = FOOT_MASKS[foot_idx(pair.heel)] | FOOT_MASKS[foot_idx(pair.toe)];
    ((s.moved_mask & !s.holding_mask) & mask) != 0
}

type FootPlacement = [Foot; MAX_COLUMNS];

const NO_PERMS: [FootPlacement; 1] = [[Foot::None; MAX_COLUMNS]];

#[derive(Debug, Clone)]
struct StepParityNode {
    state: State,
    pred: u32,
    cost: f32,
}

// --- Cost Calculations (free functions to avoid borrow issues) ---

fn did_jack(
    initial: &State,
    result: &State,
    pair: &FootPair,
    heel_col: i8,
    toe_col: i8,
    moved: bool,
    did_jump: bool,
    pair_moved_not_holding: bool,
) -> bool {
    if did_jump || !moved || !pair_moved_not_holding {
        return false;
    }

    let check = |col: i8, foot: Foot| -> bool {
        col > INVALID_COLUMN
            && initial.combined_columns[col as usize] == foot
            && (result.holding_mask & FOOT_MASKS[foot_idx(foot)]) == 0
    };

    check(heel_col, pair.heel) || check(toe_col, pair.toe)
}

fn calc_action_cost(
    layout: &StageLayout,
    initial: &State,
    result: &State,
    placement: &FootPlacement,
    hit: [i8; NUM_FEET],
    rows: &[Row],
    row_idx: usize,
    elapsed: f32,
    cols: usize,
    left_moved_not_holding: bool,
    right_moved_not_holding: bool,
    prev_row_has_live_hold: bool,
) -> f32 {
    let row = &rows[row_idx];
    let (lh, lt, rh, rt) = (
        hit[foot_idx(Foot::LeftHeel)],
        hit[foot_idx(Foot::LeftToe)],
        hit[foot_idx(Foot::RightHeel)],
        hit[foot_idx(Foot::RightToe)],
    );
    let (moved_left, moved_right) = (foot_moved(result, &LEFT_PAIR), foot_moved(result, &RIGHT_PAIR));
    let did_jump = left_moved_not_holding && right_moved_not_holding;
    let (jacked_left, jacked_right) = (
        did_jack(
            initial,
            result,
            &LEFT_PAIR,
            lh,
            lt,
            moved_left,
            did_jump,
            left_moved_not_holding,
        ),
        did_jack(
            initial,
            result,
            &RIGHT_PAIR,
            rh,
            rt,
            moved_right,
            did_jump,
            right_moved_not_holding,
        ),
    );

    let mut cost = 0.0;
    cost += calc_mine_cost(result, row);
    cost += calc_hold_switch_cost(layout, initial, result, row);
    cost += calc_bracket_tap_cost(initial, row, lh, lt, rh, rt, elapsed);
    cost +=
        calc_bracket_jack_cost(result, moved_left, moved_right, jacked_left, jacked_right, did_jump);
    cost += calc_doublestep_cost(
        moved_left,
        moved_right,
        jacked_left,
        jacked_right,
        did_jump,
        result.holding_mask != 0,
        left_moved_not_holding,
        right_moved_not_holding,
        prev_row_has_live_hold,
    );
    cost += calc_slow_bracket_cost(row, moved_left, moved_right, elapsed);
    cost += calc_twisted_foot_cost(layout, hit);
    cost += calc_facing_cost(layout, result);
    cost += calc_spin_cost(layout, initial, result);
    cost += calc_footswitch_cost(initial, placement, row, elapsed, cols);
    cost += calc_sideswitch_cost(layout, initial, result, placement);
    cost += calc_missed_footswitch_cost(row, jacked_left, jacked_right);
    cost += calc_jack_cost(moved_left, moved_right, jacked_left, jacked_right, elapsed);
    cost += calc_big_movements_cost(layout, initial, result, hit, elapsed);
    cost
}

fn calc_mine_cost(result: &State, row: &Row) -> f32 {
    if row.mine_mask & result.occupied_mask != 0 {
        MINE_WEIGHT
    } else {
        0.0
    }
}

fn calc_hold_switch_cost(layout: &StageLayout, initial: &State, result: &State, row: &Row) -> f32 {
    let mut mask = row.hold_mask & result.occupied_mask;
    if mask == 0 {
        return 0.0;
    }
    let mut cost = 0.0;

    while mask != 0 {
        let c = mask.trailing_zeros() as usize;
        mask &= mask - 1;

        let foot = result.combined_columns[c];
        let initial_foot = initial.combined_columns[c];
        let switched = (foot_is_left(foot) && !foot_is_left(initial_foot))
            || (foot_is_right(foot) && !foot_is_right(initial_foot));

        if switched {
            let prev_col = initial.where_the_feet_are[foot_idx(foot)];
            if prev_col == INVALID_COLUMN {
                cost += HOLDSWITCH_WEIGHT;
            } else {
                cost += layout_hold_switch_cost(layout, c, prev_col as usize);
            }
        }
    }
    cost
}

fn calc_bracket_tap_cost(
    initial: &State,
    row: &Row,
    lh: i8,
    lt: i8,
    rh: i8,
    rt: i8,
    elapsed: f32,
) -> f32 {
    if row.hold_mask == 0 {
        return 0.0;
    }
    let mut cost = 0.0;

    let check_pair = |heel: i8, toe: i8, pair: &FootPair| -> f32 {
        if heel == INVALID_COLUMN || toe == INVALID_COLUMN {
            return 0.0;
        }
        let jack_penalty = if foot_moved(initial, pair) {
            1.0 / elapsed
        } else {
            1.0
        };
        let hm = (row.hold_mask & (1u8 << heel as usize)) != 0;
        let tm = (row.hold_mask & (1u8 << toe as usize)) != 0;
        if (hm && !tm) || (tm && !hm) {
            BRACKETTAP_WEIGHT * jack_penalty
        } else {
            0.0
        }
    };

    cost += check_pair(lh, lt, &LEFT_PAIR);
    cost += check_pair(rh, rt, &RIGHT_PAIR);
    cost
}

fn calc_bracket_jack_cost(
    result: &State,
    moved_left: bool,
    moved_right: bool,
    jacked_left: bool,
    jacked_right: bool,
    did_jump: bool,
) -> f32 {
    if moved_left == moved_right || result.holding_mask != 0 || did_jump {
        return 0.0;
    }

    let mut cost = 0.0;
    if jacked_left && (result.moved_mask & LEFT_FOOT_MASK) == LEFT_FOOT_MASK {
        cost += BRACKETJACK_WEIGHT;
    }
    if jacked_right && (result.moved_mask & RIGHT_FOOT_MASK) == RIGHT_FOOT_MASK {
        cost += BRACKETJACK_WEIGHT;
    }
    cost
}

fn calc_doublestep_cost(
    moved_left: bool,
    moved_right: bool,
    jacked_left: bool,
    jacked_right: bool,
    did_jump: bool,
    result_holding: bool,
    left_moved_not_holding: bool,
    right_moved_not_holding: bool,
    prev_row_has_live_hold: bool,
) -> f32 {
    if moved_left == moved_right || did_jump || result_holding {
        return 0.0;
    }

    let did_double_step =
        (moved_left && !jacked_left && left_moved_not_holding)
            || (moved_right && !jacked_right && right_moved_not_holding);
    if did_double_step && !prev_row_has_live_hold {
        DOUBLESTEP_WEIGHT
    } else {
        0.0
    }
}

fn calc_slow_bracket_cost(row: &Row, moved_left: bool, moved_right: bool, elapsed: f32) -> f32 {
    if elapsed > SLOW_BRACKET_THRESHOLD && moved_left != moved_right && row.note_count >= 2 {
        (elapsed - SLOW_BRACKET_THRESHOLD) * SLOW_BRACKET_WEIGHT
    } else {
        0.0
    }
}

fn calc_twisted_foot_cost(layout: &StageLayout, hit: [i8; NUM_FEET]) -> f32 {
    let lh = hit[1];
    let lt = hit[2];
    let rh = hit[3];
    let rt = hit[4];

    let left_pos = layout_avg_point(layout, lh, lt);
    let right_pos = layout_avg_point(layout, rh, rt);
    let crossed = right_pos.x < left_pos.x;

    let backward = |heel: i8, toe: i8| -> bool {
        heel != INVALID_COLUMN
            && toe != INVALID_COLUMN
            && layout.columns[toe as usize].y < layout.columns[heel as usize].y
    };

    if !crossed && (backward(rh, rt) || backward(lh, lt)) {
        TWISTED_FOOT_WEIGHT
    } else {
        0.0
    }
}

fn calc_facing_cost(layout: &StageLayout, result: &State) -> f32 {
    let get = |f: Foot| result.where_the_feet_are[foot_idx(f)];
    let (lh, mut lt) = (get(Foot::LeftHeel), get(Foot::LeftToe));
    let (rh, mut rt) = (get(Foot::RightHeel), get(Foot::RightToe));

    if lt == INVALID_COLUMN {
        lt = lh;
    }
    if rt == INVALID_COLUMN {
        rt = rh;
    }

    layout_facing_x(layout, lh, rh)
        + layout_facing_x(layout, lt, rt)
        + layout_facing_y(layout, lh, lt)
        + layout_facing_y(layout, rh, rt)
}

fn calc_spin_cost(layout: &StageLayout, initial: &State, result: &State) -> f32 {
    let get = |s: &State, f: Foot| s.where_the_feet_are[foot_idx(f)];

    let prev_left = layout_avg_point(layout, get(initial, Foot::LeftHeel), get(initial, Foot::LeftToe));
    let prev_right = layout_avg_point(layout, get(initial, Foot::RightHeel), get(initial, Foot::RightToe));

    let mut lt = get(result, Foot::LeftToe);
    let mut rt = get(result, Foot::RightToe);
    if lt == INVALID_COLUMN {
        lt = get(result, Foot::LeftHeel);
    }
    if rt == INVALID_COLUMN {
        rt = get(result, Foot::RightHeel);
    }

    let left = layout_avg_point(layout, get(result, Foot::LeftHeel), lt);
    let right = layout_avg_point(layout, get(result, Foot::RightHeel), rt);

    let mut cost = 0.0;
    if right.x < left.x && prev_right.x < prev_left.x
        && ((right.y < left.y && prev_right.y > prev_left.y)
            || (right.y > left.y && prev_right.y < prev_left.y))
        {
            cost += SPIN_WEIGHT;
        }
    cost
}

fn calc_footswitch_cost(
    initial: &State,
    placement: &FootPlacement,
    row: &Row,
    elapsed: f32,
    cols: usize,
) -> f32 {
    if !(SLOW_FOOTSWITCH_THRESHOLD..SLOW_FOOTSWITCH_IGNORE).contains(&elapsed) {
        return 0.0;
    }
    if row.mine_i32_mask != 0 || row.fake_mine_mask != 0 {
        return 0.0;
    }

    let time_scaled = elapsed - SLOW_FOOTSWITCH_THRESHOLD;
    for (i, &res) in placement.iter().enumerate().take(cols) {
        let init = initial.combined_columns[i];
        if init == Foot::None || res == Foot::None {
            continue;
        }
        if init != res && init != OTHER_PART_OF_FOOT[foot_idx(res)] {
            let divisor = SLOW_FOOTSWITCH_THRESHOLD + time_scaled;
            if divisor > 0.0 {
                return (time_scaled / divisor) * FOOTSWITCH_WEIGHT;
            }
        }
    }
    0.0
}

fn calc_sideswitch_cost(
    layout: &StageLayout,
    initial: &State,
    result: &State,
    placement: &FootPlacement,
) -> f32 {
    let mut mask = layout.side_mask;
    let mut count = 0u32;
    while mask != 0 {
        let c = mask.trailing_zeros() as usize;
        mask &= mask - 1;
	        if initial.combined_columns[c] != placement[c]
	            && placement[c] != Foot::None
	            && initial.combined_columns[c] != Foot::None
	            && (result.moved_mask & FOOT_MASKS[foot_idx(initial.combined_columns[c])]) == 0
	        {
	            count += 1;
	        }
    }
    count as f32 * SIDESWITCH_WEIGHT
}

const fn calc_missed_footswitch_cost(row: &Row, jacked_left: bool, jacked_right: bool) -> f32 {
    if (jacked_left || jacked_right) && (row.mine_i32_mask != 0 || row.fake_mine_mask != 0) {
        MISSED_FOOTSWITCH_WEIGHT
    } else {
        0.0
    }
}

fn calc_jack_cost(
    moved_left: bool,
    moved_right: bool,
    jacked_left: bool,
    jacked_right: bool,
    elapsed: f32,
) -> f32 {
    if elapsed < JACK_THRESHOLD && moved_left != moved_right && (jacked_left || jacked_right) {
        let ts = JACK_THRESHOLD - elapsed;
        if ts > 0.0 {
            return (1.0 / ts - 1.0 / JACK_THRESHOLD) * JACK_WEIGHT;
        }
    }
    0.0
}

fn calc_big_movements_cost(
    layout: &StageLayout,
    initial: &State,
    result: &State,
    hit: [i8; NUM_FEET],
    elapsed: f32,
) -> f32 {
	    let mut cost = 0.0;
	    for &foot in &FEET {
	        if (result.moved_mask & FOOT_MASKS[foot_idx(foot)]) == 0 {
	            continue;
	        }
	        let init_pos = initial.where_the_feet_are[foot_idx(foot)];
	        if init_pos == INVALID_COLUMN {
	            continue;
	        }

	        let res_pos = hit[foot_idx(foot)];
	        let dist = layout_dist_weighted(layout, init_pos as usize, res_pos as usize);
	        let mut d = (dist / f64::from(elapsed)) as f32;

	        let other = OTHER_PART_OF_FOOT[foot_idx(foot)];
	        let other_pos = hit[foot_idx(other)];
        if other_pos != INVALID_COLUMN {
            if other_pos == init_pos {
                continue;
            }
            d *= 0.2;
        }
        cost += d;
    }
    cost
}

#[inline(always)]
fn row_has_live_hold(row: &Row) -> bool {
    let mut mask = row.hold_mask;
    while mask != 0 {
        let idx = mask.trailing_zeros() as usize;
        mask &= mask - 1;
        if row.hold_ends[idx] > row.beat {
            return true;
        }
    }
    false
}

// --- Generator ---

struct StepParityGenerator {
    layout: &'static StageLayout,
    perm_table: &'static [Box<[FootPlacement]>; 256],
    perm_cache: [Option<&'static [FootPlacement]>; 256],
    column_count: usize,
    nodes: Vec<StepParityNode>,
    rows: Vec<Row>,
    result_columns: Vec<FootPlacement>,
    prev_ids: Vec<usize>,
    next_ids: Vec<usize>,
    state_map: RowStateMap,
}

fn parity_gen(cache: &'static LayoutCache) -> StepParityGenerator {
    StepParityGenerator {
        column_count: layout_cols(&cache.layout),
        layout: &cache.layout,
        perm_table: &cache.perm_table,
        perm_cache: [None; 256],
        nodes: Vec::new(),
        rows: Vec::new(),
        result_columns: Vec::new(),
        prev_ids: Vec::new(),
        next_ids: Vec::new(),
        state_map: row_map_new(),
    }
}

#[inline(always)]
fn parity_reset(g: &mut StepParityGenerator, cols: usize) {
    g.column_count = cols;
    g.perm_cache.fill(None);
    g.nodes.clear();
    g.rows.clear();
    g.result_columns.clear();
}

#[inline(always)]
fn parity_finish(g: &mut StepParityGenerator) -> bool {
    if g.rows.is_empty() {
        return false;
    }
    let Some(best) = parity_dp_rows(g) else {
        return false;
    };
    parity_backtrack(g, best)
}

fn parity_analyze(g: &mut StepParityGenerator, notes: Vec<IntermediateNoteData>, cols: usize) -> bool {
    parity_reset(g, cols);
    parity_create_rows(g, notes);
    parity_finish(g)
}

fn parity_create_rows(g: &mut StepParityGenerator, notes: Vec<IntermediateNoteData>) {
    let mut counter = row_counter_new();

    for note in notes {
        if note.note_type == TapNoteType::Empty {
            continue;
        }

        if note.note_type == TapNoteType::Mine {
            let bit = 1u8 << note.col;
            let mine_on = note.second != 0.0;
            let mine_i32_on = (note.second as i32) != 0;

            if note.second == counter.last_second && !g.rows.is_empty() {
                if note.fake {
                    if mine_i32_on {
                        counter.next_fake_mine_mask |= bit;
                    } else {
                        counter.next_fake_mine_mask &= !bit;
                    }
                } else {
                    if mine_on {
                        counter.next_mine_mask |= bit;
                    } else {
                        counter.next_mine_mask &= !bit;
                    }
                    if mine_i32_on {
                        counter.next_mine_i32_mask |= bit;
                    } else {
                        counter.next_mine_i32_mask &= !bit;
                    }
                }
            } else if note.fake {
                if mine_i32_on {
                    counter.fake_mine_mask |= bit;
                } else {
                    counter.fake_mine_mask &= !bit;
                }
            } else {
                if mine_on {
                    counter.mine_mask |= bit;
                } else {
                    counter.mine_mask &= !bit;
                }
                if mine_i32_on {
                    counter.mine_i32_mask |= bit;
                } else {
                    counter.mine_i32_mask &= !bit;
                }
            }
            continue;
        }

        if note.fake {
            continue;
        }

        if counter.last_second != note.second {
            if counter.last_second != CLM_SECOND_INVALID {
                parity_flush_row(g, &counter);
            }
            row_counter_reset(&mut counter, note.second, note.beat);
        }

        let col = note.col;
        let is_hold = note.note_type == TapNoteType::HoldHead;
        counter.note_mask |= 1u8 << col;
        if is_hold {
            counter.hold_ends[col] = note.beat + note.hold_length;
        }
    }
    parity_flush_row(g, &counter);
}

#[inline(always)]
fn row_quantized(beat_raw: f32) -> (i32, f32) {
    let row_i32 = beat_to_note_row_f32(beat_raw);
    (row_i32, row_i32 as f32 / ROWS_PER_BEAT as f32)
}

fn hold_heads_from_arrays<const LANES: usize>(
    rows: &[[u8; LANES]],
    row_to_beat: &[f32],
    cols: usize,
) -> Vec<[f32; MAX_COLUMNS]> {
    if cols == 0 || cols > 8 {
        return Vec::new();
    }
    let copy_len = cols.min(LANES);
    let mut out = vec![[HOLD_END_NONE; MAX_COLUMNS]; rows.len()];
    let mut hold_start_idx = [usize::MAX; MAX_COLUMNS];
    let mut hold_start_row = [0i32; MAX_COLUMNS];
    let mut hold_start_beat = [0.0f32; MAX_COLUMNS];

    for (idx, row) in rows.iter().enumerate() {
        if !has_obj(&row[..copy_len]) {
            continue;
        }
        let (row_i32, beat) = row_quantized(row_to_beat[idx]);
        for c in 0..copy_len {
            let ch = row[c];
            if matches!(ch, b'1' | b'K' | b'L' | b'M' | b'F') {
                hold_start_idx[c] = usize::MAX;
            }
            match ch {
                b'2' | b'4' => {
                    hold_start_idx[c] = idx;
                    hold_start_row[c] = row_i32;
                    hold_start_beat[c] = beat;
                }
                b'3' => {
                    let start_idx = hold_start_idx[c];
                    if start_idx != usize::MAX {
                        let len = (row_i32 - hold_start_row[c]) as f32 / ROWS_PER_BEAT as f32;
                        out[start_idx][c] = hold_start_beat[c] + len;
                        hold_start_idx[c] = usize::MAX;
                    }
                }
                _ => {}
            }
        }
    }
    out
}

#[inline(always)]
fn parity_push_mine(
    g: &mut StepParityGenerator,
    counter: &mut RowCounter,
    col: usize,
    second: f32,
    fake: bool,
) {
    let bit = 1u8 << col;
    let mine_on = second != 0.0;
    let mine_i32_on = (second as i32) != 0;

    if second == counter.last_second && !g.rows.is_empty() {
        if fake {
            if mine_i32_on {
                counter.next_fake_mine_mask |= bit;
            } else {
                counter.next_fake_mine_mask &= !bit;
            }
        } else {
            if mine_on {
                counter.next_mine_mask |= bit;
            } else {
                counter.next_mine_mask &= !bit;
            }
            if mine_i32_on {
                counter.next_mine_i32_mask |= bit;
            } else {
                counter.next_mine_i32_mask &= !bit;
            }
        }
    } else if fake {
        if mine_i32_on {
            counter.fake_mine_mask |= bit;
        } else {
            counter.fake_mine_mask &= !bit;
        }
    } else {
        if mine_on {
            counter.mine_mask |= bit;
        } else {
            counter.mine_mask &= !bit;
        }
        if mine_i32_on {
            counter.mine_i32_mask |= bit;
        } else {
            counter.mine_i32_mask &= !bit;
        }
    }
}

#[inline(always)]
fn parity_push_note(
    g: &mut StepParityGenerator,
    counter: &mut RowCounter,
    col: usize,
    beat: f32,
    second: f32,
    hold_end: f32,
) {
    if counter.last_second != second {
        if counter.last_second != CLM_SECOND_INVALID {
            parity_flush_row(g, counter);
        }
        row_counter_reset(counter, second, beat);
    }
    counter.note_mask |= 1u8 << col;
    if hold_end != HOLD_END_NONE {
        counter.hold_ends[col] = hold_end;
    }
}

fn parity_create_rows_from_arrays<const LANES: usize>(
    g: &mut StepParityGenerator,
    rows: &[[u8; LANES]],
    row_to_beat: &[f32],
    timing: &TimingData,
    cols: usize,
) {
    let mut counter = row_counter_new();
    let hold_heads = hold_heads_from_arrays(rows, row_to_beat, cols);
    let copy_len = cols.min(LANES);

    for (idx, row) in rows.iter().enumerate() {
        if !has_obj(&row[..copy_len]) {
            continue;
        }
        let (row_i32, beat) = row_quantized(row_to_beat[idx]);
        let second = get_time_for_beat_f32(timing, f64::from(beat)) as f32;
        let row_fake = is_fake_at_beat(timing, f64::from(row_i32));

        for c in 0..copy_len {
            match row[c] {
                b'M' => parity_push_mine(g, &mut counter, c, second, row_fake),
                b'1' | b'K' | b'L' if !row_fake => {
                    parity_push_note(g, &mut counter, c, beat, second, HOLD_END_NONE)
                }
                b'2' | b'4' if !row_fake => {
                    let hold_end = hold_heads[idx][c];
                    if hold_end != HOLD_END_NONE {
                        parity_push_note(g, &mut counter, c, beat, second, hold_end);
                    }
                }
                _ => {}
            }
        }
    }
    parity_flush_row(g, &counter);
}

fn parity_analyze_rows<const LANES: usize>(
    g: &mut StepParityGenerator,
    rows: &[[u8; LANES]],
    row_to_beat: &[f32],
    timing: &TimingData,
    cols: usize,
) -> bool {
    parity_reset(g, cols);
    parity_create_rows_from_arrays(g, rows, row_to_beat, timing, cols);
    parity_finish(g)
}

fn parity_flush_row(g: &mut StepParityGenerator, counter: &RowCounter) {
    if counter.last_second == CLM_SECOND_INVALID {
        return;
    }
    let row = parity_build_row(g, counter);
    g.rows.push(row);
}

fn parity_build_row(g: &StepParityGenerator, counter: &RowCounter) -> Row {
    let mut row = row_new();
    row.second = counter.last_second;
    row.beat = counter.last_beat;
    row.note_mask = counter.note_mask;
    row.note_count = row.note_mask.count_ones() as u8;
    row.mine_mask = counter.next_mine_mask;
    row.mine_i32_mask = counter.next_mine_i32_mask;
    row.fake_mine_mask = counter.next_fake_mine_mask;
    row.hold_ends = counter.hold_ends;

    if let Some(prev) = g.rows.last() {
        for c in 0..g.column_count.min(MAX_COLUMNS) {
            let end = prev.hold_ends[c];
            if end >= row.beat && row.hold_ends[c] < 0.0 {
                row.hold_mask |= 1u8 << c;
                row.hold_ends[c] = end;
            }
        }
    }
    row
}

fn parity_add_node(g: &mut StepParityGenerator, state: State) -> usize {
    let idx = g.nodes.len();
    g.nodes.push(StepParityNode {
        state,
        pred: u32::MAX,
        cost: f32::MAX,
    });
    idx
}

fn parity_perms_for_row(g: &mut StepParityGenerator, row_idx: usize) -> &'static [FootPlacement] {
    let row = &g.rows[row_idx];
    let key = (row.note_mask | row.hold_mask) as usize;
    if let Some(perms) = g.perm_cache[key] {
        return perms;
    }

    let union = g.perm_table[key].as_ref();
    let perms = if union.is_empty() {
        let note = g.perm_table[row.note_mask as usize].as_ref();
        if note.is_empty() { &NO_PERMS } else { note }
    } else {
        union
    };

    g.perm_cache[key] = Some(perms);
    perms
}

fn parity_dp_rows(g: &mut StepParityGenerator) -> Option<usize> {
    let start_id = parity_add_node(g, state_new());
    g.nodes[start_id].cost = 0.0;

    g.prev_ids.clear();
    g.prev_ids.push(start_id);
    g.next_ids.clear();

    let mut prev_second = g.rows.first().map_or(-1.0, |r| r.second - 1.0);

    for i in 0..g.rows.len() {
        let row_second = g.rows[i].second;
        let hold_mask = g.rows[i].hold_mask;
        let elapsed = row_second - prev_second;
        let can_prune = elapsed >= 0.0;
        prev_second = row_second;
        let prev_row_has_live_hold = i > 0 && row_has_live_hold(&g.rows[i - 1]);

        let perms = parity_perms_for_row(g, i);
        let estimate = g.prev_ids.len().saturating_mul(perms.len());
        g.next_ids.clear();
        row_map_reset(&mut g.state_map, estimate);
        g.next_ids.reserve(estimate);

        for j in 0..g.prev_ids.len() {
            let init_id = g.prev_ids[j];
            let init_state = g.nodes[init_id].state;
            let init_cost = g.nodes[init_id].cost;
            let left_moved_not_holding = foot_moved_not_holding(&init_state, &LEFT_PAIR);
            let right_moved_not_holding = foot_moved_not_holding(&init_state, &RIGHT_PAIR);
            for perm in perms {
                let (result, hit, key) = parity_result_state(&init_state, perm, g.column_count, hold_mask);
                let res_id = if let Some(id) = row_map_get(&g.state_map, key) {
                    if can_prune && init_cost >= g.nodes[id].cost {
                        continue;
                    }
                    id
                } else {
                    let id = parity_add_node(g, result);
                    g.next_ids.push(id);
                    row_map_insert(&mut g.state_map, key, id);
                    id
                };
                let nc = init_cost
                    + calc_action_cost(
                        g.layout,
                        &init_state,
                        &result,
                        perm,
                        hit,
                        &g.rows,
                        i,
                        elapsed,
                        g.column_count,
                        left_moved_not_holding,
                        right_moved_not_holding,
                        prev_row_has_live_hold,
                    );
                let node = &mut g.nodes[res_id];
                if nc < node.cost {
                    node.cost = nc;
                    node.pred = init_id as u32;
                }
            }
        }

        std::mem::swap(&mut g.prev_ids, &mut g.next_ids);
    }

    g.prev_ids
        .iter()
        .copied()
        .min_by(|&a, &b| g.nodes[a].cost.total_cmp(&g.nodes[b].cost))
}

fn parity_result_state(
    initial: &State,
    cols: &FootPlacement,
    col_count: usize,
    hold_mask: u8,
) -> (State, [i8; NUM_FEET], u32) {
    let n = col_count;
    let (mut combined, mut hit) = ([Foot::None; MAX_COLUMNS], [INVALID_COLUMN; NUM_FEET]);
    let (mut moved_mask, mut holding_mask) = (0u8, 0u8);
    for i in 0..n {
        let foot = cols[i];
        if foot == Foot::None {
            continue;
        }
        combined[i] = foot;
        let fi = foot_idx(foot);
        hit[fi] = i as i8;
        let fm = FOOT_MASKS[fi];
        let bit = 1u8 << i;
        if (hold_mask & bit) != 0 {
            holding_mask |= fm;
        }
        if (hold_mask & bit) == 0 || initial.combined_columns[i] != foot {
            moved_mask |= fm;
        }
    }

    let (moved_left, moved_right) =
        ((moved_mask & LEFT_FOOT_MASK) != 0, (moved_mask & RIGHT_FOOT_MASK) != 0);
    let (mut where_the_feet_are, mut comb_p, mut occupied_mask) =
        ([INVALID_COLUMN; NUM_FEET], 0u32, 0u8);
    for (i, slot) in combined.iter_mut().enumerate().take(n) {
        let foot = if *slot == Foot::None {
            let prev = initial.combined_columns[i];
            match prev {
                Foot::LeftHeel | Foot::RightHeel
                    if (moved_mask & FOOT_MASKS[foot_idx(prev)]) == 0 => prev,
                Foot::LeftToe if !moved_left => prev,
                Foot::RightToe if !moved_right => prev,
                _ => Foot::None,
            }
        } else {
            *slot
        };
        *slot = foot;
        comb_p |= (foot as u32) << (i * 3);
        if foot != Foot::None {
            where_the_feet_are[foot_idx(foot)] = i as i8;
            occupied_mask |= 1u8 << i;
        }
    }

    let key = comb_p | (u32::from(moved_mask) << 24) | (u32::from(holding_mask) << 28);
    (
        State {
            combined_columns: combined,
            where_the_feet_are,
            occupied_mask,
            moved_mask,
            holding_mask,
        },
        hit,
        key,
    )
}

fn parity_backtrack(g: &mut StepParityGenerator, mut cur: usize) -> bool {
    let rows = g.rows.len();
    g.result_columns.clear();
    g.result_columns.resize(rows, [Foot::None; MAX_COLUMNS]);

    let mut write = rows;
    while write != 0 {
        write -= 1;
        g.result_columns[write] = g.nodes[cur].state.combined_columns;
        let prev = g.nodes[cur].pred;
        if prev == u32::MAX {
            return false;
        }
        cur = prev as usize;
    }

    cur == 0
}

// --- RowCounter ---

struct RowCounter {
    note_mask: u8,
    hold_ends: [f32; MAX_COLUMNS],
    mine_mask: u8,
    mine_i32_mask: u8,
    fake_mine_mask: u8,
    next_mine_mask: u8,
    next_mine_i32_mask: u8,
    next_fake_mine_mask: u8,
    last_second: f32,
    last_beat: f32,
}

const fn row_counter_new() -> RowCounter {
    RowCounter {
        note_mask: 0,
        hold_ends: [HOLD_END_NONE; MAX_COLUMNS],
        mine_mask: 0,
        mine_i32_mask: 0,
        fake_mine_mask: 0,
        next_mine_mask: 0,
        next_mine_i32_mask: 0,
        next_fake_mine_mask: 0,
        last_second: CLM_SECOND_INVALID,
        last_beat: CLM_SECOND_INVALID,
    }
}

fn row_counter_reset(c: &mut RowCounter, second: f32, beat: f32) {
    c.last_second = second;
    c.last_beat = beat;
    c.next_mine_mask = c.mine_mask;
    c.next_mine_i32_mask = c.mine_i32_mask;
    c.next_fake_mine_mask = c.fake_mine_mask;
    c.note_mask = 0;
    c.hold_ends.fill(HOLD_END_NONE);
    c.mine_mask = 0;
    c.mine_i32_mask = 0;
    c.fake_mine_mask = 0;
}

// --- Permutation ---

fn permute_row(
    layout: &StageLayout,
    mask: u8,
    cols: &mut FootPlacement,
    col: usize,
    col_count: usize,
    used: u8,
    out: &mut Vec<FootPlacement>,
) {
    if col >= col_count {
        let (mut lh, mut lt, mut rh, mut rt) = (
            INVALID_COLUMN,
            INVALID_COLUMN,
            INVALID_COLUMN,
            INVALID_COLUMN,
        );
        for (i, &f) in cols.iter().enumerate().take(col_count) {
            match f {
                Foot::LeftHeel => lh = i as i8,
                Foot::LeftToe => lt = i as i8,
                Foot::RightHeel => rh = i as i8,
                Foot::RightToe => rt = i as i8,
                Foot::None => {}
            }
        }

        // Toe without heel check
        if (lh == INVALID_COLUMN && lt != INVALID_COLUMN)
            || (rh == INVALID_COLUMN && rt != INVALID_COLUMN)
        {
            return;
        }

        // Bracket distance check
        if lh != INVALID_COLUMN
            && lt != INVALID_COLUMN
            && !layout_bracket_ok(layout, lh as usize, lt as usize)
        {
            return;
        }
        if rh != INVALID_COLUMN
            && rt != INVALID_COLUMN
            && !layout_bracket_ok(layout, rh as usize, rt as usize)
        {
            return;
        }

        out.push(*cols);
        return;
    }

    let active = (mask & (1u8 << col)) != 0;

	    if active {
	        for &foot in &FEET {
	            let fm = FOOT_MASKS[foot_idx(foot)];
	            if used & fm != 0 {
	                continue;
	            }
            cols[col] = foot;
            permute_row(
                layout,
                mask,
                cols,
                col + 1,
                col_count,
                used | fm,
                out,
            );
            cols[col] = Foot::None;
        }
    } else {
        permute_row(
            layout,
            mask,
            cols,
            col + 1,
            col_count,
            used,
            out,
        );
    }
}

// --- Tech Counts ---

#[derive(Default, Clone, Copy, PartialEq, Eq, Debug)]
pub struct TechCounts {
    pub crossovers: u32,
    pub half_crossovers: u32,
    pub full_crossovers: u32,
    pub footswitches: u32,
    pub up_footswitches: u32,
    pub down_footswitches: u32,
    pub sideswitches: u32,
    pub jacks: u32,
    pub brackets: u32,
    pub doublesteps: u32,
}

fn calculate_tech_counts(rows: &[Row], placements: &[FootPlacement], layout: &StageLayout) -> TechCounts {
    let mut out = TechCounts::default();
    if rows.len() < 2 || placements.len() != rows.len() {
        return out;
    }

    let cols = layout_cols(layout).min(MAX_COLUMNS);

    let hit_positions = |combined: &FootPlacement, mask: u8| -> [i8; NUM_FEET] {
        let mut pos = [INVALID_COLUMN; NUM_FEET];
        let mut m = mask;
        while m != 0 {
            let c = m.trailing_zeros() as usize;
            m &= m - 1;
            if c >= cols {
                continue;
            }
            let foot = combined[c];
            if foot != Foot::None {
                pos[foot_idx(foot)] = c as i8;
            }
        }
        pos
    };

    let col_foot = |combined: &FootPlacement, mask: u8, c: usize| -> Foot {
        if (mask & (1u8 << c)) != 0 {
            combined[c]
        } else {
            Foot::None
        }
    };

    for i in 1..rows.len() {
        let (curr, prev) = (&rows[i], &rows[i - 1]);
        let (curr_combined, prev_combined) = (&placements[i], &placements[i - 1]);
        let elapsed = curr.second - prev.second;

        let curr_pos = hit_positions(curr_combined, curr.note_mask);
        let prev_pos = hit_positions(prev_combined, prev.note_mask);

        // Jacks and doublesteps
        if curr.note_count == 1 && prev.note_count == 1 {
            for &foot in &FEET {
                let (cc, pc) = (curr_pos[foot_idx(foot)], prev_pos[foot_idx(foot)]);
                if cc == INVALID_COLUMN || pc == INVALID_COLUMN {
                    continue;
                }
                if cc == pc && elapsed < JACK_CUTOFF {
                    out.jacks += 1;
                } else if cc != pc && elapsed < DOUBLESTEP_CUTOFF {
                    out.doublesteps += 1;
                }
            }
        }

        // Brackets
        if curr.note_count >= 2 {
            if curr_pos[1] != INVALID_COLUMN && curr_pos[2] != INVALID_COLUMN {
                out.brackets += 1;
            }
            if curr_pos[3] != INVALID_COLUMN && curr_pos[4] != INVALID_COLUMN {
                out.brackets += 1;
            }
        }

        // Footswitches by arrow type
        let is_switch = |c: usize| -> bool {
            let (p, r) = (
                col_foot(prev_combined, prev.note_mask, c),
                col_foot(curr_combined, curr.note_mask, c),
            );
            p != Foot::None
                && r != Foot::None
                && p != r
                && OTHER_PART_OF_FOOT[foot_idx(p)] != r
                && elapsed < FOOTSWITCH_CUTOFF
        };

        let mut mask = layout.up_mask;
        while mask != 0 {
            let c = mask.trailing_zeros() as usize;
            mask &= mask - 1;
            if is_switch(c) {
                out.up_footswitches += 1;
                out.footswitches += 1;
            }
        }
        mask = layout.down_mask;
        while mask != 0 {
            let c = mask.trailing_zeros() as usize;
            mask &= mask - 1;
            if is_switch(c) {
                out.down_footswitches += 1;
                out.footswitches += 1;
            }
        }
        mask = layout.side_mask;
        while mask != 0 {
            let c = mask.trailing_zeros() as usize;
            mask &= mask - 1;
            if is_switch(c) {
                out.sideswitches += 1;
            }
        }

        // Crossovers - restored original logic with prev_prev checks
        let left_heel = curr_pos[foot_idx(Foot::LeftHeel)];
        let left_toe = curr_pos[foot_idx(Foot::LeftToe)];
        let right_heel = curr_pos[foot_idx(Foot::RightHeel)];
        let right_toe = curr_pos[foot_idx(Foot::RightToe)];

        let prev_left_heel = prev_pos[foot_idx(Foot::LeftHeel)];
        let prev_left_toe = prev_pos[foot_idx(Foot::LeftToe)];
        let prev_right_heel = prev_pos[foot_idx(Foot::RightHeel)];
        let prev_right_toe = prev_pos[foot_idx(Foot::RightToe)];

        // Right foot crossing over left
        if right_heel != INVALID_COLUMN
            && prev_left_heel != INVALID_COLUMN
            && prev_right_heel == INVALID_COLUMN
        {
            let left_pos = layout_avg_point(layout, prev_left_heel, prev_left_toe);
            let right_pos = layout_avg_point(layout, right_heel, right_toe);
            if right_pos.x < left_pos.x {
                if i > 1 {
                    let prev_prev = &rows[i - 2];
                    let prev_prev_pos = hit_positions(&placements[i - 2], prev_prev.note_mask);
                    let prev_prev_rh = prev_prev_pos[foot_idx(Foot::RightHeel)];
                    if prev_prev_rh != INVALID_COLUMN && prev_prev_rh != right_heel {
                        let prev_prev_pos = layout.columns[prev_prev_rh as usize];
                        if prev_prev_pos.x > left_pos.x {
                            out.full_crossovers += 1;
                        } else {
                            out.half_crossovers += 1;
                        }
                        out.crossovers += 1;
                    }
                } else {
                    out.half_crossovers += 1;
                    out.crossovers += 1;
                }
            }
        // Left foot crossing over right
        } else if left_heel != INVALID_COLUMN
            && prev_right_heel != INVALID_COLUMN
            && prev_left_heel == INVALID_COLUMN
        {
            let left_pos = layout_avg_point(layout, left_heel, left_toe);
            let right_pos = layout_avg_point(layout, prev_right_heel, prev_right_toe);
            if right_pos.x < left_pos.x {
                if i > 1 {
                    let prev_prev = &rows[i - 2];
                    let prev_prev_pos = hit_positions(&placements[i - 2], prev_prev.note_mask);
                    let prev_prev_lh = prev_prev_pos[foot_idx(Foot::LeftHeel)];
                    if prev_prev_lh != INVALID_COLUMN && prev_prev_lh != left_heel {
                        let prev_prev_pos = layout.columns[prev_prev_lh as usize];
                        if right_pos.x > prev_prev_pos.x {
                            out.full_crossovers += 1;
                        } else {
                            out.half_crossovers += 1;
                        }
                        out.crossovers += 1;
                    }
                } else {
                    out.half_crossovers += 1;
                    out.crossovers += 1;
                }
            }
        }
    }
    out
}

// --- Parsing ---

#[derive(Clone)]
struct ParsedRow {
    chars: [u8; 8],
    columns: u8,
    row: i32,
    beat: f32,
    second: f32,
}

struct LayoutCache {
    layout: StageLayout,
    perm_table: [Box<[FootPlacement]>; 256],
}

fn layout_cache_new(layout: StageLayout) -> LayoutCache {
    let perm_table = build_perm_table(&layout);
    LayoutCache { layout, perm_table }
}

fn build_perm_table(layout: &StageLayout) -> [Box<[FootPlacement]>; 256] {
    let col_count = layout_cols(layout);
    std::array::from_fn(|mask| {
        let mask = mask as u8;
        let bits = mask.count_ones() as usize;
        if bits > 4 {
            return Vec::new().into_boxed_slice();
        }

        let mut cols = [Foot::None; MAX_COLUMNS];
        let mut perms = Vec::with_capacity(PERM_CAP[bits]);
        permute_row(layout, mask, &mut cols, 0, col_count, 0, &mut perms);
        perms.into_boxed_slice()
    })
}

fn dance_single_cache() -> &'static LayoutCache {
    static CACHE: OnceLock<LayoutCache> = OnceLock::new();
    CACHE.get_or_init(|| layout_cache_new(dance_single_layout()))
}

fn dance_double_cache() -> &'static LayoutCache {
    static CACHE: OnceLock<LayoutCache> = OnceLock::new();
    CACHE.get_or_init(|| layout_cache_new(dance_double_layout()))
}

fn layout_for_lanes(lanes: usize) -> Option<&'static LayoutCache> {
    match lanes {
        4 => Some(dance_single_cache()),
        8 => Some(dance_double_cache()),
        _ => None,
    }
}

#[inline(always)]
const fn trim_ws(mut s: &[u8]) -> &[u8] {
    while let Some((&f, r)) = s.split_first() {
        if f.is_ascii_whitespace() {
            s = r;
        } else {
            break;
        }
    }
    while let Some((&l, r)) = s.split_last() {
        if l.is_ascii_whitespace() {
            s = r;
        } else {
            break;
        }
    }
    s
}

#[inline(always)]
fn has_obj(line: &[u8]) -> bool {
    line.iter().any(|&b| b != b'0')
}

fn parse_rows<F>(data: &[u8], cols: usize, mut get_second: F) -> Vec<ParsedRow>
where
    F: FnMut(f32) -> f32,
{
    let mut rows = Vec::new();
    if cols == 0 || cols > 8 {
        return rows;
    }

    let mut measure_idx = 0usize;
    let mut lines: Vec<&[u8]> = Vec::new();
    for measure in data.split(|&b| b == b',') {
        lines.clear();
        for line in measure.split(|&b| b == b'\n') {
            let line = trim_ws(line);
            if !line.is_empty() {
                lines.push(line);
            }
        }
        if lines.is_empty() {
            measure_idx += 1;
            continue;
        }

        let num = lines.len();
        let start = measure_idx as f32 * 4.0;
        let step = 4.0 / num as f32;

        for (j, &line) in lines.iter().enumerate() {
            let copy = line.len().min(cols);
            if !has_obj(&line[..copy]) {
                continue;
            }

            let beat = (j as f32).mul_add(step, start);
            let note_row = beat_to_note_row_f32(beat);
            let beat = note_row as f32 / ROWS_PER_BEAT as f32;
            let second = get_second(beat);

            let mut chars = [b'0'; 8];
            chars[..copy].copy_from_slice(&line[..copy]);
            rows.push(ParsedRow {
                chars,
                columns: cols as u8,
                row: note_row,
                beat,
                second,
            });
        }
        measure_idx += 1;
    }
    rows
}

#[inline(always)]
fn invalidate_hold(
    notes: &mut Vec<IntermediateNoteData>,
    hold_idx: &mut [usize; MAX_COLUMNS],
    col: usize,
) {
    let idx = hold_idx[col];
    if idx != usize::MAX {
        notes[idx].note_type = TapNoteType::Empty;
        hold_idx[col] = usize::MAX;
        if idx + 1 == notes.len() {
            while notes
                .last()
                .is_some_and(|n| n.note_type == TapNoteType::Empty)
            {
                notes.pop();
            }
        }
    }
}

#[inline(always)]
const fn note_new(
    note_type: TapNoteType,
    col: usize,
    beat: f32,
    second: f32,
    hold_length: f32,
    fake: bool,
) -> IntermediateNoteData {
    IntermediateNoteData {
        note_type,
        col,
        beat,
        hold_length,
        fake,
        second,
    }
}

#[inline(always)]
fn parse_note_char(
    notes: &mut Vec<IntermediateNoteData>,
    hold_idx: &mut [usize; MAX_COLUMNS],
    hold_row: &mut [i32; MAX_COLUMNS],
    ch: u8,
    col: usize,
    row_i32: i32,
    beat: f32,
    second: f32,
    row_fake: bool,
) {
    if matches!(ch, b'1' | b'M' | b'L' | b'F') {
        invalidate_hold(notes, hold_idx, col);
    }
    match ch {
        b'2' | b'4' => {
            invalidate_hold(notes, hold_idx, col);
            hold_idx[col] = notes.len();
            hold_row[col] = row_i32;
            notes.push(note_new(
                TapNoteType::HoldHead,
                col,
                beat,
                second,
                MISSING_HOLD_LENGTH_BEATS,
                row_fake,
            ));
        }
        b'3' => {
            let idx = hold_idx[col];
            if idx != usize::MAX {
                notes[idx].hold_length = (row_i32 - hold_row[col]) as f32 / ROWS_PER_BEAT as f32;
                hold_idx[col] = usize::MAX;
            }
        }
        b'1' | b'K' | b'L' => notes.push(note_new(
            TapNoteType::Tap,
            col,
            beat,
            second,
            0.0,
            row_fake,
        )),
        b'M' => notes.push(note_new(
            TapNoteType::Mine,
            col,
            beat,
            second,
            0.0,
            row_fake,
        )),
        b'F' => notes.push(note_new(
            TapNoteType::Fake,
            col,
            beat,
            second,
            0.0,
            true,
        )),
        _ => {}
    }
}

fn build_notes(rows: &[ParsedRow], timing: Option<&TimingData>) -> Vec<IntermediateNoteData> {
    let cols = rows.first().map_or(0, |r| r.columns as usize);
    if cols == 0 {
        return Vec::new();
    }

    let mut hold_idx = [usize::MAX; MAX_COLUMNS];
    let mut hold_row = [0i32; MAX_COLUMNS];
    let mut notes: Vec<IntermediateNoteData> = Vec::with_capacity(rows.len());

    for row in rows {
        let row_fake = timing.is_some_and(|t| is_fake_at_beat(t, f64::from(row.row)));

        for c in 0..cols {
            parse_note_char(
                &mut notes,
                &mut hold_idx,
                &mut hold_row,
                row.chars[c],
                c,
                row.row,
                row.beat,
                row.second,
                row_fake,
            );
        }
    }

    for col in 0..cols {
        invalidate_hold(&mut notes, &mut hold_idx, col);
    }
    notes
}

// --- Public API ---

fn analyze_core<F>(
    cache: &'static LayoutCache,
    data: &[u8],
    cols: usize,
    timing: Option<&TimingData>,
    get_second: F,
) -> TechCounts
where
    F: FnMut(f32) -> f32,
{
    let rows = parse_rows(data, cols, get_second);
    let notes = build_notes(&rows, timing);

    let mut generator = parity_gen(cache);
    if !parity_analyze(&mut generator, notes, cols) {
        return TechCounts::default();
    }
    calculate_tech_counts(&generator.rows, &generator.result_columns, generator.layout)
}

#[must_use] 
pub fn analyze_lanes(
    minimized_note_data: &[u8],
    bpm_map: &[(f64, f64)],
    offset: f64,
    lanes: usize,
) -> TechCounts {
    let Some(cache) = layout_for_lanes(lanes) else {
        return TechCounts::default();
    };

    let cols = layout_cols(&cache.layout);
    debug_assert!(!minimized_note_data.contains(&b';'));
    analyze_core(cache, minimized_note_data, cols, None, |beat| {
        time_between_beats(0.0, beat, bpm_map) as f32 - offset as f32
    })
}

#[must_use] 
pub fn analyze_timing_lanes(minimized_note_data: &[u8], timing: &TimingData, lanes: usize) -> TechCounts {
    let Some(cache) = layout_for_lanes(lanes) else {
        return TechCounts::default();
    };

    let cols = layout_cols(&cache.layout);
    debug_assert!(!minimized_note_data.contains(&b';'));
	    analyze_core(cache, minimized_note_data, cols, Some(timing), |beat| {
	        get_time_for_beat_f32(timing, f64::from(beat)) as f32
	    })
}

pub(crate) struct TimingRowsScratch<const LANES: usize> {
    generator: StepParityGenerator,
}

pub(crate) fn timing_rows_scratch<const LANES: usize>() -> Option<TimingRowsScratch<LANES>> {
    let cache = layout_for_lanes(LANES)?;
    Some(TimingRowsScratch {
        generator: parity_gen(cache),
    })
}

pub(crate) fn analyze_timing_rows<const LANES: usize>(
    rows: &[[u8; LANES]],
    row_to_beat: &[f32],
    timing: &TimingData,
    scratch: &mut TimingRowsScratch<LANES>,
) -> TechCounts {
    let cols = layout_cols(scratch.generator.layout);
    if !parity_analyze_rows(&mut scratch.generator, rows, row_to_beat, timing, cols) {
        return TechCounts::default();
    }
    calculate_tech_counts(
        &scratch.generator.rows,
        &scratch.generator.result_columns,
        scratch.generator.layout,
    )
}

fn time_between_beats(start: f32, end: f32, bpm_map: &[(f64, f64)]) -> f64 {
    if end <= start {
        return 0.0;
    }
    let mut bpm = bpm_map.first().map_or(60.0, |b| b.1);
    let mut time = 0.0;
    let mut last = f64::from(start);

    for &(beat, value) in bpm_map {
        if beat <= last {
            bpm = value;
            continue;
        }
        if beat >= f64::from(end) {
            break;
        }
        time += (beat - last) * 60.0 / bpm;
        last = beat;
        bpm = value;
    }
    time + (f64::from(end) - last) * 60.0 / bpm
}

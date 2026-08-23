use std::sync::OnceLock;

use crate::timing::{
    BeatTimeCursorF32, FakeRowCursor, FixedTimingParts, ROWS_PER_BEAT, TimingData,
    beat_to_note_row_f32, fakes as timing_fakes, fixed_timing_parts, get_time_for_beat_f32,
};

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
const fn foot_side(f: Foot) -> u8 {
    (f as u8 + 1) >> 1
}

const NUM_FEET: usize = 5;
const MAX_COLUMNS: usize = 8;
const PAIR_STRIDE: usize = MAX_COLUMNS + 1;
const PAIR_LEN: usize = PAIR_STRIDE * PAIR_STRIDE;
const DIST_LEN: usize = MAX_COLUMNS * MAX_COLUMNS;
const SINGLE_COLS: usize = 4;
const FEET: [Foot; 4] = [
    Foot::LeftHeel,
    Foot::LeftToe,
    Foot::RightHeel,
    Foot::RightToe,
];
const TAP_FEET: [Foot; 2] = [Foot::LeftHeel, Foot::RightHeel];
const IDLE_FEET: [Foot; 1] = [Foot::None];
const FOOT_FROM_KEY_BITS: [Foot; 8] = [
    Foot::None,
    Foot::LeftHeel,
    Foot::LeftToe,
    Foot::RightHeel,
    Foot::RightToe,
    Foot::None,
    Foot::None,
    Foot::None,
];
const FOOT_MASKS: [u8; NUM_FEET] = [0, 1, 2, 4, 8];
const OTHER_FOOT_IDXS: [usize; NUM_FEET] = [0, 2, 1, 4, 3];
const LEFT_FOOT_MASK: u8 = FOOT_MASKS[1] | FOOT_MASKS[2];
const RIGHT_FOOT_MASK: u8 = FOOT_MASKS[3] | FOOT_MASKS[4];
// Exact permutation totals for the fixed dance-single and dance-double layouts.
const PERM_TOTALS: [usize; 9] = [0, 0, 0, 0, 85, 0, 0, 0, 517];
#[cfg(feature = "bench-support")]
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

const ROW_MAP_MIN_CAP: usize = 256;

struct RowStateMap {
    entries: Vec<RowMapEntry>,
    epoch: u32,
    mask: usize,
    #[cfg(feature = "bench-support")]
    legacy_hash: bool,
}

#[derive(Clone, Copy, Default)]
struct RowMapEntry {
    key: u32,
    meta: u32,
}

const fn row_map_hash(x: u32) -> usize {
    // 0x9E3779B9 is the 32-bit golden ratio prime
    x.wrapping_mul(0x9E3779B9) as usize
}

// Dance-single has at most 625 states per layer; dance-double can encode at
// most 868,608 moved/holding placements. The narrower single value leaves a
// 22-bit epoch, avoiding periodic table clears on long charts.
const fn row_map_val_bits<const COLS: usize, const LAYER_LOCAL: bool>() -> u32 {
    if COLS == 4 && LAYER_LOCAL { 10 } else { 20 }
}

const fn row_map_val_mask<const COLS: usize, const LAYER_LOCAL: bool>() -> u32 {
    (1 << row_map_val_bits::<COLS, LAYER_LOCAL>()) - 1
}

const fn row_map_epoch_max<const COLS: usize, const LAYER_LOCAL: bool>() -> u32 {
    (1 << (u32::BITS - row_map_val_bits::<COLS, LAYER_LOCAL>())) - 1
}

#[derive(Clone, Copy)]
enum RowMapProbe {
    Found(usize),
    Vacant(usize),
}

#[inline(always)]
const fn row_map_hash_for_key<const COLS: usize>(key: u32) -> usize {
    let hash = row_map_hash(key);
    if COLS == SINGLE_COLS {
        // The minimum table mask selects the low product byte. Fold the upper
        // byte into it so column 4 and moved/holding flags affect every size.
        hash ^ (hash >> 24)
    } else if COLS == MAX_COLUMNS && key >> 28 == 0 {
        // Dance-double column bits extend above the low bits selected by the
        // power-of-two table. Fold them down for states without active holds.
        hash ^ (hash >> 16)
    } else {
        hash
    }
}

#[cfg(feature = "bench-support")]
const fn row_map_hash_legacy<const COLS: usize>(key: u32) -> usize {
    let hash = row_map_hash(key);
    if COLS == MAX_COLUMNS && key >> 28 == 0 {
        hash ^ (hash >> 16)
    } else {
        hash
    }
}

const fn row_map_new() -> RowStateMap {
    RowStateMap {
        entries: Vec::new(),
        epoch: 1,
        mask: 0,
        #[cfg(feature = "bench-support")]
        legacy_hash: false,
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

fn row_map_reset<const COLS: usize, const LAYER_LOCAL: bool>(
    map: &mut RowStateMap,
    expected: usize,
) {
    let need = row_map_cap(expected);
    if need > map.entries.len() {
        map.entries.resize(need, RowMapEntry::default());
        map.mask = need - 1;
    }
    map.epoch += 1;
    if map.epoch > row_map_epoch_max::<COLS, LAYER_LOCAL>() {
        for entry in &mut map.entries {
            entry.meta = 0;
        }
        map.epoch = 1;
    }
}

#[inline(always)]
fn row_map_probe<const COLS: usize, const LAYER_LOCAL: bool>(
    map: &RowStateMap,
    key: u32,
) -> RowMapProbe {
    debug_assert!(map.mask != 0);
    #[cfg(feature = "bench-support")]
    let hash = if map.legacy_hash {
        row_map_hash_legacy::<COLS>(key)
    } else {
        row_map_hash_for_key::<COLS>(key)
    };
    #[cfg(not(feature = "bench-support"))]
    let hash = row_map_hash_for_key::<COLS>(key);
    let mut idx = hash & map.mask;
    loop {
        let entry = &map.entries[idx];
        let meta = entry.meta;
        if meta >> row_map_val_bits::<COLS, LAYER_LOCAL>() != map.epoch {
            return RowMapProbe::Vacant(idx);
        }
        if entry.key == key {
            return RowMapProbe::Found((meta & row_map_val_mask::<COLS, LAYER_LOCAL>()) as usize);
        }
        idx = (idx + 1) & map.mask;
    }
}

#[inline(always)]
fn row_map_insert_at<const COLS: usize, const LAYER_LOCAL: bool>(
    map: &mut RowStateMap,
    idx: usize,
    key: u32,
    val: usize,
) {
    debug_assert!(idx < map.entries.len());
    debug_assert!(val <= row_map_val_mask::<COLS, LAYER_LOCAL>() as usize);
    let entry = &mut map.entries[idx];
    entry.key = key;
    entry.meta = (map.epoch << row_map_val_bits::<COLS, LAYER_LOCAL>()) | val as u32;
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
    movement_costs: [f32; DIST_LEN],
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

    // ITGmania calls C++ pow on float inputs and narrows back to float.
    // Matching that order keeps path costs stable on exact-tie charts.
    let facing_penalty = |v: f32| -> f32 {
        let base = -(v.min(0.0));
        if base > 0.0 {
            ((base as f64).powf(1.8) * 100.0) as f32
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
                    x: (l.x + r.x) / 2.0,
                    y: (l.y + r.y) / 2.0,
                },
            };

            if left == right || left == invalid || right == invalid {
                continue;
            }

            let (dx, dy) = (
                columns[right].x - columns[left].x,
                columns[right].y - columns[left].y,
            );
            let dist = (dx * dx + dy * dy).sqrt();
            if dist == 0.0 {
                continue;
            }

            let (ndx, ndy) = (dx / dist, dy / dist);
            let (mut xm, mut ym) = ((ndx as f64).powf(4.0) as f32, (ndy as f64).powf(4.0) as f32);
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
    let mut movement_costs = [0.0f32; DIST_LEN];

    for l in 0..cols {
        for r in 0..cols {
            let (dx, dy) = (columns[l].x - columns[r].x, columns[l].y - columns[r].y);
            let sq = dx * dx + dy * dy;
            let idx = l * MAX_COLUMNS + r;
            bracket_ok[idx] = sq <= 2.0;
            let dist = sq.sqrt();
            hold_switch_cost[idx] = dist * HOLDSWITCH_WEIGHT;
            movement_costs[idx] = dist * DISTANCE_WEIGHT;
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
        movement_costs,
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
const fn layout_movement_cost(layout: &StageLayout, c1: usize, c2: usize) -> f32 {
    layout.movement_costs[c1 * MAX_COLUMNS + c2]
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
    Lift,
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

#[derive(Debug, Clone, Copy)]
struct Row {
    second: f32,
    beat: f32,
    note_count: u8,
    note_mask: u8,
    tech_mask: u8,
    hold_mask: u8,
    mine_mask: u8,
    mine_i32_mask: u8,
    fake_mine_mask: u8,
    // Derived while rows are built so the DP never rescans hold endpoints.
    has_live_hold: bool,
}

#[derive(Clone, Copy)]
struct RowCostCtx {
    active_mask: u8,
    mine_mask: u8,
    side_mask: u8,
    multi_active: bool,
    has_hold: bool,
}

struct TapCostCtx {
    footswitch: f32,
    jack: f32,
    movement: [f32; SINGLE_COLS],
}

const fn row_new() -> Row {
    Row {
        second: 0.0,
        beat: 0.0,
        note_count: 0,
        note_mask: 0,
        tech_mask: 0,
        hold_mask: 0,
        mine_mask: 0,
        mine_i32_mask: 0,
        fake_mine_mask: 0,
        has_live_hold: false,
    }
}

#[inline(always)]
const fn row_cost_ctx(row: &Row, layout: &StageLayout) -> RowCostCtx {
    let active_mask = row.note_mask | row.hold_mask;
    let mine_mask = row.mine_mask | row.fake_mine_mask;
    RowCostCtx {
        active_mask,
        mine_mask,
        side_mask: active_mask & layout.side_mask,
        multi_active: active_mask.count_ones() > 1,
        has_hold: row.hold_mask != 0,
    }
}

fn tap_cost_ctx(layout: &StageLayout, hit_col: usize, elapsed: f32) -> TapCostCtx {
    let footswitch = if (SLOW_FOOTSWITCH_THRESHOLD..SLOW_FOOTSWITCH_IGNORE).contains(&elapsed) {
        let scaled = elapsed - SLOW_FOOTSWITCH_THRESHOLD;
        (scaled / (SLOW_FOOTSWITCH_THRESHOLD + scaled)) * FOOTSWITCH_WEIGHT
    } else {
        0.0
    };
    let jack = if elapsed < JACK_THRESHOLD {
        let scaled = JACK_THRESHOLD - elapsed;
        if scaled > 0.0 {
            (1.0 / scaled - 1.0 / JACK_THRESHOLD) * JACK_WEIGHT
        } else {
            0.0
        }
    } else {
        0.0
    };
    // The target column is fixed for the row, so only its four possible source
    // columns need divisions. Every candidate reuses these exact results.
    let mut movement = [0.0; SINGLE_COLS];
    if hit_col < SINGLE_COLS {
        for (from, cost) in movement.iter_mut().enumerate() {
            *cost = layout_movement_cost(layout, from, hit_col) / elapsed;
        }
    }
    TapCostCtx {
        footswitch,
        jack,
        movement,
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
        // ITGmania's default State zero-initializes this array even before any
        // foot is placed. That affects the first movement cost and path ties.
        where_the_feet_are: [0; NUM_FEET],
        occupied_mask: 0,
        moved_mask: 0,
        holding_mask: 0,
    }
}

#[derive(Clone, Copy)]
struct StateBase4 {
    combined_columns: [Foot; 4],
    where_the_feet_are: [i8; NUM_FEET],
    occupied_mask: u8,
}

const START_BASE4: StateBase4 = StateBase4 {
    combined_columns: [Foot::None; 4],
    where_the_feet_are: [0; NUM_FEET],
    occupied_mask: 0,
};

const SINGLE_STATE_COUNT: usize = 1 << 12;
const SINGLE_LAYER_MAX: usize = 625;
const SINGLE_STATE_MASK: u32 = 0xff00_0fff;
const SINGLE_PRED_SHIFT: u32 = 12;
const SINGLE_PRED_MASK: u32 = 0x0fff;
// Compile-time, process-lifetime dance-single decode table. It is immutable and
// shared lock-free, has exactly 4,096 entries, never misses or evicts, performs
// no warmup/allocation/destruction work, and makes every lookup constant-time.
static STATE_BASE4: [StateBase4; SINGLE_STATE_COUNT] = {
    const EMPTY: StateBase4 = StateBase4 {
        combined_columns: [Foot::None; 4],
        where_the_feet_are: [INVALID_COLUMN; NUM_FEET],
        occupied_mask: 0,
    };

    let mut table = [EMPTY; SINGLE_STATE_COUNT];
    let mut key = 0usize;
    while key < SINGLE_STATE_COUNT {
        let mut entry = EMPTY;
        let mut column = 0usize;
        while column < 4 {
            let foot = FOOT_FROM_KEY_BITS[(key >> (column * 3)) & 0b111];
            entry.combined_columns[column] = foot;
            if foot as u8 != Foot::None as u8 {
                entry.where_the_feet_are[foot_idx(foot)] = column as i8;
                entry.occupied_mask |= 1u8 << column;
            }
            column += 1;
        }
        table[key] = entry;
        key += 1;
    }
    table
};

const SINGLE_TAP_COUNT: usize = SINGLE_STATE_COUNT * 16;
// Compile-time, process-lifetime single-tap transition table. Its 65,536 u16
// entries are immutable and shared lock-free, with no warmup, miss, allocation,
// eviction, or destruction work. A lookup replaces the repeated packed-state
// clears required by every dance-single tap candidate.
static TAP_BASE4: [u16; SINGLE_TAP_COUNT] = {
    let mut table = [0u16; SINGLE_TAP_COUNT];
    let mut key = 0usize;
    while key < SINGLE_STATE_COUNT {
        let mut column = 0usize;
        while column < 4 {
            let mut foot_index = 0usize;
            while foot_index < FEET.len() {
                let foot = FEET[foot_index];
                let fi = foot_idx(foot);
                let mut combined = key as u32 & !(0b111 << (column * 3));
                let previous_col = STATE_BASE4[key].where_the_feet_are[fi];
                if previous_col != INVALID_COLUMN {
                    combined &= !(0b111 << (previous_col as usize * 3));
                }
                if foot as u8 == Foot::LeftHeel as u8 || foot as u8 == Foot::RightHeel as u8 {
                    let toe_col = STATE_BASE4[key].where_the_feet_are[fi + 1];
                    if toe_col != INVALID_COLUMN {
                        combined &= !(0b111 << (toe_col as usize * 3));
                    }
                }
                combined |= (foot as u32) << (column * 3);
                table[key * 16 + column * 4 + foot_index] = combined as u16;
                foot_index += 1;
            }
            column += 1;
        }
        key += 1;
    }
    table
};

#[inline(always)]
fn state_from_key<const COLS: usize>(key: u32) -> State {
    if COLS == 4 {
        let base = STATE_BASE4[key as usize & (SINGLE_STATE_COUNT - 1)];
        let mut combined_columns = [Foot::None; MAX_COLUMNS];
        combined_columns[..4].copy_from_slice(&base.combined_columns);
        return State {
            combined_columns,
            where_the_feet_are: base.where_the_feet_are,
            occupied_mask: base.occupied_mask,
            moved_mask: ((key >> 24) & 0x0f) as u8,
            holding_mask: ((key >> 28) & 0x0f) as u8,
        };
    }

    state_from_key_scalar::<COLS>(key)
}

fn state_from_key_scalar<const COLS: usize>(key: u32) -> State {
    let mut combined_columns = [Foot::None; MAX_COLUMNS];
    let mut where_the_feet_are = [INVALID_COLUMN; NUM_FEET];
    let mut occupied_mask = 0u8;
    let mut column = 0usize;
    while column < COLS {
        let foot = FOOT_FROM_KEY_BITS[((key >> (column * 3)) & 0b111) as usize];
        combined_columns[column] = foot;
        if foot != Foot::None {
            where_the_feet_are[foot_idx(foot)] = column as i8;
            occupied_mask |= 1u8 << column;
        }
        column += 1;
    }

    State {
        combined_columns,
        where_the_feet_are,
        occupied_mask,
        moved_mask: ((key >> 24) & 0x0f) as u8,
        holding_mask: ((key >> 28) & 0x0f) as u8,
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
    state_key: u32,
    pred: u32,
}

type SingleParityNode = u32;

#[derive(Clone, Copy)]
struct LayerLink {
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

fn calc_action_cost<const CACHED_GEOMETRY: bool>(
    layout: &StageLayout,
    initial: &State,
    result: &State,
    placement: &FootPlacement,
    hit: [i8; NUM_FEET],
    row: &Row,
    row_ctx: RowCostCtx,
    elapsed: f32,
    left_moved_not_holding: bool,
    right_moved_not_holding: bool,
    prev_row_has_live_hold: bool,
    facing_cost: f32,
    spin_cost: f32,
) -> f32 {
    let (lh, lt, rh, rt) = (
        hit[foot_idx(Foot::LeftHeel)],
        hit[foot_idx(Foot::LeftToe)],
        hit[foot_idx(Foot::RightHeel)],
        hit[foot_idx(Foot::RightToe)],
    );
    let (moved_left, moved_right) = (
        foot_moved(result, &LEFT_PAIR),
        foot_moved(result, &RIGHT_PAIR),
    );
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
    if row_ctx.mine_mask != 0 {
        cost += calc_mine_cost(result, row);
    }
    if row_ctx.has_hold {
        cost += calc_hold_switch_cost(layout, initial, result, row);
        cost += calc_bracket_tap_cost(initial, row, lh, lt, rh, rt, elapsed);
    }
    if row_ctx.multi_active {
        cost += calc_bracket_jack_cost(
            result,
            moved_left,
            moved_right,
            jacked_left,
            jacked_right,
            did_jump,
        );
    }
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
    if row.note_count >= 2 {
        cost += calc_slow_bracket_cost(row, moved_left, moved_right, elapsed);
    }
    if row_ctx.multi_active {
        cost += calc_twisted_foot_cost(layout, hit);
    }
    cost += if CACHED_GEOMETRY {
        facing_cost
    } else {
        calc_facing_cost(layout, result)
    };
    cost += if CACHED_GEOMETRY {
        spin_cost
    } else {
        calc_spin_cost(layout, initial, result)
    };
    if row_ctx.mine_mask == 0
        && (SLOW_FOOTSWITCH_THRESHOLD..SLOW_FOOTSWITCH_IGNORE).contains(&elapsed)
    {
        cost += calc_footswitch_cost(initial, placement, row_ctx.active_mask, elapsed);
    }
    if row_ctx.side_mask != 0 {
        cost += calc_sideswitch_cost(initial, result, placement, row_ctx.side_mask);
    }
    if row_ctx.mine_mask != 0 {
        cost += calc_missed_footswitch_cost(row, jacked_left, jacked_right);
    }
    cost += calc_jack_cost(moved_left, moved_right, jacked_left, jacked_right, elapsed);
    cost += calc_big_movements_cost(layout, initial, result, hit, elapsed);
    cost
}

fn calc_tap_cost(
    initial: &StateBase4,
    moved_foot: Foot,
    hit_col: usize,
    side_hit: bool,
    left_moved_not_holding: bool,
    right_moved_not_holding: bool,
    prev_row_has_live_hold: bool,
    facing_cost: f32,
    spin_cost: f32,
    ctx: &TapCostCtx,
) -> f32 {
    if moved_foot == Foot::None {
        return facing_cost + spin_cost;
    }
    let moved_idx = foot_idx(moved_foot);
    let moved_mask = FOOT_MASKS[moved_idx];
    let moved_left = moved_mask & LEFT_FOOT_MASK != 0;
    let prev_moved = if moved_left {
        left_moved_not_holding
    } else {
        right_moved_not_holding
    };
    let did_jump = left_moved_not_holding && right_moved_not_holding;
    let jacked = !did_jump && prev_moved && initial.combined_columns[hit_col] == moved_foot;

    let mut cost = 0.0;
    if !did_jump && !jacked && prev_moved && !prev_row_has_live_hold {
        cost += DOUBLESTEP_WEIGHT;
    }
    cost += facing_cost;
    cost += spin_cost;
    if ctx.footswitch != 0.0 {
        let initial_foot = initial.combined_columns[hit_col];
        if initial_foot != Foot::None
            && initial_foot != moved_foot
            && initial_foot != OTHER_PART_OF_FOOT[moved_idx]
        {
            cost += ctx.footswitch;
        }
    }
    if side_hit {
        let initial_foot = initial.combined_columns[hit_col];
        if initial_foot != moved_foot
            && initial_foot != Foot::None
            && moved_mask & FOOT_MASKS[foot_idx(initial_foot)] == 0
        {
            cost += SIDESWITCH_WEIGHT;
        }
    }
    if jacked {
        cost += ctx.jack;
    }
    let initial_col = initial.where_the_feet_are[moved_idx];
    if initial_col != INVALID_COLUMN {
        cost += ctx.movement[initial_col as usize];
    }
    cost
}

#[cfg(any(test, feature = "bench-support"))]
fn calc_tap_cost_legacy(
    layout: &StageLayout,
    initial: &StateBase4,
    result_key: u32,
    hit_col: usize,
    side_hit: bool,
    elapsed: f32,
    left_moved_not_holding: bool,
    right_moved_not_holding: bool,
    prev_row_has_live_hold: bool,
    facing_cost: f32,
    spin_cost: f32,
) -> f32 {
    let moved_mask = ((result_key >> 24) & 0x0f) as u8;
    let moved = moved_mask != 0;
    let moved_idx = if moved {
        moved_mask.trailing_zeros() as usize + 1
    } else {
        0
    };
    let moved_foot = if moved {
        FEET[moved_idx - 1]
    } else {
        Foot::None
    };
    let moved_left = moved_mask & LEFT_FOOT_MASK != 0;
    let prev_moved = if moved_left {
        left_moved_not_holding
    } else {
        right_moved_not_holding
    };
    let did_jump = left_moved_not_holding && right_moved_not_holding;
    let jacked =
        moved && !did_jump && prev_moved && initial.combined_columns[hit_col] == moved_foot;

    let mut cost = 0.0;
    if moved && !did_jump && !jacked && prev_moved && !prev_row_has_live_hold {
        cost += DOUBLESTEP_WEIGHT;
    }
    cost += facing_cost;
    cost += spin_cost;
    if moved && (SLOW_FOOTSWITCH_THRESHOLD..SLOW_FOOTSWITCH_IGNORE).contains(&elapsed) {
        let initial_foot = initial.combined_columns[hit_col];
        if initial_foot != Foot::None
            && initial_foot != moved_foot
            && initial_foot != OTHER_PART_OF_FOOT[moved_idx]
        {
            let scaled = elapsed - SLOW_FOOTSWITCH_THRESHOLD;
            cost += (scaled / (SLOW_FOOTSWITCH_THRESHOLD + scaled)) * FOOTSWITCH_WEIGHT;
        }
    }
    if side_hit && moved {
        let initial_foot = initial.combined_columns[hit_col];
        if initial_foot != moved_foot
            && initial_foot != Foot::None
            && moved_mask & FOOT_MASKS[foot_idx(initial_foot)] == 0
        {
            cost += SIDESWITCH_WEIGHT;
        }
    }
    if moved && jacked && elapsed < JACK_THRESHOLD {
        let scaled = JACK_THRESHOLD - elapsed;
        if scaled > 0.0 {
            cost += (1.0 / scaled - 1.0 / JACK_THRESHOLD) * JACK_WEIGHT;
        }
    }
    if moved {
        let initial_col = initial.where_the_feet_are[moved_idx];
        if initial_col != INVALID_COLUMN {
            cost += layout_movement_cost(layout, initial_col as usize, hit_col) / elapsed;
        }
    }
    cost
}

fn calc_mine_cost(result: &State, row: &Row) -> f32 {
    if (row.mine_mask | row.fake_mine_mask) & result.occupied_mask != 0 {
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
        let side = foot_side(foot);
        let switched = side != 0 && side != foot_side(initial_foot);

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

    let did_double_step = (moved_left && !jacked_left && left_moved_not_holding)
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

    let heel_facing = layout_facing_x(layout, lh, rh) * FACING_WEIGHT;
    let toe_facing = layout_facing_x(layout, lt, rt) * FACING_WEIGHT;
    let left_facing = layout_facing_y(layout, lh, lt) * FACING_WEIGHT;
    let right_facing = layout_facing_y(layout, rh, rt) * FACING_WEIGHT;
    heel_facing + toe_facing + left_facing + right_facing
}

fn calc_spin_cost(layout: &StageLayout, initial: &State, result: &State) -> f32 {
    if spin_class(layout, initial, false) + spin_class(layout, result, true) == 3 {
        SPIN_WEIGHT
    } else {
        0.0
    }
}

fn spin_class(layout: &StageLayout, state: &State, toe_fallback: bool) -> u8 {
    let get = |f: Foot| state.where_the_feet_are[foot_idx(f)];
    let (lh, mut lt) = (get(Foot::LeftHeel), get(Foot::LeftToe));
    let (rh, mut rt) = (get(Foot::RightHeel), get(Foot::RightToe));
    if toe_fallback {
        if lt == INVALID_COLUMN {
            lt = lh;
        }
        if rt == INVALID_COLUMN {
            rt = rh;
        }
    }
    let left = layout_avg_point(layout, lh, lt);
    let right = layout_avg_point(layout, rh, rt);
    if right.x >= left.x {
        0
    } else if right.y < left.y {
        1
    } else if right.y > left.y {
        2
    } else {
        0
    }
}

fn calc_footswitch_cost(
    initial: &State,
    placement: &FootPlacement,
    active_mask: u8,
    elapsed: f32,
) -> f32 {
    let time_scaled = elapsed - SLOW_FOOTSWITCH_THRESHOLD;
    let mut mask = active_mask;
    while mask != 0 {
        let i = mask.trailing_zeros() as usize;
        mask &= mask - 1;
        let res = placement[i];
        let init = initial.combined_columns[i];
        if init == Foot::None || res == Foot::None {
            continue;
        }
        if init != res && init != OTHER_PART_OF_FOOT[foot_idx(res)] {
            return (time_scaled / (SLOW_FOOTSWITCH_THRESHOLD + time_scaled)) * FOOTSWITCH_WEIGHT;
        }
    }
    0.0
}

fn calc_sideswitch_cost(
    initial: &State,
    result: &State,
    placement: &FootPlacement,
    side_mask: u8,
) -> f32 {
    let mut mask = side_mask;
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
    if (jacked_left || jacked_right) && (row.mine_mask != 0 || row.fake_mine_mask != 0) {
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
    let mut moved_feet = result.moved_mask & (LEFT_FOOT_MASK | RIGHT_FOOT_MASK);
    while moved_feet != 0 {
        let fi = moved_feet.trailing_zeros() as usize + 1;
        moved_feet &= moved_feet - 1;

        let init_pos = initial.where_the_feet_are[fi];
        if init_pos == INVALID_COLUMN {
            continue;
        }

        let res_pos = hit[fi];
        let mut d = layout_movement_cost(layout, init_pos as usize, res_pos as usize) / elapsed;

        let other_pos = hit[OTHER_FOOT_IDXS[fi]];
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
    row.has_live_hold
}

// --- Generator ---

struct StepParityGenerator {
    layout: &'static StageLayout,
    perm_table: &'static PermTable,
    facing_cost4: Option<&'static [f32; SINGLE_STATE_COUNT]>,
    spin_class4: Option<&'static [u8; SINGLE_STATE_COUNT]>,
    column_count: usize,
    single_nodes: Vec<SingleParityNode>,
    double_nodes: Vec<StepParityNode>,
    rows: Vec<Row>,
    // Chosen packed state per row; decoded only at the columns classification reads.
    result_keys: Vec<u32>,
    prev_links: Vec<LayerLink>,
    next_links: Vec<LayerLink>,
    state_map: RowStateMap,
    // Row-build state shared by every row instead of 32 endpoint bytes per row.
    active_hold_ends: [f32; MAX_COLUMNS],
    #[cfg(feature = "bench-support")]
    legacy_tap_path: bool,
}

#[cfg(feature = "bench-support")]
#[derive(Clone, Copy)]
struct LegacyNode {
    state_key: u32,
    pred: u32,
    cost: f32,
}

#[cfg(feature = "bench-support")]
struct LegacyDp {
    nodes: Vec<LegacyNode>,
    prev_ids: Vec<usize>,
    next_ids: Vec<usize>,
    state_map: RowStateMap,
}

#[cfg(feature = "bench-support")]
impl Default for LegacyDp {
    fn default() -> Self {
        Self {
            nodes: Vec::new(),
            prev_ids: Vec::new(),
            next_ids: Vec::new(),
            state_map: row_map_new(),
        }
    }
}

fn parity_gen(cache: &'static LayoutCache) -> StepParityGenerator {
    let facing_cost4 = (layout_cols(&cache.layout) == 4).then(|| facing_cost4(&cache.layout));
    let spin_class4 = (layout_cols(&cache.layout) == 4).then(|| spin_class4(&cache.layout));
    StepParityGenerator {
        column_count: layout_cols(&cache.layout),
        layout: &cache.layout,
        perm_table: &cache.perm_table,
        facing_cost4,
        spin_class4,
        single_nodes: Vec::new(),
        double_nodes: Vec::new(),
        rows: Vec::new(),
        result_keys: Vec::new(),
        prev_links: Vec::new(),
        next_links: Vec::new(),
        state_map: row_map_new(),
        active_hold_ends: [HOLD_END_NONE; MAX_COLUMNS],
        #[cfg(feature = "bench-support")]
        legacy_tap_path: false,
    }
}

#[inline(always)]
fn parity_reset(g: &mut StepParityGenerator, cols: usize) {
    g.column_count = cols;
    g.single_nodes.clear();
    g.double_nodes.clear();
    g.rows.clear();
    g.result_keys.clear();
    g.active_hold_ends.fill(HOLD_END_NONE);
}

#[inline(always)]
fn parity_finish(g: &mut StepParityGenerator) -> bool {
    if g.rows.is_empty() {
        return false;
    }
    match g.column_count {
        4 => parity_finish_columns::<4>(g),
        8 => parity_finish_columns::<8>(g),
        _ => false,
    }
}

#[inline(always)]
fn parity_finish_columns<const COLS: usize>(g: &mut StepParityGenerator) -> bool {
    let Some(best) = parity_dp_rows::<COLS>(g) else {
        return false;
    };
    parity_backtrack::<COLS>(g, best)
}

fn parity_analyze(
    g: &mut StepParityGenerator,
    notes: Vec<IntermediateNoteData>,
    cols: usize,
) -> bool {
    parity_reset(g, cols);
    parity_create_rows(g, notes);
    parity_reserve(g);
    parity_finish(g)
}

fn parity_reserve(g: &mut StepParityGenerator) {
    let node_floor = g.rows.len().saturating_add(1);
    if g.column_count == 4 {
        g.single_nodes.reserve(node_floor);
    } else {
        g.double_nodes.reserve(node_floor);
    }
    g.result_keys.reserve(g.rows.len());
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
                    if mine_on {
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
                if mine_on {
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
        row_counter_add_note(&mut counter, col, note.note_type != TapNoteType::Lift);
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

#[inline(always)]
fn row_nonzero_mask<const LANES: usize>(row: &[u8; LANES], cols: usize) -> u8 {
    if cols == 4 {
        return u8::from(row[0] != b'0')
            | (u8::from(row[1] != b'0') << 1)
            | (u8::from(row[2] != b'0') << 2)
            | (u8::from(row[3] != b'0') << 3);
    }
    if cols == 8 {
        return u8::from(row[0] != b'0')
            | (u8::from(row[1] != b'0') << 1)
            | (u8::from(row[2] != b'0') << 2)
            | (u8::from(row[3] != b'0') << 3)
            | (u8::from(row[4] != b'0') << 4)
            | (u8::from(row[5] != b'0') << 5)
            | (u8::from(row[6] != b'0') << 6)
            | (u8::from(row[7] != b'0') << 7);
    }

    let mut mask = 0u8;
    let mut c = 0usize;
    while c < cols {
        mask |= u8::from(row[c] != b'0') << c;
        c += 1;
    }
    mask
}

fn fill_hold_heads_from_arrays<const LANES: usize>(
    rows: &[[u8; LANES]],
    row_to_beat: &[f32],
    cols: usize,
    out: &mut Vec<[f32; MAX_COLUMNS]>,
) {
    out.clear();
    if cols == 0 || cols > 8 {
        return;
    }
    let copy_len = cols.min(LANES);
    out.resize(rows.len(), [HOLD_END_NONE; MAX_COLUMNS]);
    let mut hold_start_idx = [usize::MAX; MAX_COLUMNS];
    let mut hold_start_row = [0i32; MAX_COLUMNS];
    let mut hold_start_beat = [0.0f32; MAX_COLUMNS];

    for (idx, row) in rows.iter().enumerate() {
        let mut nonzero_mask = row_nonzero_mask(row, copy_len);
        if nonzero_mask == 0 {
            continue;
        }
        let (row_i32, beat) = row_quantized(row_to_beat[idx]);
        while nonzero_mask != 0 {
            let c = nonzero_mask.trailing_zeros() as usize;
            nonzero_mask &= nonzero_mask - 1;
            let ch = row[c];
            if ch == b'1' {
                hold_start_idx[c] = usize::MAX;
                continue;
            }
            if ch == b'2' || ch == b'4' {
                hold_start_idx[c] = idx;
                hold_start_row[c] = row_i32;
                hold_start_beat[c] = beat;
                continue;
            }
            if ch == b'3' {
                let start_idx = hold_start_idx[c];
                if start_idx != usize::MAX {
                    let len = (row_i32 - hold_start_row[c]) as f32 / ROWS_PER_BEAT as f32;
                    out[start_idx][c] = hold_start_beat[c] + len;
                    hold_start_idx[c] = usize::MAX;
                }
                continue;
            }
            if matches!(ch, b'L' | b'M' | b'F') {
                hold_start_idx[c] = usize::MAX;
            }
        }
    }
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
            if mine_on {
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
        if mine_on {
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
    counts_for_placement: bool,
) {
    if counter.last_second != second {
        if counter.last_second != CLM_SECOND_INVALID {
            parity_flush_row(g, counter);
        }
        row_counter_reset(counter, second, beat);
    }
    row_counter_add_note(counter, col, counts_for_placement);
    if hold_end != HOLD_END_NONE {
        counter.hold_ends[col] = hold_end;
    }
}

fn parity_create_rows_from_arrays<const LANES: usize>(
    g: &mut StepParityGenerator,
    hold_heads: &mut Vec<[f32; MAX_COLUMNS]>,
    rows: &[[u8; LANES]],
    row_to_beat: &[f32],
    timing: &TimingData,
    cols: usize,
    has_holds: bool,
) {
    if !has_holds {
        hold_heads.clear();
        if timing_fakes(timing).is_empty() {
            if let Some(fixed) = fixed_timing_parts(timing)
                && parity_create_tap_rows_fixed(g, rows, row_to_beat, cols, fixed)
            {
                return;
            }
            parity_create_rows_no_holds::<LANES, false>(g, rows, row_to_beat, timing, cols);
        } else {
            parity_create_rows_no_holds::<LANES, true>(g, rows, row_to_beat, timing, cols);
        }
        return;
    }

    if timing_fakes(timing).is_empty() {
        parity_create_rows_holds::<LANES, false>(g, hold_heads, rows, row_to_beat, timing, cols);
    } else {
        parity_create_rows_holds::<LANES, true>(g, hold_heads, rows, row_to_beat, timing, cols);
    }
}

#[inline(always)]
fn tap_only_mask<const LANES: usize>(row: &[u8; LANES], cols: usize) -> Option<u8> {
    let mut mask = 0u8;
    let mut c = 0usize;
    while c < cols {
        match row[c] {
            b'0' => {}
            b'1' => mask |= 1u8 << c,
            _ => return None,
        }
        c += 1;
    }
    Some(mask)
}

fn parity_create_tap_rows_fixed<const LANES: usize>(
    g: &mut StepParityGenerator,
    rows: &[[u8; LANES]],
    row_to_beat: &[f32],
    cols: usize,
    fixed: FixedTimingParts,
) -> bool {
    let copy_len = cols.min(LANES);
    for (idx, row) in rows.iter().enumerate() {
        let Some(mask) = tap_only_mask(row, copy_len) else {
            g.rows.clear();
            return false;
        };
        if mask == 0 {
            continue;
        }

        let (row_i32, beat) = row_quantized(row_to_beat[idx]);
        let mut out = row_new();
        out.second = fixed_row_time(fixed, row_i32);
        out.beat = beat;
        out.note_count = mask.count_ones() as u8;
        out.note_mask = mask;
        out.tech_mask = mask;
        g.rows.push(out);
    }
    true
}

fn parity_create_rows_holds<const LANES: usize, const HAS_FAKES: bool>(
    g: &mut StepParityGenerator,
    hold_heads: &mut Vec<[f32; MAX_COLUMNS]>,
    rows: &[[u8; LANES]],
    row_to_beat: &[f32],
    timing: &TimingData,
    cols: usize,
) {
    let mut counter = row_counter_new();
    fill_hold_heads_from_arrays(rows, row_to_beat, cols, hold_heads);
    let copy_len = cols.min(LANES);
    let fixed = fixed_timing_parts(timing);
    let mut time_cursor = BeatTimeCursorF32::new(timing);
    let mut fake_cursor = FakeRowCursor::new(timing);

    for (idx, row) in rows.iter().enumerate() {
        let mut nonzero_mask = row_nonzero_mask(row, copy_len);
        if nonzero_mask == 0 {
            continue;
        }
        let (row_i32, beat) = row_quantized(row_to_beat[idx]);
        let second = time_for_parity_row(&mut time_cursor, fixed, row_i32, beat);
        let row_fake = HAS_FAKES && fake_cursor.is_fake(row_i32);

        while nonzero_mask != 0 {
            let c = nonzero_mask.trailing_zeros() as usize;
            nonzero_mask &= nonzero_mask - 1;
            let ch = row[c];
            if ch == b'1' {
                if !row_fake {
                    parity_push_note(g, &mut counter, c, beat, second, HOLD_END_NONE, true);
                }
                continue;
            }
            match ch {
                b'M' => parity_push_mine(g, &mut counter, c, second, row_fake),
                b'L' if !row_fake => {
                    parity_push_note(g, &mut counter, c, beat, second, HOLD_END_NONE, false)
                }
                b'2' | b'4' => {
                    let hold_end = hold_heads[idx][c];
                    if !row_fake && hold_end != HOLD_END_NONE {
                        parity_push_note(g, &mut counter, c, beat, second, hold_end, true);
                    }
                }
                _ => {}
            }
        }
    }
    parity_flush_row(g, &counter);
}

fn parity_create_rows_no_holds<const LANES: usize, const HAS_FAKES: bool>(
    g: &mut StepParityGenerator,
    rows: &[[u8; LANES]],
    row_to_beat: &[f32],
    timing: &TimingData,
    cols: usize,
) {
    let mut counter = row_counter_new();
    let copy_len = cols.min(LANES);
    let fixed = fixed_timing_parts(timing);
    let mut time_cursor = BeatTimeCursorF32::new(timing);
    let mut fake_cursor = FakeRowCursor::new(timing);

    for (idx, row) in rows.iter().enumerate() {
        let mut nonzero_mask = row_nonzero_mask(row, copy_len);
        if nonzero_mask == 0 {
            continue;
        }
        let (row_i32, beat) = row_quantized(row_to_beat[idx]);
        let second = time_for_parity_row(&mut time_cursor, fixed, row_i32, beat);
        let row_fake = HAS_FAKES && fake_cursor.is_fake(row_i32);

        while nonzero_mask != 0 {
            let c = nonzero_mask.trailing_zeros() as usize;
            nonzero_mask &= nonzero_mask - 1;
            match row[c] {
                b'1' if !row_fake => {
                    parity_push_note(g, &mut counter, c, beat, second, HOLD_END_NONE, true);
                }
                b'M' => parity_push_mine(g, &mut counter, c, second, row_fake),
                b'L' if !row_fake => {
                    parity_push_note(g, &mut counter, c, beat, second, HOLD_END_NONE, false);
                }
                _ => {}
            }
        }
    }
    parity_flush_row(g, &counter);
}

#[inline(always)]
fn time_for_parity_row(
    time_cursor: &mut BeatTimeCursorF32<'_>,
    fixed: Option<FixedTimingParts>,
    row: i32,
    beat: f32,
) -> f32 {
    fixed.map_or_else(
        || time_cursor.time_for_beat(f64::from(beat)) as f32,
        |parts| fixed_row_time(parts, row),
    )
}

#[inline(always)]
fn fixed_row_time(parts: FixedTimingParts, row: i32) -> f32 {
    let (start, bps, global_offset) = parts;
    (f64::from(start + (row as f32 / ROWS_PER_BEAT as f32) / bps) - global_offset) as f32
}

fn rows_have_holds<const LANES: usize>(rows: &[[u8; LANES]], cols: usize) -> bool {
    let copy_len = cols.min(LANES);
    rows.iter().any(|row| {
        row.iter()
            .take(copy_len)
            .any(|&ch| matches!(ch, b'2' | b'4'))
    })
}

fn parity_analyze_rows<const LANES: usize>(
    g: &mut StepParityGenerator,
    hold_heads: &mut Vec<[f32; MAX_COLUMNS]>,
    rows: &[[u8; LANES]],
    row_to_beat: &[f32],
    timing: &TimingData,
    cols: usize,
    has_holds: bool,
) -> bool {
    parity_reset(g, cols);
    g.rows.reserve(rows.len());
    parity_create_rows_from_arrays(g, hold_heads, rows, row_to_beat, timing, cols, has_holds);
    parity_reserve(g);
    parity_finish(g)
}

fn parity_flush_row(g: &mut StepParityGenerator, counter: &RowCounter) {
    if counter.last_second == CLM_SECOND_INVALID {
        return;
    }
    let row = parity_build_row(g, counter);
    g.rows.push(row);
}

fn parity_build_row(g: &mut StepParityGenerator, counter: &RowCounter) -> Row {
    let mut row = row_new();
    row.second = counter.last_second;
    row.beat = counter.last_beat;
    row.note_mask = counter.note_mask;
    row.tech_mask = counter.tech_mask;
    row.note_count = counter.note_count;
    row.mine_mask = counter.next_mine_mask;
    row.mine_i32_mask = counter.next_mine_i32_mask;
    row.fake_mine_mask = counter.next_fake_mine_mask;

    for c in 0..g.column_count.min(MAX_COLUMNS) {
        let previous_end = g.active_hold_ends[c];
        let next_end = counter.hold_ends[c];
        if previous_end >= row.beat && next_end < 0.0 {
            row.hold_mask |= 1u8 << c;
            row.has_live_hold |= previous_end > row.beat;
        }
        if next_end >= 0.0 {
            g.active_hold_ends[c] = next_end;
        }
    }
    row
}

#[inline(always)]
fn parity_node_len<const COLS: usize>(g: &StepParityGenerator) -> usize {
    if COLS == 4 {
        g.single_nodes.len()
    } else {
        g.double_nodes.len()
    }
}

#[inline(always)]
fn parity_add_node<const COLS: usize>(g: &mut StepParityGenerator, state_key: u32) -> usize {
    if COLS == 4 {
        let idx = g.single_nodes.len();
        debug_assert_eq!(state_key & !SINGLE_STATE_MASK, 0);
        g.single_nodes.push(state_key);
        idx
    } else {
        let idx = g.double_nodes.len();
        g.double_nodes.push(StepParityNode {
            state_key,
            pred: u32::MAX,
        });
        idx
    }
}

#[inline(always)]
fn parity_state_key<const COLS: usize>(g: &StepParityGenerator, id: usize) -> u32 {
    if COLS == 4 {
        g.single_nodes[id]
    } else {
        g.double_nodes[id].state_key
    }
}

#[inline(always)]
fn parity_set_pred<const COLS: usize>(g: &mut StepParityGenerator, id: usize, pred: u32) {
    if COLS == 4 {
        debug_assert!(id > pred as usize);
        let delta = id - pred as usize;
        debug_assert!(delta <= SINGLE_PRED_MASK as usize);
        g.single_nodes[id] =
            (g.single_nodes[id] & SINGLE_STATE_MASK) | (delta as u32) << SINGLE_PRED_SHIFT;
    } else {
        g.double_nodes[id].pred = pred;
    }
}

fn parity_perms_for_row(g: &mut StepParityGenerator, row_idx: usize) -> &'static [FootPlacement] {
    let row = &g.rows[row_idx];
    let union = g.perm_table.get(row.note_mask | row.hold_mask);
    if union.is_empty() {
        let note = g.perm_table.get(row.note_mask);
        if note.is_empty() { &NO_PERMS } else { note }
    } else {
        union
    }
}

// The common dance-single tap case stays separate so its inner loop carries no
// general hold/mine/double state. This is intentionally one complete DP loop:
// splitting its map, prune, and update steps would add hot call boundaries.
fn parity_tap_row4(
    g: &mut StepParityGenerator,
    prev_start: usize,
    start_id: usize,
    next_start: usize,
    tap_col: usize,
    side_hit: bool,
    elapsed: f32,
    prev_row_has_live_hold: bool,
    costs_nonnegative: bool,
    facing_cost4: &[f32],
    spin_class4: &[u8],
) {
    let cost_ctx = tap_cost_ctx(g.layout, tap_col, elapsed);
    let feet: &[Foot] = if tap_col < SINGLE_COLS {
        &TAP_FEET
    } else {
        &IDLE_FEET
    };
    for j in 0..g.prev_links.len() {
        let init_id = prev_start + j;
        let init_key = if init_id == start_id {
            0
        } else {
            parity_state_key::<4>(g, init_id)
        };
        let initial = if init_id == start_id {
            &START_BASE4
        } else {
            &STATE_BASE4[init_key as usize & (SINGLE_STATE_COUNT - 1)]
        };
        let init_cost = g.prev_links[j].cost;
        let moved_mask = ((init_key >> 24) & 0x0f) as u8;
        let holding_mask = ((init_key >> 28) & 0x0f) as u8;
        let moved_not_holding = moved_mask & !holding_mask;
        let left_moved = moved_not_holding & LEFT_FOOT_MASK != 0;
        let right_moved = moved_not_holding & RIGHT_FOOT_MASK != 0;

        for &moved_foot in feet {
            let key = parity_result_tap_key4(init_key, moved_foot, tap_col);
            let cost_idx = match row_map_probe::<4, true>(&g.state_map, key) {
                RowMapProbe::Found(index) => index,
                RowMapProbe::Vacant(slot) => {
                    let id = parity_add_node::<4>(g, key);
                    let index = g.next_links.len();
                    debug_assert_eq!(id, next_start + index);
                    g.next_links.push(LayerLink { cost: f32::MAX });
                    row_map_insert_at::<4, true>(&mut g.state_map, slot, key, index);
                    index
                }
            };
            let best_cost = g.next_links[cost_idx].cost;
            if costs_nonnegative && init_cost >= best_cost {
                continue;
            }
            let facing = facing_cost4[key as usize & (SINGLE_STATE_COUNT - 1)];
            let spin = cached_spin_cost4::<true>(spin_class4, init_key, key);
            let action_cost = calc_tap_cost(
                initial,
                moved_foot,
                tap_col,
                side_hit,
                left_moved,
                right_moved,
                prev_row_has_live_hold,
                facing,
                spin,
                &cost_ctx,
            );
            let cost = init_cost + action_cost;
            if cost < best_cost {
                g.next_links[cost_idx].cost = cost;
                parity_set_pred::<4>(g, next_start + cost_idx, init_id as u32);
            }
        }
    }
}

#[cfg(feature = "bench-support")]
fn parity_tap_row4_legacy(
    g: &mut StepParityGenerator,
    perms: &[FootPlacement],
    prev_start: usize,
    start_id: usize,
    next_start: usize,
    tap_col: usize,
    side_hit: bool,
    elapsed: f32,
    prev_row_has_live_hold: bool,
    costs_nonnegative: bool,
    facing_cost4: &[f32],
    spin_class4: &[u8],
) {
    for j in 0..g.prev_links.len() {
        let init_id = prev_start + j;
        let init_key = if init_id == start_id {
            0
        } else {
            parity_state_key::<4>(g, init_id)
        };
        let initial = if init_id == start_id {
            &START_BASE4
        } else {
            &STATE_BASE4[init_key as usize & (SINGLE_STATE_COUNT - 1)]
        };
        let init_cost = g.prev_links[j].cost;
        let moved_mask = ((init_key >> 24) & 0x0f) as u8;
        let holding_mask = ((init_key >> 28) & 0x0f) as u8;
        let moved_not_holding = moved_mask & !holding_mask;
        let left_moved = moved_not_holding & LEFT_FOOT_MASK != 0;
        let right_moved = moved_not_holding & RIGHT_FOOT_MASK != 0;

        for perm in perms {
            let key = parity_result_tap_key4_legacy(init_key, perm, tap_col);
            let cost_idx = match row_map_probe::<4, true>(&g.state_map, key) {
                RowMapProbe::Found(index) => index,
                RowMapProbe::Vacant(slot) => {
                    let id = parity_add_node::<4>(g, key);
                    let index = g.next_links.len();
                    debug_assert_eq!(id, next_start + index);
                    g.next_links.push(LayerLink { cost: f32::MAX });
                    row_map_insert_at::<4, true>(&mut g.state_map, slot, key, index);
                    index
                }
            };
            let best_cost = g.next_links[cost_idx].cost;
            if costs_nonnegative && init_cost >= best_cost {
                continue;
            }
            let cost = init_cost
                + calc_tap_cost_legacy(
                    g.layout,
                    initial,
                    key,
                    tap_col,
                    side_hit,
                    elapsed,
                    left_moved,
                    right_moved,
                    prev_row_has_live_hold,
                    facing_cost4[key as usize & (SINGLE_STATE_COUNT - 1)],
                    cached_spin_cost4::<true>(spin_class4, init_key, key),
                );
            if cost < best_cost {
                g.next_links[cost_idx].cost = cost;
                parity_set_pred::<4>(g, next_start + cost_idx, init_id as u32);
            }
        }
    }
}

// Multi-panel dance-single rows use the general cost model, but need none of
// the double/hold/mine dispatch carried by the fallback DP loop. Keeping the
// complete state loop here lets LLVM optimize that row class as one unit.
fn parity_jump_row4(
    g: &mut StepParityGenerator,
    perms: &[FootPlacement],
    prev_start: usize,
    start_id: usize,
    next_start: usize,
    row: &Row,
    row_ctx: RowCostCtx,
    elapsed: f32,
    prev_row_has_live_hold: bool,
    costs_nonnegative: bool,
    facing_cost4: &[f32],
    spin_class4: &[u8],
) {
    for j in 0..g.prev_links.len() {
        let init_id = prev_start + j;
        let init_key = if init_id == start_id {
            0
        } else {
            parity_state_key::<4>(g, init_id)
        };
        let init_state = if init_id == start_id {
            state_new()
        } else {
            state_from_key::<4>(init_key)
        };
        let init_cost = g.prev_links[j].cost;
        let left_moved = foot_moved_not_holding(&init_state, &LEFT_PAIR);
        let right_moved = foot_moved_not_holding(&init_state, &RIGHT_PAIR);

        for perm in perms {
            let (hit, key) = parity_result_key4::<false>(&init_state, perm, 0, row_ctx.active_mask);
            let cost_idx = match row_map_probe::<4, true>(&g.state_map, key) {
                RowMapProbe::Found(index) => index,
                RowMapProbe::Vacant(slot) => {
                    let id = parity_add_node::<4>(g, key);
                    let index = g.next_links.len();
                    debug_assert_eq!(id, next_start + index);
                    g.next_links.push(LayerLink { cost: f32::MAX });
                    row_map_insert_at::<4, true>(&mut g.state_map, slot, key, index);
                    index
                }
            };
            let best_cost = g.next_links[cost_idx].cost;
            if costs_nonnegative && init_cost >= best_cost {
                continue;
            }
            let result = state_from_key::<4>(key);
            let cost = init_cost
                + calc_action_cost::<true>(
                    g.layout,
                    &init_state,
                    &result,
                    perm,
                    hit,
                    row,
                    row_ctx,
                    elapsed,
                    left_moved,
                    right_moved,
                    prev_row_has_live_hold,
                    facing_cost4[key as usize & (SINGLE_STATE_COUNT - 1)],
                    cached_spin_cost4::<false>(spin_class4, init_key, key),
                );
            if cost < best_cost {
                g.next_links[cost_idx].cost = cost;
                parity_set_pred::<4>(g, next_start + cost_idx, init_id as u32);
            }
        }
    }
}

fn parity_dp_rows<const COLS: usize>(g: &mut StepParityGenerator) -> Option<usize> {
    // Sample enough layers for mixed row types, then size the arena once from
    // the file's observed state density instead of repeatedly doubling it.
    const RESERVE_SAMPLE_ROWS: usize = 64;

    debug_assert_eq!(g.column_count, COLS);
    let facing_cost4 = g.facing_cost4.map_or(&[][..], |costs| &costs[..]);
    let spin_class4 = g.spin_class4.map_or(&[][..], |classes| &classes[..]);
    let start_id = parity_add_node::<COLS>(g, 0);
    let mut prev_start = start_id;
    g.prev_links.clear();
    g.prev_links.push(LayerLink { cost: 0.0 });
    g.next_links.clear();

    let mut prev_second = g.rows.first().map_or(-1.0, |r| r.second - 1.0);

    for i in 0..g.rows.len() {
        let row_second = g.rows[i].second;
        let hold_mask = g.rows[i].hold_mask;
        let elapsed = row_second - prev_second;
        prev_second = row_second;
        let costs_nonnegative = elapsed >= 0.0;
        let prev_row_has_live_hold = i > 0 && row_has_live_hold(&g.rows[i - 1]);

        let row = g.rows[i];
        let row_ctx = row_cost_ctx(&row, g.layout);
        let layout = g.layout;
        let active_mask = row_ctx.active_mask;
        let tap_col = active_mask.trailing_zeros() as usize;
        let side_hit = row_ctx.side_mask != 0;
        let simple_tap = row_ctx.mine_mask == 0
            && !row_ctx.has_hold
            && !row_ctx.multi_active
            && row.note_count < 2;
        #[cfg(feature = "bench-support")]
        let direct_tap = COLS == 4 && simple_tap && !g.legacy_tap_path;
        #[cfg(not(feature = "bench-support"))]
        let direct_tap = COLS == 4 && simple_tap;
        let perms = if direct_tap {
            &[][..]
        } else {
            parity_perms_for_row(g, i)
        };
        let perm_count = if direct_tap {
            [IDLE_FEET.len(), TAP_FEET.len()][(active_mask != 0) as usize]
        } else {
            perms.len()
        };
        let estimate = g.prev_links.len().saturating_mul(perm_count);
        let layer_estimate = if COLS == 4 {
            estimate.min(SINGLE_LAYER_MAX)
        } else {
            estimate
        };
        let next_start = parity_node_len::<COLS>(g);
        g.next_links.clear();
        row_map_reset::<COLS, true>(&mut g.state_map, layer_estimate);
        g.next_links.reserve(layer_estimate);

        if COLS == 4 && simple_tap {
            #[cfg(feature = "bench-support")]
            if g.legacy_tap_path {
                parity_tap_row4_legacy(
                    g,
                    perms,
                    prev_start,
                    start_id,
                    next_start,
                    tap_col,
                    side_hit,
                    elapsed,
                    prev_row_has_live_hold,
                    costs_nonnegative,
                    facing_cost4,
                    spin_class4,
                );
            } else {
                parity_tap_row4(
                    g,
                    prev_start,
                    start_id,
                    next_start,
                    tap_col,
                    side_hit,
                    elapsed,
                    prev_row_has_live_hold,
                    costs_nonnegative,
                    facing_cost4,
                    spin_class4,
                );
            }
            #[cfg(not(feature = "bench-support"))]
            parity_tap_row4(
                g,
                prev_start,
                start_id,
                next_start,
                tap_col,
                side_hit,
                elapsed,
                prev_row_has_live_hold,
                costs_nonnegative,
                facing_cost4,
                spin_class4,
            );
        } else if COLS == 4 && row_ctx.mine_mask == 0 && !row_ctx.has_hold && row_ctx.multi_active {
            parity_jump_row4(
                g,
                perms,
                prev_start,
                start_id,
                next_start,
                &row,
                row_ctx,
                elapsed,
                prev_row_has_live_hold,
                costs_nonnegative,
                facing_cost4,
                spin_class4,
            );
        } else {
            for j in 0..g.prev_links.len() {
                let init_id = prev_start + j;
                // ITGmania zero-initializes the synthetic starting state's foot
                // positions. A legitimate solved state may also have key zero, so
                // the node identity—not the key—must distinguish the two.
                let init_key = if init_id == start_id {
                    0
                } else {
                    parity_state_key::<COLS>(g, init_id)
                };
                let init_state = if init_id == start_id {
                    state_new()
                } else {
                    state_from_key::<COLS>(init_key)
                };
                let init_cost = g.prev_links[j].cost;
                let left_moved_not_holding = foot_moved_not_holding(&init_state, &LEFT_PAIR);
                let right_moved_not_holding = foot_moved_not_holding(&init_state, &RIGHT_PAIR);
                for perm in perms {
                    let (result, hit, key) = if COLS == 4 {
                        let (hit, key) = if hold_mask == 0 {
                            parity_result_key4::<false>(&init_state, perm, 0, active_mask)
                        } else {
                            parity_result_key4::<true>(&init_state, perm, hold_mask, active_mask)
                        };
                        (None, hit, key)
                    } else {
                        let (result, hit, key) = if hold_mask == 0 {
                            parity_result_state_no_holds::<COLS>(&init_state, perm, active_mask)
                        } else {
                            parity_result_state::<COLS>(&init_state, perm, hold_mask, active_mask)
                        };
                        (Some(result), hit, key)
                    };
                    let cost_idx = match row_map_probe::<COLS, true>(&g.state_map, key) {
                        RowMapProbe::Found(index) => index,
                        RowMapProbe::Vacant(slot) => {
                            let id = parity_add_node::<COLS>(g, key);
                            let index = g.next_links.len();
                            debug_assert_eq!(id, next_start + index);
                            g.next_links.push(LayerLink { cost: f32::MAX });
                            row_map_insert_at::<COLS, true>(&mut g.state_map, slot, key, index);
                            index
                        }
                    };
                    let calc_cost = || {
                        let action_cost = if COLS == 4 {
                            let result = state_from_key::<4>(key);
                            calc_action_cost::<true>(
                                layout,
                                &init_state,
                                &result,
                                perm,
                                hit,
                                &row,
                                row_ctx,
                                elapsed,
                                left_moved_not_holding,
                                right_moved_not_holding,
                                prev_row_has_live_hold,
                                facing_cost4[key as usize & (SINGLE_STATE_COUNT - 1)],
                                cached_spin_cost4::<false>(spin_class4, init_key, key),
                            )
                        } else {
                            let result = result.unwrap_or_else(|| {
                                unreachable!("double transition must have state")
                            });
                            calc_action_cost::<false>(
                                layout,
                                &init_state,
                                &result,
                                perm,
                                hit,
                                &row,
                                row_ctx,
                                elapsed,
                                left_moved_not_holding,
                                right_moved_not_holding,
                                prev_row_has_live_hold,
                                0.0,
                                0.0,
                            )
                        };
                        init_cost + action_cost
                    };
                    // Keep the dense single update monomorphic so it carries no
                    // double-node predecessor path through the hot loop.
                    if COLS == 4 {
                        let best_cost = g.next_links[cost_idx].cost;
                        // With nonnegative elapsed time, action costs cannot lower the path cost.
                        if costs_nonnegative && init_cost >= best_cost {
                            continue;
                        }
                        let nc = calc_cost();
                        if nc < best_cost {
                            g.next_links[cost_idx].cost = nc;
                            parity_set_pred::<4>(g, next_start + cost_idx, init_id as u32);
                        }
                        continue;
                    }

                    let best_cost = g.next_links[cost_idx].cost;
                    if costs_nonnegative && init_cost >= best_cost {
                        continue;
                    }
                    let nc = calc_cost();
                    if nc < best_cost {
                        g.next_links[cost_idx].cost = nc;
                        parity_set_pred::<COLS>(g, next_start + cost_idx, init_id as u32);
                    }
                }
            }
        }

        if COLS == 4 {
            assert!(
                g.prev_links.len() + g.next_links.len() <= SINGLE_PRED_MASK as usize,
                "single-panel layer pair exceeded packed predecessor domain"
            );
            debug_assert_eq!(parity_node_len::<COLS>(g) - next_start, g.next_links.len());
        }
        prev_start = next_start;
        std::mem::swap(&mut g.prev_links, &mut g.next_links);

        if i + 1 == RESERVE_SAMPLE_ROWS && g.rows.len() > RESERVE_SAMPLE_ROWS {
            let node_len = parity_node_len::<COLS>(g);
            let states_per_row = node_len.div_ceil(RESERVE_SAMPLE_ROWS);
            let expected = states_per_row.saturating_mul(g.rows.len());
            let additional = expected.saturating_sub(node_len);
            if COLS == 4 {
                g.single_nodes.reserve(additional);
            } else {
                g.double_nodes.reserve(additional);
            }
        }
    }

    g.prev_links
        .iter()
        .enumerate()
        .min_by(|(_, a), (_, b)| a.cost.total_cmp(&b.cost))
        .map(|(index, _)| prev_start + index)
}

#[cfg(feature = "bench-support")]
fn legacy_add_node(dp: &mut LegacyDp, state_key: u32) -> usize {
    let id = dp.nodes.len();
    dp.nodes.push(LegacyNode {
        state_key,
        pred: u32::MAX,
        cost: f32::MAX,
    });
    id
}

#[cfg(feature = "bench-support")]
fn legacy_dp_rows<const COLS: usize>(
    g: &mut StepParityGenerator,
    dp: &mut LegacyDp,
) -> Option<usize> {
    const RESERVE_SAMPLE_ROWS: usize = 64;

    dp.nodes.clear();
    let start_id = legacy_add_node(dp, 0);
    dp.nodes[start_id].cost = 0.0;
    dp.prev_ids.clear();
    dp.prev_ids.push(start_id);
    dp.next_ids.clear();

    let mut prev_second = g.rows.first().map_or(-1.0, |row| row.second - 1.0);
    for i in 0..g.rows.len() {
        let row_second = g.rows[i].second;
        let hold_mask = g.rows[i].hold_mask;
        let elapsed = row_second - prev_second;
        prev_second = row_second;
        let costs_nonnegative = elapsed >= 0.0;
        let prev_row_has_live_hold = i > 0 && row_has_live_hold(&g.rows[i - 1]);
        let perms = parity_perms_for_row(g, i);
        let estimate = dp.prev_ids.len().saturating_mul(perms.len());
        dp.next_ids.clear();
        row_map_reset::<COLS, false>(&mut dp.state_map, estimate);
        dp.next_ids.reserve(estimate);
        let row = g.rows[i];
        let row_ctx = row_cost_ctx(&row, g.layout);
        let active_mask = row_ctx.active_mask;

        for j in 0..dp.prev_ids.len() {
            let init_id = dp.prev_ids[j];
            let init_state = if init_id == start_id {
                state_new()
            } else {
                state_from_key_scalar::<COLS>(dp.nodes[init_id].state_key)
            };
            let init_cost = dp.nodes[init_id].cost;
            let left_moved_not_holding = foot_moved_not_holding(&init_state, &LEFT_PAIR);
            let right_moved_not_holding = foot_moved_not_holding(&init_state, &RIGHT_PAIR);
            for perm in perms {
                let (result, hit, key) = if hold_mask == 0 {
                    parity_result_state_no_holds::<COLS>(&init_state, perm, active_mask)
                } else {
                    parity_result_state::<COLS>(&init_state, perm, hold_mask, active_mask)
                };
                let res_id = match row_map_probe::<COLS, false>(&dp.state_map, key) {
                    RowMapProbe::Found(id) => id,
                    RowMapProbe::Vacant(slot) => {
                        let id = legacy_add_node(dp, key);
                        dp.next_ids.push(id);
                        row_map_insert_at::<COLS, false>(&mut dp.state_map, slot, key, id);
                        id
                    }
                };
                if costs_nonnegative && init_cost >= dp.nodes[res_id].cost {
                    continue;
                }
                let cost = init_cost
                    + calc_action_cost::<false>(
                        g.layout,
                        &init_state,
                        &result,
                        perm,
                        hit,
                        &row,
                        row_ctx,
                        elapsed,
                        left_moved_not_holding,
                        right_moved_not_holding,
                        prev_row_has_live_hold,
                        0.0,
                        0.0,
                    );
                if cost < dp.nodes[res_id].cost {
                    dp.nodes[res_id].cost = cost;
                    dp.nodes[res_id].pred = init_id as u32;
                }
            }
        }

        std::mem::swap(&mut dp.prev_ids, &mut dp.next_ids);
        if i + 1 == RESERVE_SAMPLE_ROWS && g.rows.len() > RESERVE_SAMPLE_ROWS {
            let states_per_row = dp.nodes.len().div_ceil(RESERVE_SAMPLE_ROWS);
            let expected = states_per_row.saturating_mul(g.rows.len());
            dp.nodes.reserve(expected.saturating_sub(dp.nodes.len()));
        }
    }

    dp.prev_ids
        .iter()
        .copied()
        .min_by(|&a, &b| dp.nodes[a].cost.total_cmp(&dp.nodes[b].cost))
}

#[cfg(feature = "bench-support")]
fn legacy_backtrack(g: &mut StepParityGenerator, dp: &LegacyDp, mut cur: usize) -> bool {
    let rows = g.rows.len();
    g.result_keys.resize(rows, 0);
    for write in (0..rows).rev() {
        g.result_keys[write] = dp.nodes[cur].state_key;
        let pred = dp.nodes[cur].pred;
        if pred == u32::MAX {
            g.result_keys.clear();
            return false;
        }
        cur = pred as usize;
    }
    let ok = cur == 0;
    if !ok {
        g.result_keys.clear();
    }
    ok
}

#[cfg(feature = "bench-support")]
fn legacy_finish(g: &mut StepParityGenerator, dp: &mut LegacyDp) -> bool {
    if g.rows.is_empty() {
        return false;
    }
    let best = match g.column_count {
        4 => legacy_dp_rows::<4>(g, dp),
        8 => legacy_dp_rows::<8>(g, dp),
        _ => None,
    };
    match (g.column_count, best) {
        (4 | 8, Some(best)) => legacy_backtrack(g, dp, best),
        _ => false,
    }
}

fn parity_result_state<const COLS: usize>(
    initial: &State,
    cols: &FootPlacement,
    hold_mask: u8,
    active_mask: u8,
) -> (State, [i8; NUM_FEET], u32) {
    let (mut combined, mut hit) = ([Foot::None; MAX_COLUMNS], [INVALID_COLUMN; NUM_FEET]);
    let (mut moved_mask, mut holding_mask) = (0u8, 0u8);
    let mut mask = active_mask;
    while mask != 0 {
        let i = mask.trailing_zeros() as usize;
        mask &= mask - 1;
        if i >= COLS {
            continue;
        }
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

    let (moved_left, moved_right) = (
        (moved_mask & LEFT_FOOT_MASK) != 0,
        (moved_mask & RIGHT_FOOT_MASK) != 0,
    );
    let (mut where_the_feet_are, mut comb_p, mut occupied_mask) =
        ([INVALID_COLUMN; NUM_FEET], 0u32, 0u8);
    for i in 0..COLS {
        let mut foot = combined[i];
        if foot == Foot::None {
            let prev = initial.combined_columns[i];
            foot = match prev {
                Foot::LeftHeel | Foot::RightHeel
                    if (moved_mask & FOOT_MASKS[foot_idx(prev)]) == 0 =>
                {
                    prev
                }
                Foot::LeftToe if !moved_left => prev,
                Foot::RightToe if !moved_right => prev,
                _ => Foot::None,
            };
        }
        combined[i] = foot;
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

fn parity_result_state_no_holds<const COLS: usize>(
    initial: &State,
    cols: &FootPlacement,
    active_mask: u8,
) -> (State, [i8; NUM_FEET], u32) {
    let (mut combined, mut hit) = ([Foot::None; MAX_COLUMNS], [INVALID_COLUMN; NUM_FEET]);
    let mut moved_mask = 0u8;
    let mut mask = active_mask;
    while mask != 0 {
        let i = mask.trailing_zeros() as usize;
        mask &= mask - 1;
        if i >= COLS {
            continue;
        }
        let foot = cols[i];
        if foot == Foot::None {
            continue;
        }
        combined[i] = foot;
        let fi = foot_idx(foot);
        hit[fi] = i as i8;
        moved_mask |= FOOT_MASKS[fi];
    }

    let (moved_left, moved_right) = (
        (moved_mask & LEFT_FOOT_MASK) != 0,
        (moved_mask & RIGHT_FOOT_MASK) != 0,
    );
    let (mut where_the_feet_are, mut comb_p, mut occupied_mask) =
        ([INVALID_COLUMN; NUM_FEET], 0u32, 0u8);
    for i in 0..COLS {
        let mut foot = combined[i];
        if foot == Foot::None {
            let prev = initial.combined_columns[i];
            foot = match prev {
                Foot::LeftHeel | Foot::RightHeel
                    if (moved_mask & FOOT_MASKS[foot_idx(prev)]) == 0 =>
                {
                    prev
                }
                Foot::LeftToe if !moved_left => prev,
                Foot::RightToe if !moved_right => prev,
                _ => Foot::None,
            };
        }
        combined[i] = foot;
        comb_p |= (foot as u32) << (i * 3);
        if foot != Foot::None {
            where_the_feet_are[foot_idx(foot)] = i as i8;
            occupied_mask |= 1u8 << i;
        }
    }

    let key = comb_p | (u32::from(moved_mask) << 24);
    (
        State {
            combined_columns: combined,
            where_the_feet_are,
            occupied_mask,
            moved_mask,
            holding_mask: 0,
        },
        hit,
        key,
    )
}

#[inline(always)]
fn parity_combined_key4(initial: &State, cols: &FootPlacement, moved_mask: u8) -> u32 {
    let moved_left = moved_mask & LEFT_FOOT_MASK != 0;
    let moved_right = moved_mask & RIGHT_FOOT_MASK != 0;
    let mut combined = 0u32;
    for (i, &placed) in cols.iter().enumerate().take(4) {
        let foot = if placed != Foot::None {
            placed
        } else {
            let previous = initial.combined_columns[i];
            match previous {
                Foot::LeftHeel | Foot::RightHeel
                    if moved_mask & FOOT_MASKS[foot_idx(previous)] == 0 =>
                {
                    previous
                }
                Foot::LeftToe if !moved_left => previous,
                Foot::RightToe if !moved_right => previous,
                _ => Foot::None,
            }
        };
        combined |= (foot as u32) << (i * 3);
    }
    combined
}

#[inline(always)]
fn parity_result_tap_key4(initial_key: u32, foot: Foot, hit_col: usize) -> u32 {
    if hit_col >= SINGLE_COLS || foot == Foot::None {
        return initial_key & (SINGLE_STATE_COUNT - 1) as u32;
    }

    let fi = foot_idx(foot);
    let moved_mask = FOOT_MASKS[fi];
    let base = initial_key as usize & (SINGLE_STATE_COUNT - 1);
    let combined = u32::from(TAP_BASE4[base * 16 + hit_col * 4 + fi - 1]);
    combined | (u32::from(moved_mask) << 24)
}

#[cfg(feature = "bench-support")]
#[inline(always)]
fn parity_result_tap_key4_legacy(initial_key: u32, cols: &FootPlacement, hit_col: usize) -> u32 {
    if hit_col >= SINGLE_COLS || cols[hit_col] == Foot::None {
        return initial_key & (SINGLE_STATE_COUNT - 1) as u32;
    }

    let foot = cols[hit_col];
    let fi = foot_idx(foot);
    let moved_mask = FOOT_MASKS[fi];
    let base = initial_key as usize & (SINGLE_STATE_COUNT - 1);
    let combined = u32::from(TAP_BASE4[base * 16 + hit_col * 4 + fi - 1]);
    combined | (u32::from(moved_mask) << 24)
}

#[inline(always)]
fn parity_result_key4<const HAS_HOLDS: bool>(
    initial: &State,
    cols: &FootPlacement,
    hold_mask: u8,
    active_mask: u8,
) -> ([i8; NUM_FEET], u32) {
    let mut hit = [INVALID_COLUMN; NUM_FEET];
    let (mut moved_mask, mut holding_mask) = (0u8, 0u8);
    let mut mask = active_mask & 0x0f;
    while mask != 0 {
        let i = mask.trailing_zeros() as usize;
        mask &= mask - 1;
        let foot = cols[i];
        if foot == Foot::None {
            continue;
        }
        let fi = foot_idx(foot);
        hit[fi] = i as i8;
        let foot_mask = FOOT_MASKS[fi];
        let held = HAS_HOLDS && hold_mask & (1u8 << i) != 0;
        holding_mask |= foot_mask * u8::from(held);
        if !held || initial.combined_columns[i] != foot {
            moved_mask |= foot_mask;
        }
    }

    let combined = parity_combined_key4(initial, cols, moved_mask);

    (
        hit,
        combined | (u32::from(moved_mask) << 24) | (u32::from(holding_mask) << 28),
    )
}

fn parity_backtrack<const COLS: usize>(g: &mut StepParityGenerator, mut cur: usize) -> bool {
    let rows = g.rows.len();
    if g.result_keys.len() < rows {
        g.result_keys.resize(rows, 0);
    } else {
        g.result_keys.truncate(rows);
    }

    let mut write = rows;
    while write > 0 {
        write -= 1;
        let (key, prev) = if COLS == 4 {
            let node = g.single_nodes[cur];
            let delta = (node >> SINGLE_PRED_SHIFT) & SINGLE_PRED_MASK;
            let prev = if delta == 0 {
                u32::MAX
            } else {
                cur as u32 - delta
            };
            (node & SINGLE_STATE_MASK, prev)
        } else {
            let node = &g.double_nodes[cur];
            (node.state_key, node.pred)
        };
        g.result_keys[write] = key;
        if prev == u32::MAX {
            g.result_keys.clear();
            return false;
        }
        cur = prev as usize;
    }

    let ok = cur == 0;
    if !ok {
        g.result_keys.clear();
    }
    ok
}

// --- RowCounter ---

struct RowCounter {
    note_count: u8,
    note_mask: u8,
    tech_mask: u8,
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
        note_count: 0,
        note_mask: 0,
        tech_mask: 0,
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
    c.note_count = 0;
    c.note_mask = 0;
    c.tech_mask = 0;
    c.hold_ends.fill(HOLD_END_NONE);
    c.mine_mask = 0;
    c.mine_i32_mask = 0;
    c.fake_mine_mask = 0;
}

fn row_counter_add_note(c: &mut RowCounter, col: usize, counts_for_placement: bool) {
    let bit = 1u8 << col;
    // ITGmania stores one note per column in a parity row, so warp-collapsed
    // same-column notes contribute one note count, not one count per event.
    if c.tech_mask & bit == 0 {
        c.note_count = c.note_count.saturating_add(1);
    }
    c.tech_mask |= bit;
    if counts_for_placement {
        c.note_mask |= bit;
    }
}

// --- Permutation ---

fn permute_row<F: FnMut(FootPlacement)>(
    layout: &StageLayout,
    mask: u8,
    cols: &mut FootPlacement,
    col: usize,
    col_count: usize,
    used: u8,
    emit: &mut F,
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

        emit(*cols);
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
            permute_row(layout, mask, cols, col + 1, col_count, used | fm, emit);
            cols[col] = Foot::None;
        }
    } else {
        permute_row(layout, mask, cols, col + 1, col_count, used, emit);
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

impl core::ops::AddAssign for TechCounts {
    fn add_assign(&mut self, o: Self) {
        self.crossovers += o.crossovers;
        self.half_crossovers += o.half_crossovers;
        self.full_crossovers += o.full_crossovers;
        self.footswitches += o.footswitches;
        self.up_footswitches += o.up_footswitches;
        self.down_footswitches += o.down_footswitches;
        self.sideswitches += o.sideswitches;
        self.jacks += o.jacks;
        self.brackets += o.brackets;
        self.doublesteps += o.doublesteps;
    }
}

#[inline(always)]
const fn key_foot(key: u32, column: usize) -> Foot {
    FOOT_FROM_KEY_BITS[((key >> (column * 3)) & 0b111) as usize]
}

fn calculate_tech_counts(rows: &[Row], keys: &[u32], layout: &StageLayout) -> TechCounts {
    let mut out = TechCounts::default();
    if rows.len() < 2 || keys.len() != rows.len() {
        return out;
    }

    let cols = layout_cols(layout).min(MAX_COLUMNS);
    let col_mask = u8::MAX >> (MAX_COLUMNS - cols);
    debug_assert!(rows.iter().all(|row| row.tech_mask & !col_mask == 0));

    let hit_positions = |key: u32, row: &Row| -> [i8; NUM_FEET] {
        let mut pos = [INVALID_COLUMN; NUM_FEET];
        let mut m = row.tech_mask;
        if cols == 4 && row.note_count == 1 {
            debug_assert!(m.is_power_of_two());
            let c = m.trailing_zeros() as usize;
            let foot = key_foot(key, c);
            if foot != Foot::None {
                pos[foot_idx(foot)] = c as i8;
            }
            return pos;
        }
        while m != 0 {
            let c = m.trailing_zeros() as usize;
            m &= m - 1;
            let foot = key_foot(key, c);
            if foot != Foot::None {
                pos[foot_idx(foot)] = c as i8;
            }
        }
        pos
    };

    let mut prev_prev_pos = [INVALID_COLUMN; NUM_FEET];
    let mut prev_key = keys[0];
    let mut prev_pos = hit_positions(prev_key, &rows[0]);

    for i in 1..rows.len() {
        let (curr, prev) = (&rows[i], &rows[i - 1]);
        let curr_key = keys[i];

        let curr_pos = hit_positions(curr_key, curr);

        // Per-row tech is computed by the shared classifier so the aggregate
        // counts here and the per-row annotation flags never drift.
        classify_row_tech(
            layout,
            curr,
            prev,
            curr_key,
            prev_key,
            &curr_pos,
            &prev_pos,
            &prev_prev_pos,
            i,
            &mut out,
        );

        prev_prev_pos = prev_pos;
        prev_pos = curr_pos;
        prev_key = curr_key;
    }
    out
}

/// Classify all tech categories triggered by a single judged row. This is the
/// single source of truth shared by
/// [`calculate_tech_counts`] (which accumulates the counts) and
/// [`collect_annotations`] (which stores them per row). Summing the per-row
/// results over a chart reproduces the aggregate [`TechCounts`], so the two can
/// never drift apart.
#[allow(clippy::too_many_arguments)]
fn classify_row_tech(
    layout: &StageLayout,
    curr: &Row,
    prev: &Row,
    curr_key: u32,
    prev_key: u32,
    curr_pos: &[i8; NUM_FEET],
    prev_pos: &[i8; NUM_FEET],
    prev_prev_pos: &[i8; NUM_FEET],
    i: usize,
    out: &mut TechCounts,
) {
    let elapsed = curr.second - prev.second;

    // Jacks and doublesteps
    if curr.note_count == 1 && prev.note_count == 1 {
        if layout.cols == 4 {
            let cc = curr.tech_mask.trailing_zeros() as i8;
            let foot = key_foot(curr_key, cc as usize);
            if foot != Foot::None {
                let pc = prev_pos[foot_idx(foot)];
                if cc == pc && elapsed < JACK_CUTOFF {
                    out.jacks += 1;
                } else if cc != pc && pc != INVALID_COLUMN && elapsed < DOUBLESTEP_CUTOFF {
                    out.doublesteps += 1;
                }
            }
        } else {
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
    if elapsed < FOOTSWITCH_CUTOFF {
        let switch_mask = prev.tech_mask & curr.tech_mask;
        let mut mask = layout.up_mask & switch_mask;
        while mask != 0 {
            let c = mask.trailing_zeros() as usize;
            mask &= mask - 1;
            if is_footswitch(key_foot(prev_key, c), key_foot(curr_key, c)) {
                out.up_footswitches += 1;
                out.footswitches += 1;
            }
        }
        mask = layout.down_mask & switch_mask;
        while mask != 0 {
            let c = mask.trailing_zeros() as usize;
            mask &= mask - 1;
            if is_footswitch(key_foot(prev_key, c), key_foot(curr_key, c)) {
                out.down_footswitches += 1;
                out.footswitches += 1;
            }
        }
        mask = layout.side_mask & switch_mask;
        while mask != 0 {
            let c = mask.trailing_zeros() as usize;
            mask &= mask - 1;
            if is_footswitch(key_foot(prev_key, c), key_foot(curr_key, c)) {
                out.sideswitches += 1;
            }
        }
    }

    // Crossovers
    match classify_crossover(layout, curr_pos, prev_pos, prev_prev_pos, i) {
        CrossoverKind::Full => {
            out.full_crossovers += 1;
            out.crossovers += 1;
        }
        CrossoverKind::Half => {
            out.half_crossovers += 1;
            out.crossovers += 1;
        }
        CrossoverKind::None => {}
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum CrossoverKind {
    None,
    Half,
    Full,
}

/// Classify whether the current row is a crossover relative to the previous
/// (and prev-prev) foot positions. This is the single source of truth shared by
/// `calculate_tech_counts` (for aggregate counting) and `collect_annotations`
/// (for per-row crossover cue annotations), so the two never drift apart.
fn classify_crossover(
    layout: &StageLayout,
    curr_pos: &[i8; NUM_FEET],
    prev_pos: &[i8; NUM_FEET],
    prev_prev_pos: &[i8; NUM_FEET],
    i: usize,
) -> CrossoverKind {
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
                let prev_prev_rh = prev_prev_pos[foot_idx(Foot::RightHeel)];
                if prev_prev_rh != INVALID_COLUMN && prev_prev_rh != right_heel {
                    let prev_prev_point = layout.columns[prev_prev_rh as usize];
                    return if prev_prev_point.x > left_pos.x {
                        CrossoverKind::Full
                    } else {
                        CrossoverKind::Half
                    };
                }
            } else {
                return CrossoverKind::Half;
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
                let prev_prev_lh = prev_prev_pos[foot_idx(Foot::LeftHeel)];
                if prev_prev_lh != INVALID_COLUMN && prev_prev_lh != left_heel {
                    let prev_prev_point = layout.columns[prev_prev_lh as usize];
                    return if right_pos.x > prev_prev_point.x {
                        CrossoverKind::Full
                    } else {
                        CrossoverKind::Half
                    };
                }
            } else {
                return CrossoverKind::Half;
            }
        }
    }

    CrossoverKind::None
}

/// Per-row StepParity annotation, mirroring the full data the engine's
/// `GetNoteAnnotations()` exposes (and Simply Love's `CrossoverCues.lua` reads):
/// the beat, the elapsed second, the set of foot-bearing columns
/// (`column_mask`, equivalent to `footPlacement` keys), the foot assigned to
/// each column (via [`foot`](Self::foot) / [`feet`](Self::feet)), and the full
/// per-row tech classification (`row_tech`).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct RowAnnotation {
    pub beat: f32,
    pub second: f32,
    /// Occupancy bitmask of the foot-bearing columns this row (0-indexed) — the
    /// bitset form of [`feet`](Self::feet). Equivalent to the engine's
    /// `footPlacement` keys.
    pub column_mask: u8,
    /// Foot assigned to each column (`Foot::None` where no foot steps this row).
    /// Private so the storage width is not part of the public API; read it via
    /// [`foot`](Self::foot) / [`feet`](Self::feet).
    feet: [Foot; MAX_COLUMNS],
    /// Full per-row tech classification (per-category counts for this row). The
    /// per-row counts sum to the chart's aggregate [`TechCounts`].
    pub row_tech: TechCounts,
}

impl RowAnnotation {
    /// Number of feet placed on this row (`== column_mask.count_ones()`).
    #[inline]
    pub fn foot_count(&self) -> u32 {
        self.column_mask.count_ones()
    }

    /// Foot assigned to `column` (`Foot::None` if no foot steps there or the
    /// column is out of range).
    #[inline]
    pub fn foot(&self, column: usize) -> Foot {
        self.feet.get(column).copied().unwrap_or(Foot::None)
    }

    /// Per-column foot assignment, indexed by column (`Foot::None` where no foot
    /// steps). Only the columns in `column_mask` are set.
    #[inline]
    pub fn feet(&self) -> &[Foot] {
        &self.feet
    }
}

fn collect_annotations_in(
    rows: &[Row],
    keys: &[u32],
    layout: &StageLayout,
    out: &mut Vec<RowAnnotation>,
) -> TechCounts {
    let n = rows.len();
    out.clear();
    if n == 0 || keys.len() != n {
        return TechCounts::default();
    }
    out.reserve(n);
    let mut counts = TechCounts::default();

    let cols = layout_cols(layout).min(MAX_COLUMNS);
    let col_mask = u8::MAX >> (MAX_COLUMNS - cols);
    debug_assert!(rows.iter().all(|row| row.tech_mask & !col_mask == 0));

    let hit_positions = |key: u32, row: &Row| -> [i8; NUM_FEET] {
        let mut pos = [INVALID_COLUMN; NUM_FEET];
        let mut m = row.tech_mask;
        if cols == 4 && row.note_count == 1 {
            debug_assert!(m.is_power_of_two());
            let c = m.trailing_zeros() as usize;
            let foot = key_foot(key, c);
            if foot != Foot::None {
                pos[foot_idx(foot)] = c as i8;
            }
            return pos;
        }
        while m != 0 {
            let c = m.trailing_zeros() as usize;
            m &= m - 1;
            let foot = key_foot(key, c);
            if foot != Foot::None {
                pos[foot_idx(foot)] = c as i8;
            }
        }
        pos
    };

    // Foot assigned to each foot-bearing column (`Foot::None` elsewhere).
    let feet_of = |key: u32, mask: u8| -> [Foot; MAX_COLUMNS] {
        let mut feet = [Foot::None; MAX_COLUMNS];
        let mut m = mask;
        while m != 0 {
            let c = m.trailing_zeros() as usize;
            m &= m - 1;
            feet[c] = key_foot(key, c);
        }
        feet
    };

    let mut prev_key = keys[0];
    out.push(RowAnnotation {
        beat: rows[0].beat,
        second: rows[0].second,
        column_mask: rows[0].tech_mask,
        feet: feet_of(prev_key, rows[0].tech_mask),
        row_tech: TechCounts::default(),
    });

    let mut prev_prev_pos = [INVALID_COLUMN; NUM_FEET];
    let mut prev_pos = hit_positions(prev_key, &rows[0]);

    for i in 1..n {
        let (curr, prev) = (&rows[i], &rows[i - 1]);
        let curr_key = keys[i];
        let curr_pos = hit_positions(curr_key, curr);

        let mut tech = TechCounts::default();
        classify_row_tech(
            layout,
            curr,
            prev,
            curr_key,
            prev_key,
            &curr_pos,
            &prev_pos,
            &prev_prev_pos,
            i,
            &mut tech,
        );
        counts += tech;

        out.push(RowAnnotation {
            beat: curr.beat,
            second: curr.second,
            column_mask: curr.tech_mask,
            feet: feet_of(curr_key, curr.tech_mask),
            row_tech: tech,
        });

        prev_prev_pos = prev_pos;
        prev_pos = curr_pos;
        prev_key = curr_key;
    }

    counts
}

#[inline(always)]
fn is_footswitch(prev: Foot, curr: Foot) -> bool {
    prev != Foot::None
        && curr != Foot::None
        && prev != curr
        && OTHER_PART_OF_FOOT[foot_idx(prev)] != curr
}

// --- Parsing ---

#[derive(Clone)]
struct ParsedRow {
    chars: [u8; 8],
    columns: u8,
    mask: u8,
    row: i32,
    beat: f32,
    second: f32,
}

// Process-lifetime analysis cache, initialized through OnceLock and immutable
// afterwards, so concurrent readers need no locks. Callers may warm it by
// creating parity scratch at load time; a first-use miss only enumerates the
// fixed layout in memory and performs one exact allocation (no I/O). Capacity
// is fixed at 85 single or 517 double placements, with no eviction; storage is
// released at process teardown. The parity-cache allocation/cycle benchmarks
// instrument construction. Worst-case work is bounded to 256 masks/517 writes.
struct LayoutCache {
    layout: StageLayout,
    perm_table: PermTable,
}

struct PermTable {
    values: Box<[FootPlacement]>,
    // Low 16 bits are the start, high 16 bits are the length. The largest
    // supported table has fewer than 3,400 entries and at most 24 per mask.
    ranges: [u32; 256],
}

impl PermTable {
    #[inline(always)]
    fn get(&self, mask: u8) -> &[FootPlacement] {
        let range = self.ranges[mask as usize];
        let start = (range & 0xffff) as usize;
        let len = (range >> 16) as usize;
        debug_assert!(start + len <= self.values.len());
        // SAFETY: build_perm_table creates every range from the buffer's length
        // before and after appending that mask, and the boxed buffer never moves.
        unsafe { std::slice::from_raw_parts(self.values.as_ptr().add(start), len) }
    }
}

fn layout_cache_new(layout: StageLayout) -> LayoutCache {
    let perm_table = build_perm_table(&layout);
    LayoutCache { layout, perm_table }
}

// Process-lifetime, single-panel-only table initialized with its owning layout.
// Once warmed it is immutable and lock-free, has 4,096 inline entries, never
// misses or evicts, performs no heap allocation, and drops at process teardown.
fn facing_cost4(layout: &StageLayout) -> &'static [f32; SINGLE_STATE_COUNT] {
    static COSTS: OnceLock<[f32; SINGLE_STATE_COUNT]> = OnceLock::new();
    COSTS.get_or_init(|| {
        std::array::from_fn(|key| {
            let base = STATE_BASE4[key];
            let mut combined_columns = [Foot::None; MAX_COLUMNS];
            combined_columns[..4].copy_from_slice(&base.combined_columns);
            let state = State {
                combined_columns,
                where_the_feet_are: base.where_the_feet_are,
                occupied_mask: base.occupied_mask,
                moved_mask: 0,
                holding_mask: 0,
            };
            calc_facing_cost(layout, &state)
        })
    })
}

fn spin_class4(layout: &StageLayout) -> &'static [u8; SINGLE_STATE_COUNT] {
    // Process-lifetime dance-single table: OnceLock provides thread-safe startup,
    // parity scratch creation warms all 4,096 inline bytes before solving, and
    // solves have no misses, allocation, eviction, or destruction work. The
    // cycle/allocation harnesses cover its constant lookup and worst-row cost.
    static CLASSES: OnceLock<[u8; SINGLE_STATE_COUNT]> = OnceLock::new();
    CLASSES.get_or_init(|| {
        std::array::from_fn(|key| {
            let state = state_from_key::<4>(key as u32);
            spin_class(layout, &state, false) | (spin_class(layout, &state, true) << 2)
        })
    })
}

#[inline(always)]
fn cached_spin_cost4<const BRANCHLESS: bool>(
    classes: &[u8],
    initial_key: u32,
    result_key: u32,
) -> f32 {
    let initial = classes[initial_key as usize & (SINGLE_STATE_COUNT - 1)] & 0b11;
    let result = classes[result_key as usize & (SINGLE_STATE_COUNT - 1)] >> 2;
    let is_spin = initial + result == 3;
    if BRANCHLESS {
        [0.0, SPIN_WEIGHT][is_spin as usize]
    } else if is_spin {
        SPIN_WEIGHT
    } else {
        0.0
    }
}

fn build_perm_table(layout: &StageLayout) -> PermTable {
    let col_count = layout_cols(layout);
    let mask_count = 1usize << col_count;
    let mut ranges = [0u32; 256];
    let mut values = Vec::with_capacity(PERM_TOTALS[col_count]);

    for (mask, range) in ranges.iter_mut().enumerate().take(mask_count) {
        if mask.count_ones() > 4 {
            continue;
        }
        let start = values.len();
        let mut cols = [Foot::None; MAX_COLUMNS];
        permute_row(
            layout,
            mask as u8,
            &mut cols,
            0,
            col_count,
            0,
            &mut |placement| values.push(placement),
        );
        let len = values.len() - start;
        debug_assert!(start <= u16::MAX as usize && len <= u16::MAX as usize);
        *range = start as u32 | ((len as u32) << 16);
    }

    assert_eq!(values.len(), PERM_TOTALS[col_count]);
    PermTable {
        values: values.into_boxed_slice(),
        ranges,
    }
}

#[cfg(feature = "bench-support")]
fn build_legacy_perm_table(layout: &StageLayout) -> [Box<[FootPlacement]>; 256] {
    let col_count = layout_cols(layout);
    std::array::from_fn(|mask| {
        let bits = mask.count_ones() as usize;
        if bits > 4 {
            return Vec::new().into_boxed_slice();
        }

        let mut cols = [Foot::None; MAX_COLUMNS];
        let mut perms = Vec::with_capacity(PERM_CAP[bits]);
        permute_row(
            layout,
            mask as u8,
            &mut cols,
            0,
            col_count,
            0,
            &mut |placement| perms.push(placement),
        );
        perms.into_boxed_slice()
    })
}

#[cfg(feature = "bench-support")]
fn perm_fingerprint<'a>(
    mask_count: usize,
    mut get: impl FnMut(u8) -> &'a [FootPlacement],
) -> (usize, u64) {
    let mut entries = 0usize;
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for mask in 0..mask_count {
        let perms = get(mask as u8);
        entries += perms.len();
        hash = (hash ^ mask as u64).wrapping_mul(0x0100_0000_01b3);
        hash = (hash ^ perms.len() as u64).wrapping_mul(0x0100_0000_01b3);
        for placement in perms {
            for &foot in placement {
                hash = (hash ^ foot as u64).wrapping_mul(0x0100_0000_01b3);
            }
        }
    }
    (entries, hash)
}

#[cfg(feature = "bench-support")]
fn perm_layout(lanes: usize) -> Option<StageLayout> {
    match lanes {
        4 => Some(dance_single_layout()),
        8 => Some(dance_double_layout()),
        _ => None,
    }
}

#[cfg(feature = "bench-support")]
#[doc(hidden)]
pub fn legacy_perm_build_for_bench(lanes: usize) -> Option<(usize, u64)> {
    let layout = perm_layout(lanes)?;
    let table = build_legacy_perm_table(&layout);
    Some(perm_fingerprint(1usize << lanes, |mask| {
        table[mask as usize].as_ref()
    }))
}

#[cfg(feature = "bench-support")]
#[doc(hidden)]
pub fn packed_perm_build_for_bench(lanes: usize) -> Option<(usize, u64)> {
    let layout = perm_layout(lanes)?;
    let table = build_perm_table(&layout);
    Some(perm_fingerprint(1usize << lanes, |mask| table.get(mask)))
}

#[cfg(feature = "bench-support")]
#[doc(hidden)]
pub fn perm_builds_match_for_bench(lanes: usize) -> bool {
    let Some(layout) = perm_layout(lanes) else {
        return false;
    };
    let mask_count = 1usize << lanes;
    let legacy = build_legacy_perm_table(&layout);
    let packed = build_perm_table(&layout);
    (0..mask_count).all(|mask| legacy[mask].as_ref() == packed.get(mask as u8))
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
fn obj_mask(line: &[u8]) -> u8 {
    if line.len() == 4 {
        return u8::from(line[0] != b'0')
            | (u8::from(line[1] != b'0') << 1)
            | (u8::from(line[2] != b'0') << 2)
            | (u8::from(line[3] != b'0') << 3);
    }
    if line.len() == 8 {
        return u8::from(line[0] != b'0')
            | (u8::from(line[1] != b'0') << 1)
            | (u8::from(line[2] != b'0') << 2)
            | (u8::from(line[3] != b'0') << 3)
            | (u8::from(line[4] != b'0') << 4)
            | (u8::from(line[5] != b'0') << 5)
            | (u8::from(line[6] != b'0') << 6)
            | (u8::from(line[7] != b'0') << 7);
    }

    let mut mask = 0u8;
    for (i, &b) in line.iter().enumerate() {
        mask |= u8::from(b != b'0') << i;
    }
    mask
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
            let mask = obj_mask(&line[..copy]);
            if mask == 0 {
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
                mask,
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
        b'1' => notes.push(note_new(TapNoteType::Tap, col, beat, second, 0.0, row_fake)),
        b'L' => notes.push(note_new(
            TapNoteType::Lift,
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
        b'F' => notes.push(note_new(TapNoteType::Fake, col, beat, second, 0.0, true)),
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
    let mut fake_cursor = timing.map(FakeRowCursor::new);

    for row in rows {
        let row_fake = fake_cursor
            .as_mut()
            .is_some_and(|cursor| cursor.is_fake(row.row));

        let mut mask = row.mask;
        while mask != 0 {
            let c = mask.trailing_zeros() as usize;
            mask &= mask - 1;
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
    calculate_tech_counts(&generator.rows, &generator.result_keys, generator.layout)
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
    let offset = offset as f32;
    let mut bpm_idx = 0usize;
    let mut bpm = bpm_map.first().map_or(60.0, |b| b.1);
    let mut last_beat = 0.0f64;
    let mut last_time = 0.0f64;
    while bpm_idx < bpm_map.len() && bpm_map[bpm_idx].0 <= last_beat {
        bpm = bpm_map[bpm_idx].1;
        bpm_idx += 1;
    }
    analyze_core(cache, minimized_note_data, cols, None, |beat| {
        let target = f64::from(beat);
        if target < last_beat {
            return time_between_beats(0.0, beat, bpm_map) as f32 - offset;
        }

        while bpm_idx < bpm_map.len() {
            let (change_beat, change_bpm) = bpm_map[bpm_idx];
            if change_beat <= last_beat {
                bpm = change_bpm;
                bpm_idx += 1;
                continue;
            }
            if change_beat >= target {
                break;
            }
            last_time += (change_beat - last_beat) * 60.0 / bpm;
            last_beat = change_beat;
            bpm = change_bpm;
            bpm_idx += 1;
        }
        if target > last_beat {
            last_time += (target - last_beat) * 60.0 / bpm;
            last_beat = target;
        }
        last_time as f32 - offset
    })
}

#[must_use]
pub fn analyze_timing_lanes(
    minimized_note_data: &[u8],
    timing: &TimingData,
    lanes: usize,
) -> TechCounts {
    let Some(cache) = layout_for_lanes(lanes) else {
        return TechCounts::default();
    };

    let cols = layout_cols(&cache.layout);
    debug_assert!(!minimized_note_data.contains(&b';'));
    analyze_core(cache, minimized_note_data, cols, Some(timing), |beat| {
        get_time_for_beat_f32(timing, f64::from(beat)) as f32
    })
}

pub struct TimingRowsScratch<const LANES: usize> {
    generator: StepParityGenerator,
    hold_heads: Vec<[f32; MAX_COLUMNS]>,
}

#[cfg(feature = "bench-support")]
#[doc(hidden)]
pub struct LegacyTimingRowsScratch<const LANES: usize> {
    generator: StepParityGenerator,
    hold_heads: Vec<[f32; MAX_COLUMNS]>,
    dp: LegacyDp,
}

pub fn timing_rows_scratch<const LANES: usize>() -> Option<TimingRowsScratch<LANES>> {
    let cache = layout_for_lanes(LANES)?;
    Some(TimingRowsScratch {
        generator: parity_gen(cache),
        hold_heads: Vec::new(),
    })
}

#[cfg(feature = "bench-support")]
#[doc(hidden)]
pub fn legacy_timing_rows_scratch<const LANES: usize>() -> Option<LegacyTimingRowsScratch<LANES>> {
    let cache = layout_for_lanes(LANES)?;
    Some(LegacyTimingRowsScratch {
        generator: parity_gen(cache),
        hold_heads: Vec::new(),
        dp: LegacyDp::default(),
    })
}

#[cfg(feature = "bench-support")]
#[doc(hidden)]
pub fn analyze_timing_rows_legacy_for_bench<const LANES: usize>(
    rows: &[[u8; LANES]],
    row_to_beat: &[f32],
    timing: &TimingData,
    has_holds: bool,
    scratch: &mut LegacyTimingRowsScratch<LANES>,
) -> TechCounts {
    let cols = layout_cols(scratch.generator.layout);
    parity_reset(&mut scratch.generator, cols);
    scratch.generator.rows.reserve(rows.len());
    parity_create_rows_from_arrays(
        &mut scratch.generator,
        &mut scratch.hold_heads,
        rows,
        row_to_beat,
        timing,
        cols,
        has_holds,
    );
    if !legacy_finish(&mut scratch.generator, &mut scratch.dp) {
        return TechCounts::default();
    }
    calculate_tech_counts(
        &scratch.generator.rows,
        &scratch.generator.result_keys,
        scratch.generator.layout,
    )
}

pub fn analyze_timing_rows<const LANES: usize>(
    rows: &[[u8; LANES]],
    row_to_beat: &[f32],
    timing: &TimingData,
    scratch: &mut TimingRowsScratch<LANES>,
) -> TechCounts {
    let has_holds = rows_have_holds(rows, layout_cols(scratch.generator.layout));
    analyze_timing_rows_known_holds(rows, row_to_beat, timing, has_holds, scratch)
}

pub fn analyze_timing_rows_known_holds<const LANES: usize>(
    rows: &[[u8; LANES]],
    row_to_beat: &[f32],
    timing: &TimingData,
    has_holds: bool,
    scratch: &mut TimingRowsScratch<LANES>,
) -> TechCounts {
    let cols = layout_cols(scratch.generator.layout);
    if !parity_analyze_rows(
        &mut scratch.generator,
        &mut scratch.hold_heads,
        rows,
        row_to_beat,
        timing,
        cols,
        has_holds,
    ) {
        return TechCounts::default();
    }
    calculate_tech_counts(
        &scratch.generator.rows,
        &scratch.generator.result_keys,
        scratch.generator.layout,
    )
}

#[cfg(feature = "bench-support")]
#[doc(hidden)]
#[must_use]
pub fn analyze_timing_rows_tap_path_for_bench<const LANES: usize>(
    rows: &[[u8; LANES]],
    row_to_beat: &[f32],
    timing: &TimingData,
    has_holds: bool,
    legacy_tap_path: bool,
    scratch: &mut TimingRowsScratch<LANES>,
) -> TechCounts {
    scratch.generator.legacy_tap_path = legacy_tap_path;
    analyze_timing_rows_known_holds(rows, row_to_beat, timing, has_holds, scratch)
}

#[cfg(feature = "bench-support")]
#[doc(hidden)]
#[must_use]
pub fn analyze_timing_rows_hash_for_bench<const LANES: usize>(
    rows: &[[u8; LANES]],
    row_to_beat: &[f32],
    timing: &TimingData,
    has_holds: bool,
    legacy_hash: bool,
    scratch: &mut TimingRowsScratch<LANES>,
) -> TechCounts {
    scratch.generator.state_map.legacy_hash = legacy_hash;
    analyze_timing_rows_known_holds(rows, row_to_beat, timing, has_holds, scratch)
}

pub fn analyze_and_annotate_timing_rows<const LANES: usize>(
    rows: &[[u8; LANES]],
    row_to_beat: &[f32],
    timing: &TimingData,
    scratch: &mut TimingRowsScratch<LANES>,
) -> (TechCounts, Vec<RowAnnotation>) {
    let has_holds = rows_have_holds(rows, layout_cols(scratch.generator.layout));
    analyze_and_annotate_timing_rows_known_holds(rows, row_to_beat, timing, has_holds, scratch)
}

pub fn analyze_and_annotate_timing_rows_known_holds<const LANES: usize>(
    rows: &[[u8; LANES]],
    row_to_beat: &[f32],
    timing: &TimingData,
    has_holds: bool,
    scratch: &mut TimingRowsScratch<LANES>,
) -> (TechCounts, Vec<RowAnnotation>) {
    let mut out = Vec::new();
    let counts = analyze_and_annotate_timing_rows_known_holds_in(
        rows,
        row_to_beat,
        timing,
        has_holds,
        scratch,
        &mut out,
    );
    (counts, out)
}

/// An allocation-reusing variant of [`analyze_and_annotate_timing_rows`].
/// Clears `out` while retaining its capacity, then fills it with the current
/// row annotations and returns their aggregate tech counts.
pub fn analyze_and_annotate_timing_rows_in<const LANES: usize>(
    rows: &[[u8; LANES]],
    row_to_beat: &[f32],
    timing: &TimingData,
    scratch: &mut TimingRowsScratch<LANES>,
    out: &mut Vec<RowAnnotation>,
) -> TechCounts {
    let has_holds = rows_have_holds(rows, layout_cols(scratch.generator.layout));
    analyze_and_annotate_timing_rows_known_holds_in(
        rows,
        row_to_beat,
        timing,
        has_holds,
        scratch,
        out,
    )
}

/// An allocation-reusing variant of
/// [`analyze_and_annotate_timing_rows_known_holds`].
pub fn analyze_and_annotate_timing_rows_known_holds_in<const LANES: usize>(
    rows: &[[u8; LANES]],
    row_to_beat: &[f32],
    timing: &TimingData,
    has_holds: bool,
    scratch: &mut TimingRowsScratch<LANES>,
    out: &mut Vec<RowAnnotation>,
) -> TechCounts {
    out.clear();
    let cols = layout_cols(scratch.generator.layout);
    if !parity_analyze_rows(
        &mut scratch.generator,
        &mut scratch.hold_heads,
        rows,
        row_to_beat,
        timing,
        cols,
        has_holds,
    ) {
        return TechCounts::default();
    }
    collect_annotations_in(
        &scratch.generator.rows,
        &scratch.generator.result_keys,
        scratch.generator.layout,
        out,
    )
}

/// Per-row crossover annotations for the given row arrays, mirroring
/// [`analyze_timing_rows`] but returning [`RowAnnotation`] for every judged
/// parity row instead of aggregate [`TechCounts`]. Intended for gameplay
/// features (e.g. crossover cues) that need to know which rows are crossovers
/// and where the feet land.
pub fn annotate_timing_rows<const LANES: usize>(
    rows: &[[u8; LANES]],
    row_to_beat: &[f32],
    timing: &TimingData,
    scratch: &mut TimingRowsScratch<LANES>,
) -> Vec<RowAnnotation> {
    let has_holds = rows_have_holds(rows, layout_cols(scratch.generator.layout));
    annotate_timing_rows_known_holds(rows, row_to_beat, timing, has_holds, scratch)
}

pub fn annotate_timing_rows_known_holds<const LANES: usize>(
    rows: &[[u8; LANES]],
    row_to_beat: &[f32],
    timing: &TimingData,
    has_holds: bool,
    scratch: &mut TimingRowsScratch<LANES>,
) -> Vec<RowAnnotation> {
    let mut out = Vec::new();
    analyze_and_annotate_timing_rows_known_holds_in(
        rows,
        row_to_beat,
        timing,
        has_holds,
        scratch,
        &mut out,
    );
    out
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stats::minimize_rows_typed;
    use crate::timing::{TimingFormat, timing_data_from_chart_data};

    fn basic_timing() -> TimingData {
        timing_data_from_chart_data(
            0.0,
            0.0,
            None,
            "0.000=120.000",
            None,
            "",
            None,
            "",
            None,
            "",
            None,
            "",
            None,
            "",
            None,
            "",
            TimingFormat::Ssc,
            true,
        )
    }

    #[test]
    fn hot_storage_stays_compact() {
        assert_eq!(std::mem::size_of::<Row>(), 16);
        assert_eq!(std::mem::size_of::<StateBase4>(), 10);
        assert_eq!(std::mem::size_of::<StepParityNode>(), 8);
        assert_eq!(std::mem::size_of::<SingleParityNode>(), 4);
        assert_eq!(std::mem::size_of::<LayerLink>(), 4);
        assert_eq!(std::mem::size_of::<RowMapEntry>(), 8);
        assert_eq!(std::mem::size_of_val(&STATE_BASE4), 40_960);
        assert_eq!(std::mem::size_of_val(&TAP_BASE4), 131_072);
        assert_eq!(
            std::mem::size_of_val(facing_cost4(&dance_single_cache().layout)),
            16_384
        );
        assert_eq!(
            std::mem::size_of_val(spin_class4(&dance_single_cache().layout)),
            4_096
        );
    }

    #[test]
    fn row_map_epoch_wrap_forgets_entries() {
        let mut map = row_map_new();
        row_map_reset::<4, true>(&mut map, 1);
        let RowMapProbe::Vacant(slot) = row_map_probe::<4, true>(&map, 7) else {
            panic!("new map entry should be vacant");
        };
        row_map_insert_at::<4, true>(&mut map, slot, 7, row_map_val_mask::<4, true>() as usize);
        assert!(matches!(
            row_map_probe::<4, true>(&map, 7),
            RowMapProbe::Found(value) if value == row_map_val_mask::<4, true>() as usize
        ));

        map.epoch = row_map_epoch_max::<4, true>();
        row_map_reset::<4, true>(&mut map, 1);
        assert_eq!(map.epoch, 1);
        assert!(matches!(
            row_map_probe::<4, true>(&map, 7),
            RowMapProbe::Vacant(_)
        ));
    }

    #[test]
    fn single_row_hash_uses_all_state_regions() {
        const MASK: usize = ROW_MAP_MIN_CAP - 1;
        let keys = [0x19, 0x0100_0019, 0x1000_0019, 0x1100_0019, 0x619];
        let buckets = keys.map(|key| row_map_hash_for_key::<4>(key) & MASK);
        for i in 0..buckets.len() {
            for &other in &buckets[i + 1..] {
                assert_ne!(buckets[i], other);
            }
        }
    }

    #[test]
    fn movement_table_matches_scalar_geometry() {
        for layout in [dance_single_layout(), dance_double_layout()] {
            for initial in 0..layout_cols(&layout) {
                for result in 0..layout_cols(&layout) {
                    let dx = layout.columns[initial].x - layout.columns[result].x;
                    let dy = layout.columns[initial].y - layout.columns[result].y;
                    let expected = (dx * dx + dy * dy).sqrt() * DISTANCE_WEIGHT;
                    assert_eq!(
                        layout_movement_cost(&layout, initial, result).to_bits(),
                        expected.to_bits()
                    );
                }
            }
        }
    }

    fn assert_rows_match_lanes(data: &[u8], has_holds: bool) {
        let timing = basic_timing();
        let (minimized, _stats, _densities, rows, row_to_beat, _last) =
            minimize_rows_typed::<4>(data);
        let Some(mut scratch) = timing_rows_scratch::<4>() else {
            panic!("dance-single parity layout should exist");
        };

        assert_eq!(
            analyze_timing_lanes(&minimized, &timing, 4),
            analyze_timing_rows_known_holds(&rows, &row_to_beat, &timing, has_holds, &mut scratch)
        );
    }

    #[test]
    fn lift_only_row_does_not_panic() {
        let Some(mut scratch) = timing_rows_scratch::<4>() else {
            panic!("dance-single parity layout should exist");
        };
        let rows = [[b'0', b'L', b'0', b'0']];
        let beats = [0.0];

        let counts = analyze_timing_rows_known_holds(
            &rows,
            &beats,
            &TimingData::default(),
            false,
            &mut scratch,
        );

        assert_eq!(counts, TechCounts::default());
    }

    #[test]
    fn no_hold_rows_match_lanes_path() {
        assert_rows_match_lanes(
            b"1000
0100
0010
0001
,
1100
0011
0000
1000
;",
            false,
        );
    }

    #[test]
    fn hold_rows_match_lanes_path() {
        assert_rows_match_lanes(
            b"2000
0100
3000
0001
,
0200
0010
0300
1000
;",
            true,
        );
    }

    #[cfg(feature = "bench-support")]
    #[test]
    fn compact_arena_matches_legacy_solver() {
        let rows = [
            [b'1', b'0', b'0', b'0'],
            [b'0', b'1', b'0', b'0'],
            [b'0', b'0', b'1', b'0'],
            [b'0', b'0', b'0', b'1'],
            [b'1', b'1', b'0', b'0'],
            [b'0', b'0', b'1', b'1'],
        ];
        let beats = [0.0, 0.25, 0.5, 0.75, 1.0, 1.25];
        let timing = basic_timing();
        let mut compact = timing_rows_scratch::<4>().expect("dance-single layout exists");
        let mut legacy = legacy_timing_rows_scratch::<4>().expect("dance-single layout exists");

        assert_eq!(
            analyze_timing_rows_known_holds(&rows, &beats, &timing, false, &mut compact),
            analyze_timing_rows_legacy_for_bench(&rows, &beats, &timing, false, &mut legacy),
        );
    }

    #[test]
    fn invalid_hold_rows_match_lanes_path() {
        assert_rows_match_lanes(
            b"2000
0100
0001
,
0200
0020
0010
0300
1000
;",
            true,
        );
    }

    fn annotations_for(data: &[u8]) -> (TechCounts, Vec<RowAnnotation>) {
        let timing = basic_timing();
        let (minimized, _stats, _densities, rows, row_to_beat, _last) =
            minimize_rows_typed::<4>(data);
        let counts = analyze_timing_lanes(&minimized, &timing, 4);
        let mut scratch = timing_rows_scratch::<4>().expect("dance-single layout");
        let has_holds = rows_have_holds(&rows, 4);
        let annotations =
            annotate_timing_rows_known_holds(&rows, &row_to_beat, &timing, has_holds, &mut scratch);
        (counts, annotations)
    }

    #[test]
    fn annotation_crossover_count_matches_tech_counts() {
        // A crossover-heavy candle/crossover stream in dance-single.
        let data = b"1000
0010
0001
0010
1000
0010
0001
0010
1000
;";
        let (counts, annotations) = annotations_for(data);
        let annotated_crossovers = annotations
            .iter()
            .filter(|a| a.row_tech.crossovers > 0)
            .count() as u32;
        assert_eq!(annotated_crossovers, counts.crossovers);
        // The pattern is designed to actually contain crossovers, so this
        // guards against the annotation path silently classifying nothing.
        assert!(
            counts.crossovers > 0,
            "expected the test pattern to crossover"
        );
    }

    #[test]
    fn per_row_tech_sums_to_aggregate() {
        // Every per-row tech category, summed over the chart, must reproduce the
        // aggregate TechCounts (computed independently). This guards the shared
        // classify_row_tech invariant for the whole tech vector, not just
        // crossovers.
        let data = b"1000
0010
0001
0010
1000
0010
0001
0010
1000
;";
        let (counts, annotations) = annotations_for(data);
        let mut summed = TechCounts::default();
        for a in &annotations {
            summed += a.row_tech;
        }
        assert_eq!(summed, counts);
    }

    #[test]
    fn single_rows_classify_jacks_and_doublesteps() {
        let row = |second: f32, mask: u8| {
            let mut row = row_new();
            row.second = second;
            row.note_count = mask.count_ones() as u8;
            row.note_mask = mask;
            row.tech_mask = mask;
            row
        };
        let placement = |feet: &[(usize, Foot)]| {
            let mut placement = [Foot::None; MAX_COLUMNS];
            for &(column, foot) in feet {
                placement[column] = foot;
            }
            placement
        };
        let rows = [
            row(0.0, 0b0001),
            row(0.1, 0b0001),
            row(0.3, 0b0010),
            row(0.5, 0b0010),
            row(1.0, 0b0011),
        ];
        let placements = [
            placement(&[(0, Foot::LeftHeel)]),
            placement(&[(0, Foot::LeftHeel)]),
            placement(&[(1, Foot::LeftHeel)]),
            placement(&[(1, Foot::LeftHeel)]),
            placement(&[(0, Foot::LeftHeel), (1, Foot::LeftToe)]),
        ];
        let keys = placements.map(|placement| {
            placement
                .iter()
                .enumerate()
                .fold(0u32, |key, (column, &foot)| {
                    key | (foot as u32) << (column * 3)
                })
        });
        let layout = dance_single_layout();

        let counts = calculate_tech_counts(&rows, &keys, &layout);
        assert_eq!(counts.jacks, 1);
        assert_eq!(counts.doublesteps, 1);
        assert_eq!(counts.brackets, 1);
        assert_eq!(
            calculate_tech_counts(&rows, &keys, &dance_double_layout()),
            counts
        );

        let mut annotations = Vec::new();
        assert_eq!(
            collect_annotations_in(&rows, &keys, &layout, &mut annotations),
            counts
        );
        assert_eq!(annotations[1].row_tech.jacks, 1);
        assert_eq!(annotations[2].row_tech.doublesteps, 1);
        assert_eq!(annotations[3].row_tech, TechCounts::default());
        assert_eq!(annotations[4].row_tech.brackets, 1);
    }

    #[test]
    fn combined_counts_and_annotations_match_separate_solves() {
        let data = b"2000
0010
3000
0001
,
1000
0010
0001
0010
1000
;";
        let timing = basic_timing();
        let (_minimized, _stats, _densities, rows, row_to_beat, _last) =
            minimize_rows_typed::<4>(data);
        let has_holds = rows_have_holds(&rows, 4);

        let mut separate_scratch = timing_rows_scratch::<4>().expect("dance-single layout");
        let separate_counts = analyze_timing_rows_known_holds(
            &rows,
            &row_to_beat,
            &timing,
            has_holds,
            &mut separate_scratch,
        );
        let separate_annotations = annotate_timing_rows_known_holds(
            &rows,
            &row_to_beat,
            &timing,
            has_holds,
            &mut separate_scratch,
        );

        let mut combined_scratch = timing_rows_scratch::<4>().expect("dance-single layout");
        let combined = analyze_and_annotate_timing_rows_known_holds(
            &rows,
            &row_to_beat,
            &timing,
            has_holds,
            &mut combined_scratch,
        );

        assert_eq!(combined.0, separate_counts);
        assert_eq!(combined.1, separate_annotations);
    }

    #[test]
    fn in_place_annotations_match_owned_and_reuse_capacity() {
        let data = b"1000
0010
0001
0100
1001
;";
        let timing = basic_timing();
        let (_minimized, _stats, _densities, rows, row_to_beat, _last) =
            minimize_rows_typed::<4>(data);
        let mut owned_scratch = timing_rows_scratch::<4>().expect("dance-single layout");
        let owned =
            analyze_and_annotate_timing_rows(&rows, &row_to_beat, &timing, &mut owned_scratch);

        let mut reused = Vec::with_capacity(rows.len() + 8);
        reused.push(RowAnnotation::default());
        let allocation = reused.as_ptr();
        let capacity = reused.capacity();
        let mut reused_scratch = timing_rows_scratch::<4>().expect("dance-single layout");
        let counts = analyze_and_annotate_timing_rows_in(
            &rows,
            &row_to_beat,
            &timing,
            &mut reused_scratch,
            &mut reused,
        );

        assert_eq!(counts, owned.0);
        assert_eq!(reused, owned.1);
        assert_eq!(reused.capacity(), capacity);
        assert_eq!(reused.as_ptr(), allocation);

        let empty_counts = analyze_and_annotate_timing_rows_in(
            &[],
            &[],
            &timing,
            &mut reused_scratch,
            &mut reused,
        );
        assert_eq!(empty_counts, TechCounts::default());
        assert!(reused.is_empty());
        assert_eq!(reused.capacity(), capacity);
    }

    #[test]
    fn long_solve_keeps_row_annotations_aligned() {
        const ROWS: usize = 128;
        const MASKS: [u8; 8] = [
            0b0001, 0b0100, 0b1000, 0b0010, 0b0011, 0b1100, 0b0101, 0b1010,
        ];
        let rows: Vec<[u8; 4]> = (0..ROWS)
            .map(|idx| {
                let mask = MASKS[idx % MASKS.len()];
                std::array::from_fn(|column| {
                    if mask & (1 << column) == 0 {
                        b'0'
                    } else {
                        b'1'
                    }
                })
            })
            .collect();
        let beats: Vec<_> = (0..u16::try_from(ROWS).expect("test row count fits u16"))
            .map(|idx| f32::from(idx) * 0.25)
            .collect();
        let mut scratch = timing_rows_scratch::<4>().expect("dance-single layout");

        let (counts, annotations) = analyze_and_annotate_timing_rows_known_holds(
            &rows,
            &beats,
            &basic_timing(),
            false,
            &mut scratch,
        );

        assert_eq!(annotations.len(), ROWS);
        let mut summed = TechCounts::default();
        for (idx, annotation) in annotations.iter().enumerate() {
            let mask = MASKS[idx % MASKS.len()];
            assert_eq!(annotation.beat.to_bits(), beats[idx].to_bits());
            assert_eq!(annotation.column_mask, mask);
            assert_eq!(annotation.foot_count(), mask.count_ones());
            summed += annotation.row_tech;
        }
        assert_eq!(summed, counts);
    }

    #[test]
    fn packed_state_keys_round_trip_transition_states() {
        let mut placement = [Foot::None; MAX_COLUMNS];
        placement[0] = Foot::LeftHeel;
        placement[1] = Foot::RightHeel;

        let (no_holds, _, no_holds_key) =
            parity_result_state_no_holds::<4>(&state_new(), &placement, 0b0011);
        assert_eq!(state_from_key::<4>(no_holds_key), no_holds);

        let (with_hold, _, with_hold_key) =
            parity_result_state::<4>(&state_new(), &placement, 0b0001, 0b0011);
        assert_eq!(state_from_key::<4>(with_hold_key), with_hold);

        // A failed/empty placement can legitimately produce key zero. It is
        // distinct from the synthetic starting state because its foot
        // positions use INVALID_COLUMN rather than ITGmania's initial zeros.
        let (zero_state, _, zero_key) =
            parity_result_state_no_holds::<4>(&state_new(), &NO_PERMS[0], 0b0001);
        assert_eq!(zero_key, 0);
        assert_eq!(state_from_key::<4>(zero_key), zero_state);
        assert_ne!(zero_state, state_new());
    }

    #[test]
    fn single_state_tables_match_scalar_calculation() {
        let cache = dance_single_cache();
        let facing_cost4 = facing_cost4(&cache.layout);
        let spin_class4 = spin_class4(&cache.layout);
        for key in 0..SINGLE_STATE_COUNT as u32 {
            let table_state = state_from_key::<4>(key);
            assert_eq!(table_state, state_from_key_scalar::<4>(key));
            assert_eq!(
                facing_cost4[key as usize].to_bits(),
                calc_facing_cost(&cache.layout, &table_state).to_bits()
            );
            assert_eq!(
                spin_class4[key as usize],
                spin_class(&cache.layout, &table_state, false)
                    | (spin_class(&cache.layout, &table_state, true) << 2)
            );
        }

        let mut initial_keys = [None; 3];
        for (key, &classes) in spin_class4.iter().enumerate() {
            let initial = usize::from(classes & 0b11);
            if initial < initial_keys.len() {
                initial_keys[initial].get_or_insert(key as u32);
            }
        }
        for initial_key in initial_keys.into_iter().flatten() {
            for result_key in 0..SINGLE_STATE_COUNT as u32 {
                assert_eq!(
                    cached_spin_cost4::<true>(spin_class4, initial_key, result_key).to_bits(),
                    cached_spin_cost4::<false>(spin_class4, initial_key, result_key).to_bits()
                );
            }
        }
    }

    #[test]
    fn foot_side_arithmetic_matches_hold_switch_classification() {
        let feet = [
            Foot::None,
            Foot::LeftHeel,
            Foot::LeftToe,
            Foot::RightHeel,
            Foot::RightToe,
        ];
        for foot in feet {
            for initial in feet {
                let expected = (matches!(foot, Foot::LeftHeel | Foot::LeftToe)
                    && !matches!(initial, Foot::LeftHeel | Foot::LeftToe))
                    || (matches!(foot, Foot::RightHeel | Foot::RightToe)
                        && !matches!(initial, Foot::RightHeel | Foot::RightToe));
                let side = foot_side(foot);
                let switched = side != 0 && side != foot_side(initial);
                assert_eq!(switched, expected);
            }
        }
    }

    #[test]
    fn specialized_column_transitions_agree_for_single_panel_states() {
        let cache = dance_single_cache();
        for active_mask in [0u8, 1, 2, 4, 8] {
            let hit_col = active_mask.trailing_zeros() as usize;
            let expected: &[Foot] = if active_mask == 0 {
                &IDLE_FEET
            } else {
                &TAP_FEET
            };
            let permutations = cache.perm_table.get(active_mask);
            assert_eq!(permutations.len(), expected.len());
            for (placement, &foot) in permutations.iter().zip(expected) {
                assert_eq!(placement.get(hit_col).copied().unwrap_or(Foot::None), foot);
            }
        }
        let canonical_state = |state: &State, key: u32| {
            let mut seen = 0u8;
            let mut canonical = 0u32;
            let unique = state.combined_columns[..4]
                .iter()
                .enumerate()
                .all(|(column, &foot)| {
                    let mask = FOOT_MASKS[foot_idx(foot)];
                    let unique = seen & mask == 0;
                    seen |= mask;
                    canonical |= (foot as u32) << (column * 3);
                    unique
                });
            unique && canonical == key & (SINGLE_STATE_COUNT - 1) as u32
        };
        let initial_states = std::iter::once((state_new(), 0))
            .chain((0..SINGLE_STATE_COUNT as u32).map(|key| (state_from_key::<4>(key), key)));

        for (initial, initial_key) in initial_states {
            let initial_canonical = canonical_state(&initial, initial_key);
            for active_mask in 0u8..16 {
                let permutations = cache.perm_table.get(active_mask);
                let permutations = if permutations.is_empty() {
                    &NO_PERMS
                } else {
                    permutations
                };
                for placement in permutations {
                    let no_holds =
                        parity_result_state_no_holds::<4>(&initial, placement, active_mask);
                    if initial_canonical {
                        assert!(canonical_state(&no_holds.0, no_holds.2));
                    }
                    assert_eq!(
                        no_holds,
                        parity_result_state_no_holds::<8>(&initial, placement, active_mask)
                    );
                    assert_eq!(
                        parity_result_key4::<false>(&initial, placement, 0, active_mask),
                        (no_holds.1, no_holds.2)
                    );
                    if active_mask.count_ones() <= 1 && initial_canonical {
                        assert_eq!(
                            parity_result_tap_key4(
                                initial_key,
                                placement
                                    .get(active_mask.trailing_zeros() as usize)
                                    .copied()
                                    .unwrap_or(Foot::None),
                                active_mask.trailing_zeros() as usize,
                            ),
                            no_holds.2,
                            "initial={initial_key:#x} active={active_mask:#x} placement={placement:?}"
                        );
                    }
                    let hold_mask = active_mask & 0b0101;
                    let with_holds =
                        parity_result_state::<4>(&initial, placement, hold_mask, active_mask);
                    if initial_canonical {
                        assert!(canonical_state(&with_holds.0, with_holds.2));
                    }
                    assert_eq!(
                        with_holds,
                        parity_result_state::<8>(&initial, placement, hold_mask, active_mask)
                    );
                    assert_eq!(
                        parity_result_key4::<true>(&initial, placement, hold_mask, active_mask,),
                        (with_holds.1, with_holds.2)
                    );
                }
            }
        }

        let mut max_states = 0usize;
        let mut max_row = (0u8, 0u8);
        for active_mask in 0u8..16 {
            let permutations = cache.perm_table.get(active_mask);
            let permutations = if permutations.is_empty() {
                &NO_PERMS
            } else {
                permutations
            };
            for hold_mask in 0u8..16 {
                if hold_mask & !active_mask != 0 {
                    continue;
                }
                let mut keys = Vec::new();
                for initial_key in 0..SINGLE_STATE_COUNT as u32 {
                    let initial = state_from_key::<4>(initial_key);
                    for placement in permutations {
                        keys.push(
                            parity_result_key4::<true>(&initial, placement, hold_mask, active_mask)
                                .1,
                        );
                    }
                }
                keys.sort_unstable();
                keys.dedup();
                if keys.len() > max_states {
                    max_states = keys.len();
                    max_row = (active_mask, hold_mask);
                }
            }
        }
        assert!(
            max_states <= SINGLE_LAYER_MAX,
            "single row {max_row:?} produced {max_states} states"
        );
    }

    #[test]
    fn simple_tap_cost_matches_general_path() {
        let cache = dance_single_cache();
        let facing_costs = facing_cost4(&cache.layout);
        let spin_classes = spin_class4(&cache.layout);
        let mut initial_states = Vec::with_capacity(1 + SINGLE_STATE_COUNT * 16);
        initial_states.push((state_new(), 0));
        for base_key in 0..SINGLE_STATE_COUNT as u32 {
            for moved_mask in [0u8, 1, 2, 3, 4, 8, 12, 15] {
                for holding_mask in [0, moved_mask] {
                    let key =
                        base_key | (u32::from(moved_mask) << 24) | (u32::from(holding_mask) << 28);
                    initial_states.push((state_from_key::<4>(key), key));
                }
            }
        }

        for (initial, initial_key) in initial_states {
            let left_moved = foot_moved_not_holding(&initial, &LEFT_PAIR);
            let right_moved = foot_moved_not_holding(&initial, &RIGHT_PAIR);
            for active_mask in [0, 1, 2, 4, 8] {
                let mut row = row_new();
                row.note_mask = active_mask;
                row.tech_mask = active_mask;
                row.note_count = active_mask.count_ones() as u8;
                let row_ctx = row_cost_ctx(&row, &cache.layout);
                let permutations = cache.perm_table.get(active_mask);
                let permutations = if permutations.is_empty() {
                    &NO_PERMS
                } else {
                    permutations
                };

                for placement in permutations {
                    let (result, hit, key) =
                        parity_result_state_no_holds::<4>(&initial, placement, active_mask);
                    let initial_base = StateBase4 {
                        combined_columns: std::array::from_fn(|i| initial.combined_columns[i]),
                        where_the_feet_are: initial.where_the_feet_are,
                        occupied_mask: initial.occupied_mask,
                    };
                    for elapsed in [0.05, 0.25, 0.5] {
                        let facing = facing_costs[key as usize & (SINGLE_STATE_COUNT - 1)];
                        let hit_col = active_mask.trailing_zeros() as usize;
                        let cost_ctx = tap_cost_ctx(&cache.layout, hit_col, elapsed);
                        let moved_foot = placement.get(hit_col).copied().unwrap_or(Foot::None);
                        let spin = cached_spin_cost4::<true>(spin_classes, initial_key, key);
                        let simple = calc_tap_cost(
                            &initial_base,
                            moved_foot,
                            hit_col,
                            row_ctx.side_mask != 0,
                            left_moved,
                            right_moved,
                            false,
                            facing,
                            spin,
                            &cost_ctx,
                        );
                        let legacy = calc_tap_cost_legacy(
                            &cache.layout,
                            &initial_base,
                            key,
                            hit_col,
                            row_ctx.side_mask != 0,
                            elapsed,
                            left_moved,
                            right_moved,
                            false,
                            facing,
                            cached_spin_cost4::<true>(spin_classes, initial_key, key),
                        );
                        let general = calc_action_cost::<false>(
                            &cache.layout,
                            &initial,
                            &result,
                            placement,
                            hit,
                            &row,
                            row_ctx,
                            elapsed,
                            left_moved,
                            right_moved,
                            false,
                            0.0,
                            0.0,
                        );
                        assert_eq!(simple.to_bits(), legacy.to_bits());
                        assert_eq!(simple.to_bits(), general.to_bits());
                    }
                }
            }
        }
    }

    #[test]
    fn annotation_rows_align_with_parity_rows() {
        let data = b"1000
0100
0010
0001
,
1100
0011
0000
1000
;";
        let (_counts, annotations) = annotations_for(data);
        assert!(!annotations.is_empty());
        // Beats are strictly non-decreasing and every annotated row carries at
        // least one foot-bearing column.
        let mut prev_beat = f32::NEG_INFINITY;
        for ann in &annotations {
            assert!(ann.beat >= prev_beat, "annotation beats must be ordered");
            prev_beat = ann.beat;
            assert!(ann.column_mask != 0, "annotated row should have notes");
            assert_eq!(ann.foot_count(), ann.column_mask.count_ones());
        }
        // The first row can never be a crossover (no predecessor).
        assert_eq!(annotations[0].row_tech.crossovers, 0);
    }

    #[test]
    fn annotation_empty_for_no_rows() {
        let mut scratch = timing_rows_scratch::<4>().expect("dance-single layout");
        let rows: [[u8; 4]; 0] = [];
        let beats: [f32; 0] = [];
        let annotations = annotate_timing_rows_known_holds(
            &rows,
            &beats,
            &TimingData::default(),
            false,
            &mut scratch,
        );
        assert!(annotations.is_empty());
    }
}

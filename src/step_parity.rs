use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hasher};
use std::sync::OnceLock;

use crate::timing::{ROWS_PER_BEAT, TimingData, beat_to_note_row_f32};

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
const TWISTED_FOOT_WEIGHT: f32 = 100000.0;
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

impl Foot {
    #[inline(always)]
    const fn as_index(self) -> usize {
        self as usize
    }

    #[inline(always)]
    const fn is_left(self) -> bool {
        matches!(self, Foot::LeftHeel | Foot::LeftToe)
    }

    #[inline(always)]
    const fn is_right(self) -> bool {
        matches!(self, Foot::RightHeel | Foot::RightToe)
    }
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

#[derive(Default)]
struct IdentityHasher(u64);

impl Hasher for IdentityHasher {
    fn write(&mut self, bytes: &[u8]) {
        let mut hash = 0u64;
        for &b in bytes {
            hash = hash.wrapping_mul(0x100_0000_01b3).wrapping_add(b as u64);
        }
        self.0 = hash;
    }
    fn write_usize(&mut self, v: usize) {
        self.0 = v as u64;
    }
    fn write_u32(&mut self, v: u32) {
        self.0 = v as u64;
    }
    fn write_u64(&mut self, v: u64) {
        self.0 = v;
    }
    fn finish(&self) -> u64 {
        self.0
    }
}

type FastMap<K, V> = HashMap<K, V, BuildHasherDefault<IdentityHasher>>;

#[derive(Debug, Clone, Copy)]
struct Edge {
    to: u32,
    cost: f32,
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

impl StageLayout {
    fn new_dance_single() -> Self {
        Self::new(
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

    fn new_dance_double() -> Self {
        Self::new(
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

    fn new(
        points: &[StagePoint],
        up_mask: u8,
        down_mask: u8,
        side_mask: u8,
    ) -> Self {
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
                (base as f64).powf(1.8) as f32 * 100.0 * FACING_WEIGHT
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
                    (columns[right].x - columns[left].x) as f64,
                    (columns[right].y - columns[left].y) as f64,
                );
                let dist = (dx * dx + dy * dy).sqrt();
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
                let sq = dx * dx + dy * dy;
                let idx = l * MAX_COLUMNS + r;
                bracket_ok[idx] = sq <= 2.0;
                let dist = (sq as f64).sqrt();
                hold_switch_cost[idx] = dist as f32 * HOLDSWITCH_WEIGHT;
                dist_weighted[idx] = dist * DISTANCE_WEIGHT as f64;
            }
        }

        Self {
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
    fn column_count(&self) -> usize {
        self.cols as usize
    }

    #[inline(always)]
    fn bracket_check(&self, c1: usize, c2: usize) -> bool {
        self.bracket_ok[c1 * MAX_COLUMNS + c2]
    }

    #[inline(always)]
    fn get_hold_switch_cost(&self, c1: usize, c2: usize) -> f32 {
        self.hold_switch_cost[c1 * MAX_COLUMNS + c2]
    }

    #[inline(always)]
    fn get_distance_weighted(&self, c1: usize, c2: usize) -> f64 {
        self.dist_weighted[c1 * MAX_COLUMNS + c2]
    }

    #[inline(always)]
    fn pair_index(&self, left: i8, right: i8) -> usize {
        let max = self.cols as usize;
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
    fn get_facing_x_penalty(&self, l: i8, r: i8) -> f32 {
        self.facing_x_penalty[self.pair_index(l, r)]
    }

    #[inline(always)]
    fn get_facing_y_penalty(&self, l: i8, r: i8) -> f32 {
        self.facing_y_penalty[self.pair_index(l, r)]
    }

    #[inline(always)]
    fn avg_point(&self, l: i8, r: i8) -> StagePoint {
        self.avg_points[self.pair_index(l, r)]
    }
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

impl Row {
    fn new() -> Self {
        Self {
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
}

#[derive(Debug, Clone, Copy)]
struct State {
    columns: [Foot; MAX_COLUMNS],
    combined_columns: [Foot; MAX_COLUMNS],
    where_the_feet_are: [i8; NUM_FEET],
    what_note_the_foot_is_hitting: [i8; NUM_FEET],
    moved_mask: u8,
    holding_mask: u8,
}

impl State {
    fn new(_cols: usize) -> Self {
        Self {
            columns: [Foot::None; MAX_COLUMNS],
            combined_columns: [Foot::None; MAX_COLUMNS],
            where_the_feet_are: [INVALID_COLUMN; NUM_FEET],
            what_note_the_foot_is_hitting: [INVALID_COLUMN; NUM_FEET],
            moved_mask: 0,
            holding_mask: 0,
        }
    }

    #[inline(always)]
    fn foot_moved(&self, pair: &FootPair) -> bool {
        let mask = FOOT_MASKS[pair.heel.as_index()] | FOOT_MASKS[pair.toe.as_index()];
        (self.moved_mask & mask) != 0
    }

    #[inline(always)]
    fn foot_moved_not_holding(&self, pair: &FootPair) -> bool {
        let mask = FOOT_MASKS[pair.heel.as_index()] | FOOT_MASKS[pair.toe.as_index()];
        ((self.moved_mask & !self.holding_mask) & mask) != 0
    }
}

impl PartialEq for State {
    fn eq(&self, other: &Self) -> bool {
        self.columns == other.columns
            && self.combined_columns == other.combined_columns
            && self.moved_mask == other.moved_mask
            && self.holding_mask == other.holding_mask
    }
}
impl Eq for State {}

type FootPlacement = [Foot; MAX_COLUMNS];

const NO_PERMS: [FootPlacement; 1] = [[Foot::None; MAX_COLUMNS]];

#[inline(always)]
fn pack_cols(cols: &[Foot; MAX_COLUMNS]) -> u32 {
    let mut out = 0u32;
    for i in 0..MAX_COLUMNS {
        out |= (cols[i] as u32) << (i * 3);
    }
    out
}

#[derive(Debug, Clone)]
struct StepParityNode {
    state: State,
    first_edge: u32,
    edge_count: u16,
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
) -> bool {
    if did_jump || !moved {
        return false;
    }

    let check = |col: i8, foot: Foot| -> bool {
        col > INVALID_COLUMN
            && initial.combined_columns[col as usize] == foot
            && (result.holding_mask & FOOT_MASKS[foot.as_index()]) == 0
            && initial.foot_moved_not_holding(pair)
    };

    check(heel_col, pair.heel) || check(toe_col, pair.toe)
}

fn calc_action_cost(
    layout: &StageLayout,
    initial: &State,
    result: &State,
    rows: &[Row],
    row_idx: usize,
    elapsed: f32,
    cols: usize,
) -> f32 {
    let row = &rows[row_idx];

    let lh = result.what_note_the_foot_is_hitting[Foot::LeftHeel.as_index()];
    let lt = result.what_note_the_foot_is_hitting[Foot::LeftToe.as_index()];
    let rh = result.what_note_the_foot_is_hitting[Foot::RightHeel.as_index()];
    let rt = result.what_note_the_foot_is_hitting[Foot::RightToe.as_index()];

    let moved_left = result.foot_moved(&LEFT_PAIR);
    let moved_right = result.foot_moved(&RIGHT_PAIR);

    let did_jump =
        initial.foot_moved_not_holding(&LEFT_PAIR) && initial.foot_moved_not_holding(&RIGHT_PAIR);

    let jacked_left = did_jack(initial, result, &LEFT_PAIR, lh, lt, moved_left, did_jump);
    let jacked_right = did_jack(initial, result, &RIGHT_PAIR, rh, rt, moved_right, did_jump);

    let mut cost = 0.0;
    cost += calc_mine_cost(result, row, cols);
    cost += calc_hold_switch_cost(layout, initial, result, row);
    cost += calc_bracket_tap_cost(initial, row, lh, lt, rh, rt, elapsed);
    cost += calc_bracket_jack_cost(
        result,
        moved_left,
        moved_right,
        jacked_left,
        jacked_right,
        did_jump,
    );
    cost += calc_doublestep_cost(
        initial,
        result,
        rows,
        row_idx,
        moved_left,
        moved_right,
        jacked_left,
        jacked_right,
        did_jump,
    );
    cost += calc_slow_bracket_cost(row, moved_left, moved_right, elapsed);
    cost += calc_twisted_foot_cost(layout, result);
    cost += calc_facing_cost(layout, result);
    cost += calc_spin_cost(layout, initial, result);
    cost += calc_footswitch_cost(initial, result, row, elapsed, cols);
    cost += calc_sideswitch_cost(layout, initial, result);
    cost += calc_missed_footswitch_cost(row, jacked_left, jacked_right);
    cost += calc_jack_cost(moved_left, moved_right, jacked_left, jacked_right, elapsed);
    cost += calc_big_movements_cost(layout, initial, result, elapsed);
    cost
}

fn calc_mine_cost(result: &State, row: &Row, cols: usize) -> f32 {
    if row.mine_mask == 0 {
        return 0.0;
    }
    let mut mask = row.mine_mask;
    while mask != 0 {
        let idx = mask.trailing_zeros() as usize;
        if idx < cols && result.combined_columns[idx] != Foot::None {
            return MINE_WEIGHT;
        }
        mask &= mask - 1;
    }
    0.0
}

fn calc_hold_switch_cost(layout: &StageLayout, initial: &State, result: &State, row: &Row) -> f32 {
    if row.hold_mask == 0 {
        return 0.0;
    }
    let mut cost = 0.0;
    let mut mask = row.hold_mask;

    while mask != 0 {
        let c = mask.trailing_zeros() as usize;
        mask &= mask - 1;

        let foot = result.combined_columns[c];
        if foot == Foot::None {
            continue;
        }

        let initial_foot = initial.combined_columns[c];
        let switched = (foot.is_left() && !initial_foot.is_left())
            || (foot.is_right() && !initial_foot.is_right());

        if switched {
            let prev_col = initial.where_the_feet_are[foot.as_index()];
            if prev_col == INVALID_COLUMN {
                cost += HOLDSWITCH_WEIGHT;
            } else {
                cost += layout.get_hold_switch_cost(c, prev_col as usize);
            };
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
        let jack_penalty = if initial.foot_moved(pair) {
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
    initial: &State,
    result: &State,
    rows: &[Row],
    row_idx: usize,
    moved_left: bool,
    moved_right: bool,
    jacked_left: bool,
    jacked_right: bool,
    did_jump: bool,
) -> f32 {
    if moved_left == moved_right || result.holding_mask != 0 || did_jump {
        return 0.0;
    }

    if did_double_step(
        initial,
        rows,
        row_idx,
        moved_left,
        jacked_left,
        moved_right,
        jacked_right,
    ) {
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

fn calc_twisted_foot_cost(layout: &StageLayout, result: &State) -> f32 {
    let lh = result.what_note_the_foot_is_hitting[1];
    let lt = result.what_note_the_foot_is_hitting[2];
    let rh = result.what_note_the_foot_is_hitting[3];
    let rt = result.what_note_the_foot_is_hitting[4];

    let left_pos = layout.avg_point(lh, lt);
    let right_pos = layout.avg_point(rh, rt);
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
    let get = |f: Foot| result.where_the_feet_are[f.as_index()];
    let (lh, mut lt) = (get(Foot::LeftHeel), get(Foot::LeftToe));
    let (rh, mut rt) = (get(Foot::RightHeel), get(Foot::RightToe));

    if lt == INVALID_COLUMN {
        lt = lh;
    }
    if rt == INVALID_COLUMN {
        rt = rh;
    }

    layout.get_facing_x_penalty(lh, rh)
        + layout.get_facing_x_penalty(lt, rt)
        + layout.get_facing_y_penalty(lh, lt)
        + layout.get_facing_y_penalty(rh, rt)
}

fn calc_spin_cost(layout: &StageLayout, initial: &State, result: &State) -> f32 {
    let get = |s: &State, f: Foot| s.where_the_feet_are[f.as_index()];

    let prev_left = layout.avg_point(get(initial, Foot::LeftHeel), get(initial, Foot::LeftToe));
    let prev_right = layout.avg_point(get(initial, Foot::RightHeel), get(initial, Foot::RightToe));

    let mut lt = get(result, Foot::LeftToe);
    let mut rt = get(result, Foot::RightToe);
    if lt == INVALID_COLUMN {
        lt = get(result, Foot::LeftHeel);
    }
    if rt == INVALID_COLUMN {
        rt = get(result, Foot::RightHeel);
    }

    let left = layout.avg_point(get(result, Foot::LeftHeel), lt);
    let right = layout.avg_point(get(result, Foot::RightHeel), rt);

    let mut cost = 0.0;
    if right.x < left.x && prev_right.x < prev_left.x {
        if (right.y < left.y && prev_right.y > prev_left.y)
            || (right.y > left.y && prev_right.y < prev_left.y)
        {
            cost += SPIN_WEIGHT;
        }
    }
    cost
}

fn calc_footswitch_cost(
    initial: &State,
    result: &State,
    row: &Row,
    elapsed: f32,
    cols: usize,
) -> f32 {
    if elapsed < SLOW_FOOTSWITCH_THRESHOLD || elapsed >= SLOW_FOOTSWITCH_IGNORE {
        return 0.0;
    }
    if row.mine_i32_mask != 0 || row.fake_mine_mask != 0 {
        return 0.0;
    }

    let time_scaled = elapsed - SLOW_FOOTSWITCH_THRESHOLD;
    for i in 0..cols {
        let (init, res) = (initial.combined_columns[i], result.columns[i]);
        if init == Foot::None || res == Foot::None {
            continue;
        }
        if init != res && init != OTHER_PART_OF_FOOT[res.as_index()] {
            let divisor = SLOW_FOOTSWITCH_THRESHOLD + time_scaled;
            if divisor > 0.0 {
                return (time_scaled / divisor) * FOOTSWITCH_WEIGHT;
            }
        }
    }
    0.0
}

fn calc_sideswitch_cost(layout: &StageLayout, initial: &State, result: &State) -> f32 {
    let mut mask = layout.side_mask;
    let mut count = 0u32;
    while mask != 0 {
        let c = mask.trailing_zeros() as usize;
        mask &= mask - 1;
        if initial.combined_columns[c] != result.columns[c]
            && result.columns[c] != Foot::None
            && initial.combined_columns[c] != Foot::None
            && (result.moved_mask & FOOT_MASKS[initial.combined_columns[c].as_index()]) == 0
        {
            count += 1;
        }
    }
    count as f32 * SIDESWITCH_WEIGHT
}

fn calc_missed_footswitch_cost(row: &Row, jacked_left: bool, jacked_right: bool) -> f32 {
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
    elapsed: f32,
) -> f32 {
    let mut cost = 0.0;
    for &foot in &FEET {
        if (result.moved_mask & FOOT_MASKS[foot.as_index()]) == 0 {
            continue;
        }
        let init_pos = initial.where_the_feet_are[foot.as_index()];
        if init_pos == INVALID_COLUMN {
            continue;
        }

        let res_pos = result.what_note_the_foot_is_hitting[foot.as_index()];
        let dist = layout.get_distance_weighted(init_pos as usize, res_pos as usize);
        let mut d = (dist / elapsed as f64) as f32;

        let other = OTHER_PART_OF_FOOT[foot.as_index()];
        let other_pos = result.what_note_the_foot_is_hitting[other.as_index()];
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

fn did_double_step(
    initial: &State,
    rows: &[Row],
    row_idx: usize,
    moved_left: bool,
    jacked_left: bool,
    moved_right: bool,
    jacked_right: bool,
) -> bool {
    let mut ds = false;
    if moved_left && !jacked_left && initial.foot_moved_not_holding(&LEFT_PAIR) {
        ds = true;
    }
    if moved_right && !jacked_right && initial.foot_moved_not_holding(&RIGHT_PAIR) {
        ds = true;
    }

    if row_idx > 0 && ds {
        let last = &rows[row_idx - 1];
        let start = last.beat;

        let mut mask = last.hold_mask;
        while mask != 0 {
            let idx = mask.trailing_zeros() as usize;
            mask &= mask - 1;
            if last.hold_ends[idx] > start {
                ds = false;
                break;
            }
        }
    }
    ds
}

// --- Generator ---

struct StepParityGenerator {
    layout: &'static StageLayout,
    perm_table: &'static [Box<[FootPlacement]>; 256],
    perm_cache: [Option<&'static [FootPlacement]>; 256],
    column_count: usize,
    nodes: Vec<StepParityNode>,
    edges: Vec<Edge>,
    rows: Vec<Row>,
    result_columns: Vec<FootPlacement>,
}

impl StepParityGenerator {
    fn new(cache: &'static LayoutCache) -> Self {
        Self {
            column_count: cache.layout.column_count(),
            layout: &cache.layout,
            perm_table: &cache.perm_table,
            perm_cache: [None; 256],
            nodes: Vec::new(),
            edges: Vec::new(),
            rows: Vec::new(),
            result_columns: Vec::new(),
        }
    }

    fn analyze(&mut self, notes: Vec<IntermediateNoteData>, cols: usize) -> bool {
        self.column_count = cols;
        self.perm_cache.fill(None);
        self.nodes.clear();
        self.edges.clear();
        self.rows.clear();
        self.result_columns.clear();
        self.create_rows(notes);
        if self.rows.is_empty() {
            return false;
        }
        self.build_graph();
        self.trace_path()
    }

    fn create_rows(&mut self, notes: Vec<IntermediateNoteData>) {
        let mut counter = RowCounter::new();

        for note in notes {
            if note.note_type == TapNoteType::Empty {
                continue;
            }

            if note.note_type == TapNoteType::Mine {
                let bit = 1u8 << note.col;
                let mine_on = note.second != 0.0;
                let mine_i32_on = (note.second as i32) != 0;

                if note.second == counter.last_second && !self.rows.is_empty() {
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
                    self.flush_row(&mut counter);
                }
                counter.reset_for_row(note.second, note.beat);
            }

            let col = note.col;
            let is_hold = note.note_type == TapNoteType::HoldHead;
            counter.note_mask |= 1u8 << col;
            if is_hold {
                counter.hold_ends[col] = note.beat + note.hold_length;
            }
        }
        self.flush_row(&mut counter);
    }

    fn flush_row(&mut self, counter: &mut RowCounter) {
        if counter.last_second == CLM_SECOND_INVALID {
            return;
        }
        self.rows.push(self.build_row(counter));
    }

    fn build_row(&self, counter: &RowCounter) -> Row {
        let mut row = Row::new();
        row.second = counter.last_second;
        row.beat = counter.last_beat;
        row.note_mask = counter.note_mask;
        row.note_count = row.note_mask.count_ones() as u8;
        row.mine_mask = counter.next_mine_mask;
        row.mine_i32_mask = counter.next_mine_i32_mask;
        row.fake_mine_mask = counter.next_fake_mine_mask;
        row.hold_ends = counter.hold_ends;

        if let Some(prev) = self.rows.last() {
            for c in 0..self.column_count.min(MAX_COLUMNS) {
                let end = prev.hold_ends[c];
                if end >= row.beat && row.hold_ends[c] < 0.0 {
                    row.hold_mask |= 1u8 << c;
                    row.hold_ends[c] = end;
                }
            }
        }
        row
    }

    fn build_graph(&mut self) {
        let start_id = self.add_node(State::new(self.column_count));

        let mut prev_ids = vec![start_id];
        let mut next_ids: Vec<usize> = Vec::new();
        let mut state_map: FastMap<u64, usize> = FastMap::default();

        let mut prev_second = self.rows.first().map(|r| r.second - 1.0).unwrap_or(-1.0);

        for i in 0..self.rows.len() {
            let row_second = self.rows[i].second;
            let elapsed = row_second - prev_second;
            prev_second = row_second;

            let perms = self.perms_for_row(i);
            next_ids.clear();
            state_map.clear();
            state_map.reserve(perms.len());
            self.edges
                .reserve(prev_ids.len().saturating_mul(perms.len()));

            for &init_id in &prev_ids {
                let node_edge_start = self.edges.len() as u32;
                let init_state = self.nodes[init_id].state;

                for perm in perms.iter() {
                    let (result, key) = self.result_state(&init_state, i, perm);
                    let cost = calc_action_cost(
                        self.layout,
                        &init_state,
                        &result,
                        &self.rows,
                        i,
                        elapsed,
                        self.column_count,
                    );

                    let res_id = if let Some(&id) = state_map.get(&key) {
                        id
                    } else {
                        let id = self.add_node(result);
                        next_ids.push(id);
                        state_map.insert(key, id);
                        id
                    };
                    self.edges.push(Edge {
                        to: res_id as u32,
                        cost,
                    });
                }
                let edge_count = (self.edges.len() as u32 - node_edge_start) as u16;
                let node = &mut self.nodes[init_id];
                node.first_edge = node_edge_start;
                node.edge_count = edge_count;
            }

            std::mem::swap(&mut prev_ids, &mut next_ids);
        }

        let end_id = self.add_node(State::new(self.column_count));

        for &id in &prev_ids {
            let edge_start = self.edges.len() as u32;
            self.edges.push(Edge {
                to: end_id as u32,
                cost: 0.0,
            });
            let node = &mut self.nodes[id];
            node.first_edge = edge_start;
            node.edge_count = 1;
        }
    }

    fn add_node(&mut self, state: State) -> usize {
        let idx = self.nodes.len();
        self.nodes.push(StepParityNode {
            state,
            first_edge: 0,
            edge_count: 0,
        });
        idx
    }

    fn perms_for_row(&mut self, row_idx: usize) -> &'static [FootPlacement] {
        let row = &self.rows[row_idx];
        let key = (row.note_mask | row.hold_mask) as usize;
        if let Some(perms) = self.perm_cache[key] {
            return perms;
        }

        let union = self.perm_table[key].as_ref();
        let perms = if !union.is_empty() {
            union
        } else {
            let note = self.perm_table[row.note_mask as usize].as_ref();
            if !note.is_empty() { note } else { &NO_PERMS }
        };

        self.perm_cache[key] = Some(perms);
        perms
    }

    fn result_state(&self, initial: &State, row_idx: usize, cols: &FootPlacement) -> (State, u64) {
        let row = &self.rows[row_idx];
        let n = self.column_count;
        let hold_mask = row.hold_mask;

        let mut state = State::new(n);
        let mut moved_mask = 0u8;

        for i in 0..n {
            let foot = cols[i];
            state.columns[i] = foot;
            let hold_empty = (hold_mask & (1u8 << i)) == 0;
            let moved = foot != Foot::None && (hold_empty || initial.combined_columns[i] != foot);

            if foot != Foot::None {
                let fi = foot.as_index();
                state.what_note_the_foot_is_hitting[fi] = i as i8;

                if moved {
                    moved_mask |= FOOT_MASKS[fi];
                }
                if !hold_empty {
                    state.holding_mask |= FOOT_MASKS[fi];
                }
            }
        }

        state.moved_mask = moved_mask;
        let moved_left = (moved_mask & LEFT_FOOT_MASK) != 0;
        let moved_right = (moved_mask & RIGHT_FOOT_MASK) != 0;

        for i in 0..n {
            let combined = if state.columns[i] != Foot::None {
                state.columns[i]
            } else {
                let prev = initial.combined_columns[i];
                match prev {
                    Foot::LeftHeel | Foot::RightHeel
                        if (moved_mask & FOOT_MASKS[prev.as_index()]) == 0 =>
                    {
                        prev
                    }
                    Foot::LeftToe if !moved_left => prev,
                    Foot::RightToe if !moved_right => prev,
                    _ => Foot::None,
                }
            };
            if combined != Foot::None {
                state.combined_columns[i] = combined;
                state.where_the_feet_are[combined.as_index()] = i as i8;
            }
        }

        let cols_p = pack_cols(&state.columns) as u64;
        let comb_p = pack_cols(&state.combined_columns) as u64;
        let key = cols_p
            | (comb_p << 24)
            | ((state.moved_mask as u64) << 48)
            | ((state.holding_mask as u64) << 56);
        (state, key)
    }

    fn trace_path(&mut self) -> bool {
        if self.nodes.is_empty() {
            return false;
        }

        let n = self.nodes.len();
        let mut cost = vec![f32::MAX; n];
        let mut pred = vec![usize::MAX; n];
        cost[0] = 0.0;

        for i in 0..n {
            if cost[i] == f32::MAX {
                continue;
            }
            let node = &self.nodes[i];
            let start = node.first_edge as usize;
            let end = start + node.edge_count as usize;
            for e in &self.edges[start..end] {
                let to = e.to as usize;
                let nc = cost[i] + e.cost;
                if nc < cost[to] {
                    cost[to] = nc;
                    pred[to] = i;
                }
            }
        }

        if pred[n - 1] == usize::MAX {
            return false;
        }

        let nodes = &self.nodes;
        let rows = &self.rows;

        let mut cur = n - 1;
        self.result_columns.clear();
        self.result_columns
            .resize(rows.len(), [Foot::None; MAX_COLUMNS]);

        let mut write = self.result_columns.len();
        while cur != 0 {
            let prev = pred[cur];
            if prev == usize::MAX {
                return false;
            }
            cur = prev;
            if cur == 0 {
                break;
            }
            if write == 0 {
                return false;
            }
            write -= 1;
            self.result_columns[write] = nodes[cur].state.combined_columns;
        }

        write == 0
    }
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

impl RowCounter {
    fn new() -> Self {
        Self {
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

    fn reset_for_row(&mut self, second: f32, beat: f32) {
        self.last_second = second;
        self.last_beat = beat;
        self.next_mine_mask = self.mine_mask;
        self.next_mine_i32_mask = self.mine_i32_mask;
        self.next_fake_mine_mask = self.fake_mine_mask;
        self.note_mask = 0;
        self.hold_ends.fill(HOLD_END_NONE);
        self.mine_mask = 0;
        self.mine_i32_mask = 0;
        self.fake_mine_mask = 0;
    }
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
            && !layout.bracket_check(lh as usize, lt as usize)
        {
            return;
        }
        if rh != INVALID_COLUMN
            && rt != INVALID_COLUMN
            && !layout.bracket_check(rh as usize, rt as usize)
        {
            return;
        }

        out.push(*cols);
        return;
    }

    let active = (mask & (1u8 << col)) != 0;

    if active {
        for &foot in &FEET {
            let fm = FOOT_MASKS[foot.as_index()];
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

    let cols = layout.column_count().min(MAX_COLUMNS);

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
                pos[foot.as_index()] = c as i8;
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
                let (cc, pc) = (curr_pos[foot.as_index()], prev_pos[foot.as_index()]);
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
                && OTHER_PART_OF_FOOT[p.as_index()] != r
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
        let left_heel = curr_pos[Foot::LeftHeel.as_index()];
        let left_toe = curr_pos[Foot::LeftToe.as_index()];
        let right_heel = curr_pos[Foot::RightHeel.as_index()];
        let right_toe = curr_pos[Foot::RightToe.as_index()];

        let prev_left_heel = prev_pos[Foot::LeftHeel.as_index()];
        let prev_left_toe = prev_pos[Foot::LeftToe.as_index()];
        let prev_right_heel = prev_pos[Foot::RightHeel.as_index()];
        let prev_right_toe = prev_pos[Foot::RightToe.as_index()];

        // Right foot crossing over left
        if right_heel != INVALID_COLUMN
            && prev_left_heel != INVALID_COLUMN
            && prev_right_heel == INVALID_COLUMN
        {
            let left_pos = layout.avg_point(prev_left_heel, prev_left_toe);
            let right_pos = layout.avg_point(right_heel, right_toe);
            if right_pos.x < left_pos.x {
                if i > 1 {
                    let prev_prev = &rows[i - 2];
                    let prev_prev_pos = hit_positions(&placements[i - 2], prev_prev.note_mask);
                    let prev_prev_rh = prev_prev_pos[Foot::RightHeel.as_index()];
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
            let left_pos = layout.avg_point(left_heel, left_toe);
            let right_pos = layout.avg_point(prev_right_heel, prev_right_toe);
            if right_pos.x < left_pos.x {
                if i > 1 {
                    let prev_prev = &rows[i - 2];
                    let prev_prev_pos = hit_positions(&placements[i - 2], prev_prev.note_mask);
                    let prev_prev_lh = prev_prev_pos[Foot::LeftHeel.as_index()];
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

impl LayoutCache {
    fn new(layout: StageLayout) -> Self {
        let perm_table = build_perm_table(&layout);
        Self { layout, perm_table }
    }
}

fn build_perm_table(layout: &StageLayout) -> [Box<[FootPlacement]>; 256] {
    let col_count = layout.column_count();
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
    CACHE.get_or_init(|| LayoutCache::new(StageLayout::new_dance_single()))
}

fn dance_double_cache() -> &'static LayoutCache {
    static CACHE: OnceLock<LayoutCache> = OnceLock::new();
    CACHE.get_or_init(|| LayoutCache::new(StageLayout::new_dance_double()))
}

fn layout_for_lanes(lanes: usize) -> Option<&'static LayoutCache> {
    match lanes {
        4 => Some(dance_single_cache()),
        8 => Some(dance_double_cache()),
        _ => None,
    }
}

#[inline(always)]
fn trim_ws(mut s: &[u8]) -> &[u8] {
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

            let beat = start + j as f32 * step;
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

fn parse_rows_from_arrays<const LANES: usize>(
    rows: &[[u8; LANES]],
    row_to_beat: &[f32],
    timing: &TimingData,
    cols: usize,
) -> Vec<ParsedRow> {
    let mut parsed = Vec::new();
    if cols == 0 || cols > 8 {
        return parsed;
    }

    let copy_len = cols.min(LANES);
    for (idx, row) in rows.iter().enumerate() {
        if !has_obj(&row[..copy_len]) {
            continue;
        }

        let beat_raw = row_to_beat[idx];
        let note_row = beat_to_note_row_f32(beat_raw);
        let beat = note_row as f32 / ROWS_PER_BEAT as f32;
        let second = timing.get_time_for_beat_f32(beat as f64) as f32;

        let mut chars = [b'0'; 8];
        chars[..copy_len].copy_from_slice(&row[..copy_len]);
        parsed.push(ParsedRow {
            chars,
            columns: cols as u8,
            row: note_row,
            beat,
            second,
        });
    }
    parsed
}

fn build_notes(rows: &[ParsedRow], timing: Option<&TimingData>) -> Vec<IntermediateNoteData> {
    let cols = rows.first().map(|r| r.columns as usize).unwrap_or(0);
    if cols == 0 {
        return Vec::new();
    }

    // Compute hold lengths
    let mut hold_starts = vec![None; cols];
    let mut lengths = vec![MISSING_HOLD_LENGTH_BEATS; rows.len() * cols];

    for (idx, row) in rows.iter().enumerate() {
        for c in 0..cols {
            match row.chars[c] {
                b'1' | b'M' | b'L' | b'F' => hold_starts[c] = None,
                b'2' | b'4' => hold_starts[c] = Some((idx, row.row)),
                b'3' => {
                    if let Some((si, sr)) = hold_starts[c] {
                        lengths[si * cols + c] = (row.row - sr) as f32 / ROWS_PER_BEAT as f32;
                        hold_starts[c] = None;
                    }
                }
                _ => {}
            }
        }
    }

    let mut notes = Vec::new();
    for (idx, row) in rows.iter().enumerate() {
        let row_fake = timing.map_or(false, |t| t.is_fake_at_beat(row.row as f64));

        for c in 0..cols {
            let ch = row.chars[c];
            let note_type = match ch {
                b'1' | b'K' | b'L' => TapNoteType::Tap,
                b'2' | b'4' => TapNoteType::HoldHead,
                b'M' => TapNoteType::Mine,
                b'F' => TapNoteType::Fake,
                _ => continue,
            };

            let mut note = IntermediateNoteData {
                note_type,
                col: c,
                beat: row.beat,
                second: row.second,
                fake: note_type == TapNoteType::Fake || row_fake,
                ..Default::default()
            };

            if note_type == TapNoteType::HoldHead {
                let len = lengths[idx * cols + c];
                if len >= MISSING_HOLD_LENGTH_BEATS {
                    continue;
                }
                note.hold_length = len;
            }
            notes.push(note);
        }
    }
    notes
}

// --- Public API ---

pub fn analyze_lanes(data: &[u8], bpm_map: &[(f64, f64)], offset: f64, lanes: usize) -> TechCounts {
    let Some(cache) = layout_for_lanes(lanes) else {
        return TechCounts::default();
    };

    let cols = cache.layout.column_count();
    let minimized = crate::stats::minimize_chart_for_hash(data, cols);
    let rows = parse_rows(&minimized, cols, |beat| {
        time_between_beats(0.0, beat, bpm_map) as f32 - offset as f32
    });
    let notes = build_notes(&rows, None);

    let mut generator = StepParityGenerator::new(cache);
    if !generator.analyze(notes, cols) {
        return TechCounts::default();
    }
    calculate_tech_counts(&generator.rows, &generator.result_columns, generator.layout)
}

pub fn analyze_timing_lanes(data: &[u8], timing: &TimingData, lanes: usize) -> TechCounts {
    let Some(cache) = layout_for_lanes(lanes) else {
        return TechCounts::default();
    };

    let cols = cache.layout.column_count();
    let minimized = crate::stats::minimize_chart_for_hash(data, cols);
    let rows = parse_rows(&minimized, cols, |beat| {
        timing.get_time_for_beat_f32(beat as f64) as f32
    });
    let notes = build_notes(&rows, Some(timing));

    let mut generator = StepParityGenerator::new(cache);
    if !generator.analyze(notes, cols) {
        return TechCounts::default();
    }
    calculate_tech_counts(&generator.rows, &generator.result_columns, generator.layout)
}

pub(crate) fn analyze_timing_rows<const LANES: usize>(
    rows: &[[u8; LANES]],
    row_to_beat: &[f32],
    timing: &TimingData,
    _minimized_note_data: &[u8],
) -> TechCounts {
    let Some(cache) = layout_for_lanes(LANES) else {
        return TechCounts::default();
    };

    let cols = cache.layout.column_count();
    let parsed = parse_rows_from_arrays(rows, row_to_beat, timing, cols);
    let notes = build_notes(&parsed, Some(timing));

    let mut generator = StepParityGenerator::new(cache);
    if !generator.analyze(notes, cols) {
        return TechCounts::default();
    }
    calculate_tech_counts(&generator.rows, &generator.result_columns, generator.layout)
}

fn time_between_beats(start: f32, end: f32, bpm_map: &[(f64, f64)]) -> f64 {
    if end <= start {
        return 0.0;
    }
    let mut bpm = bpm_map.first().map(|b| b.1).unwrap_or(60.0);
    let mut time = 0.0;
    let mut last = start as f64;

    for &(beat, value) in bpm_map {
        if beat <= last {
            bpm = value;
            continue;
        }
        if beat >= end as f64 {
            break;
        }
        time += (beat - last) * 60.0 / bpm;
        last = beat;
        bpm = value;
    }
    time + (end as f64 - last) * 60.0 / bpm
}

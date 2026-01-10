use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hasher};
use std::ops::{Index, IndexMut};
use std::rc::Rc;

use crate::timing::{ROWS_PER_BEAT, TimingData, beat_to_note_row_f32};

const INVALID_COLUMN: isize = -1;
const CLM_SECOND_INVALID: f32 = -1.0;
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
#[repr(usize)]
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
    fn as_index(self) -> usize {
        self as usize
    }

    #[inline(always)]
    fn is_left(self) -> bool {
        matches!(self, Foot::LeftHeel | Foot::LeftToe)
    }

    #[inline(always)]
    fn is_right(self) -> bool {
        matches!(self, Foot::RightHeel | Foot::RightToe)
    }
}

const NUM_FEET: usize = 5;
const MAX_COLUMNS: usize = 8;
const FEET: [Foot; 4] = [
    Foot::LeftHeel,
    Foot::LeftToe,
    Foot::RightHeel,
    Foot::RightToe,
];
const FOOT_MASKS: [u8; NUM_FEET] = [0, 1, 2, 4, 8];
const LEFT_FOOT_MASK: u8 = FOOT_MASKS[1] | FOOT_MASKS[2];
const RIGHT_FOOT_MASK: u8 = FOOT_MASKS[3] | FOOT_MASKS[4];
const OTHER_PART_OF_FOOT: [Foot; NUM_FEET] = [
    Foot::None,
    Foot::LeftToe,
    Foot::LeftHeel,
    Foot::RightToe,
    Foot::RightHeel,
];
const STATE_HASH_PRIME: u64 = 31;

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
struct NeighborEntry {
    neighbor_id: usize,
    cost: f32,
    next: usize,
    hash_key: usize,
}

const BUCKET_EMPTY: usize = usize::MAX;
const BUCKET_SENTINEL: usize = usize::MAX - 1;

#[derive(Debug, Clone)]
struct NeighborMap {
    entries: Vec<NeighborEntry>,
    head: usize,
    bucket_before: Vec<usize>,
    bucket_count: usize,
}

impl Default for NeighborMap {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
            head: BUCKET_EMPTY,
            bucket_before: vec![BUCKET_EMPTY; 13],
            bucket_count: 13,
        }
    }
}

impl NeighborMap {
    fn reserve(&mut self, additional: usize) {
        if additional == 0 {
            return;
        }
        let target = self.entries.len().saturating_add(additional);
        if target > self.bucket_count {
            self.rehash(next_prime(target.max(1)));
        }
        self.entries.reserve(additional);
    }

    fn insert_reserved(&mut self, neighbor_id: usize, hash_key: usize, cost: f32) {
        #[cfg(debug_assertions)]
        if let Some(entry) = self
            .entries
            .iter_mut()
            .find(|e| e.neighbor_id == neighbor_id)
        {
            entry.cost = cost;
            return;
        }
        self.insert_new_reserved(neighbor_id, hash_key, cost);
    }

    fn insert_new_reserved(&mut self, neighbor_id: usize, hash_key: usize, cost: f32) {
        debug_assert!(self.entries.len() + 1 <= self.bucket_count);
        let idx = self.entries.len();
        self.entries.push(NeighborEntry {
            neighbor_id,
            cost,
            next: BUCKET_EMPTY,
            hash_key,
        });
        self.insert_index(idx);
    }

    fn get(&self, neighbor_id: usize) -> Option<f32> {
        self.entries
            .iter()
            .find(|e| e.neighbor_id == neighbor_id)
            .map(|e| e.cost)
    }

    fn rehash(&mut self, new_count: usize) {
        self.bucket_count = new_count;
        self.bucket_before = vec![BUCKET_EMPTY; new_count];
        let mut prev = None;
        let mut current = self.head;
        while current != BUCKET_EMPTY {
            let bucket = self.entries[current].hash_key % new_count;
            if self.bucket_before[bucket] == BUCKET_EMPTY {
                self.bucket_before[bucket] = prev.unwrap_or(BUCKET_SENTINEL);
            }
            prev = Some(current);
            current = self.entries[current].next;
        }
    }

    fn insert_index(&mut self, idx: usize) {
        let bucket = self.entries[idx].hash_key % self.bucket_count;
        let before = self.bucket_before[bucket];

        if before == BUCKET_EMPTY {
            let old_head = self.head;
            self.entries[idx].next = old_head;
            self.head = idx;
            self.bucket_before[bucket] = BUCKET_SENTINEL;
            if old_head != BUCKET_EMPTY {
                let old_bucket = self.entries[old_head].hash_key % self.bucket_count;
                self.bucket_before[old_bucket] = idx;
            }
        } else if before == BUCKET_SENTINEL {
            self.entries[idx].next = self.head;
            self.head = idx;
        } else {
            self.entries[idx].next = self.entries[before].next;
            self.entries[before].next = idx;
        }
    }
}

fn next_prime(start: usize) -> usize {
    if start <= 2 {
        return 2;
    }
    let mut c = if start % 2 == 0 { start + 1 } else { start };
    while !is_prime(c) {
        c = c.saturating_add(2);
    }
    c
}

fn is_prime(v: usize) -> bool {
    if v <= 3 {
        return v > 1;
    }
    if v % 2 == 0 || v % 3 == 0 {
        return false;
    }
    let mut i = 5usize;
    while i.saturating_mul(i) <= v {
        if v % i == 0 || v % (i + 2) == 0 {
            return false;
        }
        i += 6;
    }
    true
}

#[derive(Debug, Clone, Copy, Default)]
struct StagePoint {
    x: f32,
    y: f32,
}

#[derive(Debug, Clone)]
struct StageLayout {
    columns: Vec<StagePoint>,
    up_arrows: Vec<usize>,
    down_arrows: Vec<usize>,
    side_arrows: Vec<usize>,
    pair_stride: usize,
    avg_points: Vec<StagePoint>,
    x_diffs: Vec<f32>,
    y_diffs: Vec<f32>,
    dist_stride: usize,
    dist_sq: Vec<f32>,
    dist_weighted: Vec<f64>,
}

impl StageLayout {
    fn new_dance_single() -> Self {
        Self::new(
            vec![
                StagePoint { x: 0.0, y: 1.0 },
                StagePoint { x: 1.0, y: 0.0 },
                StagePoint { x: 1.0, y: 2.0 },
                StagePoint { x: 2.0, y: 1.0 },
            ],
            vec![2],
            vec![1],
            vec![0, 3],
        )
    }

    fn new_dance_double() -> Self {
        Self::new(
            vec![
                StagePoint { x: 0.0, y: 1.0 },
                StagePoint { x: 1.0, y: 0.0 },
                StagePoint { x: 1.0, y: 2.0 },
                StagePoint { x: 2.0, y: 1.0 },
                StagePoint { x: 3.0, y: 1.0 },
                StagePoint { x: 4.0, y: 0.0 },
                StagePoint { x: 4.0, y: 2.0 },
                StagePoint { x: 5.0, y: 1.0 },
            ],
            vec![2, 6],
            vec![1, 5],
            vec![0, 3, 4, 7],
        )
    }

    fn new(
        columns: Vec<StagePoint>,
        up_arrows: Vec<usize>,
        down_arrows: Vec<usize>,
        side_arrows: Vec<usize>,
    ) -> Self {
        let pair_stride = columns.len() + 1;
        let pair_len = pair_stride * pair_stride;
        let invalid = columns.len();

        let mut avg_points = vec![StagePoint::default(); pair_len];
        let mut x_diffs = vec![0.0f32; pair_len];
        let mut y_diffs = vec![0.0f32; pair_len];

        for left in 0..pair_stride {
            for right in 0..pair_stride {
                let idx = left * pair_stride + right;
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
                let (mut xm, mut ym) = (ndx.abs().powf(4.0) as f32, ndy.abs().powf(4.0) as f32);
                if ndx <= 0.0 {
                    xm = -xm;
                }
                if ndy <= 0.0 {
                    ym = -ym;
                }
                x_diffs[idx] = xm;
                y_diffs[idx] = ym;
            }
        }

        let dist_stride = columns.len();
        let dist_len = dist_stride * dist_stride;
        let mut dist_sq = vec![0.0f32; dist_len];
        let mut dist_weighted = vec![0.0f64; dist_len];

        for l in 0..dist_stride {
            for r in 0..dist_stride {
                let (dx, dy) = (columns[l].x - columns[r].x, columns[l].y - columns[r].y);
                let sq = dx * dx + dy * dy;
                let idx = l * dist_stride + r;
                dist_sq[idx] = sq;
                dist_weighted[idx] = (sq as f64).sqrt() * DISTANCE_WEIGHT as f64;
            }
        }

        Self {
            columns,
            up_arrows,
            down_arrows,
            side_arrows,
            pair_stride,
            avg_points,
            x_diffs,
            y_diffs,
            dist_stride,
            dist_sq,
            dist_weighted,
        }
    }

    #[inline(always)]
    fn column_count(&self) -> usize {
        self.columns.len()
    }

    #[inline(always)]
    fn bracket_check(&self, c1: usize, c2: usize) -> bool {
        let (p1, p2) = (self.columns[c1], self.columns[c2]);
        let (dx, dy) = (p1.x - p2.x, p1.y - p2.y);
        dx * dx + dy * dy <= 2.0
    }

    #[inline(always)]
    fn get_distance_sq(&self, c1: usize, c2: usize) -> f32 {
        self.dist_sq[c1 * self.dist_stride + c2]
    }

    #[inline(always)]
    fn get_distance_weighted(&self, c1: usize, c2: usize) -> f64 {
        self.dist_weighted[c1 * self.dist_stride + c2]
    }

    #[inline(always)]
    fn pair_index(&self, left: isize, right: isize) -> usize {
        let max = self.pair_stride - 1;
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
        l * self.pair_stride + r
    }

    #[inline(always)]
    fn get_x_diff(&self, l: isize, r: isize) -> f32 {
        self.x_diffs[self.pair_index(l, r)]
    }

    #[inline(always)]
    fn get_y_diff(&self, l: isize, r: isize) -> f32 {
        self.y_diffs[self.pair_index(l, r)]
    }

    #[inline(always)]
    fn avg_point(&self, l: isize, r: isize) -> StagePoint {
        self.avg_points[self.pair_index(l, r)]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum TapNoteType {
    #[default]
    Empty,
    Tap,
    HoldHead,
    HoldTail,
    Mine,
    Fake,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum TapNoteSubType {
    #[default]
    Invalid,
    Hold,
    Roll,
}

#[derive(Debug, Clone, Default)]
struct IntermediateNoteData {
    note_type: TapNoteType,
    subtype: TapNoteSubType,
    col: usize,
    row: usize,
    beat: f32,
    hold_length: f32,
    fake: bool,
    second: f32,
    parity: Foot,
}

#[derive(Debug, Clone)]
struct Row {
    notes: Vec<IntermediateNoteData>,
    holds: Vec<IntermediateNoteData>,
    mines: Vec<f32>,
    fake_mines: Vec<f32>,
    columns: Vec<Foot>,
    where_the_feet_are: Vec<isize>,
    second: f32,
    beat: f32,
    row_index: usize,
    column_count: usize,
    note_count: usize,
    note_mask: u8,
    hold_mask: u8,
    mine_mask: u8,
    mine_i32_mask: u8,
    fake_mine_mask: u8,
}

impl Row {
    fn new(cols: usize) -> Self {
        Self {
            notes: vec![IntermediateNoteData::default(); cols],
            holds: vec![IntermediateNoteData::default(); cols],
            mines: vec![0.0; cols],
            fake_mines: vec![0.0; cols],
            columns: vec![Foot::None; cols],
            where_the_feet_are: vec![INVALID_COLUMN; NUM_FEET],
            second: 0.0,
            beat: 0.0,
            row_index: 0,
            column_count: cols,
            note_count: 0,
            note_mask: 0,
            hold_mask: 0,
            mine_mask: 0,
            mine_i32_mask: 0,
            fake_mine_mask: 0,
        }
    }

    fn set_foot_placement(&mut self, placement: &[Foot]) {
        self.note_count = self.note_mask.count_ones() as usize;
        for c in 0..self.column_count.min(MAX_COLUMNS) {
            if (self.note_mask & (1u8 << c)) != 0 {
                let foot = placement[c];
                self.notes[c].parity = foot;
                self.columns[c] = foot;
                if foot != Foot::None {
                    self.where_the_feet_are[foot.as_index()] = c as isize;
                }
            } else {
                self.columns[c] = Foot::None;
            }
        }
    }
}

#[derive(Debug, Clone)]
struct State {
    columns: Vec<Foot>,
    combined_columns: Vec<Foot>,
    moved_feet: Vec<Foot>,
    hold_feet: Vec<Foot>,
    where_the_feet_are: [isize; NUM_FEET],
    what_note_the_foot_is_hitting: [isize; NUM_FEET],
    did_the_foot_move: [bool; NUM_FEET],
    is_the_foot_holding: [bool; NUM_FEET],
}

impl State {
    fn new(cols: usize) -> Self {
        Self {
            columns: vec![Foot::None; cols],
            combined_columns: vec![Foot::None; cols],
            moved_feet: vec![Foot::None; cols],
            hold_feet: vec![Foot::None; cols],
            where_the_feet_are: [INVALID_COLUMN; NUM_FEET],
            what_note_the_foot_is_hitting: [INVALID_COLUMN; NUM_FEET],
            did_the_foot_move: [false; NUM_FEET],
            is_the_foot_holding: [false; NUM_FEET],
        }
    }

    #[inline(always)]
    fn foot_moved(&self, pair: &FootPair) -> bool {
        self.did_the_foot_move[pair.heel.as_index()] || self.did_the_foot_move[pair.toe.as_index()]
    }

    #[inline(always)]
    fn foot_moved_not_holding(&self, pair: &FootPair) -> bool {
        (self.did_the_foot_move[pair.heel.as_index()]
            && !self.is_the_foot_holding[pair.heel.as_index()])
            || (self.did_the_foot_move[pair.toe.as_index()]
                && !self.is_the_foot_holding[pair.toe.as_index()])
    }
}

impl PartialEq for State {
    fn eq(&self, other: &Self) -> bool {
        self.columns == other.columns
            && self.combined_columns == other.combined_columns
            && self.moved_feet == other.moved_feet
            && self.hold_feet == other.hold_feet
    }
}
impl Eq for State {}

type FootPlacement = [Foot; MAX_COLUMNS];

#[derive(Debug, Clone)]
struct StepParityNode {
    state: Rc<State>,
    second: f32,
    neighbors: NeighborMap,
}

const NODE_CHUNK_SIZE: usize = 1024;

struct NodeArena {
    chunks: Vec<Vec<StepParityNode>>,
    len: usize,
}

impl NodeArena {
    fn new() -> Self {
        Self {
            chunks: Vec::new(),
            len: 0,
        }
    }
    fn len(&self) -> usize {
        self.len
    }
    fn is_empty(&self) -> bool {
        self.len == 0
    }

    fn clear(&mut self) {
        for chunk in &mut self.chunks {
            chunk.clear();
        }
        self.len = 0;
    }

    fn push(&mut self, node: StepParityNode) -> usize {
        let idx = self.len;
        if self
            .chunks
            .last()
            .map_or(true, |c| c.len() == NODE_CHUNK_SIZE)
        {
            self.chunks.push(Vec::with_capacity(NODE_CHUNK_SIZE));
        }
        self.chunks.last_mut().unwrap().push(node);
        self.len = idx + 1;
        idx
    }

    fn get(&self, idx: usize) -> Option<&StepParityNode> {
        if idx < self.len {
            self.chunks
                .get(idx / NODE_CHUNK_SIZE)?
                .get(idx % NODE_CHUNK_SIZE)
        } else {
            None
        }
    }

    fn get_mut(&mut self, idx: usize) -> Option<&mut StepParityNode> {
        if idx < self.len {
            self.chunks
                .get_mut(idx / NODE_CHUNK_SIZE)?
                .get_mut(idx % NODE_CHUNK_SIZE)
        } else {
            None
        }
    }

    fn ptr(&self, idx: usize) -> *const StepParityNode {
        self.get(idx)
            .map(|n| n as *const _)
            .unwrap_or(std::ptr::null())
    }
}

impl Index<usize> for NodeArena {
    type Output = StepParityNode;
    fn index(&self, idx: usize) -> &Self::Output {
        self.get(idx).expect("node index out of bounds")
    }
}

impl IndexMut<usize> for NodeArena {
    fn index_mut(&mut self, idx: usize) -> &mut Self::Output {
        self.get_mut(idx).expect("node index out of bounds")
    }
}

// --- Cost Calculations (free functions to avoid borrow issues) ---

fn did_jack(
    initial: &State,
    result: &State,
    pair: &FootPair,
    heel_col: isize,
    toe_col: isize,
    moved: bool,
    did_jump: bool,
) -> bool {
    if did_jump || !moved {
        return false;
    }

    let check = |col: isize, foot: Foot| -> bool {
        col > INVALID_COLUMN
            && initial.combined_columns[col as usize] == foot
            && !result.is_the_foot_holding[foot.as_index()]
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
) -> f32 {
    let row = &rows[row_idx];
    let cols = row.column_count;

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
            let dist = if prev_col == INVALID_COLUMN {
                1.0
            } else {
                (layout.get_distance_sq(c, prev_col as usize) as f64).sqrt() as f32
            };
            cost += HOLDSWITCH_WEIGHT * dist;
        }
    }
    cost
}

fn calc_bracket_tap_cost(
    initial: &State,
    row: &Row,
    lh: isize,
    lt: isize,
    rh: isize,
    rt: isize,
    elapsed: f32,
) -> f32 {
    if row.hold_mask == 0 {
        return 0.0;
    }
    let mut cost = 0.0;

    let check_pair = |heel: isize, toe: isize, pair: &FootPair| -> f32 {
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
    let hold_empty = result.hold_feet.iter().all(|&f| f == Foot::None);
    if moved_left == moved_right || !hold_empty || did_jump {
        return 0.0;
    }

    let mut cost = 0.0;
    if jacked_left && result.did_the_foot_move[1] && result.did_the_foot_move[2] {
        cost += BRACKETJACK_WEIGHT;
    }
    if jacked_right && result.did_the_foot_move[3] && result.did_the_foot_move[4] {
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
    let hold_empty = result.hold_feet.iter().all(|&f| f == Foot::None);
    if moved_left == moved_right || !hold_empty || did_jump {
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

    let backward = |heel: isize, toe: isize| -> bool {
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

    let facing = |a: isize, b: isize, f: fn(&StageLayout, isize, isize) -> f32| -> f32 {
        if a != INVALID_COLUMN && b != INVALID_COLUMN {
            f(layout, a, b)
        } else {
            0.0
        }
    };

    let penalty = |v: f32| -> f32 {
        let base = -(v.min(0.0));
        if base > 0.0 {
            (base as f64).powf(1.8) as f32 * 100.0 * FACING_WEIGHT
        } else {
            0.0
        }
    };

    penalty(facing(lh, rh, StageLayout::get_x_diff))
        + penalty(facing(lt, rt, StageLayout::get_x_diff))
        + penalty(facing(lh, lt, StageLayout::get_y_diff))
        + penalty(facing(rh, rt, StageLayout::get_y_diff))
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
    layout
        .side_arrows
        .iter()
        .filter(|&&c| {
            initial.combined_columns[c] != result.columns[c]
                && result.columns[c] != Foot::None
                && initial.combined_columns[c] != Foot::None
                && !result.did_the_foot_move[initial.combined_columns[c].as_index()]
        })
        .count() as f32
        * SIDESWITCH_WEIGHT
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
    for &foot in &result.moved_feet {
        if foot == Foot::None {
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

        if last.column_count <= MAX_COLUMNS {
            let mut mask = last.hold_mask;
            while mask != 0 {
                let idx = mask.trailing_zeros() as usize;
                mask &= mask - 1;
                let hold = &last.holds[idx];
                let hold_end = hold.beat + hold.hold_length;
                if hold_end > start {
                    ds = false;
                    break;
                }
            }
        } else {
            for hold in &last.holds {
                if hold.note_type == TapNoteType::Empty {
                    continue;
                }
                let hold_end = hold.beat + hold.hold_length;
                if hold_end > start {
                    ds = false;
                    break;
                }
            }
        }
    }
    ds
}

// --- Generator ---

struct StepParityGenerator {
    layout: StageLayout,
    column_count: usize,
    permute_cache: FastMap<u32, Rc<[FootPlacement]>>,
    state_cache: FastMap<u64, Rc<State>>,
    nodes: NodeArena,
    rows: Vec<Row>,
}

impl StepParityGenerator {
    fn new(layout: StageLayout) -> Self {
        Self {
            column_count: layout.column_count(),
            layout,
            permute_cache: FastMap::default(),
            state_cache: FastMap::default(),
            nodes: NodeArena::new(),
            rows: Vec::new(),
        }
    }

    fn analyze(&mut self, notes: Vec<IntermediateNoteData>, cols: usize) -> bool {
        self.column_count = cols;
        self.permute_cache.clear();
        self.state_cache.clear();
        self.nodes.clear();
        self.rows.clear();
        self.create_rows(notes);
        if self.rows.is_empty() {
            return false;
        }
        self.build_graph();
        self.trace_path()
    }

    fn create_rows(&mut self, notes: Vec<IntermediateNoteData>) {
        let cols = self.column_count;
        let mut counter = RowCounter::new(cols);

        for note in notes {
            if note.note_type == TapNoteType::Empty {
                continue;
            }

            if note.note_type == TapNoteType::Mine {
                let target = if note.second == counter.last_second && !self.rows.is_empty() {
                    if note.fake {
                        &mut counter.next_fake_mines
                    } else {
                        &mut counter.next_mines
                    }
                } else if note.fake {
                    &mut counter.fake_mines
                } else {
                    &mut counter.mines
                };
                target[note.col] = note.second;
                continue;
            }

            if note.fake {
                continue;
            }

            if counter.last_second != note.second {
                if counter.last_second != CLM_SECOND_INVALID {
                    self.flush_row(&mut counter);
                }
                counter.reset_for_row(note.second, note.beat, cols);
            }

            let col = note.col;
            let is_hold = note.note_type == TapNoteType::HoldHead;
            counter.notes[col] = note.clone();
            if is_hold {
                counter.active_holds[col] = note;
            }
        }
        self.flush_row(&mut counter);
    }

    fn flush_row(&mut self, counter: &mut RowCounter) {
        if counter.last_second == CLM_SECOND_INVALID {
            return;
        }
        let mut row = self.build_row(counter);
        row.row_index = self.rows.len();
        self.rows.push(row);
    }

    fn build_row(&self, counter: &RowCounter) -> Row {
        let mut row = Row::new(self.column_count);
        row.notes.clone_from(&counter.notes);
        row.mines.clone_from(&counter.next_mines);
        row.fake_mines.clone_from(&counter.next_fake_mines);
        row.second = counter.last_second;
        row.beat = counter.last_beat;

        for c in 0..self.column_count.min(MAX_COLUMNS) {
            if row.notes[c].note_type != TapNoteType::Empty {
                row.note_count += 1;
                row.note_mask |= 1u8 << c;
            }

            row.holds[c] = if counter.active_holds[c].note_type == TapNoteType::Empty
                || counter.active_holds[c].second >= counter.last_second
            {
                IntermediateNoteData::default()
            } else {
                counter.active_holds[c].clone()
            };

            if row.holds[c].note_type != TapNoteType::Empty {
                row.hold_mask |= 1u8 << c;
            }
            if row.mines[c] != 0.0 {
                row.mine_mask |= 1u8 << c;
            }
            if (row.mines[c] as i32) != 0 {
                row.mine_i32_mask |= 1u8 << c;
            }
            if (row.fake_mines[c] as i32) != 0 {
                row.fake_mine_mask |= 1u8 << c;
            }
        }
        row
    }

    fn build_graph(&mut self) {
        let start = Rc::new(State::new(self.column_count));
        let start_sec = self.rows.first().map(|r| r.second - 1.0).unwrap_or(-1.0);
        let start_id = self.add_node(start, start_sec);

        let mut prev_ids = vec![start_id];
        let mut next_ids: Vec<usize> = Vec::new();
        let mut result_map: FastMap<usize, usize> = FastMap::default();

        for i in 0..self.rows.len() {
            let perms = self.perms_for_row(i);
            next_ids.clear();
            result_map.clear();
            result_map.reserve(perms.len());

            for &init_id in &prev_ids {
                self.nodes[init_id].neighbors.reserve(perms.len());
                let (init_state, init_sec) = {
                    let n = &self.nodes[init_id];
                    (Rc::clone(&n.state), n.second)
                };
                let row_second = self.rows[i].second;
                let elapsed = row_second - init_sec;

                for perm in perms.iter() {
                    let result = self.init_result_state(&init_state, i, perm);
                    let cost = calc_action_cost(
                        &self.layout,
                        &init_state,
                        &result,
                        &self.rows,
                        i,
                        elapsed,
                    );

                    let key = Rc::as_ptr(&result) as usize;
                    let res_id = if let Some(&id) = result_map.get(&key) {
                        id
                    } else {
                        let id = self.add_node(Rc::clone(&result), row_second);
                        next_ids.push(id);
                        result_map.insert(key, id);
                        id
                    };
                    self.add_edge(init_id, res_id, cost);
                }
            }
            std::mem::swap(&mut prev_ids, &mut next_ids);
        }

        let end = Rc::new(State::new(self.column_count));
        let end_sec = self.rows.last().map(|r| r.second + 1.0).unwrap_or(1.0);
        let end_id = self.add_node(end, end_sec);

        for &id in &prev_ids {
            self.nodes[id].neighbors.reserve(1);
            self.add_edge(id, end_id, 0.0);
        }
    }

    fn add_node(&mut self, state: Rc<State>, second: f32) -> usize {
        self.nodes.push(StepParityNode {
            state,
            second,
            neighbors: NeighborMap::default(),
        })
    }

    fn add_edge(&mut self, from: usize, to: usize, cost: f32) {
        let key = self.nodes.ptr(to) as usize;
        self.nodes[from].neighbors.insert_reserved(to, key, cost);
    }

    fn perms_for_row(&mut self, row_idx: usize) -> Rc<[FootPlacement]> {
        let row = &self.rows[row_idx];
        let key = (row.note_mask | row.hold_mask) as u32;
        if let Some(p) = self.permute_cache.get(&key) {
            return Rc::clone(p);
        }

        let mut cols = [Foot::None; MAX_COLUMNS];
        let mut perms = Vec::new();
        permute_row(
            &self.layout,
            row,
            &mut cols,
            0,
            row.column_count,
            false,
            0,
            &mut perms,
        );
        if perms.is_empty() {
            permute_row(
                &self.layout,
                row,
                &mut cols,
                0,
                row.column_count,
                true,
                0,
                &mut perms,
            );
        }
        if perms.is_empty() {
            perms.push([Foot::None; MAX_COLUMNS]);
        }

        let rc = Rc::from(perms.into_boxed_slice());
        self.permute_cache.insert(key, Rc::clone(&rc));
        rc
    }

    fn init_result_state(
        &mut self,
        initial: &State,
        row_idx: usize,
        cols: &FootPlacement,
    ) -> Rc<State> {
        let row = &self.rows[row_idx];
        let n = self.column_count;
        let hold_mask = row.hold_mask;

        // Compute hash inline
        let mut moved_mask = 0u8;
        let mut hash = 0u64;

        for i in 0..n {
            let foot = cols[i];
            hash = hash
                .wrapping_mul(STATE_HASH_PRIME)
                .wrapping_add(foot as u64);
            if foot != Foot::None
                && ((hold_mask & (1u8 << i)) == 0 || initial.combined_columns[i] != foot)
            {
                moved_mask |= FOOT_MASKS[foot.as_index()];
            }
        }

        // combined_columns hash
        let moved_left = (moved_mask & LEFT_FOOT_MASK) != 0;
        let moved_right = (moved_mask & RIGHT_FOOT_MASK) != 0;
        for i in 0..n {
            let combined = if cols[i] != Foot::None {
                cols[i]
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
            hash = hash
                .wrapping_mul(STATE_HASH_PRIME)
                .wrapping_add(combined as u64);
        }

        // moved and hold hashes
        for i in 0..n {
            let foot = cols[i];
            let moved = if foot != Foot::None
                && ((hold_mask & (1u8 << i)) == 0 || initial.combined_columns[i] != foot)
            {
                foot
            } else {
                Foot::None
            };
            hash = hash
                .wrapping_mul(STATE_HASH_PRIME)
                .wrapping_add(moved as u64);
        }
        for i in 0..n {
            let hold = if cols[i] != Foot::None && (hold_mask & (1u8 << i)) != 0 {
                cols[i]
            } else {
                Foot::None
            };
            hash = hash
                .wrapping_mul(STATE_HASH_PRIME)
                .wrapping_add(hold as u64);
        }

        if let Some(existing) = self.state_cache.get(&hash) {
            return Rc::clone(existing);
        }

        let mut state = State::new(n);
        for i in 0..n {
            let foot = cols[i];
            state.columns[i] = foot;
            if foot == Foot::None {
                continue;
            }

            let fi = foot.as_index();
            state.what_note_the_foot_is_hitting[fi] = i as isize;

            let hold_empty = (hold_mask & (1u8 << i)) == 0;
            if hold_empty || initial.combined_columns[i] != foot {
                state.moved_feet[i] = foot;
                state.did_the_foot_move[fi] = true;
            }
            if !hold_empty {
                state.hold_feet[i] = foot;
                state.is_the_foot_holding[fi] = true;
            }
        }

        // Merge combined_columns
        for i in 0..n {
            let combined = if cols[i] != Foot::None {
                cols[i]
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
                state.where_the_feet_are[combined.as_index()] = i as isize;
            }
        }

        let rc = Rc::new(state);
        self.state_cache.insert(hash, Rc::clone(&rc));
        rc
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
            let neighbors = &self.nodes[i].neighbors;
            let mut cur = neighbors.head;
            while cur != BUCKET_EMPTY {
                let e = &neighbors.entries[cur];
                let nc = cost[i] + e.cost;
                if nc < cost[e.neighbor_id] {
                    cost[e.neighbor_id] = nc;
                    pred[e.neighbor_id] = i;
                }
                cur = e.next;
            }
        }

        if pred[n - 1] == usize::MAX {
            return false;
        }

        // Trace back
        let mut path = vec![usize::MAX; self.rows.len()];
        let mut cur = n - 1;
        let mut write = self.rows.len();

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
            path[write] = cur;
        }

        if write != 0 {
            return false;
        }

        for (i, &node_id) in path.iter().enumerate() {
            let state = Rc::clone(&self.nodes[node_id].state);
            self.rows[i].set_foot_placement(&state.combined_columns);
        }
        true
    }
}

// --- RowCounter ---

struct RowCounter {
    notes: Vec<IntermediateNoteData>,
    active_holds: Vec<IntermediateNoteData>,
    mines: Vec<f32>,
    fake_mines: Vec<f32>,
    next_mines: Vec<f32>,
    next_fake_mines: Vec<f32>,
    last_second: f32,
    last_beat: f32,
}

impl RowCounter {
    fn new(cols: usize) -> Self {
        Self {
            notes: vec![IntermediateNoteData::default(); cols],
            active_holds: vec![IntermediateNoteData::default(); cols],
            mines: vec![0.0; cols],
            fake_mines: vec![0.0; cols],
            next_mines: vec![0.0; cols],
            next_fake_mines: vec![0.0; cols],
            last_second: CLM_SECOND_INVALID,
            last_beat: CLM_SECOND_INVALID,
        }
    }

    fn reset_for_row(&mut self, second: f32, beat: f32, cols: usize) {
        self.last_second = second;
        self.last_beat = beat;
        self.next_mines.clone_from(&self.mines);
        self.next_fake_mines.clone_from(&self.fake_mines);
        self.notes.fill(IntermediateNoteData::default());
        self.mines.fill(0.0);
        self.fake_mines.fill(0.0);

        for c in 0..cols {
            if self.active_holds[c].note_type == TapNoteType::Empty
                || beat > self.active_holds[c].beat + self.active_holds[c].hold_length
            {
                self.active_holds[c] = IntermediateNoteData::default();
            }
        }
    }
}

// --- Permutation ---

fn permute_row(
    layout: &StageLayout,
    row: &Row,
    cols: &mut FootPlacement,
    col: usize,
    col_count: usize,
    ignore_holds: bool,
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
                Foot::LeftHeel => lh = i as isize,
                Foot::LeftToe => lt = i as isize,
                Foot::RightHeel => rh = i as isize,
                Foot::RightToe => rt = i as isize,
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

    let mask = if ignore_holds {
        row.note_mask
    } else {
        row.note_mask | row.hold_mask
    };
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
                row,
                cols,
                col + 1,
                col_count,
                ignore_holds,
                used | fm,
                out,
            );
            cols[col] = Foot::None;
        }
    } else {
        permute_row(
            layout,
            row,
            cols,
            col + 1,
            col_count,
            ignore_holds,
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

fn calculate_tech_counts(rows: &[Row], layout: &StageLayout) -> TechCounts {
    let mut out = TechCounts::default();
    if rows.len() < 2 {
        return out;
    }

    for i in 1..rows.len() {
        let (curr, prev) = (&rows[i], &rows[i - 1]);
        let elapsed = curr.second - prev.second;

        // Jacks and doublesteps
        if curr.note_count == 1 && prev.note_count == 1 {
            for &foot in &FEET {
                let (cc, pc) = (
                    curr.where_the_feet_are[foot.as_index()],
                    prev.where_the_feet_are[foot.as_index()],
                );
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
            if curr.where_the_feet_are[1] != INVALID_COLUMN
                && curr.where_the_feet_are[2] != INVALID_COLUMN
            {
                out.brackets += 1;
            }
            if curr.where_the_feet_are[3] != INVALID_COLUMN
                && curr.where_the_feet_are[4] != INVALID_COLUMN
            {
                out.brackets += 1;
            }
        }

        // Footswitches by arrow type
        let is_switch = |c: usize| -> bool {
            let (p, r) = (prev.columns[c], curr.columns[c]);
            p != Foot::None
                && r != Foot::None
                && p != r
                && OTHER_PART_OF_FOOT[p.as_index()] != r
                && elapsed < FOOTSWITCH_CUTOFF
        };

        for &c in &layout.up_arrows {
            if is_switch(c) {
                out.up_footswitches += 1;
                out.footswitches += 1;
            }
        }
        for &c in &layout.down_arrows {
            if is_switch(c) {
                out.down_footswitches += 1;
                out.footswitches += 1;
            }
        }
        for &c in &layout.side_arrows {
            if is_switch(c) {
                out.sideswitches += 1;
            }
        }

        // Crossovers - restored original logic with prev_prev checks
        let left_heel = curr.where_the_feet_are[Foot::LeftHeel.as_index()];
        let left_toe = curr.where_the_feet_are[Foot::LeftToe.as_index()];
        let right_heel = curr.where_the_feet_are[Foot::RightHeel.as_index()];
        let right_toe = curr.where_the_feet_are[Foot::RightToe.as_index()];

        let prev_left_heel = prev.where_the_feet_are[Foot::LeftHeel.as_index()];
        let prev_left_toe = prev.where_the_feet_are[Foot::LeftToe.as_index()];
        let prev_right_heel = prev.where_the_feet_are[Foot::RightHeel.as_index()];
        let prev_right_toe = prev.where_the_feet_are[Foot::RightToe.as_index()];

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
                    let prev_prev_rh = prev_prev.where_the_feet_are[Foot::RightHeel.as_index()];
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
                    let prev_prev_lh = prev_prev.where_the_feet_are[Foot::LeftHeel.as_index()];
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

fn layout_for_lanes(lanes: usize) -> Option<StageLayout> {
    match lanes {
        4 => Some(StageLayout::new_dance_single()),
        8 => Some(StageLayout::new_dance_double()),
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
    for measure in data.split(|&b| b == b',') {
        let lines: Vec<_> = measure
            .split(|&b| b == b'\n')
            .map(trim_ws)
            .filter(|l| !l.is_empty())
            .collect();
        if lines.is_empty() {
            measure_idx += 1;
            continue;
        }

        let num = lines.len();
        let start = measure_idx as f32 * 4.0;
        let step = 4.0 / num as f32;

        for (j, line) in lines.into_iter().enumerate() {
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
                row: row.row as usize,
                beat: row.beat,
                second: row.second,
                fake: note_type == TapNoteType::Fake || row_fake,
                subtype: match ch {
                    b'4' => TapNoteSubType::Roll,
                    b'2' => TapNoteSubType::Hold,
                    _ => TapNoteSubType::Invalid,
                },
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
    let Some(layout) = layout_for_lanes(lanes) else {
        return TechCounts::default();
    };

    let rows = parse_rows(data, layout.column_count(), |beat| {
        time_between_beats(0.0, beat, bpm_map) as f32 - offset as f32
    });
    let notes = build_notes(&rows, None);

    let mut generator = StepParityGenerator::new(layout.clone());
    if !generator.analyze(notes, layout.column_count()) {
        return TechCounts::default();
    }
    calculate_tech_counts(&generator.rows, &generator.layout)
}

pub fn analyze_timing_lanes(data: &[u8], timing: &TimingData, lanes: usize) -> TechCounts {
    let Some(layout) = layout_for_lanes(lanes) else {
        return TechCounts::default();
    };

    let rows = parse_rows(data, layout.column_count(), |beat| {
        timing.get_time_for_beat_f32(beat as f64) as f32
    });
    let notes = build_notes(&rows, Some(timing));

    let mut generator = StepParityGenerator::new(layout.clone());
    if !generator.analyze(notes, layout.column_count()) {
        return TechCounts::default();
    }
    calculate_tech_counts(&generator.rows, &generator.layout)
}

pub(crate) fn analyze_timing_rows<const LANES: usize>(
    rows: &[[u8; LANES]],
    row_to_beat: &[f32],
    timing: &TimingData,
    _minimized_note_data: &[u8],
) -> TechCounts {
    let Some(layout) = layout_for_lanes(LANES) else {
        return TechCounts::default();
    };

    let parsed = parse_rows_from_arrays(rows, row_to_beat, timing, layout.column_count());
    let notes = build_notes(&parsed, Some(timing));

    let mut generator = StepParityGenerator::new(layout.clone());
    if !generator.analyze(notes, layout.column_count()) {
        return TechCounts::default();
    }
    calculate_tech_counts(&generator.rows, &generator.layout)
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

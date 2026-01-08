use std::collections::{HashMap, VecDeque};
use std::hash::{BuildHasherDefault, Hasher};
use std::rc::Rc;
use std::sync::OnceLock;

use crate::timing::{beat_to_note_row_f32_exact, TimingData, ROWS_PER_BEAT};

const INVALID_COLUMN: isize = -1;
const CLM_SECOND_INVALID: f32 = -1.0;
const MAX_NOTE_ROW: i32 = 1 << 30;
// Sentinel for unmatched hold heads (NoteData uses MAX_NOTE_ROW).
const MISSING_HOLD_LENGTH_BEATS: f32 = MAX_NOTE_ROW as f32 / ROWS_PER_BEAT as f32;

// Weights and thresholds from ITGmania source
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

// 0.1 = 1/16th at 150bpm. Jacks quicker than this are harder.
const JACK_THRESHOLD: f32 = 0.1;
// 0.15 = 1/8th at 200bpm.
const SLOW_BRACKET_THRESHOLD: f32 = 0.15;
// 0.2 = 1/8th at 150bpm.
const SLOW_FOOTSWITCH_THRESHOLD: f32 = 0.2;
// 0.4 = 1/4th at 150bpm. Ignore footswitch penalty after this.
const SLOW_FOOTSWITCH_IGNORE: f32 = 0.4;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Ord, PartialOrd)]
#[repr(usize)]
pub enum Foot {
    None = 0,
    LeftHeel = 1,
    LeftToe = 2,
    RightHeel = 3,
    RightToe = 4,
}

impl Foot {
    fn as_index(self) -> usize {
        self as usize
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
const OTHER_PART_OF_FOOT: [Foot; NUM_FEET] = [
    Foot::None,
    Foot::LeftToe,
    Foot::LeftHeel,
    Foot::RightToe,
    Foot::RightHeel,
];

fn foot_label(foot: Foot) -> &'static str {
    match foot {
        Foot::None => "N",
        Foot::LeftHeel => "LH",
        Foot::LeftToe => "LT",
        Foot::RightHeel => "RH",
        Foot::RightToe => "RT",
    }
}

fn format_foot_vec(feet: &[Foot]) -> String {
    let mut out = String::from("[");
    for (i, foot) in feet.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str(foot_label(*foot));
    }
    out.push(']');
    out
}

fn format_foot_positions(positions: &[isize]) -> String {
    let get = |foot: Foot| positions.get(foot.as_index()).copied().unwrap_or(INVALID_COLUMN);
    format!(
        "lh={} lt={} rh={} rt={}",
        get(Foot::LeftHeel),
        get(Foot::LeftToe),
        get(Foot::RightHeel),
        get(Foot::RightToe)
    )
}

fn format_foot_flags(flags: &[bool]) -> String {
    let get = |foot: Foot| flags.get(foot.as_index()).copied().unwrap_or(false);
    let as_u8 = |flag| if flag { 1 } else { 0 };
    format!(
        "lh={} lt={} rh={} rt={}",
        as_u8(get(Foot::LeftHeel)),
        as_u8(get(Foot::LeftToe)),
        as_u8(get(Foot::RightHeel)),
        as_u8(get(Foot::RightToe))
    )
}

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

    fn write_usize(&mut self, value: usize) {
        self.0 = value as u64;
    }

    fn write_u32(&mut self, value: u32) {
        self.0 = value as u64;
    }

    fn write_u64(&mut self, value: u64) {
        self.0 = value;
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
    next: Option<usize>,
    hash_key: usize,
}

const BUCKET_EMPTY: usize = usize::MAX;
const BUCKET_SENTINEL: usize = usize::MAX - 1;

// Match libstdc++ unordered_map iteration order to keep tie-breaking aligned.
#[derive(Debug, Clone)]
struct NeighborMap {
    entries: Vec<NeighborEntry>,
    head: Option<usize>,
    bucket_before: Vec<usize>,
    bucket_count: usize,
}

impl Default for NeighborMap {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
            head: None,
            bucket_before: vec![BUCKET_EMPTY; 13],
            bucket_count: 13,
        }
    }
}

impl NeighborMap {
    fn insert(&mut self, neighbor_id: usize, hash_key: usize, cost: f32) {
        if let Some(entry) = self
            .entries
            .iter_mut()
            .find(|entry| entry.neighbor_id == neighbor_id)
        {
            entry.cost = cost;
            return;
        }

        let new_size = self.entries.len() + 1;
        if new_size > self.bucket_count {
            self.rehash(next_prime(self.bucket_count.saturating_mul(2).max(1)));
        }

        let idx = self.entries.len();
        self.entries.push(NeighborEntry {
            neighbor_id,
            cost,
            next: None,
            hash_key,
        });
        self.insert_index(idx);
    }

    fn get(&self, neighbor_id: usize) -> Option<f32> {
        self.entries
            .iter()
            .find(|entry| entry.neighbor_id == neighbor_id)
            .map(|entry| entry.cost)
    }

    fn for_each_in_order<F>(&self, mut visit: F)
    where
        F: FnMut(usize, f32),
    {
        let mut current = self.head;
        while let Some(idx) = current {
            let entry = &self.entries[idx];
            visit(entry.neighbor_id, entry.cost);
            current = entry.next;
        }
    }

    fn rehash(&mut self, new_bucket_count: usize) {
        self.bucket_count = new_bucket_count;
        self.bucket_before = vec![BUCKET_EMPTY; new_bucket_count];
        let mut prev = None;
        let mut current = self.head;
        while let Some(idx) = current {
            let bucket = self.bucket_index(self.entries[idx].hash_key);
            if self.bucket_before[bucket] == BUCKET_EMPTY {
                self.bucket_before[bucket] = match prev {
                    None => BUCKET_SENTINEL,
                    Some(prev_idx) => prev_idx,
                };
            }
            prev = Some(idx);
            current = self.entries[idx].next;
        }
    }

    fn insert_index(&mut self, idx: usize) {
        let bucket = self.bucket_index(self.entries[idx].hash_key);
        let before = self.bucket_before[bucket];

        if before == BUCKET_EMPTY {
            let old_head = self.head;
            self.entries[idx].next = old_head;
            self.head = Some(idx);
            self.bucket_before[bucket] = BUCKET_SENTINEL;
            if let Some(old_idx) = old_head {
                let old_bucket = self.bucket_index(self.entries[old_idx].hash_key);
                self.bucket_before[old_bucket] = idx;
            }
            return;
        }

        if before == BUCKET_SENTINEL {
            let old_head = self.head;
            self.entries[idx].next = old_head;
            self.head = Some(idx);
            return;
        }

        let before_idx = before;
        let after = self.entries[before_idx].next;
        self.entries[idx].next = after;
        self.entries[before_idx].next = Some(idx);
    }

    fn bucket_index(&self, key: usize) -> usize {
        key % self.bucket_count
    }
}

fn next_prime(start: usize) -> usize {
    if start <= 2 {
        return 2;
    }
    let mut candidate = if start % 2 == 0 { start + 1 } else { start };
    loop {
        if is_prime(candidate) {
            return candidate;
        }
        candidate = candidate.saturating_add(2);
    }
}

fn is_prime(value: usize) -> bool {
    if value <= 3 {
        return value > 1;
    }
    if value % 2 == 0 || value % 3 == 0 {
        return false;
    }
    let mut i = 5usize;
    while i.saturating_mul(i) <= value {
        if value % i == 0 || value % (i + 2) == 0 {
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
        let invalid_index = columns.len();
        let mut avg_points = vec![StagePoint::default(); pair_len];
        let mut x_diffs = vec![0.0f32; pair_len];
        let mut y_diffs = vec![0.0f32; pair_len];

        for left in 0..pair_stride {
            for right in 0..pair_stride {
                let idx = left * pair_stride + right;
                let left_point = if left == invalid_index {
                    None
                } else {
                    Some(columns[left])
                };
                let right_point = if right == invalid_index {
                    None
                } else {
                    Some(columns[right])
                };

                avg_points[idx] = match (left_point, right_point) {
                    (None, None) => StagePoint::default(),
                    (None, Some(r)) => r,
                    (Some(l), None) => l,
                    (Some(l), Some(r)) => StagePoint {
                        x: (l.x + r.x) / 2.0,
                        y: (l.y + r.y) / 2.0,
                    },
                };

                if left == right || left == invalid_index || right == invalid_index {
                    continue;
                }

                let left = columns[left];
                let right = columns[right];
                let dx = (right.x - left.x) as f64;
                let dy = (right.y - left.y) as f64;
                let distance = (dx * dx + dy * dy).sqrt();
                if distance == 0.0 {
                    continue;
                }

                let norm_dx = dx / distance;
                let norm_dy = dy / distance;
                let mut x_mag = norm_dx.abs().powf(4.0) as f32;
                let mut y_mag = norm_dy.abs().powf(4.0) as f32;
                if norm_dx <= 0.0 {
                    x_mag = -x_mag;
                }
                if norm_dy <= 0.0 {
                    y_mag = -y_mag;
                }
                x_diffs[idx] = x_mag;
                y_diffs[idx] = y_mag;
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
        }
    }

    fn column_count(&self) -> usize {
        self.columns.len()
    }

    fn bracket_check(&self, column1: usize, column2: usize) -> bool {
        let p1 = self.columns[column1];
        let p2 = self.columns[column2];
        self.get_distance_sq_points(p1, p2) <= 2.0
    }

    fn get_distance_sq(&self, c1: usize, c2: usize) -> f32 {
        self.get_distance_sq_points(self.columns[c1], self.columns[c2])
    }

    fn get_distance_sq_points(&self, p1: StagePoint, p2: StagePoint) -> f32 {
        let dx = p1.x - p2.x;
        let dy = p1.y - p2.y;
        dx * dx + dy * dy
    }

    fn get_x_difference(&self, left_index: isize, right_index: isize) -> f32 {
        let idx = self.pair_index(left_index, right_index);
        self.x_diffs[idx]
    }

    fn get_y_difference(&self, left_index: isize, right_index: isize) -> f32 {
        let idx = self.pair_index(left_index, right_index);
        self.y_diffs[idx]
    }

    fn average_point(&self, left_index: isize, right_index: isize) -> StagePoint {
        let idx = self.pair_index(left_index, right_index);
        self.avg_points[idx]
    }

    fn pair_index(&self, left_index: isize, right_index: isize) -> usize {
        let max_index = self.pair_stride - 1;
        let to_index = |idx| if idx == INVALID_COLUMN { max_index } else { idx as usize };
        let left = to_index(left_index);
        let right = to_index(right_index);
        left * self.pair_stride + right
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TapNoteType {
    Empty,
    Tap,
    HoldHead,
    HoldTail,
    Mine,
    Fake,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TapNoteSubType {
    Invalid,
    Hold,
    Roll,
}

impl Default for TapNoteSubType {
    fn default() -> Self {
        TapNoteSubType::Invalid
    }
}

#[derive(Debug, Clone)]
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

impl Default for IntermediateNoteData {
    fn default() -> Self {
        Self {
            note_type: TapNoteType::Empty,
            subtype: TapNoteSubType::Invalid,
            col: 0,
            row: 0,
            beat: 0.0,
            hold_length: -1.0,
            fake: false,
            second: 0.0,
            parity: Foot::None,
        }
    }
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
}

impl Row {
    fn new(column_count: usize) -> Self {
        Self {
            notes: vec![IntermediateNoteData::default(); column_count],
            holds: vec![IntermediateNoteData::default(); column_count],
            mines: vec![0.0; column_count],
            fake_mines: vec![0.0; column_count],
            columns: vec![Foot::None; column_count],
            where_the_feet_are: vec![INVALID_COLUMN; NUM_FEET],
            second: 0.0,
            beat: 0.0,
            row_index: 0,
            column_count,
            note_count: 0,
        }
    }

    fn set_foot_placement(&mut self, foot_placement: &[Foot]) {
        self.note_count = 0;
        for c in 0..self.column_count {
            if self.notes[c].note_type != TapNoteType::Empty {
                let foot = foot_placement[c];
                self.notes[c].parity = foot;
                self.columns[c] = foot;
                if foot != Foot::None {
                    self.where_the_feet_are[foot.as_index()] = c as isize;
                }
                self.note_count += 1;
            } else {
                self.columns[c] = Foot::None;
            }
        }
    }
}

#[derive(Debug, Clone)]
struct RowCounter {
    notes: Vec<IntermediateNoteData>,
    active_holds: Vec<IntermediateNoteData>,
    mines: Vec<f32>,
    fake_mines: Vec<f32>,
    next_mines: Vec<f32>,
    next_fake_mines: Vec<f32>,
    last_column_second: f32,
    last_column_beat: f32,
}

impl RowCounter {
    fn new(column_count: usize) -> Self {
        Self {
            notes: vec![IntermediateNoteData::default(); column_count],
            active_holds: vec![IntermediateNoteData::default(); column_count],
            mines: vec![0.0; column_count],
            fake_mines: vec![0.0; column_count],
            next_mines: vec![0.0; column_count],
            next_fake_mines: vec![0.0; column_count],
            last_column_second: CLM_SECOND_INVALID,
            last_column_beat: CLM_SECOND_INVALID,
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
    fn new(column_count: usize) -> Self {
        Self {
            columns: vec![Foot::None; column_count],
            combined_columns: vec![Foot::None; column_count],
            moved_feet: vec![Foot::None; column_count],
            hold_feet: vec![Foot::None; column_count],
            where_the_feet_are: [INVALID_COLUMN; NUM_FEET],
            what_note_the_foot_is_hitting: [INVALID_COLUMN; NUM_FEET],
            did_the_foot_move: [false; NUM_FEET],
            is_the_foot_holding: [false; NUM_FEET],
        }
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

type FootPlacement = Vec<Foot>;

#[derive(Debug, Clone)]
struct StepParityNode {
    state: Rc<State>,
    second: f32,
    neighbors: NeighborMap,
}

impl StepParityNode {
    fn new(state: Rc<State>, second: f32) -> Self {
        Self {
            state,
            second,
            neighbors: NeighborMap::default(),
        }
    }
}

struct StepParityGenerator {
    layout: StageLayout,
    column_count: usize,
    permute_cache: FastMap<u32, Rc<[FootPlacement]>>,
    state_cache: FastMap<u64, Rc<State>>,
    nodes: Vec<Box<StepParityNode>>,
    rows: Vec<Row>,
}

#[derive(Default)]
struct StepParityStats {
    rows: usize,
    perms_total: usize,
    perm_calls_total: usize,
    prev_nodes_peak: usize,
    result_nodes_new: usize,
    nodes_total: usize,
    edges_total: usize,
    state_cache_hits: usize,
    state_cache_misses: usize,
    perm_cache_hits: usize,
    perm_cache_misses: usize,
}

impl StepParityGenerator {
    fn new(layout: StageLayout) -> Self {
        Self {
            column_count: layout.column_count(),
            layout,
            permute_cache: FastMap::default(),
            state_cache: FastMap::default(),
            nodes: Vec::new(),
            rows: Vec::new(),
        }
    }

    #[cfg_attr(feature = "profile", inline(never))]
    fn analyze_note_data(
        &mut self,
        note_data: Vec<IntermediateNoteData>,
        column_count: usize,
    ) -> bool {
        self.column_count = column_count;
        self.permute_cache.clear();
        self.state_cache.clear();
        self.nodes.clear();
        self.rows.clear();
        self.create_rows(note_data);
        if self.rows.is_empty() {
            return false;
        }
        self.build_state_graph();
        self.analyze_graph()
    }

    #[cfg_attr(feature = "profile", inline(never))]
    fn create_rows(&mut self, note_data: Vec<IntermediateNoteData>) {
        let column_count = self.column_count;
        let mut counter = RowCounter::new(column_count);

        for note in note_data.into_iter() {
            if note.note_type == TapNoteType::Empty {
                continue;
            }

            if note.note_type == TapNoteType::Mine {
                if note.second == counter.last_column_second && !self.rows.is_empty() {
                    if note.fake {
                        counter.next_fake_mines[note.col] = note.second;
                    } else {
                        counter.next_mines[note.col] = note.second;
                    }
                } else if note.fake {
                    counter.fake_mines[note.col] = note.second;
                } else {
                    counter.mines[note.col] = note.second;
                }
                continue;
            }

            if note.fake {
                continue;
            }

            if counter.last_column_second != note.second {
                if counter.last_column_second != CLM_SECOND_INVALID {
                    self.add_row(&mut counter);
                }

                counter.last_column_second = note.second;
                counter.last_column_beat = note.beat;
                counter.next_mines.clone_from(&counter.mines);
                counter.next_fake_mines.clone_from(&counter.fake_mines);
                counter.notes.fill(IntermediateNoteData::default());
                counter.mines.fill(0.0);
                counter.fake_mines.fill(0.0);

                for c in 0..column_count {
                    if counter.active_holds[c].note_type == TapNoteType::Empty
                        || note.beat
                            > counter.active_holds[c].beat + counter.active_holds[c].hold_length
                    {
                        counter.active_holds[c] = IntermediateNoteData::default();
                    }
                }
            }

            counter.notes[note.col] = note.clone();
            if note.note_type == TapNoteType::HoldHead {
                counter.active_holds[note.col] = note.clone();
            }
        }

        self.add_row(&mut counter);
    }

    #[cfg_attr(feature = "profile", inline(never))]
    fn add_row(&mut self, counter: &mut RowCounter) {
        if counter.last_column_second == CLM_SECOND_INVALID {
            return;
        }
        let mut row = self.create_row(counter);
        row.row_index = self.rows.len();
        self.rows.push(row);
    }

    #[cfg_attr(feature = "profile", inline(never))]
    fn create_row(&self, counter: &RowCounter) -> Row {
        let mut row = Row::new(self.column_count);
        row.notes.clone_from(&counter.notes);
        row.mines.clone_from(&counter.next_mines);
        row.fake_mines.clone_from(&counter.next_fake_mines);
        row.second = counter.last_column_second;
        row.beat = counter.last_column_beat;

        for c in 0..self.column_count {
            if counter.active_holds[c].note_type == TapNoteType::Empty
                || counter.active_holds[c].second >= counter.last_column_second
            {
                row.holds[c] = IntermediateNoteData::default();
            } else {
                row.holds[c] = counter.active_holds[c].clone();
            }
        }

        row
    }

    #[cfg_attr(feature = "profile", inline(never))]
    fn build_state_graph(&mut self) {
        self.nodes.clear();
        self.state_cache.clear();

        let column_count = self.column_count;
        let layout = &self.layout;
        let cost_calculator = CostCalculator::new(layout);
        let rows = &self.rows;
        let permute_cache = &mut self.permute_cache;
        let state_cache = &mut self.state_cache;
        let nodes = &mut self.nodes;
        let track_stats = env_flags().dump_stats;
        let mut stats = StepParityStats::default();
        if track_stats {
            stats.rows = rows.len();
        }

        let start_state = Rc::new(State::new(column_count));
        let start_second = rows.first().map(|r| r.second - 1.0).unwrap_or(-1.0);
        if track_stats {
            stats.nodes_total += 1;
        }
        let start_id = add_node(nodes, start_state, start_second);

        let mut prev_node_ids = vec![start_id];
        if track_stats {
            stats.prev_nodes_peak = prev_node_ids.len();
        }

        for (i, row) in rows.iter().enumerate() {
            let permutations = perms_for_row(
                permute_cache,
                layout,
                row,
                if track_stats { Some(&mut stats) } else { None },
            );
            if track_stats {
                stats.perms_total += permutations.len();
                stats.prev_nodes_peak = stats.prev_nodes_peak.max(prev_node_ids.len());
                stats.perm_calls_total += prev_node_ids.len() * permutations.len();
                stats.edges_total += prev_node_ids.len() * permutations.len();
            }
            let mut result_nodes_for_row: Vec<usize> = Vec::with_capacity(permutations.len());
            let mut result_node_map: FastMap<usize, usize> = FastMap::default();
            result_node_map.reserve(permutations.len());

            for &initial_node_id in &prev_node_ids {
                let (initial_state, initial_second) = {
                    let node = &nodes[initial_node_id];
                    (Rc::clone(&node.state), node.second)
                };
                let elapsed = row.second - initial_second;

                for perm in permutations.iter() {
                    let result_state = init_result_state(
                        state_cache,
                        &initial_state,
                        row,
                        perm,
                        if track_stats { Some(&mut stats) } else { None },
                    );
                    let cost = cost_calculator.get_action_cost(
                        &initial_state,
                        &result_state,
                        rows,
                        i,
                        elapsed,
                    );

                    // Rc pointers are stable; use the address for per-row dedupe.
                    let state_key = Rc::as_ptr(&result_state) as usize;
                    let result_node_id = if let Some(&id) = result_node_map.get(&state_key) {
                        id
                    } else {
                        if track_stats {
                            stats.nodes_total += 1;
                        }
                        let id = add_node(nodes, Rc::clone(&result_state), row.second);
                        result_nodes_for_row.push(id);
                        result_node_map.insert(state_key, id);
                        id
                    };

                    add_edge(nodes, initial_node_id, result_node_id, cost);
                }
            }

            if track_stats {
                stats.result_nodes_new += result_nodes_for_row.len();
            }
            prev_node_ids = result_nodes_for_row;
        }

        let end_state = Rc::new(State::new(column_count));
        let end_second = rows.last().map(|r| r.second + 1.0).unwrap_or(1.0);
        if track_stats {
            stats.nodes_total += 1;
        }
        let end_id = add_node(nodes, end_state, end_second);

        let end_edge_count = prev_node_ids.len();
        for &node_id in &prev_node_ids {
            add_edge(nodes, node_id, end_id, 0.0);
        }
        if track_stats {
            stats.edges_total += end_edge_count;
            let result_nodes_reused = stats
                .perm_calls_total
                .saturating_sub(stats.result_nodes_new);
            eprintln!(
                "STEP_PARITY_STATS rows={} perms_total={} perm_calls={} nodes={} edges={} prev_nodes_peak={} result_nodes_new={} result_nodes_reused={} state_cache_hits={} state_cache_misses={} state_cache_len={} perm_cache_hits={} perm_cache_misses={} perm_cache_len={}",
                stats.rows,
                stats.perms_total,
                stats.perm_calls_total,
                stats.nodes_total,
                stats.edges_total,
                stats.prev_nodes_peak,
                stats.result_nodes_new,
                result_nodes_reused,
                stats.state_cache_hits,
                stats.state_cache_misses,
                state_cache.len(),
                stats.perm_cache_hits,
                stats.perm_cache_misses,
                permute_cache.len()
            );
        }
    }

    #[cfg_attr(feature = "profile", inline(never))]
    fn compute_cheapest_path(&self) -> Vec<usize> {
        if self.nodes.is_empty() {
            return Vec::new();
        }

        let start_id = 0;
        let end_id = self.nodes.len() - 1;
        let mut cost = vec![f32::MAX; self.nodes.len()];
        let mut predecessor = vec![usize::MAX; self.nodes.len()];
        let dump_ties = env_flags().dump_ties;
        let mut tie_count = 0usize;
        cost[start_id] = 0.0;

        for i in start_id..=end_id {
            if cost[i] == f32::MAX {
                continue;
            }
            self.nodes[i]
                .neighbors
                .for_each_in_order(|neighbor_id, weight| {
                    let new_cost = cost[i] + weight;
                    if new_cost < cost[neighbor_id] {
                        cost[neighbor_id] = new_cost;
                        predecessor[neighbor_id] = i;
                    } else if dump_ties && new_cost == cost[neighbor_id] {
                        tie_count += 1;
                    }
                });
        }
        if dump_ties {
            eprintln!("STEP_PARITY_TIES count={tie_count}");
        }

        let mut path = VecDeque::new();
        let mut current = end_id;
        if predecessor[current] == usize::MAX {
            return Vec::new();
        }

        while current != start_id {
            if current == usize::MAX {
                return Vec::new();
            }
            if current != end_id {
                path.push_front(current);
            }
            let next = predecessor[current];
            if next == usize::MAX && current != start_id {
                return Vec::new();
            }
            current = next;
        }

        path.into_iter().collect()
    }

    #[cfg_attr(feature = "profile", inline(never))]
    fn analyze_graph(&mut self) -> bool {
        let nodes_for_rows = self.compute_cheapest_path();
        if nodes_for_rows.len() != self.rows.len() {
            return false;
        }
        let dump_path = env_flags().dump_path;
        let mut total_cost = 0.0f32;
        if dump_path {
            let end_id = self.nodes.len().saturating_sub(1);
            eprintln!(
                "STEP_PARITY_PATH start rows={} nodes={} start=0 end={}",
                self.rows.len(),
                self.nodes.len(),
                end_id
            );
        }
        for (i, &node_id) in nodes_for_rows.iter().enumerate() {
            let state = Rc::clone(&self.nodes[node_id].state);
            self.rows[i].set_foot_placement(&state.combined_columns);
            if dump_path {
                let prev_id = if i == 0 { 0 } else { nodes_for_rows[i - 1] };
                let edge_cost = self.nodes[prev_id]
                    .neighbors
                    .get(node_id)
                    .unwrap_or(-1.0);
                total_cost += edge_cost;
                let row = &self.rows[i];
                let columns = format_foot_vec(&state.columns);
                let combined = format_foot_vec(&state.combined_columns);
                let moved = format_foot_vec(&state.moved_feet);
                let hold = format_foot_vec(&state.hold_feet);
                let row_feet = format_foot_positions(row.where_the_feet_are.as_slice());
                let state_feet = format_foot_positions(&state.where_the_feet_are);
                let moved_flags = format_foot_flags(&state.did_the_foot_move);
                let hold_flags = format_foot_flags(&state.is_the_foot_holding);
                eprintln!(
                    "STEP_PARITY_PATH row_idx={} node={} prev={} edge_cost={:.6} total_cost={:.6} beat={:.6} second={:.6} note_count={} columns={} combined={} moved={} hold={} row_feet={} state_feet={} moved_flags={} hold_flags={}",
                    i,
                    node_id,
                    prev_id,
                    edge_cost,
                    total_cost,
                    row.beat,
                    row.second,
                    row.note_count,
                    columns,
                    combined,
                    moved,
                    hold,
                    row_feet,
                    state_feet,
                    moved_flags,
                    hold_flags
                );
            }
        }
        if dump_path {
            let end_id = self.nodes.len().saturating_sub(1);
            let last_id = nodes_for_rows.last().copied().unwrap_or(0);
            let edge_cost = self.nodes[last_id]
                .neighbors
                .get(end_id)
                .unwrap_or(-1.0);
            total_cost += edge_cost;
            eprintln!(
                "STEP_PARITY_PATH end last_node={} end_node={} edge_cost={:.6} total_cost={:.6}",
                last_id,
                end_id,
                edge_cost,
                total_cost
            );
        }
        true
    }

}

#[cfg_attr(feature = "profile", inline(never))]
fn init_result_state(
    state_cache: &mut FastMap<u64, Rc<State>>,
    initial_state: &State,
    row: &Row,
    columns: &[Foot],
    mut stats: Option<&mut StepParityStats>,
) -> Rc<State> {
    let column_count = columns.len();
    let dump_collisions = env_flags().dump_state_collisions;

    if !dump_collisions && column_count <= MAX_COLUMNS {
        let mut columns_buf = [Foot::None; MAX_COLUMNS];
        let mut combined_buf = [Foot::None; MAX_COLUMNS];
        let mut moved_buf = [Foot::None; MAX_COLUMNS];
        let mut hold_buf = [Foot::None; MAX_COLUMNS];
        let mut did_move = [false; NUM_FEET];

        for (i, &foot) in columns.iter().enumerate() {
            columns_buf[i] = foot;
            if foot == Foot::None {
                continue;
            }
            let foot_index = foot.as_index();
            let hold_empty = row.holds[i].note_type == TapNoteType::Empty;
            if hold_empty {
                moved_buf[i] = foot;
                did_move[foot_index] = true;
            } else if initial_state.combined_columns[i] != foot {
                moved_buf[i] = foot;
                did_move[foot_index] = true;
            }
            if !hold_empty {
                hold_buf[i] = foot;
            }
        }

        merge_initial_and_result_position_parts(
            initial_state,
            &columns_buf[..column_count],
            &mut combined_buf[..column_count],
            &did_move,
        );

        let hash = state_hash_from_parts(
            &columns_buf[..column_count],
            &combined_buf[..column_count],
            &moved_buf[..column_count],
            &hold_buf[..column_count],
        );
        if let Some(existing) = state_cache.get(&hash) {
            if let Some(stats) = stats.as_deref_mut() {
                stats.state_cache_hits += 1;
            }
            return Rc::clone(existing);
        }

        if let Some(stats) = stats.as_deref_mut() {
            stats.state_cache_misses += 1;
        }

        let mut what_note = [INVALID_COLUMN; NUM_FEET];
        let mut is_holding = [false; NUM_FEET];
        for (i, &foot) in columns_buf[..column_count].iter().enumerate() {
            if foot == Foot::None {
                continue;
            }
            let foot_index = foot.as_index();
            what_note[foot_index] = i as isize;
            if row.holds[i].note_type != TapNoteType::Empty {
                is_holding[foot_index] = true;
            }
        }

        let mut where_the_feet_are = [INVALID_COLUMN; NUM_FEET];
        for (col, &foot) in combined_buf[..column_count].iter().enumerate() {
            if foot != Foot::None {
                where_the_feet_are[foot.as_index()] = col as isize;
            }
        }

        let result_state = State {
            columns: columns_buf[..column_count].to_vec(),
            combined_columns: combined_buf[..column_count].to_vec(),
            moved_feet: moved_buf[..column_count].to_vec(),
            hold_feet: hold_buf[..column_count].to_vec(),
            where_the_feet_are,
            what_note_the_foot_is_hitting: what_note,
            did_the_foot_move: did_move,
            is_the_foot_holding: is_holding,
        };

        let rc = Rc::new(result_state);
        state_cache.insert(hash, Rc::clone(&rc));
        return rc;
    }

    let mut result_state = State::new(column_count);
    for (i, &foot) in columns.iter().enumerate() {
        result_state.columns[i] = foot;
        if foot == Foot::None {
            continue;
        }
        let foot_index = foot.as_index();
        result_state.what_note_the_foot_is_hitting[foot_index] = i as isize;

        let hold_empty = row.holds[i].note_type == TapNoteType::Empty;
        if hold_empty {
            result_state.moved_feet[i] = foot;
            result_state.did_the_foot_move[foot_index] = true;
        } else if initial_state.combined_columns[i] != foot {
            result_state.moved_feet[i] = foot;
            result_state.did_the_foot_move[foot_index] = true;
        }

        if !hold_empty {
            result_state.hold_feet[i] = foot;
            result_state.is_the_foot_holding[foot_index] = true;
        }
    }

    merge_initial_and_result_position_parts(
        initial_state,
        &result_state.columns,
        &mut result_state.combined_columns,
        &result_state.did_the_foot_move,
    );

    for (col, &foot) in result_state.combined_columns.iter().enumerate() {
        if foot != Foot::None {
            result_state.where_the_feet_are[foot.as_index()] = col as isize;
        }
    }

    let hash = get_state_cache_key(&result_state);
    if let Some(existing) = state_cache.get(&hash) {
        if let Some(stats) = stats.as_deref_mut() {
            stats.state_cache_hits += 1;
        }
        if dump_collisions && **existing != result_state {
            eprintln!("STATE_HASH_COLLISION hash={hash}");
        }
        return Rc::clone(existing);
    }

    if let Some(stats) = stats.as_deref_mut() {
        stats.state_cache_misses += 1;
    }
    let rc = Rc::new(result_state);
    state_cache.insert(hash, Rc::clone(&rc));
    rc
}

#[cfg_attr(feature = "profile", inline(never))]
fn merge_initial_and_result_position_parts(
    initial: &State,
    columns: &[Foot],
    combined_columns: &mut [Foot],
    did_the_foot_move: &[bool],
) {
    for i in 0..columns.len() {
        if columns[i] != Foot::None {
            combined_columns[i] = columns[i];
            continue;
        }

        match initial.combined_columns[i] {
            Foot::LeftHeel | Foot::RightHeel => {
                let prev = initial.combined_columns[i];
                if prev != Foot::None && !did_the_foot_move[prev.as_index()] {
                    combined_columns[i] = prev;
                }
            }
            Foot::LeftToe => {
                if !did_the_foot_move[Foot::LeftToe.as_index()]
                    && !did_the_foot_move[Foot::LeftHeel.as_index()]
                {
                    combined_columns[i] = Foot::LeftToe;
                }
            }
            Foot::RightToe => {
                if !did_the_foot_move[Foot::RightToe.as_index()]
                    && !did_the_foot_move[Foot::RightHeel.as_index()]
                {
                    combined_columns[i] = Foot::RightToe;
                }
            }
            Foot::None => {}
        }
    }
}

#[cfg_attr(feature = "profile", inline(never))]
fn add_node(nodes: &mut Vec<Box<StepParityNode>>, state: Rc<State>, second: f32) -> usize {
    let id = nodes.len();
    nodes.push(Box::new(StepParityNode::new(state, second)));
    id
}

#[cfg_attr(feature = "profile", inline(never))]
fn add_edge(nodes: &mut Vec<Box<StepParityNode>>, from_id: usize, to_id: usize, cost: f32) {
    if to_id >= nodes.len() {
        return;
    }
    let hash_key = nodes[to_id].as_ref() as *const StepParityNode as usize;
    if let Some(node) = nodes.get_mut(from_id) {
        node.neighbors.insert(to_id, hash_key, cost);
    }
}

#[cfg_attr(feature = "profile", inline(never))]
fn perms_for_row(
    permute_cache: &mut FastMap<u32, Rc<[FootPlacement]>>,
    layout: &StageLayout,
    row: &Row,
    mut stats: Option<&mut StepParityStats>,
) -> Rc<[FootPlacement]> {
    let mut key = 0u32;
    for i in 0..row.column_count.min(32) {
        if row.notes[i].note_type != TapNoteType::Empty
            || row.holds[i].note_type != TapNoteType::Empty
        {
            key |= 1 << i;
        }
    }

    if let Some(perms) = permute_cache.get(&key) {
        if let Some(stats) = stats.as_deref_mut() {
            stats.perm_cache_hits += 1;
        }
        return Rc::clone(perms);
    }
    if let Some(stats) = stats.as_deref_mut() {
        stats.perm_cache_misses += 1;
    }

    let mut columns = vec![Foot::None; row.column_count];
    let mut perms = Vec::new();
    permute_row(layout, row, &mut columns, 0, false, 0, &mut perms);
    if perms.is_empty() {
        permute_row(layout, row, &mut columns, 0, true, 0, &mut perms);
    }
    if perms.is_empty() {
        columns.fill(Foot::None);
        perms.push(columns);
    }

    let perms = Rc::from(perms.into_boxed_slice());
    permute_cache.insert(key, Rc::clone(&perms));
    perms
}

#[cfg_attr(feature = "profile", inline(never))]
fn permute_row(
    layout: &StageLayout,
    row: &Row,
    columns: &mut [Foot],
    column: usize,
    ignore_holds: bool,
    used_mask: u8,
    out: &mut Vec<FootPlacement>,
) {
    if column >= columns.len() {
        let mut left_heel = INVALID_COLUMN;
        let mut left_toe = INVALID_COLUMN;
        let mut right_heel = INVALID_COLUMN;
        let mut right_toe = INVALID_COLUMN;

        for (idx, foot) in columns.iter().enumerate() {
            match foot {
                Foot::LeftHeel => left_heel = idx as isize,
                Foot::LeftToe => left_toe = idx as isize,
                Foot::RightHeel => right_heel = idx as isize,
                Foot::RightToe => right_toe = idx as isize,
                Foot::None => {}
            }
        }

        if (left_heel == INVALID_COLUMN && left_toe != INVALID_COLUMN)
            || (right_heel == INVALID_COLUMN && right_toe != INVALID_COLUMN)
        {
            return;
        }

        if left_heel != INVALID_COLUMN && left_toe != INVALID_COLUMN {
            if !layout.bracket_check(left_heel as usize, left_toe as usize) {
                return;
            }
        }

        if right_heel != INVALID_COLUMN && right_toe != INVALID_COLUMN {
            if !layout.bracket_check(right_heel as usize, right_toe as usize) {
                return;
            }
        }

        out.push(columns.to_vec());
        return;
    }

    if row.notes[column].note_type != TapNoteType::Empty
        || (!ignore_holds && row.holds[column].note_type != TapNoteType::Empty)
    {
        for &foot in &FEET {
            let foot_mask = FOOT_MASKS[foot.as_index()];
            if used_mask & foot_mask != 0 {
                continue;
            }
            columns[column] = foot;
            permute_row(
                layout,
                row,
                columns,
                column + 1,
                ignore_holds,
                used_mask | foot_mask,
                out,
            );
            columns[column] = Foot::None;
        }
        return;
    }

    permute_row(layout, row, columns, column + 1, ignore_holds, used_mask, out);
}

fn state_hash_from_parts(
    columns: &[Foot],
    combined_columns: &[Foot],
    moved_feet: &[Foot],
    hold_feet: &[Foot],
) -> u64 {
    let mut value = 0u64;
    let prime = 31u64;
    for &foot in columns {
        value = value.wrapping_mul(prime).wrapping_add(foot as u64);
    }
    for &foot in combined_columns {
        value = value.wrapping_mul(prime).wrapping_add(foot as u64);
    }

    for &foot in moved_feet {
        value = value.wrapping_mul(prime).wrapping_add(foot as u64);
    }

    for &foot in hold_feet {
        value = value.wrapping_mul(prime).wrapping_add(foot as u64);
    }

    value
}

fn get_state_cache_key(state: &State) -> u64 {
    state_hash_from_parts(
        &state.columns,
        &state.combined_columns,
        &state.moved_feet,
        &state.hold_feet,
    )
}

struct CostCalculator<'a> {
    layout: &'a StageLayout,
}

impl<'a> CostCalculator<'a> {
    fn new(layout: &'a StageLayout) -> Self {
        Self { layout }
    }

    #[cfg_attr(feature = "profile", inline(never))]
    fn get_action_cost(
        &self,
        initial: &State,
        result: &State,
        rows: &[Row],
        row_index: usize,
        elapsed: f32,
    ) -> f32 {
        let row = &rows[row_index];
        let column_count = row.column_count;

        let mut left_heel = INVALID_COLUMN;
        let mut left_toe = INVALID_COLUMN;
        let mut right_heel = INVALID_COLUMN;
        let mut right_toe = INVALID_COLUMN;

        for (i, &foot) in result.columns.iter().enumerate() {
            match foot {
                Foot::LeftHeel => left_heel = i as isize,
                Foot::LeftToe => left_toe = i as isize,
                Foot::RightHeel => right_heel = i as isize,
                Foot::RightToe => right_toe = i as isize,
                Foot::None => {}
            }
        }

        let moved_left = result.did_the_foot_move[Foot::LeftHeel.as_index()]
            || result.did_the_foot_move[Foot::LeftToe.as_index()];
        let moved_right = result.did_the_foot_move[Foot::RightHeel.as_index()]
            || result.did_the_foot_move[Foot::RightToe.as_index()];

        let did_jump = ((initial.did_the_foot_move[Foot::LeftHeel.as_index()]
            && !initial.is_the_foot_holding[Foot::LeftHeel.as_index()])
            || (initial.did_the_foot_move[Foot::LeftToe.as_index()]
                && !initial.is_the_foot_holding[Foot::LeftToe.as_index()]))
            && ((initial.did_the_foot_move[Foot::RightHeel.as_index()]
                && !initial.is_the_foot_holding[Foot::RightHeel.as_index()])
                || (initial.did_the_foot_move[Foot::RightToe.as_index()]
                    && !initial.is_the_foot_holding[Foot::RightToe.as_index()]));

        let jacked_left =
            self.did_jack_left(initial, result, left_heel, left_toe, moved_left, did_jump);
        let jacked_right = self.did_jack_right(
            initial,
            result,
            right_heel,
            right_toe,
            moved_right,
            did_jump,
        );

        let mut cost = 0.0;
        cost += self.calc_mine_cost(result, row, column_count);
        cost += self.calc_hold_switch_cost(initial, result, row, column_count);
        cost += self.calc_bracket_tap_cost(
            initial,
            result,
            row,
            left_heel,
            left_toe,
            right_heel,
            right_toe,
            elapsed,
            column_count,
        );
        cost += self.calc_bracket_jack_cost(
            initial,
            result,
            rows,
            row_index,
            moved_left,
            moved_right,
            jacked_left,
            jacked_right,
            did_jump,
            column_count,
        );
        cost += self.calc_doublestep_cost(
            initial,
            result,
            rows,
            row_index,
            moved_left,
            moved_right,
            jacked_left,
            jacked_right,
            did_jump,
            column_count,
        );
        cost += self.calc_slow_bracket_cost(row, moved_left, moved_right, elapsed);
        cost += self.calc_twisted_foot_cost(result);
        cost += self.calc_facing_cost(initial, result, column_count);
        cost += self.calc_spin_cost(initial, result, column_count);
        cost += self.calc_footswitch_cost(initial, result, row, elapsed, column_count);
        cost += self.calc_sideswitch_cost(initial, result);
        cost += self.calc_missed_footswitch_cost(row, jacked_left, jacked_right);
        cost += self.calc_jack_cost(moved_left, moved_right, jacked_left, jacked_right, elapsed);
        cost += self.calc_big_movements_quickly_cost(initial, result, elapsed);

        cost
    }

    #[cfg_attr(feature = "profile", inline(never))]
    fn calc_mine_cost(&self, result: &State, row: &Row, column_count: usize) -> f32 {
        for i in 0..column_count {
            if result.combined_columns[i] != Foot::None && row.mines[i] != 0.0 {
                return MINE_WEIGHT;
            }
        }
        0.0
    }

    #[cfg_attr(feature = "profile", inline(never))]
    fn calc_hold_switch_cost(
        &self,
        initial: &State,
        result: &State,
        row: &Row,
        column_count: usize,
    ) -> f32 {
        let mut cost = 0.0;
        for c in 0..column_count {
            if row.holds[c].note_type == TapNoteType::Empty {
                continue;
            }
            let current_foot = result.combined_columns[c];
            if current_foot == Foot::None {
                continue;
            }

            let is_left = matches!(current_foot, Foot::LeftHeel | Foot::LeftToe);
            let initial_foot = initial.combined_columns[c];
            let initial_is_left = matches!(initial_foot, Foot::LeftHeel | Foot::LeftToe);
            let initial_is_right = matches!(initial_foot, Foot::RightHeel | Foot::RightToe);
            let switch_left = is_left && !initial_is_left;
            let switch_right = !is_left && !initial_is_right;

            if switch_left || switch_right {
                let previous_col = initial.where_the_feet_are[current_foot.as_index()];
                let distance = if previous_col == INVALID_COLUMN {
                    1.0
                } else {
                    (self.layout.get_distance_sq(c, previous_col as usize) as f64)
                        .sqrt() as f32
                };
                cost += (HOLDSWITCH_WEIGHT as f64 * distance as f64) as f32;
            }
        }
        cost
    }

    #[cfg_attr(feature = "profile", inline(never))]
    fn calc_bracket_tap_cost(
        &self,
        initial: &State,
        _result: &State,
        row: &Row,
        left_heel: isize,
        left_toe: isize,
        right_heel: isize,
        right_toe: isize,
        elapsed: f32,
        _column_count: usize,
    ) -> f32 {
        let mut cost = 0.0;
        if left_heel != INVALID_COLUMN && left_toe != INVALID_COLUMN {
            let jack_penalty = if initial.did_the_foot_move[Foot::LeftHeel.as_index()]
                || initial.did_the_foot_move[Foot::LeftToe.as_index()]
            {
                1.0 / elapsed
            } else {
                1.0
            };

            let lh = left_heel as usize;
            let lt = left_toe as usize;
            if row.holds[lh].note_type != TapNoteType::Empty
                && row.holds[lt].note_type == TapNoteType::Empty
            {
                cost += BRACKETTAP_WEIGHT * jack_penalty;
            }
            if row.holds[lt].note_type != TapNoteType::Empty
                && row.holds[lh].note_type == TapNoteType::Empty
            {
                cost += BRACKETTAP_WEIGHT * jack_penalty;
            }
        }

        if right_heel != INVALID_COLUMN && right_toe != INVALID_COLUMN {
            let jack_penalty = if initial.did_the_foot_move[Foot::RightHeel.as_index()]
                || initial.did_the_foot_move[Foot::RightToe.as_index()]
            {
                1.0 / elapsed
            } else {
                1.0
            };

            let rh = right_heel as usize;
            let rt = right_toe as usize;
            if row.holds[rh].note_type != TapNoteType::Empty
                && row.holds[rt].note_type == TapNoteType::Empty
            {
                cost += BRACKETTAP_WEIGHT * jack_penalty;
            }
            if row.holds[rt].note_type != TapNoteType::Empty
                && row.holds[rh].note_type == TapNoteType::Empty
            {
                cost += BRACKETTAP_WEIGHT * jack_penalty;
            }
        }

        cost
    }

    #[cfg_attr(feature = "profile", inline(never))]
    fn calc_bracket_jack_cost(
        &self,
        _initial: &State,
        result: &State,
        _rows: &[Row],
        _row_index: usize,
        moved_left: bool,
        moved_right: bool,
        jacked_left: bool,
        jacked_right: bool,
        did_jump: bool,
        _column_count: usize,
    ) -> f32 {
        let hold_empty = result.hold_feet.iter().all(|&f| f == Foot::None);

        let mut cost = 0.0;
        if moved_left != moved_right && (moved_left || moved_right) && hold_empty && !did_jump {
            if jacked_left
                && result.did_the_foot_move[Foot::LeftHeel.as_index()]
                && result.did_the_foot_move[Foot::LeftToe.as_index()]
            {
                cost += BRACKETJACK_WEIGHT;
            }
            if jacked_right
                && result.did_the_foot_move[Foot::RightHeel.as_index()]
                && result.did_the_foot_move[Foot::RightToe.as_index()]
            {
                cost += BRACKETJACK_WEIGHT;
            }
        }

        cost
    }

    #[cfg_attr(feature = "profile", inline(never))]
    fn calc_doublestep_cost(
        &self,
        initial: &State,
        result: &State,
        rows: &[Row],
        row_index: usize,
        moved_left: bool,
        moved_right: bool,
        jacked_left: bool,
        jacked_right: bool,
        did_jump: bool,
        _column_count: usize,
    ) -> f32 {
        let hold_empty = result.hold_feet.iter().all(|&f| f == Foot::None);

        if moved_left != moved_right && (moved_left || moved_right) && hold_empty && !did_jump {
            if self.did_double_step(
                initial,
                result,
                rows,
                row_index,
                moved_left,
                jacked_left,
                moved_right,
                jacked_right,
            ) {
                return DOUBLESTEP_WEIGHT;
            }
        }
        0.0
    }

    #[cfg_attr(feature = "profile", inline(never))]
    fn calc_slow_bracket_cost(
        &self,
        row: &Row,
        moved_left: bool,
        moved_right: bool,
        elapsed: f32,
    ) -> f32 {
        if elapsed > SLOW_BRACKET_THRESHOLD
            && moved_left != moved_right
            && row
                .notes
                .iter()
                .filter(|note| note.note_type != TapNoteType::Empty)
                .count()
                >= 2
        {
            let time_diff = elapsed - SLOW_BRACKET_THRESHOLD;
            return time_diff * SLOW_BRACKET_WEIGHT;
        }
        0.0
    }

    #[cfg_attr(feature = "profile", inline(never))]
    fn calc_twisted_foot_cost(&self, result: &State) -> f32 {
        let left_heel = result.what_note_the_foot_is_hitting[Foot::LeftHeel.as_index()];
        let left_toe = result.what_note_the_foot_is_hitting[Foot::LeftToe.as_index()];
        let right_heel = result.what_note_the_foot_is_hitting[Foot::RightHeel.as_index()];
        let right_toe = result.what_note_the_foot_is_hitting[Foot::RightToe.as_index()];

        let left_pos = self.layout.average_point(left_heel, left_toe);
        let right_pos = self.layout.average_point(right_heel, right_toe);

        let crossed_over = right_pos.x < left_pos.x;
        let right_backwards = if right_heel != INVALID_COLUMN && right_toe != INVALID_COLUMN {
            self.layout.columns[right_toe as usize].y < self.layout.columns[right_heel as usize].y
        } else {
            false
        };
        let left_backwards = if left_heel != INVALID_COLUMN && left_toe != INVALID_COLUMN {
            self.layout.columns[left_toe as usize].y < self.layout.columns[left_heel as usize].y
        } else {
            false
        };

        if !crossed_over && (right_backwards || left_backwards) {
            TWISTED_FOOT_WEIGHT
        } else {
            0.0
        }
    }

    #[cfg_attr(feature = "profile", inline(never))]
    fn calc_facing_cost(&self, _initial: &State, result: &State, _column_count: usize) -> f32 {
        let end_left_heel = result.where_the_feet_are[Foot::LeftHeel.as_index()];
        let mut end_left_toe = result.where_the_feet_are[Foot::LeftToe.as_index()];
        let end_right_heel = result.where_the_feet_are[Foot::RightHeel.as_index()];
        let mut end_right_toe = result.where_the_feet_are[Foot::RightToe.as_index()];

        if end_left_toe == INVALID_COLUMN {
            end_left_toe = end_left_heel;
        }
        if end_right_toe == INVALID_COLUMN {
            end_right_toe = end_right_heel;
        }

        let heel_facing = if end_left_heel != INVALID_COLUMN && end_right_heel != INVALID_COLUMN {
            self.layout.get_x_difference(end_left_heel, end_right_heel)
        } else {
            0.0
        };
        let toe_facing = if end_left_toe != INVALID_COLUMN && end_right_toe != INVALID_COLUMN {
            self.layout.get_x_difference(end_left_toe, end_right_toe)
        } else {
            0.0
        };
        let left_facing = if end_left_heel != INVALID_COLUMN && end_left_toe != INVALID_COLUMN {
            self.layout.get_y_difference(end_left_heel, end_left_toe)
        } else {
            0.0
        };
        let right_facing = if end_right_heel != INVALID_COLUMN && end_right_toe != INVALID_COLUMN {
            self.layout.get_y_difference(end_right_heel, end_right_toe)
        } else {
            0.0
        };

        let heel_base = -(heel_facing.min(0.0));
        let toe_base = -(toe_facing.min(0.0));
        let left_base = -(left_facing.min(0.0));
        let right_base = -(right_facing.min(0.0));

        let heel_penalty = (heel_base as f64).powf(1.8) as f32 * 100.0;
        let toe_penalty = (toe_base as f64).powf(1.8) as f32 * 100.0;
        let left_penalty = (left_base as f64).powf(1.8) as f32 * 100.0;
        let right_penalty = (right_base as f64).powf(1.8) as f32 * 100.0;

        let mut cost = 0.0;
        if heel_penalty > 0.0 {
            cost += heel_penalty * FACING_WEIGHT;
        }
        if toe_penalty > 0.0 {
            cost += toe_penalty * FACING_WEIGHT;
        }
        if left_penalty > 0.0 {
            cost += left_penalty * FACING_WEIGHT;
        }
        if right_penalty > 0.0 {
            cost += right_penalty * FACING_WEIGHT;
        }

        cost
    }

    #[cfg_attr(feature = "profile", inline(never))]
    fn calc_spin_cost(&self, initial: &State, result: &State, _column_count: usize) -> f32 {
        let end_left_heel = result.where_the_feet_are[Foot::LeftHeel.as_index()];
        let mut end_left_toe = result.where_the_feet_are[Foot::LeftToe.as_index()];
        let end_right_heel = result.where_the_feet_are[Foot::RightHeel.as_index()];
        let mut end_right_toe = result.where_the_feet_are[Foot::RightToe.as_index()];

        if end_left_toe == INVALID_COLUMN {
            end_left_toe = end_left_heel;
        }
        if end_right_toe == INVALID_COLUMN {
            end_right_toe = end_right_heel;
        }

        let previous_left = self.layout.average_point(
            initial.where_the_feet_are[Foot::LeftHeel.as_index()],
            initial.where_the_feet_are[Foot::LeftToe.as_index()],
        );
        let previous_right = self.layout.average_point(
            initial.where_the_feet_are[Foot::RightHeel.as_index()],
            initial.where_the_feet_are[Foot::RightToe.as_index()],
        );
        let left = self.layout.average_point(end_left_heel, end_left_toe);
        let right = self.layout.average_point(end_right_heel, end_right_toe);

        let mut cost = 0.0;
        if right.x < left.x
            && previous_right.x < previous_left.x
            && right.y < left.y
            && previous_right.y > previous_left.y
        {
            cost += SPIN_WEIGHT;
        }
        if right.x < left.x
            && previous_right.x < previous_left.x
            && right.y > left.y
            && previous_right.y < previous_left.y
        {
            cost += SPIN_WEIGHT;
        }
        cost
    }

    #[cfg_attr(feature = "profile", inline(never))]
    fn calc_footswitch_cost(
        &self,
        initial: &State,
        result: &State,
        row: &Row,
        elapsed: f32,
        column_count: usize,
    ) -> f32 {
        if elapsed < SLOW_FOOTSWITCH_THRESHOLD || elapsed >= SLOW_FOOTSWITCH_IGNORE {
            return 0.0;
        }

        if row.mines.iter().all(|mine| (*mine as i32) == 0)
            && row.fake_mines.iter().all(|mine| (*mine as i32) == 0)
        {
            let time_scaled = elapsed - SLOW_FOOTSWITCH_THRESHOLD;
            for i in 0..column_count {
                if initial.combined_columns[i] == Foot::None || result.columns[i] == Foot::None {
                    continue;
                }
                let initial_foot = initial.combined_columns[i];
                let result_foot = result.columns[i];
                if initial_foot != result_foot
                    && initial_foot != OTHER_PART_OF_FOOT[result_foot.as_index()]
                {
                    let divisor = SLOW_FOOTSWITCH_THRESHOLD + time_scaled;
                    if divisor > 0.0 {
                        return (time_scaled / divisor) * FOOTSWITCH_WEIGHT;
                    }
                }
            }
        }
        0.0
    }

    #[cfg_attr(feature = "profile", inline(never))]
    fn calc_sideswitch_cost(&self, initial: &State, result: &State) -> f32 {
        let mut cost = 0.0;
        for &column in &self.layout.side_arrows {
            if initial.combined_columns[column] != result.columns[column]
                && result.columns[column] != Foot::None
                && initial.combined_columns[column] != Foot::None
                && !result.did_the_foot_move[initial.combined_columns[column].as_index()]
            {
                cost += SIDESWITCH_WEIGHT;
            }
        }
        cost
    }

    #[cfg_attr(feature = "profile", inline(never))]
    fn calc_missed_footswitch_cost(&self, row: &Row, jacked_left: bool, jacked_right: bool) -> f32 {
        if (jacked_left || jacked_right)
            && (row.mines.iter().any(|mine| (*mine as i32) != 0)
                || row.fake_mines.iter().any(|mine| (*mine as i32) != 0))
        {
            MISSED_FOOTSWITCH_WEIGHT
        } else {
            0.0
        }
    }

    #[cfg_attr(feature = "profile", inline(never))]
    fn calc_jack_cost(
        &self,
        moved_left: bool,
        moved_right: bool,
        jacked_left: bool,
        jacked_right: bool,
        elapsed: f32,
    ) -> f32 {
        if elapsed < JACK_THRESHOLD && moved_left != moved_right {
            let time_scaled = JACK_THRESHOLD - elapsed;
            if jacked_left || jacked_right {
                if time_scaled > 0.0 {
                    return (1.0 / time_scaled - 1.0 / JACK_THRESHOLD) * JACK_WEIGHT;
                }
            }
        }
        0.0
    }

    #[cfg_attr(feature = "profile", inline(never))]
    fn calc_big_movements_quickly_cost(
        &self,
        initial: &State,
        result: &State,
        elapsed: f32,
    ) -> f32 {
        let mut cost = 0.0;
        for &foot in &result.moved_feet {
            if foot == Foot::None {
                continue;
            }
            let initial_position = initial.where_the_feet_are[foot.as_index()];
            if initial_position == INVALID_COLUMN {
                continue;
            }
            let result_position = result.what_note_the_foot_is_hitting[foot.as_index()];

            let distance_sq = self
                .layout
                .get_distance_sq(initial_position as usize, result_position as usize)
                as f64;
            let mut distance =
                ((distance_sq.sqrt() * DISTANCE_WEIGHT as f64) / elapsed as f64) as f32;

            let other = OTHER_PART_OF_FOOT[foot.as_index()];
            let is_bracketing =
                result.what_note_the_foot_is_hitting[other.as_index()] != INVALID_COLUMN;
            if is_bracketing
                && result.what_note_the_foot_is_hitting[other.as_index()] == initial_position
            {
                continue;
            }
            if is_bracketing {
                distance *= 0.2;
            }
            cost += distance;
        }
        cost
    }

    #[cfg_attr(feature = "profile", inline(never))]
    fn did_double_step(
        &self,
        initial: &State,
        _result: &State,
        rows: &[Row],
        row_index: usize,
        moved_left: bool,
        jacked_left: bool,
        moved_right: bool,
        jacked_right: bool,
    ) -> bool {
        let mut doublestepped = false;
        if moved_left
            && !jacked_left
            && ((initial.did_the_foot_move[Foot::LeftHeel.as_index()]
                && !initial.is_the_foot_holding[Foot::LeftHeel.as_index()])
                || (initial.did_the_foot_move[Foot::LeftToe.as_index()]
                    && !initial.is_the_foot_holding[Foot::LeftToe.as_index()]))
        {
            doublestepped = true;
        }
        if moved_right
            && !jacked_right
            && ((initial.did_the_foot_move[Foot::RightHeel.as_index()]
                && !initial.is_the_foot_holding[Foot::RightHeel.as_index()])
                || (initial.did_the_foot_move[Foot::RightToe.as_index()]
                    && !initial.is_the_foot_holding[Foot::RightToe.as_index()]))
        {
            doublestepped = true;
        }

        if row_index > 0 {
            let last_row = &rows[row_index - 1];
            for hold in &last_row.holds {
                if hold.note_type == TapNoteType::Empty {
                    continue;
                }
                let end_beat = rows[row_index].beat;
                let start_beat = last_row.beat;
                let hold_end = hold.beat + hold.hold_length;
                if hold_end > start_beat && hold_end < end_beat {
                    doublestepped = false;
                }
                if hold_end >= end_beat {
                    doublestepped = false;
                }
            }
        }

        doublestepped
    }

    #[cfg_attr(feature = "profile", inline(never))]
    fn did_jack_left(
        &self,
        initial: &State,
        result: &State,
        left_heel: isize,
        left_toe: isize,
        moved_left: bool,
        did_jump: bool,
    ) -> bool {
        if did_jump || !moved_left {
            return false;
        }

        if left_heel > INVALID_COLUMN
            && initial.combined_columns[left_heel as usize] == Foot::LeftHeel
            && !result.is_the_foot_holding[Foot::LeftHeel.as_index()]
            && ((initial.did_the_foot_move[Foot::LeftHeel.as_index()]
                && !initial.is_the_foot_holding[Foot::LeftHeel.as_index()])
                || (initial.did_the_foot_move[Foot::LeftToe.as_index()]
                    && !initial.is_the_foot_holding[Foot::LeftToe.as_index()]))
        {
            return true;
        }

        if left_toe > INVALID_COLUMN
            && initial.combined_columns[left_toe as usize] == Foot::LeftToe
            && !result.is_the_foot_holding[Foot::LeftToe.as_index()]
            && ((initial.did_the_foot_move[Foot::LeftHeel.as_index()]
                && !initial.is_the_foot_holding[Foot::LeftHeel.as_index()])
                || (initial.did_the_foot_move[Foot::LeftToe.as_index()]
                    && !initial.is_the_foot_holding[Foot::LeftToe.as_index()]))
        {
            return true;
        }

        false
    }

    #[cfg_attr(feature = "profile", inline(never))]
    fn did_jack_right(
        &self,
        initial: &State,
        result: &State,
        right_heel: isize,
        right_toe: isize,
        moved_right: bool,
        did_jump: bool,
    ) -> bool {
        if did_jump || !moved_right {
            return false;
        }

        if right_heel > INVALID_COLUMN
            && initial.combined_columns[right_heel as usize] == Foot::RightHeel
            && !result.is_the_foot_holding[Foot::RightHeel.as_index()]
            && ((initial.did_the_foot_move[Foot::RightHeel.as_index()]
                && !initial.is_the_foot_holding[Foot::RightHeel.as_index()])
                || (initial.did_the_foot_move[Foot::RightToe.as_index()]
                    && !initial.is_the_foot_holding[Foot::RightToe.as_index()]))
        {
            return true;
        }

        if right_toe > INVALID_COLUMN
            && initial.combined_columns[right_toe as usize] == Foot::RightToe
            && !result.is_the_foot_holding[Foot::RightToe.as_index()]
            && ((initial.did_the_foot_move[Foot::RightHeel.as_index()]
                && !initial.is_the_foot_holding[Foot::RightHeel.as_index()])
                || (initial.did_the_foot_move[Foot::RightToe.as_index()]
                    && !initial.is_the_foot_holding[Foot::RightToe.as_index()]))
        {
            return true;
        }

        false
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
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

fn layout_for_lanes(lanes: usize) -> Option<StageLayout> {
    match lanes {
        4 => Some(StageLayout::new_dance_single()),
        8 => Some(StageLayout::new_dance_double()),
        _ => None,
    }
}

fn time_between_beats(start: f32, end: f32, bpm_map: &[(f64, f64)]) -> f64 {
    if end <= start {
        return 0.0;
    }

    let mut bpm = if bpm_map.is_empty() { 60.0 } else { bpm_map[0].1 };
    for &(beat, value) in bpm_map {
        if beat <= start as f64 {
            bpm = value;
        } else {
            break;
        }
    }

    let mut time = 0.0;
    let mut last = start as f64;
    let target = end as f64;
    for &(beat, value) in bpm_map {
        if beat <= last {
            continue;
        }
        if beat >= target {
            break;
        }
        time += (beat - last) * 60.0 / bpm;
        last = beat;
        bpm = value;
    }
    time += (target - last) * 60.0 / bpm;
    time
}

fn calculate_tech_counts_from_rows(
    rows: &[Row],
    layout: &StageLayout,
    _bpm_map: &[(f64, f64)],
) -> TechCounts {
    let mut out = TechCounts::default();
    if rows.len() < 2 {
        return out;
    }

    for i in 1..rows.len() {
        let current_row = &rows[i];
        let previous_row = &rows[i - 1];
        let elapsed_time = current_row.second - previous_row.second;

        if current_row.note_count == 1 && previous_row.note_count == 1 {
            for &foot in &FEET {
                let current_col = current_row.where_the_feet_are[foot.as_index()];
                let previous_col = previous_row.where_the_feet_are[foot.as_index()];
                if current_col == INVALID_COLUMN || previous_col == INVALID_COLUMN {
                    continue;
                }

                if current_col == previous_col {
                    if elapsed_time < JACK_CUTOFF {
                        out.jacks += 1;
                    }
                } else if elapsed_time < DOUBLESTEP_CUTOFF {
                    out.doublesteps += 1;
                }
            }
        }

        if current_row.note_count >= 2 {
            if current_row.where_the_feet_are[Foot::LeftHeel.as_index()] != INVALID_COLUMN
                && current_row.where_the_feet_are[Foot::LeftToe.as_index()] != INVALID_COLUMN
            {
                out.brackets += 1;
            }
            if current_row.where_the_feet_are[Foot::RightHeel.as_index()] != INVALID_COLUMN
                && current_row.where_the_feet_are[Foot::RightToe.as_index()] != INVALID_COLUMN
            {
                out.brackets += 1;
            }
        }

        for &c in &layout.up_arrows {
            if is_footswitch(c, current_row, previous_row, elapsed_time) {
                out.up_footswitches += 1;
                out.footswitches += 1;
            }
        }
        for &c in &layout.down_arrows {
            if is_footswitch(c, current_row, previous_row, elapsed_time) {
                out.down_footswitches += 1;
                out.footswitches += 1;
            }
        }
        for &c in &layout.side_arrows {
            if is_footswitch(c, current_row, previous_row, elapsed_time) {
                out.sideswitches += 1;
            }
        }

        let left_heel = current_row.where_the_feet_are[Foot::LeftHeel.as_index()];
        let left_toe = current_row.where_the_feet_are[Foot::LeftToe.as_index()];
        let right_heel = current_row.where_the_feet_are[Foot::RightHeel.as_index()];
        let right_toe = current_row.where_the_feet_are[Foot::RightToe.as_index()];

        let prev_left_heel = previous_row.where_the_feet_are[Foot::LeftHeel.as_index()];
        let prev_left_toe = previous_row.where_the_feet_are[Foot::LeftToe.as_index()];
        let prev_right_heel = previous_row.where_the_feet_are[Foot::RightHeel.as_index()];
        let prev_right_toe = previous_row.where_the_feet_are[Foot::RightToe.as_index()];

        if right_heel != INVALID_COLUMN
            && prev_left_heel != INVALID_COLUMN
            && prev_right_heel == INVALID_COLUMN
        {
            let left_pos = layout.average_point(prev_left_heel, prev_left_toe);
            let right_pos = layout.average_point(right_heel, right_toe);
            if right_pos.x < left_pos.x {
                if i > 1 {
                    let prev_prev_row = &rows[i - 2];
                    let prev_prev_right_heel =
                        prev_prev_row.where_the_feet_are[Foot::RightHeel.as_index()];
                    if prev_prev_right_heel != INVALID_COLUMN && prev_prev_right_heel != right_heel
                    {
                        let prev_prev_right_pos = layout.columns[prev_prev_right_heel as usize];
                        if prev_prev_right_pos.x > left_pos.x {
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
        } else if left_heel != INVALID_COLUMN
            && prev_right_heel != INVALID_COLUMN
            && prev_left_heel == INVALID_COLUMN
        {
            let left_pos = layout.average_point(left_heel, left_toe);
            let right_pos = layout.average_point(prev_right_heel, prev_right_toe);
            if right_pos.x < left_pos.x {
                if i > 1 {
                    let prev_prev_row = &rows[i - 2];
                    let prev_prev_left_heel =
                        prev_prev_row.where_the_feet_are[Foot::LeftHeel.as_index()];
                    if prev_prev_left_heel != INVALID_COLUMN && prev_prev_left_heel != left_heel {
                        let prev_prev_left_pos = layout.columns[prev_prev_left_heel as usize];
                        if right_pos.x > prev_prev_left_pos.x {
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

fn calculate_tech_counts_from_rows_with_timing(
    rows: &[Row],
    layout: &StageLayout,
    _timing: &TimingData,
) -> TechCounts {
    let mut out = TechCounts::default();
    if rows.len() < 2 {
        return out;
    }

    for i in 1..rows.len() {
        let current_row = &rows[i];
        let previous_row = &rows[i - 1];
        let elapsed_time = current_row.second - previous_row.second;

        if current_row.note_count == 1 && previous_row.note_count == 1 {
            for &foot in &FEET {
                let current_col = current_row.where_the_feet_are[foot.as_index()];
                let previous_col = previous_row.where_the_feet_are[foot.as_index()];
                if current_col == INVALID_COLUMN || previous_col == INVALID_COLUMN {
                    continue;
                }

                if current_col == previous_col {
                    if elapsed_time < JACK_CUTOFF {
                        out.jacks += 1;
                    }
                } else if elapsed_time < DOUBLESTEP_CUTOFF {
                    out.doublesteps += 1;
                }
            }
        }

        if current_row.note_count >= 2 {
            if current_row.where_the_feet_are[Foot::LeftHeel.as_index()] != INVALID_COLUMN
                && current_row.where_the_feet_are[Foot::LeftToe.as_index()] != INVALID_COLUMN
            {
                out.brackets += 1;
            }
            if current_row.where_the_feet_are[Foot::RightHeel.as_index()] != INVALID_COLUMN
                && current_row.where_the_feet_are[Foot::RightToe.as_index()] != INVALID_COLUMN
            {
                out.brackets += 1;
            }
        }

        for &c in &layout.up_arrows {
            if is_footswitch(c, current_row, previous_row, elapsed_time) {
                out.up_footswitches += 1;
                out.footswitches += 1;
            }
        }
        for &c in &layout.down_arrows {
            if is_footswitch(c, current_row, previous_row, elapsed_time) {
                out.down_footswitches += 1;
                out.footswitches += 1;
            }
        }
        for &c in &layout.side_arrows {
            if is_footswitch(c, current_row, previous_row, elapsed_time) {
                out.sideswitches += 1;
            }
        }

        let left_heel = current_row.where_the_feet_are[Foot::LeftHeel.as_index()];
        let left_toe = current_row.where_the_feet_are[Foot::LeftToe.as_index()];
        let right_heel = current_row.where_the_feet_are[Foot::RightHeel.as_index()];
        let right_toe = current_row.where_the_feet_are[Foot::RightToe.as_index()];

        let prev_left_heel = previous_row.where_the_feet_are[Foot::LeftHeel.as_index()];
        let prev_left_toe = previous_row.where_the_feet_are[Foot::LeftToe.as_index()];
        let prev_right_heel = previous_row.where_the_feet_are[Foot::RightHeel.as_index()];
        let prev_right_toe = previous_row.where_the_feet_are[Foot::RightToe.as_index()];

        if right_heel != INVALID_COLUMN
            && prev_left_heel != INVALID_COLUMN
            && prev_right_heel == INVALID_COLUMN
        {
            let left_pos = layout.average_point(prev_left_heel, prev_left_toe);
            let right_pos = layout.average_point(right_heel, right_toe);
            if right_pos.x < left_pos.x {
                if i > 1 {
                    let prev_prev_row = &rows[i - 2];
                    let prev_prev_right_heel =
                        prev_prev_row.where_the_feet_are[Foot::RightHeel.as_index()];
                    if prev_prev_right_heel != INVALID_COLUMN && prev_prev_right_heel != right_heel
                    {
                        let prev_prev_right_pos = layout.columns[prev_prev_right_heel as usize];
                        if prev_prev_right_pos.x > left_pos.x {
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
        } else if left_heel != INVALID_COLUMN
            && prev_right_heel != INVALID_COLUMN
            && prev_left_heel == INVALID_COLUMN
        {
            let left_pos = layout.average_point(left_heel, left_toe);
            let right_pos = layout.average_point(prev_right_heel, prev_right_toe);
            if right_pos.x < left_pos.x {
                if i > 1 {
                    let prev_prev_row = &rows[i - 2];
                    let prev_prev_left_heel =
                        prev_prev_row.where_the_feet_are[Foot::LeftHeel.as_index()];
                    if prev_prev_left_heel != INVALID_COLUMN && prev_prev_left_heel != left_heel {
                        let prev_prev_left_pos = layout.columns[prev_prev_left_heel as usize];
                        if right_pos.x > prev_prev_left_pos.x {
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

fn is_footswitch(column: usize, current_row: &Row, previous_row: &Row, elapsed_time: f32) -> bool {
    let prev = previous_row.columns[column];
    let curr = current_row.columns[column];
    if prev == Foot::None || curr == Foot::None {
        return false;
    }

    prev != curr && OTHER_PART_OF_FOOT[prev.as_index()] != curr && elapsed_time < FOOTSWITCH_CUTOFF
}

const JACK_CUTOFF: f32 = 0.176;
const FOOTSWITCH_CUTOFF: f32 = 0.3;
const DOUBLESTEP_CUTOFF: f32 = 0.235;
pub fn analyze(minimized_note_data: &[u8], bpm_map: &[(f64, f64)], offset: f64) -> TechCounts {
    analyze_lanes(minimized_note_data, bpm_map, offset, 4)
}

pub fn analyze_lanes(
    minimized_note_data: &[u8],
    bpm_map: &[(f64, f64)],
    offset: f64,
    lanes: usize,
) -> TechCounts {
    let Some(layout) = layout_for_lanes(lanes) else {
        return TechCounts::default();
    };
    let parsed_rows = parse_chart_rows(minimized_note_data, bpm_map, offset, layout.column_count());
    let note_data = build_intermediate_notes(&parsed_rows);
    let mut generator = StepParityGenerator::new(layout.clone());
    if !generator.analyze_note_data(note_data, layout.column_count()) {
        return TechCounts::default();
    }
    calculate_tech_counts_from_rows(&generator.rows, &generator.layout, bpm_map)
}

pub fn analyze_with_timing(minimized_note_data: &[u8], timing: &TimingData) -> TechCounts {
    analyze_timing_lanes(minimized_note_data, timing, 4)
}

pub fn analyze_timing_lanes(
    minimized_note_data: &[u8],
    timing: &TimingData,
    lanes: usize,
) -> TechCounts {
    let Some(layout) = layout_for_lanes(lanes) else {
        return TechCounts::default();
    };
    let parsed_rows =
        parse_chart_rows_with_timing(minimized_note_data, timing, layout.column_count());
    let note_data = build_intermediate_notes_with_timing(&parsed_rows, timing);
    let mut generator = StepParityGenerator::new(layout.clone());
    if !generator.analyze_note_data(note_data, layout.column_count()) {
        return TechCounts::default();
    }
    calculate_tech_counts_from_rows_with_timing(&generator.rows, &generator.layout, timing)
}

fn beat_to_time(beat: f64, bpm_map: &[(f64, f64)], offset: f64) -> f64 {
    time_between_beats(0.0, beat as f32, bpm_map) - offset
}

#[derive(Clone)]
struct ParsedRow {
    chars: [u8; 8],
    columns: u8,
    row: i32,
    beat: f32,
    second: f32,
}

struct StepParityEnvFlags {
    dump_ties: bool,
    dump_path: bool,
    dump_state_collisions: bool,
    dump_rows: bool,
    dump_notes: bool,
    dump_stats: bool,
}

fn env_flags() -> &'static StepParityEnvFlags {
    static FLAGS: OnceLock<StepParityEnvFlags> = OnceLock::new();
    FLAGS.get_or_init(|| StepParityEnvFlags {
        dump_ties: env_flag("RSSP_STEP_PARITY_DUMP_TIES"),
        dump_path: env_flag("RSSP_STEP_PARITY_DUMP_PATH"),
        dump_state_collisions: env_flag("RSSP_STEP_PARITY_DUMP_STATE_COLLISIONS"),
        dump_rows: env_flag("RSSP_STEP_PARITY_DUMP_ROWS"),
        dump_notes: env_flag("RSSP_STEP_PARITY_DUMP_NOTES"),
        dump_stats: env_flag("RSSP_STEP_PARITY_DUMP_STATS"),
    })
}

fn env_flag(name: &str) -> bool {
    match std::env::var(name) {
        Ok(value) => {
            let trimmed = value.trim();
            !trimmed.is_empty() && trimmed != "0"
        }
        Err(_) => false,
    }
}

fn hash_bytes(bytes: &[u8]) -> u64 {
    let mut hasher = IdentityHasher::default();
    hasher.write(bytes);
    hasher.finish()
}

fn hash_rows(rows: &[ParsedRow]) -> u64 {
    let mut hasher = IdentityHasher::default();
    for row in rows {
        let cols = row.columns as usize;
        hasher.write(&row.chars[..cols]);
        hasher.write(&row.row.to_le_bytes());
        hasher.write(&row.beat.to_bits().to_le_bytes());
        hasher.write(&row.second.to_bits().to_le_bytes());
    }
    hasher.finish()
}

#[inline(always)]
fn trim_ascii_whitespace(mut line: &[u8]) -> &[u8] {
    while let Some((&first, rest)) = line.split_first() {
        if first.is_ascii_whitespace() {
            line = rest;
        } else {
            break;
        }
    }
    while let Some((&last, rest)) = line.split_last() {
        if last.is_ascii_whitespace() {
            line = rest;
        } else {
            break;
        }
    }
    line
}

#[inline(always)]
fn row_has_obj(line: &[u8]) -> bool {
    for &b in line {
        if b != b'0' {
            return true;
        }
    }
    false
}

#[inline(always)]
fn count_measure_rows(measure: &[u8]) -> usize {
    measure
        .split(|&b| b == b'\n')
        .filter(|line| !trim_ascii_whitespace(*line).is_empty())
        .count()
}

fn parse_chart_rows(
    note_data: &[u8],
    bpm_map: &[(f64, f64)],
    offset: f64,
    column_count: usize,
) -> Vec<ParsedRow> {
    let mut rows = Vec::new();
    let mut measure_index = 0usize;
    if column_count == 0 || column_count > 8 {
        return rows;
    }

    for measure in note_data.split(|&b| b == b',') {
        let num_rows = count_measure_rows(measure);
        if num_rows == 0 {
            measure_index += 1;
            continue;
        }

        rows.reserve(num_rows);
        let measure_start = measure_index as f32 * 4.0;
        let row_step = 4.0 / num_rows as f32;
        let mut row_in_measure = 0usize;
        for line in measure.split(|&b| b == b'\n') {
            let trimmed = trim_ascii_whitespace(line);
            if trimmed.is_empty() {
                continue;
            }
            let copy_len = trimmed.len().min(column_count);
            if row_has_obj(&trimmed[..copy_len]) {
                let beat = measure_start + row_in_measure as f32 * row_step;
                let note_row = beat_to_note_row_f32_exact(beat);
                let beat = note_row as f32 / ROWS_PER_BEAT as f32;
                let second = beat_to_time(beat as f64, bpm_map, offset);
                let mut chars = [b'0'; 8];
                chars[..copy_len].copy_from_slice(&trimmed[..copy_len]);
                rows.push(ParsedRow {
                    chars,
                    columns: column_count as u8,
                    row: note_row,
                    beat,
                    second: second as f32,
                });
            }
            row_in_measure += 1;
        }

        measure_index += 1;
    }

    rows
}

fn parse_chart_rows_with_timing(
    note_data: &[u8],
    timing: &TimingData,
    column_count: usize,
) -> Vec<ParsedRow> {
    let mut rows = Vec::new();
    let mut measure_index = 0usize;
    let dump_rows = env_flags().dump_rows;
    if column_count == 0 || column_count > 8 {
        return rows;
    }

    if dump_rows {
        let hash = hash_bytes(note_data);
        eprintln!(
            "STEP_PARITY_ROWS start hash={:016x} columns={}",
            hash, column_count
        );
    }

    for measure in note_data.split(|&b| b == b',') {
        let num_rows = count_measure_rows(measure);
        if num_rows == 0 {
            measure_index += 1;
            continue;
        }

        rows.reserve(num_rows);
        let measure_start = measure_index as f32 * 4.0;
        let row_step = 4.0 / num_rows as f32;
        let mut row_in_measure = 0usize;
        for line in measure.split(|&b| b == b'\n') {
            let trimmed = trim_ascii_whitespace(line);
            if trimmed.is_empty() {
                continue;
            }
            let copy_len = trimmed.len().min(column_count);
            if row_has_obj(&trimmed[..copy_len]) {
                let beat = measure_start + row_in_measure as f32 * row_step;
                let note_row = beat_to_note_row_f32_exact(beat);
                let beat = note_row as f32 / ROWS_PER_BEAT as f32;
                let second = timing.get_time_for_beat_f32(beat as f64);
                let mut chars = [b'0'; 8];
                chars[..copy_len].copy_from_slice(&trimmed[..copy_len]);
                rows.push(ParsedRow {
                    chars,
                    columns: column_count as u8,
                    row: note_row,
                    beat,
                    second: second as f32,
                });
                let row_index = rows.len() - 1;
                if dump_rows {
                    let cols = rows[row_index].columns as usize;
                    let row_text = String::from_utf8_lossy(&rows[row_index].chars[..cols]);
                    eprintln!(
                        "STEP_PARITY_ROW idx={} measure={} line={}/{} row={} beat={:.6} second={:.6} data={}",
                        row_index,
                        measure_index,
                        row_in_measure,
                        num_rows,
                        note_row,
                        beat,
                        second as f32,
                        row_text
                    );
                }
            }
            row_in_measure += 1;
        }

        measure_index += 1;
    }

    if dump_rows {
        let rows_hash = hash_rows(&rows);
        eprintln!(
            "STEP_PARITY_ROWS end total={} rows_hash={:016x}",
            rows.len(),
            rows_hash
        );
    }

    rows
}

#[inline(always)]
fn is_hold_blocker(ch: u8) -> bool {
    matches!(ch, b'1' | b'M' | b'L' | b'F')
}

#[inline(always)]
fn hold_lengths_for_rows(rows: &[ParsedRow], column_count: usize) -> Vec<f32> {
    if rows.is_empty() || column_count == 0 {
        return Vec::new();
    }

    let mut hold_starts = vec![None; column_count];
    let mut lengths = vec![MISSING_HOLD_LENGTH_BEATS; rows.len() * column_count];

    for (row_idx, row) in rows.iter().enumerate() {
        for col in 0..column_count {
            match row.chars[col] {
                ch if is_hold_blocker(ch) => {
                    hold_starts[col] = None;
                }
                b'2' | b'4' => {
                    hold_starts[col] = Some((row_idx, row.row));
                }
                b'3' => {
                    if let Some((start_idx, start_row)) = hold_starts[col] {
                        let length_rows = row.row - start_row;
                        let length = length_rows as f32 / ROWS_PER_BEAT as f32;
                        lengths[start_idx * column_count + col] = length;
                        hold_starts[col] = None;
                    }
                }
                _ => {}
            }
        }
    }

    lengths
}

fn build_intermediate_notes(rows: &[ParsedRow]) -> Vec<IntermediateNoteData> {
    let column_count = rows.first().map(|row| row.columns as usize).unwrap_or(0);
    if column_count == 0 {
        return Vec::new();
    }
    let hold_lengths = hold_lengths_for_rows(rows, column_count);

    let mut notes = Vec::new();
    for (row_idx, row) in rows.iter().enumerate() {
        for col in 0..column_count {
            let ch = row.chars[col];
            let note_type = match ch {
                b'0' => TapNoteType::Empty,
                b'1' => TapNoteType::Tap,
                b'2' | b'4' => TapNoteType::HoldHead,
                b'3' => TapNoteType::HoldTail,
                b'M' => TapNoteType::Mine,
                b'K' | b'L' => TapNoteType::Tap,
                b'F' => TapNoteType::Fake,
                _ => TapNoteType::Empty,
            };

            if matches!(note_type, TapNoteType::Empty | TapNoteType::HoldTail) {
                continue;
            }

            let mut note = IntermediateNoteData::default();
            note.note_type = note_type;
            note.col = col;
            note.row = row.row as usize;
            note.beat = row.beat;
            note.second = row.second;
            note.fake = note_type == TapNoteType::Fake;
            note.subtype = match ch {
                b'4' => TapNoteSubType::Roll,
                b'2' => TapNoteSubType::Hold,
                _ => TapNoteSubType::Invalid,
            };

            if note_type == TapNoteType::HoldHead {
                let hold_length = hold_lengths[row_idx * column_count + col];
                if hold_length >= MISSING_HOLD_LENGTH_BEATS {
                    continue;
                }
                note.hold_length = hold_length;
            }

            notes.push(note);
        }
    }
    notes
}

fn build_intermediate_notes_with_timing(
    rows: &[ParsedRow],
    timing: &TimingData,
) -> Vec<IntermediateNoteData> {
    let column_count = rows.first().map(|row| row.columns as usize).unwrap_or(0);
    if column_count == 0 {
        return Vec::new();
    }
    let dump_notes = env_flags().dump_notes;
    let hold_lengths = hold_lengths_for_rows(rows, column_count);

    let mut notes = Vec::new();
    if dump_notes {
        let rows_hash = hash_rows(rows);
        eprintln!(
            "STEP_PARITY_NOTES start rows={} columns={} rows_hash={:016x}",
            rows.len(),
            column_count,
            rows_hash
        );
    }
    for (row_idx, row) in rows.iter().enumerate() {
        let row_fake = timing.is_fake_at_beat(row.row as f64);
        for col in 0..column_count {
            let ch = row.chars[col];
            let note_type = match ch {
                b'0' => TapNoteType::Empty,
                b'1' => TapNoteType::Tap,
                b'2' | b'4' => TapNoteType::HoldHead,
                b'3' => TapNoteType::HoldTail,
                b'M' => TapNoteType::Mine,
                b'K' | b'L' => TapNoteType::Tap,
                b'F' => TapNoteType::Fake,
                _ => TapNoteType::Empty,
            };

            if matches!(note_type, TapNoteType::Empty | TapNoteType::HoldTail) {
                continue;
            }

            let mut note = IntermediateNoteData::default();
            note.note_type = note_type;
            note.col = col;
            note.row = row.row as usize;
            note.beat = row.beat;
            note.second = row.second;
            note.fake = note_type == TapNoteType::Fake || row_fake;
            note.subtype = match ch {
                b'4' => TapNoteSubType::Roll,
                b'2' => TapNoteSubType::Hold,
                _ => TapNoteSubType::Invalid,
            };

            if note_type == TapNoteType::HoldHead {
                let hold_length = hold_lengths[row_idx * column_count + col];
                if hold_length >= MISSING_HOLD_LENGTH_BEATS {
                    continue;
                }
                note.hold_length = hold_length;
            }

            if dump_notes {
                eprintln!(
                    "STEP_PARITY_NOTE row_idx={} row={} beat={:.6} second={:.6} col={} ch={} type={:?} subtype={:?} fake={} hold_len={:.6}",
                    row_idx,
                    row.row,
                    row.beat,
                    row.second,
                    col,
                    ch as char,
                    note.note_type,
                    note.subtype,
                    note.fake,
                    note.hold_length
                );
            }

            notes.push(note);
        }
    }
    if dump_notes {
        eprintln!("STEP_PARITY_NOTES end total={}", notes.len());
    }
    notes
}

use std::collections::{HashMap, HashSet, VecDeque};
use std::hash::{BuildHasherDefault, Hasher};
use std::rc::Rc;

use crate::timing::{beat_to_note_row_f32_exact, TimingData, ROWS_PER_BEAT};

const INVALID_COLUMN: isize = -1;
const CLM_SECOND_INVALID: f32 = -1.0;
const MAX_NOTE_ROW: i32 = 1 << 30;
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
const FEET: [Foot; 4] = [
    Foot::LeftHeel,
    Foot::LeftToe,
    Foot::RightHeel,
    Foot::RightToe,
];
const OTHER_PART_OF_FOOT: [Foot; NUM_FEET] = [
    Foot::None,
    Foot::LeftToe,
    Foot::LeftHeel,
    Foot::RightToe,
    Foot::RightHeel,
];

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

    fn finish(&self) -> u64 {
        self.0
    }
}

type NeighborMap = HashMap<usize, f32, BuildHasherDefault<IdentityHasher>>;

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
}

impl StageLayout {
    fn new_dance_single() -> Self {
        Self {
            columns: vec![
                StagePoint { x: 0.0, y: 1.0 },
                StagePoint { x: 1.0, y: 0.0 },
                StagePoint { x: 1.0, y: 2.0 },
                StagePoint { x: 2.0, y: 1.0 },
            ],
            up_arrows: vec![2],
            down_arrows: vec![1],
            side_arrows: vec![0, 3],
        }
    }

    fn new_dance_double() -> Self {
        Self {
            columns: vec![
                StagePoint { x: 0.0, y: 1.0 },
                StagePoint { x: 1.0, y: 0.0 },
                StagePoint { x: 1.0, y: 2.0 },
                StagePoint { x: 2.0, y: 1.0 },
                StagePoint { x: 3.0, y: 1.0 },
                StagePoint { x: 4.0, y: 0.0 },
                StagePoint { x: 4.0, y: 2.0 },
                StagePoint { x: 5.0, y: 1.0 },
            ],
            up_arrows: vec![2, 6],
            down_arrows: vec![1, 5],
            side_arrows: vec![0, 3, 4, 7],
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
        if left_index == right_index {
            return 0.0;
        }
        if left_index == INVALID_COLUMN || right_index == INVALID_COLUMN {
            return 0.0;
        }

        let left = self.columns[left_index as usize];
        let right = self.columns[right_index as usize];

        let mut dx = right.x - left.x;
        let dy = right.y - left.y;
        let distance = (dx * dx + dy * dy).sqrt();
        if distance == 0.0 {
            return 0.0;
        }

        dx /= distance;
        let negative = dx <= 0.0;
        let mut magnitude = dx.abs().powf(4.0);
        if negative {
            magnitude = -magnitude;
        }

        magnitude
    }

    fn get_y_difference(&self, left_index: isize, right_index: isize) -> f32 {
        if left_index == right_index {
            return 0.0;
        }
        if left_index == INVALID_COLUMN || right_index == INVALID_COLUMN {
            return 0.0;
        }

        let left = self.columns[left_index as usize];
        let right = self.columns[right_index as usize];

        let mut dy = right.y - left.y;
        let dx = right.x - left.x;
        let distance = (dx * dx + dy * dy).sqrt();
        if distance == 0.0 {
            return 0.0;
        }

        dy /= distance;
        let negative = dy <= 0.0;
        let mut magnitude = dy.abs().powf(4.0);
        if negative {
            magnitude = -magnitude;
        }

        magnitude
    }

    fn average_point(&self, left_index: isize, right_index: isize) -> StagePoint {
        match (left_index, right_index) {
            (INVALID_COLUMN, INVALID_COLUMN) => StagePoint { x: 0.0, y: 0.0 },
            (INVALID_COLUMN, r) => self.columns[r as usize],
            (l, INVALID_COLUMN) => self.columns[l as usize],
            (l, r) => StagePoint {
                x: (self.columns[l as usize].x + self.columns[r as usize].x) / 2.0,
                y: (self.columns[l as usize].y + self.columns[r as usize].y) / 2.0,
            },
        }
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
    hold_tails: HashSet<usize>,
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
            hold_tails: HashSet::new(),
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
    permute_cache: HashMap<u32, Vec<FootPlacement>>,
    state_cache: HashMap<u64, Rc<State>>,
    nodes: Vec<StepParityNode>,
    rows: Vec<Row>,
}

impl StepParityGenerator {
    fn new(layout: StageLayout) -> Self {
        Self {
            column_count: layout.column_count(),
            layout,
            permute_cache: HashMap::new(),
            state_cache: HashMap::new(),
            nodes: Vec::new(),
            rows: Vec::new(),
        }
    }

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
                counter.notes = vec![IntermediateNoteData::default(); column_count];
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

    fn add_row(&mut self, counter: &mut RowCounter) {
        if counter.last_column_second == CLM_SECOND_INVALID {
            return;
        }
        let mut row = self.create_row(counter);
        row.row_index = self.rows.len();
        self.rows.push(row);
    }

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

            if counter.active_holds[c].note_type != TapNoteType::Empty {
                let end_beat = counter.active_holds[c].beat + counter.active_holds[c].hold_length;
                if (end_beat - counter.last_column_beat).abs() < 0.0005 {
                    row.hold_tails.insert(c);
                }
            }
        }

        row
    }

    fn build_state_graph(&mut self) {
        self.nodes.clear();
        self.state_cache.clear();

        let start_state = Rc::new(State::new(self.column_count));
        let start_second = self.rows.first().map(|r| r.second - 1.0).unwrap_or(-1.0);
        let start_id = self.add_node(start_state, start_second, -1);

        let mut prev_node_ids = vec![start_id];
        let layout = self.layout.clone();
        let cost_calculator = CostCalculator::new(&layout);

        for i in 0..self.rows.len() {
            let row_clone = self.rows[i].clone();
            let permutations = self.get_foot_placement_permutations(&row_clone).to_vec();
            let mut result_nodes_for_row: Vec<usize> = Vec::new();

            for &initial_node_id in &prev_node_ids {
                let initial_state = Rc::clone(&self.nodes[initial_node_id].state);
                let elapsed = row_clone.second - self.nodes[initial_node_id].second;

                for perm in &permutations {
                    let result_state = self.init_result_state(&initial_state, &row_clone, perm);
                    let cost = cost_calculator.get_action_cost(
                        &initial_state,
                        &result_state,
                        &self.rows,
                        i,
                        elapsed,
                    );

                    let result_node_id = if let Some(&id) = result_nodes_for_row
                        .iter()
                        .find(|&&id| Rc::ptr_eq(&self.nodes[id].state, &result_state))
                    {
                        id
                    } else {
                        let id = self.add_node(
                            Rc::clone(&result_state),
                            row_clone.second,
                            row_clone.row_index as isize,
                        );
                        result_nodes_for_row.push(id);
                        id
                    };

                    self.add_edge(initial_node_id, result_node_id, cost);
                }
            }

            prev_node_ids = result_nodes_for_row;
        }

        let end_state = Rc::new(State::new(self.column_count));
        let end_second = self.rows.last().map(|r| r.second + 1.0).unwrap_or(1.0);
        let end_id = self.add_node(end_state, end_second, self.rows.len() as isize);

        for node_id in prev_node_ids {
            self.add_edge(node_id, end_id, 0.0);
        }
    }

    fn init_result_state(
        &mut self,
        initial_state: &State,
        row: &Row,
        columns: &[Foot],
    ) -> Rc<State> {
        let mut result_state = State::new(self.column_count);

        for foot_idx in 0..NUM_FEET {
            result_state.where_the_feet_are[foot_idx] = INVALID_COLUMN;
            result_state.what_note_the_foot_is_hitting[foot_idx] = INVALID_COLUMN;
            result_state.did_the_foot_move[foot_idx] = false;
            result_state.is_the_foot_holding[foot_idx] = false;
        }

        for i in 0..self.column_count {
            result_state.columns[i] = columns[i];
            result_state.combined_columns[i] = Foot::None;
        }

        for (i, &foot) in columns.iter().enumerate() {
            if foot == Foot::None {
                continue;
            }
            let foot_index = foot.as_index();
            result_state.what_note_the_foot_is_hitting[foot_index] = i as isize;

            if row.holds[i].note_type == TapNoteType::Empty {
                result_state.moved_feet[i] = foot;
                result_state.did_the_foot_move[foot_index] = true;
                continue;
            }
            if initial_state.combined_columns[i] != foot {
                result_state.moved_feet[i] = foot;
                result_state.did_the_foot_move[foot_index] = true;
            }
        }

        for (i, &foot) in columns.iter().enumerate() {
            if foot == Foot::None {
                continue;
            }
            if row.holds[i].note_type != TapNoteType::Empty {
                result_state.hold_feet[i] = foot;
                result_state.is_the_foot_holding[foot.as_index()] = true;
            }
        }

        self.merge_initial_and_result_position(initial_state, &mut result_state);

        for (col, &foot) in result_state.combined_columns.iter().enumerate() {
            if foot != Foot::None {
                result_state.where_the_feet_are[foot.as_index()] = col as isize;
            }
        }

        let hash = get_state_cache_key(&result_state);
        if let Some(existing) = self.state_cache.get(&hash) {
            return Rc::clone(existing);
        }

        let rc = Rc::new(result_state);
        self.state_cache.insert(hash, Rc::clone(&rc));
        rc
    }

    fn merge_initial_and_result_position(&self, initial: &State, result: &mut State) {
        for i in 0..self.column_count {
            if result.columns[i] != Foot::None {
                result.combined_columns[i] = result.columns[i];
                continue;
            }

            match initial.combined_columns[i] {
                Foot::LeftHeel | Foot::RightHeel => {
                    let prev = initial.combined_columns[i];
                    if prev != Foot::None && !result.did_the_foot_move[prev.as_index()] {
                        result.combined_columns[i] = prev;
                    }
                }
                Foot::LeftToe => {
                    if !result.did_the_foot_move[Foot::LeftToe.as_index()]
                        && !result.did_the_foot_move[Foot::LeftHeel.as_index()]
                    {
                        result.combined_columns[i] = Foot::LeftToe;
                    }
                }
                Foot::RightToe => {
                    if !result.did_the_foot_move[Foot::RightToe.as_index()]
                        && !result.did_the_foot_move[Foot::RightHeel.as_index()]
                    {
                        result.combined_columns[i] = Foot::RightToe;
                    }
                }
                Foot::None => {}
            }
        }
    }

    fn get_foot_placement_permutations(&mut self, row: &Row) -> &Vec<FootPlacement> {
        let mut key = 0u32;
        for i in 0..row.column_count.min(32) {
            if row.notes[i].note_type != TapNoteType::Empty
                || row.holds[i].note_type != TapNoteType::Empty
            {
                key |= 1 << i;
            }
        }

        if !self.permute_cache.contains_key(&key) {
            let blank = vec![Foot::None; row.column_count];
            let mut perms = self.permute_recursive(row, blank.clone(), 0, false);
            if perms.is_empty() {
                perms = self.permute_recursive(row, blank.clone(), 0, true);
            }
            if perms.is_empty() {
                perms.push(blank);
            }
            self.permute_cache.insert(key, perms);
        }

        self.permute_cache.get(&key).unwrap()
    }

    fn permute_recursive(
        &self,
        row: &Row,
        mut columns: FootPlacement,
        column: usize,
        ignore_holds: bool,
    ) -> Vec<FootPlacement> {
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
                return Vec::new();
            }

            if left_heel != INVALID_COLUMN && left_toe != INVALID_COLUMN {
                if !self
                    .layout
                    .bracket_check(left_heel as usize, left_toe as usize)
                {
                    return Vec::new();
                }
            }

            if right_heel != INVALID_COLUMN && right_toe != INVALID_COLUMN {
                if !self
                    .layout
                    .bracket_check(right_heel as usize, right_toe as usize)
                {
                    return Vec::new();
                }
            }

            return vec![columns];
        }

        let mut permutations = Vec::new();
        if row.notes[column].note_type != TapNoteType::Empty
            || (!ignore_holds && row.holds[column].note_type != TapNoteType::Empty)
        {
            for &foot in &FEET {
                if columns.contains(&foot) {
                    continue;
                }
                columns[column] = foot;
                permutations.extend(self.permute_recursive(
                    row,
                    columns.clone(),
                    column + 1,
                    ignore_holds,
                ));
                columns[column] = Foot::None;
            }
            return permutations;
        }

        self.permute_recursive(row, columns, column + 1, ignore_holds)
    }

    fn compute_cheapest_path(&self) -> Vec<usize> {
        if self.nodes.is_empty() {
            return Vec::new();
        }

        let start_id = 0;
        let end_id = self.nodes.len() - 1;
        let mut cost = vec![f32::MAX; self.nodes.len()];
        let mut predecessor = vec![usize::MAX; self.nodes.len()];
        cost[start_id] = 0.0;

        for i in start_id..=end_id {
            if cost[i] == f32::MAX {
                continue;
            }
            for (&neighbor_id, &weight) in self.nodes[i].neighbors.iter() {
                let new_cost = cost[i] + weight;
                if new_cost < cost[neighbor_id] {
                    cost[neighbor_id] = new_cost;
                    predecessor[neighbor_id] = i;
                }
            }
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

    fn analyze_graph(&mut self) -> bool {
        let nodes_for_rows = self.compute_cheapest_path();
        if nodes_for_rows.len() != self.rows.len() {
            return false;
        }
        for (i, &node_id) in nodes_for_rows.iter().enumerate() {
            let state = Rc::clone(&self.nodes[node_id].state);
            self.rows[i].set_foot_placement(&state.combined_columns);
        }
        true
    }

    fn add_node(&mut self, state: Rc<State>, second: f32, _row_index: isize) -> usize {
        let id = self.nodes.len();
        self.nodes
            .push(StepParityNode::new(state, second));
        id
    }

    fn add_edge(&mut self, from_id: usize, to_id: usize, cost: f32) {
        if let Some(node) = self.nodes.get_mut(from_id) {
            node.neighbors.insert(to_id, cost);
        }
    }
}

fn get_state_cache_key(state: &State) -> u64 {
    let mut value = 0u64;
    let prime = 31u64;
    for &foot in &state.columns {
        value = value.wrapping_mul(prime).wrapping_add(foot as u64);
    }
    for &foot in &state.combined_columns {
        value = value.wrapping_mul(prime).wrapping_add(foot as u64);
    }

    for &f in &state.moved_feet {
        value = value.wrapping_mul(prime).wrapping_add(f as u64);
    }

    for &f in &state.hold_feet {
        value = value.wrapping_mul(prime).wrapping_add(f as u64);
    }

    value
}

struct CostCalculator<'a> {
    layout: &'a StageLayout,
}

impl<'a> CostCalculator<'a> {
    fn new(layout: &'a StageLayout) -> Self {
        Self { layout }
    }

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

    fn calc_mine_cost(&self, result: &State, row: &Row, column_count: usize) -> f32 {
        for i in 0..column_count {
            if result.combined_columns[i] != Foot::None && row.mines[i] != 0.0 {
                return MINE_WEIGHT;
            }
        }
        0.0
    }

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
                    self.layout.get_distance_sq(c, previous_col as usize).sqrt()
                };
                cost += HOLDSWITCH_WEIGHT * distance;
            }
        }
        cost
    }

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

    fn calc_facing_cost(&self, _initial: &State, result: &State, column_count: usize) -> f32 {
        let mut end_left_heel = INVALID_COLUMN;
        let mut end_left_toe = INVALID_COLUMN;
        let mut end_right_heel = INVALID_COLUMN;
        let mut end_right_toe = INVALID_COLUMN;

        for i in 0..column_count {
            match result.combined_columns[i] {
                Foot::LeftHeel => end_left_heel = i as isize,
                Foot::LeftToe => end_left_toe = i as isize,
                Foot::RightHeel => end_right_heel = i as isize,
                Foot::RightToe => end_right_toe = i as isize,
                Foot::None => {}
            }
        }

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

        let heel_penalty = (-heel_facing.min(0.0)).powf(1.8) * 100.0;
        let toe_penalty = (-toe_facing.min(0.0)).powf(1.8) * 100.0;
        let left_penalty = (-left_facing.min(0.0)).powf(1.8) * 100.0;
        let right_penalty = (-right_facing.min(0.0)).powf(1.8) * 100.0;

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

    fn calc_spin_cost(&self, initial: &State, result: &State, column_count: usize) -> f32 {
        let mut end_left_heel = INVALID_COLUMN;
        let mut end_left_toe = INVALID_COLUMN;
        let mut end_right_heel = INVALID_COLUMN;
        let mut end_right_toe = INVALID_COLUMN;

        for i in 0..column_count {
            match result.combined_columns[i] {
                Foot::LeftHeel => end_left_heel = i as isize,
                Foot::LeftToe => end_left_toe = i as isize,
                Foot::RightHeel => end_right_heel = i as isize,
                Foot::RightToe => end_right_toe = i as isize,
                Foot::None => {}
            }
        }

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

            let mut distance = self
                .layout
                .get_distance_sq(initial_position as usize, result_position as usize)
                .sqrt()
                * DISTANCE_WEIGHT
                / elapsed;

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
    chars: Vec<u8>,
    row: i32,
    beat: f32,
    second: f32,
}

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

fn parse_chart_rows(
    note_data: &[u8],
    bpm_map: &[(f64, f64)],
    offset: f64,
    column_count: usize,
) -> Vec<ParsedRow> {
    let mut rows = Vec::new();
    let mut measure_index = 0usize;
    if column_count == 0 {
        return rows;
    }

    for measure in note_data.split(|&b| b == b',') {
        if measure.is_empty() {
            continue;
        }
        let lines: Vec<&[u8]> = measure
            .split(|&b| b == b'\n')
            .filter_map(|line| {
                let trimmed = trim_ascii_whitespace(line);
                if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed)
                }
            })
            .collect();
        let num_rows = lines.len();
        if num_rows == 0 {
            measure_index += 1;
            continue;
        }

        for (i, line) in lines.iter().enumerate() {
            let percent = i as f32 / num_rows as f32;
            let beat = (measure_index as f32 + percent) * 4.0;
            let note_row = beat_to_note_row_f32_exact(beat);
            let beat = note_row as f32 / ROWS_PER_BEAT as f32;
            let second = beat_to_time(beat as f64, bpm_map, offset);
            let mut chars = vec![b'0'; column_count];
            for (col, ch) in line.iter().take(column_count).enumerate() {
                chars[col] = *ch;
            }
            rows.push(ParsedRow {
                chars,
                row: note_row,
                beat,
                second: second as f32,
            });
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
    if column_count == 0 {
        return rows;
    }

    for measure in note_data.split(|&b| b == b',') {
        if measure.is_empty() {
            continue;
        }
        let lines: Vec<&[u8]> = measure
            .split(|&b| b == b'\n')
            .filter_map(|line| {
                let trimmed = trim_ascii_whitespace(line);
                if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed)
                }
            })
            .collect();
        let num_rows = lines.len();
        if num_rows == 0 {
            measure_index += 1;
            continue;
        }

        for (i, line) in lines.iter().enumerate() {
            let percent = i as f32 / num_rows as f32;
            let beat = (measure_index as f32 + percent) * 4.0;
            let note_row = beat_to_note_row_f32_exact(beat);
            let beat = note_row as f32 / ROWS_PER_BEAT as f32;
            let second = timing.get_time_for_beat_f32(beat as f64);
            let mut chars = vec![b'0'; column_count];
            for (col, ch) in line.iter().take(column_count).enumerate() {
                chars[col] = *ch;
            }
            rows.push(ParsedRow {
                chars,
                row: note_row,
                beat,
                second: second as f32,
            });
        }

        measure_index += 1;
    }

    rows
}

fn build_intermediate_notes(rows: &[ParsedRow]) -> Vec<IntermediateNoteData> {
    let column_count = rows.first().map(|row| row.chars.len()).unwrap_or(0);
    if column_count == 0 {
        return Vec::new();
    }
    let mut hold_starts = vec![None; column_count];
    let mut hold_lengths: HashMap<(usize, usize), f32> = HashMap::new();

    for (row_idx, row) in rows.iter().enumerate() {
        for col in 0..column_count {
            match row.chars[col] {
                b'2' | b'4' => {
                    hold_starts[col] = Some((row_idx, row.beat));
                }
                b'3' => {
                    if let Some((start_idx, start_beat)) = hold_starts[col] {
                        let length = row.beat - start_beat;
                        hold_lengths.insert((start_idx, col), length);
                        hold_starts[col] = None;
                    }
                }
                _ => {}
            }
        }
    }

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
                note.hold_length = hold_lengths
                    .get(&(row_idx, col))
                    .copied()
                    .unwrap_or(MISSING_HOLD_LENGTH_BEATS);
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
    let column_count = rows.first().map(|row| row.chars.len()).unwrap_or(0);
    if column_count == 0 {
        return Vec::new();
    }
    let mut hold_starts = vec![None; column_count];
    let mut hold_lengths: HashMap<(usize, usize), f32> = HashMap::new();

    for (row_idx, row) in rows.iter().enumerate() {
        for col in 0..column_count {
            match row.chars[col] {
                b'2' | b'4' => {
                    hold_starts[col] = Some((row_idx, row.beat));
                }
                b'3' => {
                    if let Some((start_idx, start_beat)) = hold_starts[col] {
                        let length = row.beat - start_beat;
                        hold_lengths.insert((start_idx, col), length);
                        hold_starts[col] = None;
                    }
                }
                _ => {}
            }
        }
    }

    let mut notes = Vec::new();
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
                note.hold_length = hold_lengths
                    .get(&(row_idx, col))
                    .copied()
                    .unwrap_or(MISSING_HOLD_LENGTH_BEATS);
            }

            notes.push(note);
        }
    }
    notes
}

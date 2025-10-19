//! Step Parity analysis engine ported from ITGmania/StepMania.
//! This module determines the optimal foot placement for a `dance-single` chart
//! and calculates various technical statistics based on that placement.

use std::collections::{hash_map::DefaultHasher, HashMap, VecDeque};
use std::hash::{Hash, Hasher};

// --- Constants ---
const JACK_CUTOFF: f32 = 0.176;
const FOOTSWITCH_CUTOFF: f32 = 0.3;
const DOUBLESTEP_CUTOFF: f32 = 0.235;
const INVALID_COLUMN: isize = -1;
const NUM_TRACKS: usize = 4;

// Weights and thresholds from ITGmania source
const MINE: f32 = 1000.0;
const HOLDSWITCH: f32 = 50.0;
const BRACKETTAP: f32 = 20.0;
const OTHER: f32 = 150.0;
const BRACKETJACK: f32 = 100.0;
const DOUBLESTEP: f32 = 100.0;
const JUMP: f32 = 10.0;
const SLOW_BRACKET: f32 = 50.0;
const TWISTED_FOOT: f32 = 1000.0;
const FACING: f32 = 1.0;
const SPIN: f32 = 200.0;
const FOOTSWITCH: f32 = 50.0;
const SIDESWITCH: f32 = 50.0;
const MISSED_FOOTSWITCH: f32 = 100.0;
const JACK: f32 = 50.0;
const DISTANCE: f32 = 10.0;
const CROWDED_BRACKET: f32 = 100.0;

const SLOW_BRACKET_THRESHOLD: f32 = 0.15;
const JACK_THRESHOLD: f32 = 0.1;
const SLOW_FOOTSWITCH_THRESHOLD: f32 = 0.2;
const SLOW_FOOTSWITCH_IGNORE: f32 = 0.4;

// --- Enums and Core Data Structures ---

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(usize)]
pub enum Foot {
    LeftHeel = 0,
    LeftToe = 1,
    RightHeel = 2,
    RightToe = 3,
}
const NUM_FEET: usize = 4;
const FEET: [Foot; NUM_FEET] = [
    Foot::LeftHeel,
    Foot::LeftToe,
    Foot::RightHeel,
    Foot::RightToe,
];
const OTHER_PART_OF_FOOT: [Foot; NUM_FEET] = [
    Foot::LeftToe,
    Foot::LeftHeel,
    Foot::RightToe,
    Foot::RightHeel,
];

#[derive(Debug, Default, Clone, Copy)]
pub struct StagePoint {
    pub x: f32,
    pub y: f32,
}

#[derive(Debug, Clone)]
pub struct StageLayout {
    pub columns: Vec<StagePoint>,
    pub up_arrows: Vec<usize>,
    pub down_arrows: Vec<usize>,
    pub side_arrows: Vec<usize>,
}

impl StageLayout {
    pub fn new_dance_single() -> Self {
        Self {
            columns: vec![
                StagePoint { x: 0.0, y: 1.0 }, // Left
                StagePoint { x: 1.0, y: 0.0 }, // Down
                StagePoint { x: 1.0, y: 2.0 }, // Up
                StagePoint { x: 2.0, y: 1.0 }, // Right
            ],
            up_arrows: vec![2],
            down_arrows: vec![1],
            side_arrows: vec![0, 3],
        }
    }

    pub fn bracket_check(&self, c1: usize, c2: usize) -> bool {
        let p1 = self.columns[c1];
        let p2 = self.columns[c2];
        let dist_sq = (p1.y - p2.y).powi(2) + (p1.x - p2.x).powi(2);
        dist_sq <= 2.0
    }

    pub fn average_point(&self, i1: isize, i2: isize) -> StagePoint {
        match (i1, i2) {
            (INVALID_COLUMN, INVALID_COLUMN) => StagePoint::default(),
            (INVALID_COLUMN, c2) => self.columns[c2 as usize],
            (c1, INVALID_COLUMN) => self.columns[c1 as usize],
            (c1, c2) => StagePoint {
                x: (self.columns[c1 as usize].x + self.columns[c2 as usize].x) / 2.0,
                y: (self.columns[c1 as usize].y + self.columns[c2 as usize].y) / 2.0,
            },
        }
    }

    pub fn get_distance_sq(&self, c1: usize, c2: usize) -> f32 {
        let p1 = self.columns[c1];
        let p2 = self.columns[c2];
        (p1.y - p2.y).powi(2) + (p1.x - p2.x).powi(2)
    }

    pub fn get_x_difference(&self, c1: isize, c2: isize) -> f32 {
        if c1 == INVALID_COLUMN || c2 == INVALID_COLUMN {
            0.0
        } else {
            self.columns[c2 as usize].x - self.columns[c1 as usize].x
        }
    }

    pub fn get_y_difference(&self, c1: isize, c2: isize) -> f32 {
        if c1 == INVALID_COLUMN || c2 == INVALID_COLUMN {
            0.0
        } else {
            self.columns[c2 as usize].y - self.columns[c1 as usize].y
        }
    }
}

#[derive(Debug, Clone)]
pub struct Row {
    pub second: f32,
    pub beat: f32,
    pub row_index: usize,
    pub notes: [u8; NUM_TRACKS],
    pub holds: [bool; NUM_TRACKS],
    pub mines: [bool; NUM_TRACKS],
    pub parity: [Option<Foot>; NUM_TRACKS],
    pub where_the_feet_are: [isize; NUM_FEET],
    pub note_count: usize,
}

impl Row {
    fn set_foot_placement(&mut self, placement: &[Option<Foot>; NUM_TRACKS]) {
        self.parity = *placement;
        self.where_the_feet_are.fill(INVALID_COLUMN);
        for (col, &foot_opt) in placement.iter().enumerate() {
            if let Some(foot) = foot_opt {
                self.where_the_feet_are[foot as usize] = col as isize;
            }
        }
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
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

#[derive(Debug, Clone)]
struct State {
    columns: [Option<Foot>; NUM_TRACKS],
    combined_columns: [Option<Foot>; NUM_TRACKS],
    moved_columns: [Option<Foot>; NUM_TRACKS],
    hold_columns: [Option<Foot>; NUM_TRACKS],
    where_the_feet_are: [isize; NUM_FEET],
    what_note_the_foot_is_hitting: [isize; NUM_FEET],
    did_the_foot_move: [bool; NUM_FEET],
    is_the_foot_holding: [bool; NUM_FEET],
}

impl State {
    fn new() -> Self {
        Self {
            columns: [None; NUM_TRACKS],
            combined_columns: [None; NUM_TRACKS],
            moved_columns: [None; NUM_TRACKS],
            hold_columns: [None; NUM_TRACKS],
            where_the_feet_are: [INVALID_COLUMN; NUM_FEET],
            what_note_the_foot_is_hitting: [INVALID_COLUMN; NUM_FEET],
            did_the_foot_move: [false; NUM_FEET],
            is_the_foot_holding: [false; NUM_FEET],
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct FootPlacement {
    left_heel: isize,
    left_toe: isize,
    right_heel: isize,
    right_toe: isize,
    left_bracket: bool,
    right_bracket: bool,
}

impl FootPlacement {
    fn new() -> Self {
        Self {
            left_heel: INVALID_COLUMN,
            left_toe: INVALID_COLUMN,
            right_heel: INVALID_COLUMN,
            right_toe: INVALID_COLUMN,
            left_bracket: false,
            right_bracket: false,
        }
    }
}

struct StepParityNode {
    state_hash: u64,
    second: f32,
    neighbors: Vec<(usize, f32)>, // (to_node_id, cost)
}

struct StepParityGenerator {
    layout: StageLayout,
    permute_cache: HashMap<u32, Vec<[Option<Foot>; NUM_TRACKS]>>,
    state_cache: HashMap<u64, State>,
    nodes: Vec<StepParityNode>,
    rows: Vec<Row>,
}

pub fn analyze(minimized_note_data: &[u8], bpm_map: &[(f64, f64)], offset: f64) -> TechCounts {
    let mut generator = StepParityGenerator::new();
    if !generator.analyze_note_data(minimized_note_data, bpm_map, offset as f32) {
        return TechCounts::default();
    }
    calculate_tech_counts_from_rows(&generator.rows, &generator.layout)
}

fn beat_to_time(beat: f64, bpm_map: &[(f64, f64)], offset: f64) -> f64 {
    let mut time = -offset;
    let mut last_beat = 0.0;
    let mut last_bpm = if bpm_map.is_empty() {
        120.0
    } else {
        bpm_map[0].1
    };

    for &(b, bpm) in bpm_map {
        if b > beat {
            break;
        }
        if last_bpm > 0.0 {
            time += (b - last_beat) * 60.0 / last_bpm;
        }
        last_beat = b;
        last_bpm = bpm;
    }
    if last_bpm > 0.0 {
        time += (beat - last_beat) * 60.0 / last_bpm;
    }
    time
}

impl StepParityGenerator {
    fn new() -> Self {
        Self {
            layout: StageLayout::new_dance_single(),
            permute_cache: HashMap::new(),
            state_cache: HashMap::new(),
            nodes: Vec::new(),
            rows: Vec::new(),
        }
    }

    fn analyze_note_data(&mut self, note_data: &[u8], bpm_map: &[(f64, f64)], offset: f32) -> bool {
        self.create_rows(note_data, bpm_map, offset);
        if self.rows.is_empty() {
            return false;
        }
        self.build_state_graph();
        self.analyze_graph()
    }

    fn create_rows(&mut self, note_data: &[u8], bpm_map: &[(f64, f64)], offset: f32) {
        let mut row_map: HashMap<u64, Row> = HashMap::new();
        let mut row_keys_sorted = Vec::new();
        let mut hold_ends: HashMap<u64, [bool; NUM_TRACKS]> = HashMap::new();

        let mut measure_start_beat = 0.0;
        for measure_str in note_data.split(|&b| b == b',') {
            let lines: Vec<_> = measure_str
                .split(|&b| b == b'\n')
                .filter(|l| !l.is_empty())
                .collect();
            let num_rows = lines.len();
            if num_rows == 0 {
                continue;
            }
            for (i, line) in lines.iter().enumerate() {
                let beat = measure_start_beat + (i as f64 / num_rows as f64 * 4.0);
                let time_sec = beat_to_time(beat, bpm_map, offset as f64) as f32;
                let row_key = time_sec.to_bits() as u64;

                if !row_map.contains_key(&row_key) {
                    row_map.insert(
                        row_key,
                        Row {
                            second: time_sec,
                            beat: beat as f32,
                            row_index: 0,
                            notes: [b'0'; 4],
                            holds: [false; 4],
                            mines: [false; 4],
                            parity: [None; 4],
                            where_the_feet_are: [-1; 4],
                            note_count: 0,
                        },
                    );
                    row_keys_sorted.push(row_key);
                }
                let row = row_map.get_mut(&row_key).unwrap();
                for (col, &ch) in line.iter().take(NUM_TRACKS).enumerate() {
                    match ch {
                        b'1' | b'2' | b'4' => {
                            row.notes[col] = ch;
                            row.note_count += 1;
                        }
                        b'3' => {
                            hold_ends.entry(row_key).or_default()[col] = true;
                        }
                        b'M' => {
                            row.mines[col] = true;
                        }
                        _ => {}
                    }
                }
            }
            measure_start_beat += 4.0;
        }
        row_keys_sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
        row_keys_sorted.dedup();
        let mut active_holds = [false; NUM_TRACKS];
        self.rows = row_keys_sorted
            .into_iter()
            .enumerate()
            .map(|(i, key)| {
                let mut row = row_map.remove(&key).unwrap();
                row.row_index = i;
                for col in 0..NUM_TRACKS {
                    if active_holds[col] {
                        row.holds[col] = true;
                    }
                    if row.notes[col] == b'2' || row.notes[col] == b'4' {
                        active_holds[col] = true;
                    }
                    if let Some(ends) = hold_ends.get(&key) {
                        if ends[col] {
                            active_holds[col] = false;
                        }
                    }
                }
                row
            })
            .collect();
    }

    fn build_state_graph(&mut self) {
        let start_state = State::new();
        let start_hash = get_state_hash(&start_state);
        self.state_cache.insert(start_hash, start_state);
        self.add_node(
            start_hash,
            self.rows.first().map_or(0.0, |r| r.second - 1.0),
        );

        let mut prev_node_ids = vec![0];

        for i in 0..self.rows.len() {
            let mut result_nodes_for_this_row: Vec<usize> = Vec::new();

            let row = self.rows[i].clone();

            let permutations = self.get_foot_placement_permutations(&row).clone();

            for &initial_node_id in &prev_node_ids {
                let initial_state_hash = self.nodes[initial_node_id].state_hash;

                let initial_state = self.state_cache.get(&initial_state_hash).unwrap().clone();

                for perm in &permutations {
                    let result_hash = self.init_result_state(&initial_state, &row, perm);

                    let result_state = self.state_cache.get(&result_hash).unwrap();

                    let elapsed = row.second - self.nodes[initial_node_id].second;

                    let cost = CostCalculator::new(&self.layout).get_action_cost(
                        &initial_state,
                        result_state,
                        perm,
                        &self.rows,
                        i,
                        elapsed,
                    );

                    let mut existing_id: Option<usize> = None;
                    for &id in &result_nodes_for_this_row {
                        if self.nodes[id].state_hash == result_hash {
                            existing_id = Some(id);
                            break;
                        }
                    }

                    let result_node_id = if let Some(id) = existing_id {
                        id
                    } else {
                        let new_node_id = self.add_node_get_id(result_hash, row.second);
                        result_nodes_for_this_row.push(new_node_id);
                        new_node_id
                    };

                    self.add_edge(initial_node_id, result_node_id, cost);
                }
            }

            prev_node_ids = result_nodes_for_this_row;
        }

        let end_state = State::new();
        let end_hash = get_state_hash(&end_state);
        self.state_cache.insert(end_hash, end_state);
        let end_node = self.add_node_get_id(end_hash, self.rows.last().unwrap().second + 1.0);

        for node_id in prev_node_ids {
            self.add_edge(node_id, end_node, 0.0);
        }
    }

    fn init_result_state(
        &mut self,
        initial_state: &State,
        row: &Row,
        columns: &[Option<Foot>; NUM_TRACKS],
    ) -> u64 {
        let mut result_state = State::new();
        result_state.columns = *columns;

        for foot in 0..NUM_FEET {
            result_state.where_the_feet_are[foot] = INVALID_COLUMN;
            result_state.what_note_the_foot_is_hitting[foot] = INVALID_COLUMN;
            result_state.did_the_foot_move[foot] = false;
            result_state.is_the_foot_holding[foot] = false;
        }
        for col in 0..NUM_TRACKS {
            result_state.combined_columns[col] = None;
            result_state.moved_columns[col] = None;
            result_state.hold_columns[col] = None;
        }

        for (i, column_assignment) in columns.iter().enumerate() {
            if let Some(foot) = column_assignment {
                result_state.what_note_the_foot_is_hitting[*foot as usize] = i as isize;
                if !row.holds[i] {
                    result_state.moved_columns[i] = Some(*foot);
                    result_state.did_the_foot_move[*foot as usize] = true;
                    continue;
                }
                if initial_state.combined_columns[i] != Some(*foot) {
                    result_state.moved_columns[i] = Some(*foot);
                    result_state.did_the_foot_move[*foot as usize] = true;
                }
            }
        }

        for (i, column_assignment) in columns.iter().enumerate() {
            if row.holds[i] {
                if let Some(foot) = column_assignment {
                    result_state.hold_columns[i] = Some(*foot);
                    result_state.is_the_foot_holding[*foot as usize] = true;
                }
            }
        }

        self.merge_initial_and_result_position(initial_state, &mut result_state);

        for (col, &foot_opt) in result_state.combined_columns.iter().enumerate() {
            if let Some(foot) = foot_opt {
                result_state.where_the_feet_are[foot as usize] = col as isize;
            }
        }

        let hash = get_state_hash(&result_state);
        self.state_cache.entry(hash).or_insert(result_state);
        hash
    }

    fn merge_initial_and_result_position(&self, initial: &State, result: &mut State) {
        for i in 0..NUM_TRACKS {
            if let Some(foot) = result.columns[i] {
                result.combined_columns[i] = Some(foot);
                continue;
            }

            match initial.combined_columns[i] {
                Some(Foot::LeftHeel) | Some(Foot::RightHeel) => {
                    if let Some(prev_foot) = initial.combined_columns[i] {
                        if !result.did_the_foot_move[prev_foot as usize] {
                            result.combined_columns[i] = Some(prev_foot);
                        }
                    }
                }
                Some(Foot::LeftToe) => {
                    if !result.did_the_foot_move[Foot::LeftToe as usize]
                        && !result.did_the_foot_move[Foot::LeftHeel as usize]
                    {
                        result.combined_columns[i] = Some(Foot::LeftToe);
                    }
                }
                Some(Foot::RightToe) => {
                    if !result.did_the_foot_move[Foot::RightToe as usize]
                        && !result.did_the_foot_move[Foot::RightHeel as usize]
                    {
                        result.combined_columns[i] = Some(Foot::RightToe);
                    }
                }
                None => {}
            }
        }
    }

    fn get_foot_placement_permutations(&mut self, row: &Row) -> &Vec<[Option<Foot>; NUM_TRACKS]> {
        let key = row.notes.iter().enumerate().fold(0u32, |acc, (i, &note)| {
            if note != b'0' || row.holds[i] {
                acc | (1 << i)
            } else {
                acc
            }
        });
        if !self.permute_cache.contains_key(&key) {
            let mut perms = self.permute_recursive(row, [None; NUM_TRACKS], 0, false);
            if perms.is_empty() {
                perms = self.permute_recursive(row, [None; NUM_TRACKS], 0, true);
            }
            if perms.is_empty() {
                perms.push([None; NUM_TRACKS]);
            }
            self.permute_cache.insert(key, perms);
        }
        self.permute_cache.get(&key).unwrap()
    }

    fn permute_recursive(
        &self,
        row: &Row,
        columns: [Option<Foot>; NUM_TRACKS],
        col_idx: usize,
        ignore_holds: bool,
    ) -> Vec<[Option<Foot>; NUM_TRACKS]> {
        if col_idx >= NUM_TRACKS {
            let (lh, lt) = (
                columns.iter().position(|&f| f == Some(Foot::LeftHeel)),
                columns.iter().position(|&f| f == Some(Foot::LeftToe)),
            );
            let (rh, rt) = (
                columns.iter().position(|&f| f == Some(Foot::RightHeel)),
                columns.iter().position(|&f| f == Some(Foot::RightToe)),
            );
            if (lh.is_none() && lt.is_some()) || (rh.is_none() && rt.is_some()) {
                return vec![];
            }
            if let (Some(h), Some(t)) = (lh, lt) {
                if !self.layout.bracket_check(h, t) {
                    return vec![];
                }
            }
            if let (Some(h), Some(t)) = (rh, rt) {
                if !self.layout.bracket_check(h, t) {
                    return vec![];
                }
            }
            return vec![columns];
        }
        let mut permutations = Vec::new();
        if row.notes[col_idx] != b'0' || (!ignore_holds && row.holds[col_idx]) {
            for &foot in FEET.iter() {
                if !columns.contains(&Some(foot)) {
                    let mut new_cols = columns;
                    new_cols[col_idx] = Some(foot);
                    permutations.extend(self.permute_recursive(
                        row,
                        new_cols,
                        col_idx + 1,
                        ignore_holds,
                    ));
                }
            }
            permutations
        } else {
            self.permute_recursive(row, columns, col_idx + 1, ignore_holds)
        }
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
            for &(neighbor_id, weight) in &self.nodes[i].neighbors {
                if cost[i] + weight < cost[neighbor_id] {
                    cost[neighbor_id] = cost[i] + weight;
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
            let state = self
                .state_cache
                .get(&self.nodes[node_id].state_hash)
                .unwrap();
            self.rows[i].set_foot_placement(&state.combined_columns);
        }
        true
    }

    fn add_node(&mut self, state_hash: u64, second: f32) -> &mut StepParityNode {
        let id = self.nodes.len();
        self.nodes.push(StepParityNode {
            state_hash,
            second,
            neighbors: Vec::new(),
        });
        &mut self.nodes[id]
    }

    fn add_node_get_id(&mut self, state_hash: u64, second: f32) -> usize {
        let id = self.nodes.len();
        self.nodes.push(StepParityNode {
            state_hash,
            second,
            neighbors: Vec::new(),
        });
        id
    }

    fn add_edge(&mut self, from_id: usize, to_id: usize, cost: f32) {
        if let Some(node) = self.nodes.get_mut(from_id) {
            node.neighbors.push((to_id, cost));
        }
    }
}

fn get_state_hash(state: &State) -> u64 {
    let mut hasher = DefaultHasher::new();
    state.columns.hash(&mut hasher);
    state.combined_columns.hash(&mut hasher);
    state.moved_columns.hash(&mut hasher);
    state.hold_columns.hash(&mut hasher);
    state.did_the_foot_move.hash(&mut hasher);
    state.is_the_foot_holding.hash(&mut hasher);
    hasher.finish()
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
        columns: &[Option<Foot>; NUM_TRACKS],
        rows: &[Row],
        row_index: usize,
        elapsed: f32,
    ) -> f32 {
        let row = &rows[row_index];

        let moved_left = result.did_the_foot_move[Foot::LeftHeel as usize]
            || result.did_the_foot_move[Foot::LeftToe as usize];
        let moved_right = result.did_the_foot_move[Foot::RightHeel as usize]
            || result.did_the_foot_move[Foot::RightToe as usize];

        let did_jump = ((initial.did_the_foot_move[Foot::LeftHeel as usize]
            && !initial.is_the_foot_holding[Foot::LeftHeel as usize])
            || (initial.did_the_foot_move[Foot::LeftToe as usize]
                && !initial.is_the_foot_holding[Foot::LeftToe as usize]))
            && ((initial.did_the_foot_move[Foot::RightHeel as usize]
                && !initial.is_the_foot_holding[Foot::RightHeel as usize])
                || (initial.did_the_foot_move[Foot::RightToe as usize]
                    && !initial.is_the_foot_holding[Foot::RightToe as usize]));

        let initial_placement = self.foot_placement_from_columns(&initial.combined_columns);
        let result_placement = self.foot_placement_from_columns(columns);
        let combined_placement = self.foot_placement_from_columns(&result.combined_columns);

        let jacked_left =
            self.did_jack_left(initial, result, &result_placement, moved_left, did_jump);
        let jacked_right =
            self.did_jack_right(initial, result, &result_placement, moved_right, did_jump);

        let mut cost = 0.0;
        cost += self.calc_mine_cost(&result.combined_columns, row);
        cost += self.calc_hold_switch_cost(initial, &result.combined_columns, row);
        cost += self.calc_bracket_tap_cost(initial, row, &result_placement, elapsed);
        cost += self.calc_moving_foot_while_other_isnt_on_pad_cost(initial, result);
        cost += self.calc_bracket_jack_cost(
            moved_left,
            moved_right,
            did_jump,
            jacked_left,
            jacked_right,
            result,
        );
        cost += self.calc_doublestep_cost(
            initial,
            result,
            rows,
            row_index,
            moved_left,
            moved_right,
            did_jump,
            jacked_left,
            jacked_right,
        );
        cost += self.calc_jump_cost(row, moved_left, moved_right, elapsed);
        cost += self.calc_slow_bracket_cost(row, moved_left, moved_right, elapsed);
        cost += self.calc_twisted_foot_cost(&combined_placement);
        cost += self.calc_facing_cost(&combined_placement);
        cost += self.calc_spin_cost(initial, &combined_placement);
        cost += self.calc_footswitch_cost(initial, columns, row, elapsed);
        cost += self.calc_sideswitch_cost(initial, result);
        cost += self.calc_missed_footswitch_cost(row, jacked_left, jacked_right);
        cost += self.calc_jack_cost(moved_left, moved_right, jacked_left, jacked_right, elapsed);
        cost += self.calc_distance_cost(initial, result, elapsed);
        cost += self.calc_crowded_bracket_cost(&initial_placement, &result_placement, elapsed);
        cost
    }

    fn foot_placement_from_columns(&self, columns: &[Option<Foot>; NUM_TRACKS]) -> FootPlacement {
        let mut placement = FootPlacement::new();
        for i in 0..NUM_TRACKS {
            match columns[i] {
                Some(Foot::LeftHeel) => placement.left_heel = i as isize,
                Some(Foot::LeftToe) => placement.left_toe = i as isize,
                Some(Foot::RightHeel) => placement.right_heel = i as isize,
                Some(Foot::RightToe) => placement.right_toe = i as isize,
                _ => {}
            }
        }
        if placement.left_heel != INVALID_COLUMN && placement.left_toe != INVALID_COLUMN {
            placement.left_bracket = true;
        }
        if placement.right_heel != INVALID_COLUMN && placement.right_toe != INVALID_COLUMN {
            placement.right_bracket = true;
        }
        placement
    }

    fn did_jack_left(
        &self,
        initial: &State,
        result: &State,
        placement: &FootPlacement,
        moved_left: bool,
        did_jump: bool,
    ) -> bool {
        if did_jump || !moved_left {
            return false;
        }
        let mut jacked = false;
        if placement.left_heel != INVALID_COLUMN {
            if initial.combined_columns[placement.left_heel as usize] == Some(Foot::LeftHeel)
                && !result.is_the_foot_holding[Foot::LeftHeel as usize]
                && ((initial.did_the_foot_move[Foot::LeftHeel as usize]
                    && !initial.is_the_foot_holding[Foot::LeftHeel as usize])
                    || (initial.did_the_foot_move[Foot::LeftToe as usize]
                        && !initial.is_the_foot_holding[Foot::LeftToe as usize]))
            {
                jacked = true;
            }
        }
        if placement.left_toe != INVALID_COLUMN {
            if initial.combined_columns[placement.left_toe as usize] == Some(Foot::LeftToe)
                && !result.is_the_foot_holding[Foot::LeftToe as usize]
                && ((initial.did_the_foot_move[Foot::LeftHeel as usize]
                    && !initial.is_the_foot_holding[Foot::LeftHeel as usize])
                    || (initial.did_the_foot_move[Foot::LeftToe as usize]
                        && !initial.is_the_foot_holding[Foot::LeftToe as usize]))
            {
                jacked = true;
            }
        }
        jacked
    }

    fn did_jack_right(
        &self,
        initial: &State,
        result: &State,
        placement: &FootPlacement,
        moved_right: bool,
        did_jump: bool,
    ) -> bool {
        if did_jump || !moved_right {
            return false;
        }
        let mut jacked = false;
        if placement.right_heel != INVALID_COLUMN {
            if initial.combined_columns[placement.right_heel as usize] == Some(Foot::RightHeel)
                && !result.is_the_foot_holding[Foot::RightHeel as usize]
                && ((initial.did_the_foot_move[Foot::RightHeel as usize]
                    && !initial.is_the_foot_holding[Foot::RightHeel as usize])
                    || (initial.did_the_foot_move[Foot::RightToe as usize]
                        && !initial.is_the_foot_holding[Foot::RightToe as usize]))
            {
                jacked = true;
            }
        }
        if placement.right_toe != INVALID_COLUMN {
            if initial.combined_columns[placement.right_toe as usize] == Some(Foot::RightToe)
                && !result.is_the_foot_holding[Foot::RightToe as usize]
                && ((initial.did_the_foot_move[Foot::RightHeel as usize]
                    && !initial.is_the_foot_holding[Foot::RightHeel as usize])
                    || (initial.did_the_foot_move[Foot::RightToe as usize]
                        && !initial.is_the_foot_holding[Foot::RightToe as usize]))
            {
                jacked = true;
            }
        }
        jacked
    }

    fn did_double_step(
        &self,
        initial: &State,
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
            && ((initial.did_the_foot_move[Foot::LeftHeel as usize]
                && !initial.is_the_foot_holding[Foot::LeftHeel as usize])
                || (initial.did_the_foot_move[Foot::LeftToe as usize]
                    && !initial.is_the_foot_holding[Foot::LeftToe as usize]))
        {
            doublestepped = true;
        }
        if moved_right
            && !jacked_right
            && ((initial.did_the_foot_move[Foot::RightHeel as usize]
                && !initial.is_the_foot_holding[Foot::RightHeel as usize])
                || (initial.did_the_foot_move[Foot::RightToe as usize]
                    && !initial.is_the_foot_holding[Foot::RightToe as usize]))
        {
            doublestepped = true;
        }
        if row_index > 0 {
            let last_row = &rows[row_index - 1];
            let start_beat = last_row.beat;
            let end_beat = rows[row_index].beat;
            for col in 0..NUM_TRACKS {
                if last_row.holds[col] {
                    let mut end = f32::MAX;
                    for j in row_index..rows.len() {
                        if !rows[j].holds[col] {
                            end = rows[j].beat;
                            break;
                        }
                    }
                    if end > start_beat && end < end_beat {
                        doublestepped = false;
                    }
                    if end >= end_beat {
                        doublestepped = false;
                    }
                }
            }
        }
        doublestepped
    }

    fn calc_mine_cost(&self, combined_columns: &[Option<Foot>; NUM_TRACKS], row: &Row) -> f32 {
        let mut cost = 0.0;
        for i in 0..NUM_TRACKS {
            if combined_columns[i].is_some() && row.mines[i] {
                cost += MINE;
                break;
            }
        }
        cost
    }

    fn calc_hold_switch_cost(
        &self,
        initial: &State,
        combined_columns: &[Option<Foot>; NUM_TRACKS],
        row: &Row,
    ) -> f32 {
        let mut cost = 0.0;
        for c in 0..NUM_TRACKS {
            if !row.holds[c] {
                continue;
            }
            let current_foot = combined_columns[c];
            if let Some(f) = current_foot {
                let is_left = f == Foot::LeftHeel || f == Foot::LeftToe;
                let initial_foot = initial.combined_columns[c];
                let initial_is_left =
                    initial_foot == Some(Foot::LeftHeel) || initial_foot == Some(Foot::LeftToe);
                let initial_is_right =
                    initial_foot == Some(Foot::RightHeel) || initial_foot == Some(Foot::RightToe);
                let switch_left = is_left && !initial_is_left;
                let switch_right = !is_left && !initial_is_right;
                if switch_left || switch_right {
                    let previous_col = initial.where_the_feet_are[f as usize];
                    let temp_cost = HOLDSWITCH
                        * if previous_col == INVALID_COLUMN {
                            1.0
                        } else {
                            self.layout.get_distance_sq(c, previous_col as usize).sqrt()
                        };
                    cost += temp_cost;
                }
            }
        }
        cost
    }

    fn calc_bracket_tap_cost(
        &self,
        initial: &State,
        row: &Row,
        placement: &FootPlacement,
        elapsed: f32,
    ) -> f32 {
        let mut cost = 0.0;
        if placement.left_bracket {
            let jack_penalty = if initial.did_the_foot_move[Foot::LeftHeel as usize]
                || initial.did_the_foot_move[Foot::LeftToe as usize]
            {
                1.0 / elapsed
            } else {
                1.0
            };
            if row.holds[placement.left_heel as usize] && !row.holds[placement.left_toe as usize] {
                cost += BRACKETTAP * jack_penalty;
            }
            if row.holds[placement.left_toe as usize] && !row.holds[placement.left_heel as usize] {
                cost += BRACKETTAP * jack_penalty;
            }
        }
        if placement.right_bracket {
            let jack_penalty = if initial.did_the_foot_move[Foot::RightHeel as usize]
                || initial.did_the_foot_move[Foot::RightToe as usize]
            {
                1.0 / elapsed
            } else {
                1.0
            };
            if row.holds[placement.right_heel as usize] && !row.holds[placement.right_toe as usize]
            {
                cost += BRACKETTAP * jack_penalty;
            }
            if row.holds[placement.right_toe as usize] && !row.holds[placement.right_heel as usize]
            {
                cost += BRACKETTAP * jack_penalty;
            }
        }
        cost
    }

    fn calc_moving_foot_while_other_isnt_on_pad_cost(
        &self,
        initial: &State,
        result: &State,
    ) -> f32 {
        let mut cost = 0.0;
        let has_any = initial.combined_columns.iter().any(|&f| f.is_some());
        if has_any {
            for (f, &moved) in result.did_the_foot_move.iter().enumerate() {
                if !moved {
                    continue;
                }
                let foot = FEET[f];
                match foot {
                    Foot::LeftHeel | Foot::LeftToe => {
                        if initial.where_the_feet_are[Foot::RightHeel as usize] == INVALID_COLUMN
                            && initial.where_the_feet_are[Foot::RightToe as usize] == INVALID_COLUMN
                        {
                            cost += OTHER;
                        }
                    }
                    Foot::RightHeel | Foot::RightToe => {
                        if initial.where_the_feet_are[Foot::LeftHeel as usize] == INVALID_COLUMN
                            && initial.where_the_feet_are[Foot::LeftToe as usize] == INVALID_COLUMN
                        {
                            cost += OTHER;
                        }
                    }
                }
            }
        }
        cost
    }

    fn calc_bracket_jack_cost(
        &self,
        moved_left: bool,
        moved_right: bool,
        did_jump: bool,
        jacked_left: bool,
        jacked_right: bool,
        result: &State,
    ) -> f32 {
        let mut cost = 0.0;
        if moved_left != moved_right
            && (moved_left || moved_right)
            && result.hold_columns.iter().all(|c| c.is_none())
            && !did_jump
        {
            if jacked_left
                && result.did_the_foot_move[Foot::LeftHeel as usize]
                && result.did_the_foot_move[Foot::LeftToe as usize]
            {
                cost += BRACKETJACK;
            }
            if jacked_right
                && result.did_the_foot_move[Foot::RightHeel as usize]
                && result.did_the_foot_move[Foot::RightToe as usize]
            {
                cost += BRACKETJACK;
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
        did_jump: bool,
        jacked_left: bool,
        jacked_right: bool,
    ) -> f32 {
        let mut cost = 0.0;
        if moved_left != moved_right
            && (moved_left || moved_right)
            && result.hold_columns.iter().all(|c| c.is_none())
            && !did_jump
        {
            if self.did_double_step(
                initial,
                rows,
                row_index,
                moved_left,
                jacked_left,
                moved_right,
                jacked_right,
            ) {
                cost += DOUBLESTEP;
            }
        }
        cost
    }

    fn calc_jump_cost(&self, row: &Row, moved_left: bool, moved_right: bool, elapsed: f32) -> f32 {
        let mut cost = 0.0;
        if moved_left && moved_right && row.note_count >= 2 {
            cost += JUMP / elapsed;
        }
        cost
    }

    fn calc_slow_bracket_cost(
        &self,
        row: &Row,
        moved_left: bool,
        moved_right: bool,
        elapsed: f32,
    ) -> f32 {
        let mut cost = 0.0;
        if elapsed > SLOW_BRACKET_THRESHOLD && moved_left != moved_right && row.note_count >= 2 {
            let timediff = elapsed - SLOW_BRACKET_THRESHOLD;
            cost += timediff * SLOW_BRACKET;
        }
        cost
    }

    fn calc_twisted_foot_cost(&self, placement: &FootPlacement) -> f32 {
        let left_pos = self
            .layout
            .average_point(placement.left_heel, placement.left_toe);
        let right_pos = self
            .layout
            .average_point(placement.right_heel, placement.right_toe);

        let crossed_over = right_pos.x < left_pos.x;
        let right_backwards =
            if placement.right_heel != INVALID_COLUMN && placement.right_toe != INVALID_COLUMN {
                self.layout.columns[placement.right_toe as usize].y
                    < self.layout.columns[placement.right_heel as usize].y
            } else {
                false
            };
        let left_backwards =
            if placement.left_heel != INVALID_COLUMN && placement.left_toe != INVALID_COLUMN {
                self.layout.columns[placement.left_toe as usize].y
                    < self.layout.columns[placement.left_heel as usize].y
            } else {
                false
            };

        if !crossed_over && (right_backwards || left_backwards) {
            TWISTED_FOOT
        } else {
            0.0
        }
    }

    fn calc_facing_cost(&self, placement: &FootPlacement) -> f32 {
        let mut cost = 0.0;
        let heel_facing = self
            .layout
            .get_x_difference(placement.left_heel, placement.right_heel);
        let toe_facing = self
            .layout
            .get_x_difference(placement.left_toe, placement.right_toe);
        let left_facing = self
            .layout
            .get_y_difference(placement.left_heel, placement.left_toe);
        let right_facing = self
            .layout
            .get_y_difference(placement.right_heel, placement.right_toe);

        let heel_penalty = (-heel_facing.min(0.0)).powf(1.8) * 100.0;
        let toe_penalty = (-toe_facing.min(0.0)).powf(1.8) * 100.0;
        let left_penalty = (-left_facing.min(0.0)).powf(1.8) * 100.0;
        let right_penalty = (-right_facing.min(0.0)).powf(1.8) * 100.0;

        if heel_penalty > 0.0 {
            cost += heel_penalty * FACING;
        }
        if toe_penalty > 0.0 {
            cost += toe_penalty * FACING;
        }
        if left_penalty > 0.0 {
            cost += left_penalty * FACING;
        }
        if right_penalty > 0.0 {
            cost += right_penalty * FACING;
        }
        cost
    }

    fn calc_spin_cost(&self, initial: &State, placement: &FootPlacement) -> f32 {
        let mut cost = 0.0;
        let previous_left_pos = self.layout.average_point(
            initial.where_the_feet_are[Foot::LeftHeel as usize],
            initial.where_the_feet_are[Foot::LeftToe as usize],
        );
        let previous_right_pos = self.layout.average_point(
            initial.where_the_feet_are[Foot::RightHeel as usize],
            initial.where_the_feet_are[Foot::RightToe as usize],
        );
        let left_pos = self
            .layout
            .average_point(placement.left_heel, placement.left_toe);
        let right_pos = self
            .layout
            .average_point(placement.right_heel, placement.right_toe);

        if right_pos.x < left_pos.x
            && previous_right_pos.x < previous_left_pos.x
            && right_pos.y < left_pos.y
            && previous_right_pos.y > previous_left_pos.y
        {
            cost += SPIN;
        }
        if right_pos.x < left_pos.x
            && previous_right_pos.x < previous_left_pos.x
            && right_pos.y > left_pos.y
            && previous_right_pos.y < previous_left_pos.y
        {
            cost += SPIN;
        }
        cost
    }

    fn calc_footswitch_cost(
        &self,
        initial: &State,
        columns: &[Option<Foot>; NUM_TRACKS],
        row: &Row,
        elapsed: f32,
    ) -> f32 {
        let mut cost = 0.0;
        if elapsed >= SLOW_FOOTSWITCH_THRESHOLD && elapsed < SLOW_FOOTSWITCH_IGNORE {
            if !row.mines.iter().any(|&m| m) {
                let time_scaled = elapsed - SLOW_FOOTSWITCH_THRESHOLD;
                for i in 0..NUM_TRACKS {
                    let initial_foot = initial.combined_columns[i];
                    let result_foot = columns[i];
                    if initial_foot.is_none() || result_foot.is_none() {
                        continue;
                    }
                    let result_foot_value = result_foot.unwrap();
                    if initial_foot != result_foot
                        && initial_foot
                            != Some(OTHER_PART_OF_FOOT[result_foot_value as usize])
                    {
                        cost +=
                            (time_scaled / (SLOW_FOOTSWITCH_THRESHOLD + time_scaled)) * FOOTSWITCH;
                    }
                }
            }
        }
        cost
    }

    fn calc_sideswitch_cost(&self, initial: &State, result: &State) -> f32 {
        let mut cost = 0.0;
        for &c in &self.layout.side_arrows {
            let initial_foot = initial.combined_columns[c];
            let result_foot = result.columns[c];
            if let (Some(initial_foot_value), Some(result_foot_value)) = (initial_foot, result_foot) {
                if initial_foot_value != result_foot_value
                    && initial_foot_value
                        != OTHER_PART_OF_FOOT[result_foot_value as usize]
                    && !result.did_the_foot_move[initial_foot_value as usize]
                {
                    cost += SIDESWITCH;
                }
            }
        }
        cost
    }

    fn calc_missed_footswitch_cost(&self, row: &Row, jacked_left: bool, jacked_right: bool) -> f32 {
        let mut cost = 0.0;
        if (jacked_left || jacked_right) && row.mines.iter().any(|&m| m) {
            cost += MISSED_FOOTSWITCH;
        }
        cost
    }

    fn calc_jack_cost(
        &self,
        moved_left: bool,
        moved_right: bool,
        jacked_left: bool,
        jacked_right: bool,
        elapsed: f32,
    ) -> f32 {
        let mut cost = 0.0;
        if elapsed < JACK_THRESHOLD && moved_left != moved_right {
            let time_scaled = JACK_THRESHOLD - elapsed;
            if jacked_left || jacked_right {
                cost += (1.0 / time_scaled - 1.0 / JACK_THRESHOLD) * JACK;
            }
        }
        cost
    }

    fn calc_distance_cost(&self, initial: &State, result: &State, elapsed: f32) -> f32 {
        let mut cost = 0.0;
        for f in 0..NUM_FEET {
            if !result.did_the_foot_move[f] {
                continue;
            }
            let initial_col = initial.where_the_feet_are[f];
            if initial_col == INVALID_COLUMN {
                continue;
            }
            let result_col = result.where_the_feet_are[f];
            let other = OTHER_PART_OF_FOOT[f];
            let is_bracketing = result.where_the_feet_are[other as usize] != INVALID_COLUMN;
            if is_bracketing && result.where_the_feet_are[other as usize] == initial_col {
                continue;
            }
            let mut dist = self
                .layout
                .get_distance_sq(initial_col as usize, result_col as usize)
                .sqrt()
                * DISTANCE
                / elapsed;
            if is_bracketing {
                dist *= 0.2;
            }
            cost += dist;
        }
        cost
    }

    fn does_left_foot_overlap_right(
        &self,
        initial_placement: &FootPlacement,
        result_placement: &FootPlacement,
    ) -> bool {
        if initial_placement.right_heel != INVALID_COLUMN
            && (initial_placement.right_heel == result_placement.left_heel
                || initial_placement.right_heel == result_placement.left_toe)
        {
            return true;
        }
        if initial_placement.right_toe != INVALID_COLUMN
            && (initial_placement.right_toe == result_placement.left_heel
                || initial_placement.right_toe == result_placement.left_toe)
        {
            return true;
        }
        false
    }

    fn does_right_foot_overlap_left(
        &self,
        initial_placement: &FootPlacement,
        result_placement: &FootPlacement,
    ) -> bool {
        if initial_placement.left_heel != INVALID_COLUMN
            && (initial_placement.left_heel == result_placement.right_heel
                || initial_placement.left_heel == result_placement.right_toe)
        {
            return true;
        }
        if initial_placement.left_toe != INVALID_COLUMN
            && (initial_placement.left_toe == result_placement.right_heel
                || initial_placement.left_toe == result_placement.right_toe)
        {
            return true;
        }
        false
    }

    fn calc_crowded_bracket_cost(
        &self,
        initial_placement: &FootPlacement,
        result_placement: &FootPlacement,
        elapsed: f32,
    ) -> f32 {
        let mut cost = 0.0;
        if result_placement.left_bracket
            && self.does_left_foot_overlap_right(initial_placement, result_placement)
        {
            cost += CROWDED_BRACKET / elapsed;
        } else if initial_placement.left_bracket
            && self.does_right_foot_overlap_left(initial_placement, result_placement)
        {
            cost += CROWDED_BRACKET / elapsed;
        }
        if result_placement.right_bracket
            && self.does_right_foot_overlap_left(initial_placement, result_placement)
        {
            cost += CROWDED_BRACKET / elapsed;
        } else if initial_placement.right_bracket
            && self.does_left_foot_overlap_right(initial_placement, result_placement)
        {
            cost += CROWDED_BRACKET / elapsed;
        }
        cost
    }
}

fn calculate_tech_counts_from_rows(rows: &[Row], layout: &StageLayout) -> TechCounts {
    let mut out = TechCounts::default();

    if rows.len() < 2 {
        return out;
    }

    for i in 1..rows.len() {
        let current_row = &rows[i];
        let previous_row = &rows[i - 1];
        let elapsed_time = current_row.second - previous_row.second;

        if current_row.note_count == 1 && previous_row.note_count == 1 {
            for foot_idx in 0..NUM_FEET {
                let current_col = current_row.where_the_feet_are[foot_idx];
                let previous_col = previous_row.where_the_feet_are[foot_idx];
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
            if current_row.where_the_feet_are[Foot::LeftHeel as usize] != INVALID_COLUMN
                && current_row.where_the_feet_are[Foot::LeftToe as usize] != INVALID_COLUMN
            {
                out.brackets += 1;
            }
            if current_row.where_the_feet_are[Foot::RightHeel as usize] != INVALID_COLUMN
                && current_row.where_the_feet_are[Foot::RightToe as usize] != INVALID_COLUMN
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

        let left_heel = current_row.where_the_feet_are[Foot::LeftHeel as usize];
        let left_toe = current_row.where_the_feet_are[Foot::LeftToe as usize];
        let right_heel = current_row.where_the_feet_are[Foot::RightHeel as usize];
        let right_toe = current_row.where_the_feet_are[Foot::RightToe as usize];

        let previous_left_heel = previous_row.where_the_feet_are[Foot::LeftHeel as usize];
        let previous_left_toe = previous_row.where_the_feet_are[Foot::LeftToe as usize];
        let previous_right_heel = previous_row.where_the_feet_are[Foot::RightHeel as usize];
        let previous_right_toe = previous_row.where_the_feet_are[Foot::RightToe as usize];

        if right_heel != INVALID_COLUMN
            && previous_left_heel != INVALID_COLUMN
            && previous_right_heel == INVALID_COLUMN
        {
            let left_pos = layout.average_point(previous_left_heel, previous_left_toe);
            let right_pos = layout.average_point(right_heel, right_toe);

            if right_pos.x < left_pos.x {
                if i > 1 {
                    let previous_previous_row = &rows[i - 2];
                    let previous_previous_right_heel =
                        previous_previous_row.where_the_feet_are[Foot::RightHeel as usize];
                    if previous_previous_right_heel != INVALID_COLUMN
                        && previous_previous_right_heel != right_heel
                    {
                        let previous_previous_right_pos =
                            layout.columns[previous_previous_right_heel as usize];
                        if previous_previous_right_pos.x > left_pos.x {
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
            && previous_right_heel != INVALID_COLUMN
            && previous_left_heel == INVALID_COLUMN
        {
            let left_pos = layout.average_point(left_heel, left_toe);
            let right_pos = layout.average_point(previous_right_heel, previous_right_toe);

            if right_pos.x < left_pos.x {
                if i > 1 {
                    let previous_previous_row = &rows[i - 2];
                    let previous_previous_left_heel =
                        previous_previous_row.where_the_feet_are[Foot::LeftHeel as usize];
                    if previous_previous_left_heel != INVALID_COLUMN
                        && previous_previous_left_heel != left_heel
                    {
                        let previous_previous_left_pos =
                            layout.columns[previous_previous_left_heel as usize];
                        if right_pos.x > previous_previous_left_pos.x {
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
    if elapsed_time >= FOOTSWITCH_CUTOFF {
        return false;
    }

    match (previous_row.parity[column], current_row.parity[column]) {
        (Some(prev), Some(curr)) => prev != curr && OTHER_PART_OF_FOOT[prev as usize] != curr,
        _ => false,
    }
}

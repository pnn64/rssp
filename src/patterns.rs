use std::collections::HashMap;
use std::sync::LazyLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PatternVariant {
    AltStaircasesLeft,
    AltStaircasesRight,
    AltStaircasesInvLeft,
    AltStaircasesInvRight,
    BoxLR,
    BoxUD,
    BoxCornerLD,
    BoxCornerLU,
    BoxCornerRD,
    BoxCornerRU,
    CandleLeft,
    CandleRight,
    CopterLeft,
    CopterRight,
    CopterInvLeft,
    CopterInvRight,
    DoritoRight,
    DoritoLeft,
    DoritoInvRight,
    DoritoInvLeft,
    DStaircaseLeft,
    DStaircaseRight,
    DStaircaseInvLeft,
    DStaircaseInvRight,
    HipBreakerLeft,
    HipBreakerRight,
    HipBreakerInvLeft,
    HipBreakerInvRight,
    LuchiLeftDU,
    LuchiLeftUD,
    LuchiRightUD,
    LuchiRightDU,
    SideswitchLeft,
    SideswitchRight,
    SideswitchGallopLeft,
    SideswitchGallopRight,
    SpiralLeft,
    SpiralRight,
    SpiralInvLeft,
    SpiralInvRight,
    StaircaseLeft,
    StaircaseRight,
    StaircaseInvLeft,
    StaircaseInvRight,
    SweepCandleLeft,
    SweepCandleRight,
    SweepCandleInvLeft,
    SweepCandleInvRight,
    SweepLeft,
    SweepRight,
    SweepInvLeft,
    SweepInvRight,
    TowerLR,
    TowerUD,
    TowerCornerLD,
    TowerCornerLU,
    TowerCornerRD,
    TowerCornerRU,
    TriangleLDL,
    TriangleLUL,
    TriangleRDR,
    TriangleRUR,
    TurboCandleLeft,
    TurboCandleRight,
    TurboCandleInvLeft,
    TurboCandleInvRight,
}

pub static ALL_PATTERNS: LazyLock<Vec<(PatternVariant, Vec<u8>)>> = LazyLock::new(|| {
    vec![
    //Staircases
    (PatternVariant::StaircaseLeft,     string_to_pattern_bits("RUDL")),
    (PatternVariant::StaircaseRight,    string_to_pattern_bits("LDUR")),
    (PatternVariant::StaircaseInvLeft,  string_to_pattern_bits("RDUL")),
    (PatternVariant::StaircaseInvRight, string_to_pattern_bits("LUDR")),

    // Candles
    (PatternVariant::CandleLeft,  string_to_pattern_bits("ULD")),
    (PatternVariant::CandleLeft,  string_to_pattern_bits("DLU")),
    (PatternVariant::CandleRight, string_to_pattern_bits("URD")),
    (PatternVariant::CandleRight, string_to_pattern_bits("DRU")),

    // Triangles
    (PatternVariant::TriangleRUR, string_to_pattern_bits("RUR")),
    (PatternVariant::TriangleLUL, string_to_pattern_bits("LUL")),
    (PatternVariant::TriangleLDL, string_to_pattern_bits("LDL")),
    (PatternVariant::TriangleRDR, string_to_pattern_bits("RDR")),

    // Doritos
    (PatternVariant::DoritoLeft,     string_to_pattern_bits("LDUDL")),
    (PatternVariant::DoritoRight,    string_to_pattern_bits("RUDUR")),
    (PatternVariant::DoritoInvLeft,  string_to_pattern_bits("LUDUL")),
    (PatternVariant::DoritoInvRight, string_to_pattern_bits("RDUDR")),

    // Sweeps
    (PatternVariant::SweepLeft,     string_to_pattern_bits("LDURUDL")),
    (PatternVariant::SweepRight,    string_to_pattern_bits("RUDLDUR")),
    (PatternVariant::SweepInvLeft,  string_to_pattern_bits("LUDRDUL")),
    (PatternVariant::SweepInvRight, string_to_pattern_bits("RDULUDR")),

    // Boxes
    (PatternVariant::BoxLR,       string_to_pattern_bits("LRLR")),
    (PatternVariant::BoxLR,       string_to_pattern_bits("RLRL")),
    (PatternVariant::BoxUD,       string_to_pattern_bits("UDUD")),
    (PatternVariant::BoxUD,       string_to_pattern_bits("DUDU")),
    (PatternVariant::BoxCornerLD, string_to_pattern_bits("LDLD")),
    (PatternVariant::BoxCornerLD, string_to_pattern_bits("DLDL")),
    (PatternVariant::BoxCornerLU, string_to_pattern_bits("LULU")),
    (PatternVariant::BoxCornerLU, string_to_pattern_bits("ULUL")),
    (PatternVariant::BoxCornerRD, string_to_pattern_bits("RDRD")),
    (PatternVariant::BoxCornerRD, string_to_pattern_bits("DRDR")),
    (PatternVariant::BoxCornerRU, string_to_pattern_bits("RURU")),
    (PatternVariant::BoxCornerRU, string_to_pattern_bits("URUR")),

    // Towers
    (PatternVariant::TowerLR,       string_to_pattern_bits("LRLRL")),
    (PatternVariant::TowerLR,       string_to_pattern_bits("RLRLR")),
    (PatternVariant::TowerUD,       string_to_pattern_bits("UDUDU")),
    (PatternVariant::TowerUD,       string_to_pattern_bits("DUDUD")),
    (PatternVariant::TowerCornerLD, string_to_pattern_bits("LDLDL")),
    (PatternVariant::TowerCornerLD, string_to_pattern_bits("DLDLD")),
    (PatternVariant::TowerCornerLU, string_to_pattern_bits("LULUL")),
    (PatternVariant::TowerCornerLU, string_to_pattern_bits("ULULU")),
    (PatternVariant::TowerCornerRD, string_to_pattern_bits("RDRDR")),
    (PatternVariant::TowerCornerRD, string_to_pattern_bits("DRDRD")),
    (PatternVariant::TowerCornerRU, string_to_pattern_bits("RURUR")),
    (PatternVariant::TowerCornerRU, string_to_pattern_bits("URURU")),

    // Double staircases
    (PatternVariant::DStaircaseLeft,     string_to_pattern_bits("RDULRDUL")),
    (PatternVariant::DStaircaseRight,    string_to_pattern_bits("LUDRLUDR")),
    (PatternVariant::DStaircaseInvLeft,  string_to_pattern_bits("RDULRDUL")),
    (PatternVariant::DStaircaseInvRight, string_to_pattern_bits("LDURLDUR")),

    // Alternating staircases
    (PatternVariant::AltStaircasesLeft,     string_to_pattern_bits("RDULRUDL")),
    (PatternVariant::AltStaircasesRight,    string_to_pattern_bits("LUDRLDUR")),
    (PatternVariant::AltStaircasesInvLeft,  string_to_pattern_bits("RUDLRDUL")),
    (PatternVariant::AltStaircasesInvRight, string_to_pattern_bits("LDURLUDR")),

    // Luchi
    (PatternVariant::LuchiLeftDU,  string_to_pattern_bits("LDLUL")),
    (PatternVariant::LuchiLeftUD,  string_to_pattern_bits("LULDL")),
    (PatternVariant::LuchiRightUD, string_to_pattern_bits("RURDR")),
    (PatternVariant::LuchiRightDU, string_to_pattern_bits("RDRUR")),

    // Copters
    (PatternVariant::CopterLeft,     string_to_pattern_bits("LDURDULDUR")),
    (PatternVariant::CopterRight,    string_to_pattern_bits("RUDLUDRUDL")),
    (PatternVariant::CopterInvLeft,  string_to_pattern_bits("LUDRUDLUDR")),
    (PatternVariant::CopterInvRight, string_to_pattern_bits("RDULDURDUL")),

    // Hip-Breakers
    (PatternVariant::HipBreakerLeft,     string_to_pattern_bits("LDUDLUDUL")),
    (PatternVariant::HipBreakerRight,    string_to_pattern_bits("RUDURDUDR")),
    (PatternVariant::HipBreakerInvLeft,  string_to_pattern_bits("LUDULDUDL")),
    (PatternVariant::HipBreakerInvRight, string_to_pattern_bits("RDUDRUDUR")),

    // Spirals
    (PatternVariant::SpiralLeft,     string_to_pattern_bits("LDURDR")),
    (PatternVariant::SpiralRight,    string_to_pattern_bits("RUDLUL")),
    (PatternVariant::SpiralInvLeft,  string_to_pattern_bits("LUDRUR")),
    (PatternVariant::SpiralInvRight, string_to_pattern_bits("RDULDL")),

    // Turbo Candle
    (PatternVariant::TurboCandleLeft,     string_to_pattern_bits("LDLUDRUR")),
    (PatternVariant::TurboCandleRight,    string_to_pattern_bits("RURDULDL")),
    (PatternVariant::TurboCandleInvLeft,  string_to_pattern_bits("LULDURDR")),
    (PatternVariant::TurboCandleInvRight, string_to_pattern_bits("RDRUDLUL")),

    // Sweeping Candle
    (PatternVariant::SweepCandleLeft,     string_to_pattern_bits("LDURDRUDL")),
    (PatternVariant::SweepCandleRight,    string_to_pattern_bits("RUDLULDUR")),
    (PatternVariant::SweepCandleInvLeft,  string_to_pattern_bits("LUDRURDUL")),
    (PatternVariant::SweepCandleInvRight, string_to_pattern_bits("RDULDLUDR")),

    // Sideswitches (SS)
    (PatternVariant::SideswitchLeft,        string_to_pattern_bits("LURRD")),
    (PatternVariant::SideswitchRight,       string_to_pattern_bits("RDLLU")),
    (PatternVariant::SideswitchGallopLeft,  string_to_pattern_bits("LURNRD")),
    (PatternVariant::SideswitchGallopRight, string_to_pattern_bits("RDLNLU")),
    ]
});

fn string_to_pattern_bits(p: &str) -> Vec<u8> {
    let mut result = Vec::with_capacity(p.len());
    for c in p.chars() {
        let mask = match c {
            'L' => 0b0001,
            'D' => 0b0010,
            'U' => 0b0100,
            'R' => 0b1000,
            'N' => 0b0000,
            _ => 0b0000,
        };
        result.push(mask);
    }
    result
}

pub fn detect_all_patterns(bitmasks: &[u8]) -> HashMap<PatternVariant, u32> {
    let mut results: HashMap<PatternVariant, u32> = HashMap::new();
    let defs: &[(PatternVariant, Vec<u8>)] = ALL_PATTERNS.as_ref();

    for i in 0..bitmasks.len() {
        for (variant, pat_bits) in defs {
            let plen = pat_bits.len();
            if i + plen <= bitmasks.len() {
                if bitmasks[i..i + plen] == pat_bits[..] {
                    *results.entry(*variant).or_insert(0) += 1;
                }
            }
        }
    }

    results
}

pub fn count_anchors(bitmasks: &[u8]) -> (u32, u32, u32, u32) {
    let mut anchor_left = 0;
    let mut anchor_down = 0;
    let mut anchor_up = 0;
    let mut anchor_right = 0;

    let n = bitmasks.len();
    let mut i = 0;
    while i + 4 < n {
        // anchor if it's pressed at times i, i+2, and i+4
        if (bitmasks[i] & 0b0001) != 0
            && (bitmasks[i + 2] & 0b0001) != 0
            && (bitmasks[i + 4] & 0b0001) != 0
        {
            anchor_left += 1;
        }
        if (bitmasks[i] & 0b0010) != 0
            && (bitmasks[i + 2] & 0b0010) != 0
            && (bitmasks[i + 4] & 0b0010) != 0
        {
            anchor_down += 1;
        }
        if (bitmasks[i] & 0b0100) != 0
            && (bitmasks[i + 2] & 0b0100) != 0
            && (bitmasks[i + 4] & 0b0100) != 0
        {
            anchor_up += 1;
        }
        if (bitmasks[i] & 0b1000) != 0
            && (bitmasks[i + 2] & 0b1000) != 0
            && (bitmasks[i + 4] & 0b1000) != 0
        {
            anchor_right += 1;
        }
        i += 1;
    }

    (anchor_left, anchor_down, anchor_up, anchor_right)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Direction {
    Left,
    Right,
}

#[derive(Debug, Clone, Copy)]
enum FacingState {
    Waiting { count: usize },
    Left    { count: usize },
    Right   { count: usize },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Foot {
    LeftFoot,
    RightFoot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Arrow {
    L, // must use LeftFoot
    R, // must use RightFoot
    U, // can be either foot
    D, // can be either foot
}

/// Convert bitmask to Arrow. This part is the same as you already have.
fn map_bitmask_to_arrow(mask: u8) -> Option<Arrow> {
    // Exactly one bit set => identify which arrow
    // L=0b0001, D=0b0010, U=0b0100, R=0b1000
    match mask {
        0b0001 => Some(Arrow::L),
        0b0010 => Some(Arrow::D),
        0b0100 => Some(Arrow::U),
        0b1000 => Some(Arrow::R),
        _      => None, // either no arrows or multiple arrows => not a single-arrow step
    }
}


/// Returns Some(Left) if (L->U), (D->R), (R->D), (U->L).
/// Returns Some(Right) if (L->D), (U->R), (R->U), (D->L).
/// Returns None if it's neutral (L->R, R->L, U->D, D->U) or doesn't match.
fn transition_direction(a: Arrow, b: Arrow) -> Option<Direction> {
    use Arrow::*;
    use Direction::*;

    match (a, b) {
        (L, U) | (D, R) | (R, D) | (U, L) => Some(Left),
        (L, D) | (U, R) | (R, U) | (D, L) => Some(Right),
        _ => None,
    }
}

/// This function **only** handles a slice of single-arrow steps (i.e. `[Arrow, Arrow, ...]`)
/// and returns `(left_count, right_count)` using your existing logic.
fn count_facing_steps_in_arrows(arrows: &[Arrow]) -> (u32, u32) {
    use Foot::*;
    const MONO_THRESHOLD: usize = 4;

    if arrows.is_empty() {
        return (0, 0);
    }

    let mut final_left = 0u32;
    let mut final_right = 0u32;

    // This is your old FacingState logic
    let mut state = FacingState::Waiting { count: 1 };

    // NEW: track foot usage for each arrow
    // None => not yet assigned
    // Some(LeftFoot) or Some(RightFoot) => assigned
    let mut foot_usage = vec![None; arrows.len()];

    // Helper to finalize the old segment if needed
    fn finalize_segment(
        old_state: &mut FacingState,
        final_left: &mut u32,
        final_right: &mut u32,
    ) {
        match *old_state {
            FacingState::Waiting { count: _ } => {
                // do nothing
            }
            FacingState::Left { count } => {
                if count >= MONO_THRESHOLD {
                    *final_left += count as u32;
                }
            }
            FacingState::Right { count } => {
                if count >= MONO_THRESHOLD {
                    *final_right += count as u32;
                }
            }
        }
        *old_state = FacingState::Waiting { count: 0 }; // start new segment
    }

    // Back-propagate foot assignment from arrow `idx` backward for `pending_count` arrows.
    fn back_propagate_feet(
        foot_usage: &mut [Option<Foot>],
        idx: usize,
        pending_count: usize,
    ) {
        let mut i = idx as i32 - 1;
        while (idx as i32 - i) <= (pending_count as i32) && i >= 0 {
            let prev_foot = foot_usage[(i + 1) as usize].unwrap(); 
            let this_foot = match prev_foot {
                Foot::LeftFoot => Foot::RightFoot,
                Foot::RightFoot => Foot::LeftFoot,
            };
            foot_usage[i as usize] = Some(this_foot);
            i -= 1;
        }
    }

    // Forward-assign foot usage for arrow i, alternating from arrow i-1's foot.
    // Returns Some(true) if conflict => we need a segment break.
    fn forward_assign_foot(
        arrows: &[Arrow],
        foot_usage: &mut [Option<Foot>],
        i: usize,
    ) -> Option<bool> {
        let prev_foot = foot_usage[i - 1].unwrap();
        let next_foot = match prev_foot {
            Foot::LeftFoot => Foot::RightFoot,
            Foot::RightFoot => Foot::LeftFoot,
        };
        match (arrows[i], next_foot) {
            (Arrow::L, Foot::RightFoot) => Some(true), // conflict
            (Arrow::R, Foot::LeftFoot)  => Some(true), // conflict
            _ => {
                foot_usage[i] = Some(next_foot);
                None
            }
        }
    }

    // 1) Handle the very first arrow
    match arrows[0] {
        Arrow::L => foot_usage[0] = Some(LeftFoot),
        Arrow::R => foot_usage[0] = Some(RightFoot),
        Arrow::U | Arrow::D => {
            // remain None => “pending”
        }
    }
    let mut pending_count = if foot_usage[0].is_none() { 1 } else { 0 };

    // 2) Iterate from the second arrow on
    for i in 1..arrows.len() {
        let prev = arrows[i - 1];
        let curr = arrows[i];
        let trans = transition_direction(prev, curr);

        // STEP A: foot assignment logic
        if foot_usage[i - 1].is_none() {
            // The previous arrow was still “pending”.
            if matches!(curr, Arrow::L | Arrow::R) {
                // We have a forced foot => assign foot_usage[i]
                foot_usage[i] = match curr {
                    Arrow::L => Some(Foot::LeftFoot),
                    Arrow::R => Some(Foot::RightFoot),
                    _        => None,
                };
                // back-propagate for pending_count+1
                back_propagate_feet(&mut foot_usage, i, pending_count);
                pending_count = 0;
            } else {
                // still U/D => remain pending
                foot_usage[i] = None;
                pending_count += 1;
            }
        } else {
            // The previous arrow foot is assigned => alternate forward
            if matches!(curr, Arrow::L | Arrow::R) {
                let needed_foot = match curr {
                    Arrow::L => Foot::LeftFoot,
                    Arrow::R => Foot::RightFoot,
                    _ => unreachable!(),
                };
                // The foot we want if we alternate from i-1
                let alt_foot = match foot_usage[i - 1].unwrap() {
                    Foot::LeftFoot => Foot::RightFoot,
                    Foot::RightFoot => Foot::LeftFoot,
                };
                if needed_foot != alt_foot {
                    // conflict => finalize old facing segment
                    finalize_segment(&mut state, &mut final_left, &mut final_right);
                    // now start new => we forcibly assign arrow i:
                    foot_usage[i] = Some(needed_foot);
                } else {
                    foot_usage[i] = Some(alt_foot);
                }
            } else {
                // arrow i is U or D
                if let Some(conflict) = forward_assign_foot(arrows, &mut foot_usage, i) {
                    if conflict {
                        // conflict => finalize old segment
                        finalize_segment(&mut state, &mut final_left, &mut final_right);
                        // treat this arrow as new pending block?
                        foot_usage[i] = None;
                        pending_count = 1;
                    }
                }
            }
        }

        // STEP B: your existing facing direction logic
        match state {
            FacingState::Waiting { count } => {
                match trans {
                    Some(Direction::Left) => {
                        let new_count = count + 1;
                        state = FacingState::Left { count: new_count };
                    }
                    Some(Direction::Right) => {
                        let new_count = count + 1;
                        state = FacingState::Right { count: new_count };
                    }
                    None => {
                        let new_count = count + 1;
                        state = FacingState::Waiting { count: new_count };
                    }
                }
            }
            FacingState::Left { count: seg_count } => {
                match trans {
                    Some(Direction::Left) | None => {
                        state = FacingState::Left { count: seg_count + 1 };
                    }
                    Some(Direction::Right) => {
                        if seg_count >= MONO_THRESHOLD {
                            final_left += seg_count as u32;
                        }
                        state = FacingState::Right { count: 1 };
                    }
                }
            }
            FacingState::Right { count: seg_count } => {
                match trans {
                    Some(Direction::Right) | None => {
                        state = FacingState::Right { count: seg_count + 1 };
                    }
                    Some(Direction::Left) => {
                        if seg_count >= MONO_THRESHOLD {
                            final_right += seg_count as u32;
                        }
                        state = FacingState::Left { count: 1 };
                    }
                }
            }
        }
    }

    // 3) finalize leftover
    match state {
        FacingState::Waiting { count: _ } => {
            // no direction => discard
        }
        FacingState::Left { count } => {
            if count >= MONO_THRESHOLD {
                final_left += count as u32;
            }
        }
        FacingState::Right { count } => {
            if count >= MONO_THRESHOLD {
                final_right += count as u32;
            }
        }
    }

    (final_left, final_right)
}


pub fn count_facing_steps(bitmasks: &[u8]) -> (u32, u32) {
    let mut final_left = 0u32;
    let mut final_right = 0u32;

    // Temporary buffer of consecutive single-arrow lines
    let mut current_arrows = Vec::new();

    // Go line by line
    for &mask in bitmasks {
        if let Some(arrow) = map_bitmask_to_arrow(mask) {
            // Exactly one arrow => accumulate it
            current_arrows.push(arrow);
        } else {
            // "Break" line (0 arrows or multiple arrows)
            // => finalize the chunk so far
            let (l, r) = count_facing_steps_in_arrows(&current_arrows);
            final_left += l;
            final_right += r;
            current_arrows.clear();
        }
    }

    // End of file => finalize leftover arrows
    let (l, r) = count_facing_steps_in_arrows(&current_arrows);
    final_left += l;
    final_right += r;

    (final_left, final_right)
}
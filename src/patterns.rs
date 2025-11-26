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

#[derive(Debug, Clone)]
pub struct CustomPatternSummary {
    pub pattern: String,
    pub count: u32,
}

pub static DEFAULT_PATTERNS: LazyLock<Vec<(PatternVariant, Vec<u8>)>> = LazyLock::new(|| {
    vec![
    // Candles
    (PatternVariant::CandleLeft,  string_to_pattern_bits("ULD")),
    (PatternVariant::CandleLeft,  string_to_pattern_bits("DLU")),
    (PatternVariant::CandleRight, string_to_pattern_bits("URD")),
    (PatternVariant::CandleRight, string_to_pattern_bits("DRU")),

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
    ]
});

pub static EXTRA_PATTERNS: LazyLock<Vec<(PatternVariant, Vec<u8>)>> = LazyLock::new(|| {
    vec![
    //Staircases
    (PatternVariant::StaircaseLeft,     string_to_pattern_bits("LDUR")),
    (PatternVariant::StaircaseRight,    string_to_pattern_bits("RUDL")),
    (PatternVariant::StaircaseInvLeft,  string_to_pattern_bits("LUDR")),
    (PatternVariant::StaircaseInvRight, string_to_pattern_bits("RDUL")),

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

pub fn detect_patterns(
    bitmasks: &[u8],
    patterns: &[(PatternVariant, Vec<u8>)],
) -> HashMap<PatternVariant, u32> {
    let mut results = HashMap::new();
    for i in 0..bitmasks.len() {
        for (variant, pat_bits) in patterns {
            let plen = pat_bits.len();
            if i + plen <= bitmasks.len() && bitmasks[i..i + plen] == pat_bits[..] {
                *results.entry(*variant).or_insert(0) += 1;
            }
        }
    }
    results
}

pub fn detect_custom_patterns(bitmasks: &[u8], patterns: &[String]) -> Vec<CustomPatternSummary> {
    let mut summaries = Vec::new();

    for pattern_str in patterns {
        let upper = pattern_str.to_uppercase();
        let pat_bits = string_to_pattern_bits(&upper);
        let plen = pat_bits.len();
        let mut count = 0u32;

        if plen > 0 && bitmasks.len() >= plen {
            for i in 0..=bitmasks.len() - plen {
                if bitmasks[i..i + plen] == pat_bits[..] {
                    count += 1;
                }
            }
        }

        summaries.push(CustomPatternSummary {
            pattern: upper,
            count,
        });
    }

    summaries
}

#[inline]
fn count_anchors_for_bit(bitmasks: &[u8], bit_mask: u8) -> u32 {
    bitmasks.iter()
        .zip(&bitmasks[2..])
        .zip(&bitmasks[4..])
        .filter(|((a, b), c)| {
            (*a & bit_mask) != 0 && (*b & bit_mask) != 0 && (*c & bit_mask) != 0
        })
        .count() as u32
}

pub fn count_anchors(bitmasks: &[u8]) -> (u32, u32, u32, u32) {
    let anchor_left = count_anchors_for_bit(bitmasks, 0b0001);
    let anchor_down = count_anchors_for_bit(bitmasks, 0b0010);
    let anchor_up = count_anchors_for_bit(bitmasks, 0b0100);
    let anchor_right = count_anchors_for_bit(bitmasks, 0b1000);
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
    Left { count: usize },
    Right { count: usize },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Foot {
    LeftFoot,
    RightFoot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Arrow {
    L, // Must use LeftFoot
    R, // Must use RightFoot
    U, // Can be either foot
    D, // Can be either foot
}

const fn map_bitmask_to_arrow(mask: u8) -> Option<Arrow> {
    match mask {
        0b0001 => Some(Arrow::L),
        0b0010 => Some(Arrow::D),
        0b0100 => Some(Arrow::U),
        0b1000 => Some(Arrow::R),
        _ => None,
    }
}

// Add this function if it’s missing
fn determine_direction(prev: Arrow, curr: Arrow) -> Option<Direction> {
    use Arrow::*;
    use Direction::*;
    match (prev, curr) {
        (L, U) | (D, R) | (R, D) | (U, L) => Some(Left),
        (L, D) | (U, R) | (R, U) | (D, L) => Some(Right),
        _ => None,
    }
}

// Helper function to determine the opposite foot
fn opposite_foot(f: Foot) -> Foot {
    match f {
        Foot::LeftFoot => Foot::RightFoot,
        Foot::RightFoot => Foot::LeftFoot,
    }
}

// Updates the facing state based on direction
fn update_facing_state(
    state: FacingState,
    direction: Option<Direction>,
    final_left: &mut u32,
    final_right: &mut u32,
    mono_threshold: usize,
) -> FacingState {
    match state {
        FacingState::Waiting { count } => match direction {
            Some(Direction::Left) => FacingState::Left { count: count + 1 },
            Some(Direction::Right) => FacingState::Right { count: count + 1 },
            None => FacingState::Waiting { count: count + 1 },
        },
        FacingState::Left { count } => match direction {
            Some(Direction::Left) | None => FacingState::Left { count: count + 1 },
            Some(Direction::Right) => {
                if count >= mono_threshold {
                    *final_left += count as u32;
                }
                FacingState::Right { count: 1 }
            }
        },
        FacingState::Right { count } => match direction {
            Some(Direction::Right) | None => FacingState::Right { count: count + 1 },
            Some(Direction::Left) => {
                if count >= mono_threshold {
                    *final_right += count as u32;
                }
                FacingState::Left { count: 1 }
            }
        },
    }
}

// Finalizes the current segment
fn finalize_segment(
    state: &mut FacingState,
    final_left: &mut u32,
    final_right: &mut u32,
    mono_threshold: usize,
) {
    match *state {
        FacingState::Left { count } if count >= mono_threshold => *final_left += count as u32,
        FacingState::Right { count } if count >= mono_threshold => *final_right += count as u32,
        _ => {}
    }
    *state = FacingState::Waiting { count: 0 };
}

// Handles foot assignment when previous foot is None
fn handle_prev_none(
    i: usize,
    curr_arrow: Arrow,
    foot_usage: &mut [Option<Foot>],
    pending_count: &mut usize,
) {
    if matches!(curr_arrow, Arrow::L | Arrow::R) {
        let forced_foot = match curr_arrow {
            Arrow::L => Foot::LeftFoot,
            Arrow::R => Foot::RightFoot,
            _ => unreachable!(),
        };
        foot_usage[i] = Some(forced_foot);
        let start = i - *pending_count;
        let mut current_foot = forced_foot;
        for j in (start..i).rev() {
            current_foot = opposite_foot(current_foot);
            foot_usage[j] = Some(current_foot);
        }
        *pending_count = 0;
    } else {
        foot_usage[i] = None;
        *pending_count += 1;
    }
}

// Handles foot assignment when previous foot is Some
fn handle_prev_some(
    i: usize,
    prev_foot: Foot,
    curr_arrow: Arrow,
    foot_usage: &mut [Option<Foot>],
    pending_count: &mut usize,
    state: &mut FacingState,
    final_left: &mut u32,
    final_right: &mut u32,
    mono_threshold: usize,
) {
    if matches!(curr_arrow, Arrow::L | Arrow::R) {
        let forced_foot = match curr_arrow {
            Arrow::L => Foot::LeftFoot,
            Arrow::R => Foot::RightFoot,
            _ => unreachable!(),
        };
        let alt_foot = opposite_foot(prev_foot);
        if forced_foot != alt_foot {
            finalize_segment(state, final_left, final_right, mono_threshold);
            foot_usage[i] = Some(forced_foot);
        } else {
            foot_usage[i] = Some(alt_foot);
        }
    } else {
        let next_foot = opposite_foot(prev_foot);
        let conflict = matches!(
            (curr_arrow, next_foot),
            (Arrow::L, Foot::RightFoot) | (Arrow::R, Foot::LeftFoot)
        );
        if conflict {
            finalize_segment(state, final_left, final_right, mono_threshold);
            foot_usage[i] = None;
            *pending_count = 1;
        } else {
            foot_usage[i] = Some(next_foot);
        }
    }
}

// Processes each step in the arrow sequence
fn process_step(
    i: usize,
    arrows: &[Arrow],
    foot_usage: &mut [Option<Foot>],
    pending_count: &mut usize,
    state: &mut FacingState,
    final_left: &mut u32,
    final_right: &mut u32,
    mono_threshold: usize,
) {
    let prev_arrow = arrows[i - 1];
    let curr_arrow = arrows[i];
    let direction = determine_direction(prev_arrow, curr_arrow);

    match foot_usage[i - 1] {
        None => handle_prev_none(i, curr_arrow, foot_usage, pending_count),
        Some(prev_foot) => handle_prev_some(
            i,
            prev_foot,
            curr_arrow,
            foot_usage,
            pending_count,
            state,
            final_left,
            final_right,
            mono_threshold,
        ),
    }

    *state = update_facing_state(*state, direction, final_left, final_right, mono_threshold);
}

// Refactored main function
fn count_facing_steps_in_arrows(arrows: &[Arrow], mono_threshold: usize) -> (u32, u32) {
    if arrows.is_empty() {
        return (0, 0);
    }

    let mut final_left = 0_u32;
    let mut final_right = 0_u32;
    let mut state = FacingState::Waiting { count: 1 };
    let mut foot_usage = vec![None; arrows.len()];
    let mut pending_count = 0;

    foot_usage[0] = match arrows[0] {
        Arrow::L => Some(Foot::LeftFoot),
        Arrow::R => Some(Foot::RightFoot),
        _ => None,
    };

    for i in 1..arrows.len() {
        process_step(
            i,
            arrows,
            &mut foot_usage,
            &mut pending_count,
            &mut state,
            &mut final_left,
            &mut final_right,
            mono_threshold,
        );
    }

    finalize_segment(&mut state, &mut final_left, &mut final_right, mono_threshold);
    (final_left, final_right)
}

pub fn count_facing_steps(bitmasks: &[u8], mono_threshold: usize) -> (u32, u32) {
    let mut final_left = 0_u32;
    let mut final_right = 0_u32;
    let mut current_arrows = Vec::new();

    for &mask in bitmasks {
        if let Some(arrow) = map_bitmask_to_arrow(mask) {
            current_arrows.push(arrow);
        } else {
            let (l, r) = count_facing_steps_in_arrows(&current_arrows, mono_threshold);
            final_left += l;
            final_right += r;
            current_arrows.clear();
        }
    }

    let (l, r) = count_facing_steps_in_arrows(&current_arrows, mono_threshold);
    final_left += l;
    final_right += r;
    (final_left, final_right)
}

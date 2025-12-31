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
    (PatternVariant::DStaircaseLeft,     string_to_pattern_bits("LUDRLUDR")),
    (PatternVariant::DStaircaseRight,    string_to_pattern_bits("RDULRDUL")),
    (PatternVariant::DStaircaseInvLeft,  string_to_pattern_bits("LDURLDUR")),
    (PatternVariant::DStaircaseInvRight, string_to_pattern_bits("RDULRDUL")),

    // Alternating staircases
    (PatternVariant::AltStaircasesLeft,     string_to_pattern_bits("LUDRLDUR")),
    (PatternVariant::AltStaircasesRight,    string_to_pattern_bits("RDULRUDL")),
    (PatternVariant::AltStaircasesInvLeft,  string_to_pattern_bits("LDURLUDR")),
    (PatternVariant::AltStaircasesInvRight, string_to_pattern_bits("RUDLRDUL")),

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

pub static ALL_PATTERNS: LazyLock<Vec<(PatternVariant, Vec<u8>)>> = LazyLock::new(|| {
    let mut patterns = Vec::with_capacity(DEFAULT_PATTERNS.len() + EXTRA_PATTERNS.len());
    patterns.extend(DEFAULT_PATTERNS.iter().cloned());
    patterns.extend(EXTRA_PATTERNS.iter().cloned());
    patterns
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

pub fn count_anchors(bitmasks: &[u8]) -> (u32, u32, u32, u32) {
    let mut anchor_left = 0u32;
    let mut anchor_down = 0u32;
    let mut anchor_up = 0u32;
    let mut anchor_right = 0u32;

    let limit = bitmasks.len().saturating_sub(4);
    for i in 0..limit {
        let mask = bitmasks[i] & bitmasks[i + 2] & bitmasks[i + 4];
        if (mask & 0b0001) != 0 {
            anchor_left += 1;
        }
        if (mask & 0b0010) != 0 {
            anchor_down += 1;
        }
        if (mask & 0b0100) != 0 {
            anchor_up += 1;
        }
        if (mask & 0b1000) != 0 {
            anchor_right += 1;
        }
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

#[inline(always)]
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
#[inline(always)]
const fn determine_direction(prev: Arrow, curr: Arrow) -> Option<Direction> {
    use Arrow::*;
    use Direction::*;
    match (prev, curr) {
        (L, U) | (D, R) | (R, D) | (U, L) => Some(Left),
        (L, D) | (U, R) | (R, U) | (D, L) => Some(Right),
        _ => None,
    }
}

// Helper function to determine the opposite foot
#[inline(always)]
const fn opposite_foot(f: Foot) -> Foot {
    match f {
        Foot::LeftFoot => Foot::RightFoot,
        Foot::RightFoot => Foot::LeftFoot,
    }
}

#[inline(always)]
const fn forced_foot(arrow: Arrow) -> Option<Foot> {
    match arrow {
        Arrow::L => Some(Foot::LeftFoot),
        Arrow::R => Some(Foot::RightFoot),
        _ => None,
    }
}

#[inline(always)]
fn next_foot(prev_foot: Option<Foot>, curr_arrow: Arrow) -> (Option<Foot>, bool) {
    let Some(prev) = prev_foot else {
        return (forced_foot(curr_arrow), false);
    };

    if let Some(forced) = forced_foot(curr_arrow) {
        let expected = opposite_foot(prev);
        let conflict = forced != expected;
        (Some(forced), conflict)
    } else {
        (Some(opposite_foot(prev)), false)
    }
}

// Updates the facing state based on direction
#[inline(always)]
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
#[inline(always)]
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

pub fn count_facing_steps(bitmasks: &[u8], mono_threshold: usize) -> (u32, u32) {
    let mut final_left = 0_u32;
    let mut final_right = 0_u32;
    let mut state = FacingState::Waiting { count: 0 };
    let mut prev_arrow: Option<Arrow> = None;
    let mut prev_foot: Option<Foot> = None;

    for &mask in bitmasks {
        let Some(curr_arrow) = map_bitmask_to_arrow(mask) else {
            if prev_arrow.is_some() {
                finalize_segment(&mut state, &mut final_left, &mut final_right, mono_threshold);
                prev_arrow = None;
                prev_foot = None;
            }
            continue;
        };

        let Some(prev_arrow_value) = prev_arrow else {
            state = FacingState::Waiting { count: 1 };
            prev_foot = forced_foot(curr_arrow);
            prev_arrow = Some(curr_arrow);
            continue;
        };

        let direction = determine_direction(prev_arrow_value, curr_arrow);
        let (next_foot, should_finalize) = next_foot(prev_foot, curr_arrow);
        if should_finalize {
            finalize_segment(&mut state, &mut final_left, &mut final_right, mono_threshold);
        }
        prev_foot = next_foot;
        state = update_facing_state(state, direction, &mut final_left, &mut final_right, mono_threshold);
        prev_arrow = Some(curr_arrow);
    }

    if prev_arrow.is_some() {
        finalize_segment(&mut state, &mut final_left, &mut final_right, mono_threshold);
    }
    (final_left, final_right)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BoxCounts {
    pub total_boxes: u32,
    pub lr_boxes: u32,
    pub ud_boxes: u32,
    pub corner_boxes: u32,
    pub ld_boxes: u32,
    pub lu_boxes: u32,
    pub rd_boxes: u32,
    pub ru_boxes: u32,
}

#[inline(always)]
pub fn count_pattern(map: &HashMap<PatternVariant, u32>, variant: PatternVariant) -> u32 {
    *map.get(&variant).unwrap_or(&0)
}

pub fn compute_box_counts(map: &HashMap<PatternVariant, u32>) -> BoxCounts {
    let lr = count_pattern(map, PatternVariant::BoxLR);
    let ud = count_pattern(map, PatternVariant::BoxUD);
    let ld = count_pattern(map, PatternVariant::BoxCornerLD);
    let lu = count_pattern(map, PatternVariant::BoxCornerLU);
    let rd = count_pattern(map, PatternVariant::BoxCornerRD);
    let ru = count_pattern(map, PatternVariant::BoxCornerRU);
    let corner = ld + lu + rd + ru;
    let total = lr + ud + corner;

    BoxCounts {
        total_boxes: total,
        lr_boxes: lr,
        ud_boxes: ud,
        corner_boxes: corner,
        ld_boxes: ld,
        lu_boxes: lu,
        rd_boxes: rd,
        ru_boxes: ru,
    }
}

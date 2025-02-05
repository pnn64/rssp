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

/// Minimal type representing a single arrow.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Arrow {
    L, // forced left (must use left foot)
    R, // forced right (must use right foot)
    U, // ambiguous (up)
    D, // ambiguous (down)
}

/// Minimal type for indicating which foot is used.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Foot {
    Left,
    Right,
}

/// Convert a bitmask (with exactly one bit set) into an Arrow.  
/// If the mask does not represent exactly one arrow, return `None`.
pub fn map_bitmask_to_arrow(mask: u8) -> Option<Arrow> {
    match mask {
        0b0001 => Some(Arrow::L),
        0b0010 => Some(Arrow::D),
        0b0100 => Some(Arrow::U),
        0b1000 => Some(Arrow::R),
        _      => None,
    }
}

/// Returns a “transition” value between two consecutive arrows according to these rules:
///
/// - (L → U), (D → R), (R → D), (U → L) yield –1 (a left–facing transition)
/// - (L → D), (U → R), (R → U), (D → L) yield  1 (a right–facing transition)
/// - All other pairs yield no transition (i.e. return `None`)
fn transition_direction(a: Arrow, b: Arrow) -> Option<i8> {
    match (a, b) {
        (Arrow::L, Arrow::U) | (Arrow::D, Arrow::R)
        | (Arrow::R, Arrow::D) | (Arrow::U, Arrow::L) => Some(-1),
        (Arrow::L, Arrow::D) | (Arrow::U, Arrow::R)
        | (Arrow::R, Arrow::U) | (Arrow::D, Arrow::L) => Some(1),
        _ => None,
    }
}

/// The minimum group length for a facing segment to be counted.
const MONO_THRESHOLD: usize = 4;

/// Internal state used to track consecutive facing–steps.
/// (Note: the variants are named so they do not conflict with `Foot::Left` and `Foot::Right`.)
#[derive(Debug, Clone, Copy)]
enum FacingState {
    Waiting(usize),   // no direction established yet; holds the count of steps so far
    FaceLeft(usize),  // current segment is left–facing
    FaceRight(usize), // current segment is right–facing
}

/// Process a slice of single–arrow steps (already converted from your bitmask lines)
/// and return the (left_facing, right_facing) counts.  
///
/// This function implements the foot–assignment logic: forced arrows (L/R) must use
/// the proper foot, and ambiguous arrows (U/D) are assigned by alternating from the previous
/// assigned foot. When a conflict is detected the current facing segment is “flushed.”
pub fn count_facing_steps_in_arrows(arrows: &[Arrow]) -> (u32, u32) {
    if arrows.is_empty() {
        return (0, 0);
    }

    // Initialize the facing state and counters.
    let mut state = FacingState::Waiting(1);
    let mut final_left = 0u32;
    let mut final_right = 0u32;
    // For each arrow, track an optional foot assignment.
    let mut foot_usage: Vec<Option<Foot>> = vec![None; arrows.len()];

    // Helper closure: back–propagate a forced foot assignment backwards over ambiguous arrows.
    let back_propagate = |foot_usage: &mut [Option<Foot>], i: usize, pending: usize| {
        let mut j = i as i32 - 1;
        while (i as i32 - j) <= pending as i32 && j >= 0 {
            if let Some(prev_foot) = foot_usage[(j + 1) as usize] {
                foot_usage[j as usize] = Some(match prev_foot {
                    Foot::Left => Foot::Right,
                    Foot::Right => Foot::Left,
                });
            }
            j -= 1;
        }
    };

    // Helper closure: forward–assign the foot for an ambiguous arrow.
    // Returns Some(true) if a forced conflict is detected.
    let forward_assign = |arrows: &[Arrow], foot_usage: &mut [Option<Foot>], i: usize| -> Option<bool> {
        if let Some(prev) = foot_usage[i - 1] {
            let next = match prev {
                Foot::Left => Foot::Right,
                Foot::Right => Foot::Left,
            };
            match (arrows[i], next) {
                (Arrow::L, Foot::Right) | (Arrow::R, Foot::Left) => Some(true),
                _ => {
                    foot_usage[i] = Some(next);
                    None
                }
            }
        } else {
            None
        }
    };

    // 1) Initialize the first arrow.
    match arrows[0] {
        Arrow::L => foot_usage[0] = Some(Foot::Left),
        Arrow::R => foot_usage[0] = Some(Foot::Right),
        _ => {}  // Ambiguous arrows remain unassigned.
    }
    let mut pending = if foot_usage[0].is_none() { 1 } else { 0 };

    // 2) Process each subsequent arrow.
    for i in 1..arrows.len() {
        let prev = arrows[i - 1];
        let curr = arrows[i];
        let trans = transition_direction(prev, curr);

        // --- Foot Assignment Logic ---
        if foot_usage[i - 1].is_none() {
            // Previous arrow was ambiguous.
            if matches!(curr, Arrow::L | Arrow::R) {
                // Forced arrow: assign its foot and back–propagate.
                foot_usage[i] = Some(match curr {
                    Arrow::L => Foot::Left,
                    Arrow::R => Foot::Right,
                    _ => unreachable!(),
                });
                back_propagate(&mut foot_usage, i, pending + 1);
                pending = 0;
            } else {
                // Still ambiguous.
                foot_usage[i] = None;
                pending += 1;
            }
        } else {
            // Previous arrow has an assigned foot.
            if matches!(curr, Arrow::L | Arrow::R) {
                let forced = match curr {
                    Arrow::L => Foot::Left,
                    Arrow::R => Foot::Right,
                    _ => unreachable!(),
                };
                let alt = match foot_usage[i - 1].unwrap() {
                    Foot::Left => Foot::Right,
                    Foot::Right => Foot::Left,
                };
                if forced != alt {
                    // Forced conflict: flush the current facing segment.
                    match state {
                        FacingState::Waiting(cnt) => {},
                        FacingState::FaceLeft(cnt) if cnt >= MONO_THRESHOLD => { final_left += cnt as u32; },
                        FacingState::FaceRight(cnt) if cnt >= MONO_THRESHOLD => { final_right += cnt as u32; },
                        _ => {}
                    }
                    state = FacingState::Waiting(0);
                    foot_usage[i] = Some(forced);
                } else {
                    foot_usage[i] = Some(alt);
                }
            } else {
                // Ambiguous arrow: try to forward–assign.
                if let Some(conflict) = forward_assign(arrows, &mut foot_usage, i) {
                    if conflict {
                        match state {
                            FacingState::Waiting(cnt) => {},
                            FacingState::FaceLeft(cnt) if cnt >= MONO_THRESHOLD => { final_left += cnt as u32; },
                            FacingState::FaceRight(cnt) if cnt >= MONO_THRESHOLD => { final_right += cnt as u32; },
                            _ => {}
                        }
                        state = FacingState::Waiting(0);
                        foot_usage[i] = None;
                        pending = 1;
                    }
                }
            }
        }
        // --- End Foot Assignment ---

        // --- Facing Direction Logic ---
        state = match state {
            FacingState::Waiting(cnt) => match trans {
                Some(-1) => FacingState::FaceLeft(cnt + 1),
                Some(1)  => FacingState::FaceRight(cnt + 1),
                _        => FacingState::Waiting(cnt + 1),
            },
            FacingState::FaceLeft(cnt) => match trans {
                Some(1) => {
                    if cnt >= MONO_THRESHOLD { final_left += cnt as u32; }
                    FacingState::FaceRight(1)
                },
                _ => FacingState::FaceLeft(cnt + 1),
            },
            FacingState::FaceRight(cnt) => match trans {
                Some(-1) => {
                    if cnt >= MONO_THRESHOLD { final_right += cnt as u32; }
                    FacingState::FaceLeft(1)
                },
                _ => FacingState::FaceRight(cnt + 1),
            },
        };
        // --- End Facing Logic ---
    }

    // 3) Flush any leftover facing segment.
    match state {
        FacingState::FaceLeft(cnt) if cnt >= MONO_THRESHOLD => final_left += cnt as u32,
        FacingState::FaceRight(cnt) if cnt >= MONO_THRESHOLD => final_right += cnt as u32,
        _ => {}
    }

    (final_left, final_right)
}

/// Public function to process a slice of bitmasks (one per “line”).  
/// This function splits the input at break lines (i.e. lines that do not represent a single arrow),
/// converts each block into `Arrow` values, and then accumulates the facing–step counts.
pub fn count_facing_steps(bitmasks: &[u8]) -> (u32, u32) {
    let mut total_left = 0;
    let mut total_right = 0;
    let mut current_arrows = Vec::new();

    for &mask in bitmasks {
        if let Some(a) = map_bitmask_to_arrow(mask) {
            current_arrows.push(a);
        } else {
            let (l, r) = count_facing_steps_in_arrows(&current_arrows);
            total_left += l;
            total_right += r;
            current_arrows.clear();
        }
    }
    let (l, r) = count_facing_steps_in_arrows(&current_arrows);
    total_left += l;
    total_right += r;
    (total_left, total_right)
}

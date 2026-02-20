use std::collections::{HashSet, VecDeque};
use std::sync::LazyLock;

// ============================================================================
// Pattern Variant Enum
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(usize)]
pub enum PatternVariant {
    AltStaircasesLeft = 0,
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

pub const PATTERN_COUNT: usize = 62;
pub type PatternCounts = [u32; PATTERN_COUNT];

// ============================================================================
// Summary Types
// ============================================================================

#[derive(Debug, Clone)]
pub struct CustomPatternSummary {
    pub pattern: String,
    pub count: u32,
}

#[derive(Debug, Clone)]
pub(crate) struct CompiledPattern {
    pattern: String,
    bits: Vec<u8>,
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

// ============================================================================
// Aho-Corasick Core Implementation
// ============================================================================

const AC_ALPHA: usize = 16;

#[derive(Debug, Clone)]
pub(crate) struct AcDfa<T> {
    goto: Vec<u32>,
    output_starts: Vec<u32>,
    output_lens: Vec<u32>,
    flat_outputs: Vec<T>,
}

#[inline(always)]
fn ac_output_slice<T>(dfa: &AcDfa<T>, state: u32) -> &[T] {
    let idx = state as usize;
    let start = dfa.output_starts[idx] as usize;
    let len = dfa.output_lens[idx] as usize;
    &dfa.flat_outputs[start..start + len]
}

fn ac_build<T: Copy>(patterns: &[(T, &[u8])]) -> AcDfa<T> {
    let mut goto: Vec<[u32; AC_ALPHA]> = vec![[u32::MAX; AC_ALPHA]];
    let mut output: Vec<Vec<T>> = vec![vec![]];

    for &(id, pat) in patterns {
        if pat.is_empty() {
            continue;
        }
        let mut state = 0usize;
        for &b in pat {
            let sym = (b & 0x0F) as usize;
            if goto[state][sym] == u32::MAX {
                goto[state][sym] = goto.len() as u32;
                goto.push([u32::MAX; AC_ALPHA]);
                output.push(vec![]);
            }
            state = goto[state][sym] as usize;
        }
        output[state].push(id);
    }

    let n = goto.len();
    if n == 1 {
        return AcDfa {
            goto: vec![0; AC_ALPHA],
            output_starts: vec![0],
            output_lens: vec![0],
            flat_outputs: Vec::new(),
        };
    }

    let mut fail = vec![0u32; n];
    let mut queue: VecDeque<usize> = (0..AC_ALPHA)
        .filter_map(|s| {
            let next = goto[0][s];
            (next != u32::MAX).then_some(next as usize)
        })
        .collect();

    while let Some(state) = queue.pop_front() {
        for (sym, &child) in goto[state].iter().enumerate() {
            if child == u32::MAX {
                continue;
            }
            let child_idx = child as usize;
            queue.push_back(child_idx);

            let mut f = fail[state] as usize;
            while f != 0 && goto[f][sym] == u32::MAX {
                f = fail[f] as usize;
            }

            let fail_target = match goto[f][sym] {
                t if t != u32::MAX && t as usize != child_idx => t,
                _ => 0,
            };
            fail[child_idx] = fail_target;

            if fail_target != 0 {
                let ft = fail_target as usize;

                if child_idx != ft {
                    let (dst, src) = if child_idx < ft {
                        let (l, r) = output.split_at_mut(ft);
                        (&mut l[child_idx], &r[0])
                    } else {
                        let (l, r) = output.split_at_mut(child_idx);
                        (&mut r[0], &l[ft])
                    };

                    dst.extend_from_slice(src);
                }
            }
        }
    }

    for state in 0..n {
        let mut row = goto[state];

        for sym in 0..AC_ALPHA {
            if row[sym] != u32::MAX {
                continue;
            }

            let mut f = state;
            while f != 0 && goto[f][sym] == u32::MAX {
                f = fail[f] as usize;
            }

            let t = goto[f][sym];
            row[sym] = if t == u32::MAX { 0 } else { t };
        }

        goto[state] = row;
    }

    let mut flat_goto = Vec::with_capacity(goto.len() * AC_ALPHA);
    for row in &goto {
        flat_goto.extend_from_slice(row);
    }

    let output_count: usize = output.iter().map(Vec::len).sum();
    let mut output_starts = Vec::with_capacity(output.len());
    let mut output_lens = Vec::with_capacity(output.len());
    let mut flat_outputs = Vec::with_capacity(output_count);
    for state_output in output {
        output_starts.push(flat_outputs.len() as u32);
        output_lens.push(state_output.len() as u32);
        flat_outputs.extend_from_slice(&state_output);
    }

    AcDfa {
        goto: flat_goto,
        output_starts,
        output_lens,
        flat_outputs,
    }
}

/// Specialized search returning a fixed-size array for `PatternVariant`
#[inline]
fn ac_search_array(text: &[u8], dfa: &AcDfa<PatternVariant>) -> PatternCounts {
    let mut counts = [0u32; PATTERN_COUNT];
    let mut state = 0u32;

    for &b in text {
        let sym = (b & 0x0F) as usize;
        state = dfa.goto[state as usize * AC_ALPHA + sym];

        for &id in ac_output_slice(dfa, state) {
            counts[id as usize] += 1;
        }
    }

    counts
}

/// Specialized search returning a compact vector for contiguous usize IDs.
#[inline]
fn ac_search_vec(text: &[u8], dfa: &AcDfa<usize>, count: usize) -> Vec<u32> {
    let mut counts = vec![0u32; count];
    let mut state = 0u32;

    for &b in text {
        let sym = (b & 0x0F) as usize;
        state = dfa.goto[state as usize * AC_ALPHA + sym];
        for &id in ac_output_slice(dfa, state) {
            counts[id] += 1;
        }
    }

    counts
}

fn ac_empty<T>() -> AcDfa<T> {
    AcDfa {
        goto: vec![0; AC_ALPHA],
        output_starts: vec![0],
        output_lens: vec![0],
        flat_outputs: Vec::new(),
    }
}

// ============================================================================
// Pattern Conversion
// ============================================================================

fn string_to_pattern_bits(p: &str) -> Vec<u8> {
    p.chars()
        .map(|c| match c {
            'L' => 0b0001,
            'D' => 0b0010,
            'U' => 0b0100,
            'R' => 0b1000,
            _ => 0b0000,
        })
        .collect()
}

// ============================================================================
// Static Pattern Definitions
// ============================================================================

pub static DEFAULT_PATTERNS: LazyLock<Vec<(PatternVariant, Vec<u8>)>> = LazyLock::new(|| {
    vec![
        // Candles
        (PatternVariant::CandleLeft, string_to_pattern_bits("ULD")),
        (PatternVariant::CandleLeft, string_to_pattern_bits("DLU")),
        (PatternVariant::CandleRight, string_to_pattern_bits("URD")),
        (PatternVariant::CandleRight, string_to_pattern_bits("DRU")),
        // Boxes
        (PatternVariant::BoxLR, string_to_pattern_bits("LRLR")),
        (PatternVariant::BoxLR, string_to_pattern_bits("RLRL")),
        (PatternVariant::BoxUD, string_to_pattern_bits("UDUD")),
        (PatternVariant::BoxUD, string_to_pattern_bits("DUDU")),
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
        // Staircases
        (PatternVariant::StaircaseLeft, string_to_pattern_bits("LDUR")),
        (PatternVariant::StaircaseRight, string_to_pattern_bits("RUDL")),
        (PatternVariant::StaircaseInvLeft, string_to_pattern_bits("LUDR")),
        (PatternVariant::StaircaseInvRight, string_to_pattern_bits("RDUL")),
        // Triangles
        (PatternVariant::TriangleRUR, string_to_pattern_bits("RUR")),
        (PatternVariant::TriangleLUL, string_to_pattern_bits("LUL")),
        (PatternVariant::TriangleLDL, string_to_pattern_bits("LDL")),
        (PatternVariant::TriangleRDR, string_to_pattern_bits("RDR")),
        // Doritos
        (PatternVariant::DoritoLeft, string_to_pattern_bits("LDUDL")),
        (PatternVariant::DoritoRight, string_to_pattern_bits("RUDUR")),
        (PatternVariant::DoritoInvLeft, string_to_pattern_bits("LUDUL")),
        (PatternVariant::DoritoInvRight, string_to_pattern_bits("RDUDR")),
        // Sweeps
        (PatternVariant::SweepLeft, string_to_pattern_bits("LDURUDL")),
        (PatternVariant::SweepRight, string_to_pattern_bits("RUDLDUR")),
        (PatternVariant::SweepInvLeft, string_to_pattern_bits("LUDRDUL")),
        (PatternVariant::SweepInvRight, string_to_pattern_bits("RDULUDR")),
        // Towers
        (PatternVariant::TowerLR, string_to_pattern_bits("LRLRL")),
        (PatternVariant::TowerLR, string_to_pattern_bits("RLRLR")),
        (PatternVariant::TowerUD, string_to_pattern_bits("UDUDU")),
        (PatternVariant::TowerUD, string_to_pattern_bits("DUDUD")),
        (PatternVariant::TowerCornerLD, string_to_pattern_bits("LDLDL")),
        (PatternVariant::TowerCornerLD, string_to_pattern_bits("DLDLD")),
        (PatternVariant::TowerCornerLU, string_to_pattern_bits("LULUL")),
        (PatternVariant::TowerCornerLU, string_to_pattern_bits("ULULU")),
        (PatternVariant::TowerCornerRD, string_to_pattern_bits("RDRDR")),
        (PatternVariant::TowerCornerRD, string_to_pattern_bits("DRDRD")),
        (PatternVariant::TowerCornerRU, string_to_pattern_bits("RURUR")),
        (PatternVariant::TowerCornerRU, string_to_pattern_bits("URURU")),
        // Double staircases
        (PatternVariant::DStaircaseLeft, string_to_pattern_bits("LUDRLUDR")),
        (PatternVariant::DStaircaseRight, string_to_pattern_bits("RDULRDUL")),
        (PatternVariant::DStaircaseInvLeft, string_to_pattern_bits("LDURLDUR")),
        (PatternVariant::DStaircaseInvRight, string_to_pattern_bits("RDULRDUL")),
        // Alternating staircases
        (PatternVariant::AltStaircasesLeft, string_to_pattern_bits("LUDRLDUR")),
        (PatternVariant::AltStaircasesRight, string_to_pattern_bits("RDULRUDL")),
        (PatternVariant::AltStaircasesInvLeft, string_to_pattern_bits("LDURLUDR")),
        (PatternVariant::AltStaircasesInvRight, string_to_pattern_bits("RUDLRDUL")),
        // Luchi
        (PatternVariant::LuchiLeftDU, string_to_pattern_bits("LDLUL")),
        (PatternVariant::LuchiLeftUD, string_to_pattern_bits("LULDL")),
        (PatternVariant::LuchiRightUD, string_to_pattern_bits("RURDR")),
        (PatternVariant::LuchiRightDU, string_to_pattern_bits("RDRUR")),
        // Copters
        (PatternVariant::CopterLeft, string_to_pattern_bits("LDURDULDUR")),
        (PatternVariant::CopterRight, string_to_pattern_bits("RUDLUDRUDL")),
        (PatternVariant::CopterInvLeft, string_to_pattern_bits("LUDRUDLUDR")),
        (PatternVariant::CopterInvRight, string_to_pattern_bits("RDULDURDUL")),
        // Hip-Breakers
        (PatternVariant::HipBreakerLeft, string_to_pattern_bits("LDUDLUDUL")),
        (PatternVariant::HipBreakerRight, string_to_pattern_bits("RUDURDUDR")),
        (PatternVariant::HipBreakerInvLeft, string_to_pattern_bits("LUDULDUDL")),
        (PatternVariant::HipBreakerInvRight, string_to_pattern_bits("RDUDRUDUR")),
        // Spirals
        (PatternVariant::SpiralLeft, string_to_pattern_bits("LDURDR")),
        (PatternVariant::SpiralRight, string_to_pattern_bits("RUDLUL")),
        (PatternVariant::SpiralInvLeft, string_to_pattern_bits("LUDRUR")),
        (PatternVariant::SpiralInvRight, string_to_pattern_bits("RDULDL")),
        // Turbo Candle
        (PatternVariant::TurboCandleLeft, string_to_pattern_bits("LDLUDRUR")),
        (PatternVariant::TurboCandleRight, string_to_pattern_bits("RURDULDL")),
        (PatternVariant::TurboCandleInvLeft, string_to_pattern_bits("LULDURDR")),
        (PatternVariant::TurboCandleInvRight, string_to_pattern_bits("RDRUDLUL")),
        // Sweeping Candle
        (PatternVariant::SweepCandleLeft, string_to_pattern_bits("LDURDRUDL")),
        (PatternVariant::SweepCandleRight, string_to_pattern_bits("RUDLULDUR")),
        (PatternVariant::SweepCandleInvLeft, string_to_pattern_bits("LUDRURDUL")),
        (PatternVariant::SweepCandleInvRight, string_to_pattern_bits("RDULDLUDR")),
    ]
});

pub static ALL_PATTERNS: LazyLock<Vec<(PatternVariant, Vec<u8>)>> = LazyLock::new(|| {
    let mut patterns = Vec::with_capacity(DEFAULT_PATTERNS.len() + EXTRA_PATTERNS.len());
    patterns.extend(DEFAULT_PATTERNS.iter().cloned());
    patterns.extend(EXTRA_PATTERNS.iter().cloned());
    patterns
});

static PATTERN_DFA: LazyLock<AcDfa<PatternVariant>> = LazyLock::new(|| {
    let patterns: Vec<(PatternVariant, &[u8])> = ALL_PATTERNS
        .iter()
        .map(|(v, bits)| (*v, bits.as_slice()))
        .collect();
    ac_build(&patterns)
});

// ============================================================================
// Pattern Detection Functions
// ============================================================================

#[must_use] 
pub fn detect_patterns(
    bitmasks: &[u8],
    patterns: &[(PatternVariant, Vec<u8>)],
) -> PatternCounts {
    let pat_refs: Vec<(PatternVariant, &[u8])> = patterns
        .iter()
        .map(|(v, bits)| (*v, bits.as_slice()))
        .collect();
    let dfa = ac_build(&pat_refs);
    ac_search_array(bitmasks, &dfa)
}

pub(crate) fn detect_default_patterns(bitmasks: &[u8]) -> PatternCounts {
    ac_search_array(bitmasks, &PATTERN_DFA)
}

// ============================================================================
// Custom Pattern Detection
// ============================================================================

#[derive(Debug, Clone)]
pub(crate) struct CompiledCustomPatterns {
    pub patterns: Vec<CompiledPattern>,
    pub dfa: AcDfa<usize>,
}

/// Creates an empty compiled custom patterns structure
#[inline]
pub(crate) fn compiled_custom_empty() -> CompiledCustomPatterns {
    CompiledCustomPatterns {
        patterns: Vec::new(),
        dfa: ac_empty(),
    }
}

/// Checks if compiled custom patterns is empty
#[inline]
pub(crate) const fn compiled_custom_is_empty(compiled: &CompiledCustomPatterns) -> bool {
    compiled.patterns.is_empty()
}

pub(crate) fn compile_custom_patterns(patterns: &[String]) -> CompiledCustomPatterns {
    let mut compiled = Vec::with_capacity(patterns.len());
    let mut seen = HashSet::with_capacity(patterns.len());

    for pattern_str in patterns {
        let upper = pattern_str.to_ascii_uppercase();
        if !seen.insert(upper.clone()) {
            continue;
        }
        let bits = string_to_pattern_bits(&upper);
        compiled.push(CompiledPattern {
            pattern: upper,
            bits,
        });
    }

    let dfa_patterns: Vec<(usize, &[u8])> = compiled
        .iter()
        .enumerate()
        .map(|(i, p)| (i, p.bits.as_slice()))
        .collect();

    CompiledCustomPatterns {
        dfa: ac_build(&dfa_patterns),
        patterns: compiled,
    }
}

pub(crate) fn detect_custom_patterns_compiled(
    bitmasks: &[u8],
    compiled: &CompiledCustomPatterns,
) -> Vec<CustomPatternSummary> {
    let counts = ac_search_vec(bitmasks, &compiled.dfa, compiled.patterns.len());

    compiled
        .patterns
        .iter()
        .enumerate()
        .map(|(i, p)| CustomPatternSummary {
            pattern: p.pattern.clone(),
            count: counts[i],
        })
        .collect()
}

#[must_use] 
pub fn detect_custom_patterns(bitmasks: &[u8], patterns: &[String]) -> Vec<CustomPatternSummary> {
    let compiled = compile_custom_patterns(patterns);
    detect_custom_patterns_compiled(bitmasks, &compiled)
}

// ============================================================================
// Anchor Counting
// ============================================================================

#[must_use] 
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

// ============================================================================
// Facing Step Analysis
// ============================================================================

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
    L,
    R,
    U,
    D,
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

#[inline(always)]
const fn determine_direction(prev: Arrow, curr: Arrow) -> Option<Direction> {
    use Arrow::{L, U, D, R};
    use Direction::{Left, Right};
    match (prev, curr) {
        (L, U) | (D, R) | (R, D) | (U, L) => Some(Left),
        (L, D) | (U, R) | (R, U) | (D, L) => Some(Right),
        _ => None,
    }
}

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

    forced_foot(curr_arrow).map_or_else(
        || (Some(opposite_foot(prev)), false),
        |forced| {
            let expected = opposite_foot(prev);
            let conflict = forced != expected;
            (Some(forced), conflict)
        },
    )
}

#[inline(always)]
const fn update_facing_state(
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

#[inline(always)]
const fn finalize_segment(
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

#[must_use] 
pub fn count_facing_steps(bitmasks: &[u8], mono_threshold: usize) -> (u32, u32) {
    let mut final_left = 0_u32;
    let mut final_right = 0_u32;
    let mut state = FacingState::Waiting { count: 0 };
    let mut prev_arrow: Option<Arrow> = None;
    let mut prev_foot: Option<Foot> = None;

    for &mask in bitmasks {
        let Some(curr_arrow) = map_bitmask_to_arrow(mask) else {
            if prev_arrow.is_some() {
                finalize_segment(
                    &mut state,
                    &mut final_left,
                    &mut final_right,
                    mono_threshold,
                );
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
        let (new_foot, should_finalize) = next_foot(prev_foot, curr_arrow);
        if should_finalize {
            finalize_segment(
                &mut state,
                &mut final_left,
                &mut final_right,
                mono_threshold,
            );
        }
        prev_foot = new_foot;
        state = update_facing_state(
            state,
            direction,
            &mut final_left,
            &mut final_right,
            mono_threshold,
        );
        prev_arrow = Some(curr_arrow);
    }

    if prev_arrow.is_some() {
        finalize_segment(
            &mut state,
            &mut final_left,
            &mut final_right,
            mono_threshold,
        );
    }
    (final_left, final_right)
}

// ============================================================================
// Box Count Helpers
// ============================================================================

#[inline(always)]
#[must_use] 
pub const fn count_pattern(counts: &PatternCounts, variant: PatternVariant) -> u32 {
    counts[variant as usize]
}

#[must_use] 
pub const fn compute_box_counts(counts: &PatternCounts) -> BoxCounts {
    let lr = count_pattern(counts, PatternVariant::BoxLR);
    let ud = count_pattern(counts, PatternVariant::BoxUD);
    let ld = count_pattern(counts, PatternVariant::BoxCornerLD);
    let lu = count_pattern(counts, PatternVariant::BoxCornerLU);
    let rd = count_pattern(counts, PatternVariant::BoxCornerRD);
    let ru = count_pattern(counts, PatternVariant::BoxCornerRU);
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

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

const fn pattern_bit(b: u8) -> u8 {
    match b {
        b'L' | b'l' => 0b0001,
        b'D' | b'd' => 0b0010,
        b'U' | b'u' => 0b0100,
        b'R' | b'r' => 0b1000,
        _ => 0b0000,
    }
}

fn string_to_pattern_bits(p: &str) -> Vec<u8> {
    p.bytes().map(pattern_bit).collect()
}

const fn pattern_bits<const N: usize>(p: &[u8; N]) -> [u8; N] {
    let mut bits = [0u8; N];
    let mut i = 0;
    while i < N {
        bits[i] = pattern_bit(p[i]);
        i += 1;
    }
    bits
}

// ============================================================================
// Static Pattern Definitions
// ============================================================================

pub type PatternDef = (PatternVariant, &'static [u8]);

macro_rules! pattern_def {
    ($variant:ident, $bits:literal) => {
        (PatternVariant::$variant, &pattern_bits($bits))
    };
}

macro_rules! define_patterns {
    (
        default { $($default_variant:ident $default_bits:literal,)* }
        extra { $($extra_variant:ident $extra_bits:literal,)* }
    ) => {
        pub static DEFAULT_PATTERNS: &[PatternDef] = &[
            $(pattern_def!($default_variant, $default_bits),)*
        ];
        pub static EXTRA_PATTERNS: &[PatternDef] = &[
            $(pattern_def!($extra_variant, $extra_bits),)*
        ];
        pub static ALL_PATTERNS: &[PatternDef] = &[
            $(pattern_def!($default_variant, $default_bits),)*
            $(pattern_def!($extra_variant, $extra_bits),)*
        ];
    };
}

define_patterns! {
    default {
        CandleLeft b"ULD",
        CandleLeft b"DLU",
        CandleRight b"URD",
        CandleRight b"DRU",
        BoxLR b"LRLR",
        BoxLR b"RLRL",
        BoxUD b"UDUD",
        BoxUD b"DUDU",
        BoxCornerLD b"LDLD",
        BoxCornerLD b"DLDL",
        BoxCornerLU b"LULU",
        BoxCornerLU b"ULUL",
        BoxCornerRD b"RDRD",
        BoxCornerRD b"DRDR",
        BoxCornerRU b"RURU",
        BoxCornerRU b"URUR",
    }
    extra {
        StaircaseLeft b"LDUR",
        StaircaseRight b"RUDL",
        StaircaseInvLeft b"LUDR",
        StaircaseInvRight b"RDUL",
        TriangleRUR b"RUR",
        TriangleLUL b"LUL",
        TriangleLDL b"LDL",
        TriangleRDR b"RDR",
        DoritoLeft b"LDUDL",
        DoritoRight b"RUDUR",
        DoritoInvLeft b"LUDUL",
        DoritoInvRight b"RDUDR",
        SweepLeft b"LDURUDL",
        SweepRight b"RUDLDUR",
        SweepInvLeft b"LUDRDUL",
        SweepInvRight b"RDULUDR",
        TowerLR b"LRLRL",
        TowerLR b"RLRLR",
        TowerUD b"UDUDU",
        TowerUD b"DUDUD",
        TowerCornerLD b"LDLDL",
        TowerCornerLD b"DLDLD",
        TowerCornerLU b"LULUL",
        TowerCornerLU b"ULULU",
        TowerCornerRD b"RDRDR",
        TowerCornerRD b"DRDRD",
        TowerCornerRU b"RURUR",
        TowerCornerRU b"URURU",
        DStaircaseLeft b"LUDRLUDR",
        DStaircaseRight b"RDULRDUL",
        DStaircaseInvLeft b"LDURLDUR",
        DStaircaseInvRight b"RDULRDUL",
        AltStaircasesLeft b"LUDRLDUR",
        AltStaircasesRight b"RDULRUDL",
        AltStaircasesInvLeft b"LDURLUDR",
        AltStaircasesInvRight b"RUDLRDUL",
        LuchiLeftDU b"LDLUL",
        LuchiLeftUD b"LULDL",
        LuchiRightUD b"RURDR",
        LuchiRightDU b"RDRUR",
        CopterLeft b"LDURDULDUR",
        CopterRight b"RUDLUDRUDL",
        CopterInvLeft b"LUDRUDLUDR",
        CopterInvRight b"RDULDURDUL",
        HipBreakerLeft b"LDUDLUDUL",
        HipBreakerRight b"RUDURDUDR",
        HipBreakerInvLeft b"LUDULDUDL",
        HipBreakerInvRight b"RDUDRUDUR",
        SpiralLeft b"LDURDR",
        SpiralRight b"RUDLUL",
        SpiralInvLeft b"LUDRUR",
        SpiralInvRight b"RDULDL",
        TurboCandleLeft b"LDLUDRUR",
        TurboCandleRight b"RURDULDL",
        TurboCandleInvLeft b"LULDURDR",
        TurboCandleInvRight b"RDRUDLUL",
        SweepCandleLeft b"LDURDRUDL",
        SweepCandleRight b"RUDLULDUR",
        SweepCandleInvLeft b"LUDRURDUL",
        SweepCandleInvRight b"RDULDLUDR",
    }
}

static PATTERN_DFA: LazyLock<AcDfa<PatternVariant>> = LazyLock::new(|| ac_build(ALL_PATTERNS));

// ============================================================================
// Pattern Detection Functions
// ============================================================================

#[must_use]
pub fn detect_patterns<B: AsRef<[u8]>>(
    bitmasks: &[u8],
    patterns: &[(PatternVariant, B)],
) -> PatternCounts {
    let pat_refs: Vec<(PatternVariant, &[u8])> = patterns
        .iter()
        .map(|(v, bits)| (*v, bits.as_ref()))
        .collect();
    let dfa = ac_build(&pat_refs);
    ac_search_array(bitmasks, &dfa)
}

pub fn detect_default_patterns(bitmasks: &[u8]) -> PatternCounts {
    ac_search_array(bitmasks, &PATTERN_DFA)
}

// ============================================================================
// Custom Pattern Detection
// ============================================================================

#[derive(Debug, Clone)]
pub struct CompiledCustomPatterns {
    patterns: Vec<CompiledPattern>,
    dfa: AcDfa<usize>,
}

/// Creates an empty compiled custom patterns structure
#[inline]
pub fn compiled_custom_empty() -> CompiledCustomPatterns {
    CompiledCustomPatterns {
        patterns: Vec::new(),
        dfa: ac_empty(),
    }
}

/// Checks if compiled custom patterns is empty
#[inline]
pub const fn compiled_custom_is_empty(compiled: &CompiledCustomPatterns) -> bool {
    compiled.patterns.is_empty()
}

pub fn compile_custom_patterns(patterns: &[String]) -> CompiledCustomPatterns {
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

pub fn detect_custom_patterns_compiled(
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

const ARROW_NONE: u8 = 0;
const ARROW_L: u8 = 1;
const ARROW_D: u8 = 2;
const ARROW_U: u8 = 3;
const ARROW_R: u8 = 4;

const FOOT_NONE: u8 = 0;
const FOOT_LEFT: u8 = 1;
const FOOT_RIGHT: u8 = 2;

const FACE_WAIT: u8 = 0;
const FACE_LEFT: u8 = 1;
const FACE_RIGHT: u8 = 2;

const DIR_NONE: u8 = 0;
const DIR_LEFT: u8 = 1;
const DIR_RIGHT: u8 = 2;

const MASK_TO_ARROW: [u8; 16] = [
    ARROW_NONE, ARROW_L, ARROW_D, ARROW_NONE, ARROW_U, ARROW_NONE, ARROW_NONE, ARROW_NONE, ARROW_R,
    ARROW_NONE, ARROW_NONE, ARROW_NONE, ARROW_NONE, ARROW_NONE, ARROW_NONE, ARROW_NONE,
];

const FORCED_FOOT: [u8; 5] = [FOOT_NONE, FOOT_LEFT, FOOT_NONE, FOOT_NONE, FOOT_RIGHT];
const OPPOSITE_FOOT: [u8; 3] = [FOOT_NONE, FOOT_RIGHT, FOOT_LEFT];
const FOOT_CONFLICT: u8 = 1 << 2;
const FOOT_MASK: u8 = 0b11;

const fn build_dir_table() -> [[u8; 5]; 5] {
    let mut t = [[DIR_NONE; 5]; 5];
    t[ARROW_L as usize][ARROW_U as usize] = DIR_LEFT;
    t[ARROW_D as usize][ARROW_R as usize] = DIR_LEFT;
    t[ARROW_R as usize][ARROW_D as usize] = DIR_LEFT;
    t[ARROW_U as usize][ARROW_L as usize] = DIR_LEFT;
    t[ARROW_L as usize][ARROW_D as usize] = DIR_RIGHT;
    t[ARROW_U as usize][ARROW_R as usize] = DIR_RIGHT;
    t[ARROW_R as usize][ARROW_U as usize] = DIR_RIGHT;
    t[ARROW_D as usize][ARROW_L as usize] = DIR_RIGHT;
    t
}

const DIR_TABLE: [[u8; 5]; 5] = build_dir_table();

const fn build_foot_table() -> [[u8; 5]; 3] {
    let mut t = [[FOOT_NONE; 5]; 3];
    let mut prev = 0;
    while prev < 3 {
        let mut curr = 0;
        while curr < 5 {
            let forced = FORCED_FOOT[curr];
            let expected = OPPOSITE_FOOT[prev];
            t[prev][curr] = if prev == FOOT_NONE as usize {
                forced
            } else if forced == FOOT_NONE {
                expected
            } else if forced != expected {
                forced | FOOT_CONFLICT
            } else {
                forced
            };
            curr += 1;
        }
        prev += 1;
    }
    t
}

const FOOT_TABLE: [[u8; 5]; 3] = build_foot_table();

#[inline(always)]
const fn bitmask_arrow(mask: u8) -> u8 {
    if mask < 16 {
        MASK_TO_ARROW[mask as usize]
    } else {
        ARROW_NONE
    }
}

#[inline(always)]
const fn finalize_facing(
    state: u8,
    count: usize,
    final_left: &mut u32,
    final_right: &mut u32,
    mono_threshold: usize,
) {
    if count < mono_threshold {
        return;
    }
    if state == FACE_LEFT {
        *final_left += count as u32;
    } else if state == FACE_RIGHT {
        *final_right += count as u32;
    }
}

#[inline(always)]
const fn step_facing(
    state: u8,
    count: usize,
    direction: u8,
    final_left: &mut u32,
    final_right: &mut u32,
    mono_threshold: usize,
) -> (u8, usize) {
    match state {
        FACE_WAIT => match direction {
            DIR_LEFT => (FACE_LEFT, count + 1),
            DIR_RIGHT => (FACE_RIGHT, count + 1),
            _ => (FACE_WAIT, count + 1),
        },
        FACE_LEFT => match direction {
            DIR_RIGHT => {
                finalize_facing(FACE_LEFT, count, final_left, final_right, mono_threshold);
                (FACE_RIGHT, 1)
            }
            _ => (FACE_LEFT, count + 1),
        },
        _ => match direction {
            DIR_LEFT => {
                finalize_facing(FACE_RIGHT, count, final_left, final_right, mono_threshold);
                (FACE_LEFT, 1)
            }
            _ => (FACE_RIGHT, count + 1),
        },
    }
}

#[inline(always)]
const fn next_facing_foot(prev_foot: u8, curr_arrow: u8) -> (u8, bool) {
    let packed = FOOT_TABLE[prev_foot as usize][curr_arrow as usize];
    (packed & FOOT_MASK, (packed & FOOT_CONFLICT) != 0)
}

#[must_use]
pub fn count_facing_steps(bitmasks: &[u8], mono_threshold: usize) -> (u32, u32) {
    let mut final_left = 0_u32;
    let mut final_right = 0_u32;
    let mut state = FACE_WAIT;
    let mut count = 0usize;
    let mut prev_arrow = ARROW_NONE;
    let mut prev_foot = FOOT_NONE;

    for &mask in bitmasks {
        let curr_arrow = bitmask_arrow(mask);
        if curr_arrow == ARROW_NONE {
            if prev_arrow != ARROW_NONE {
                finalize_facing(
                    state,
                    count,
                    &mut final_left,
                    &mut final_right,
                    mono_threshold,
                );
                state = FACE_WAIT;
                count = 0;
                prev_arrow = ARROW_NONE;
                prev_foot = FOOT_NONE;
            }
            continue;
        };

        if prev_arrow == ARROW_NONE {
            state = FACE_WAIT;
            count = 1;
            prev_foot = FORCED_FOOT[curr_arrow as usize];
            prev_arrow = curr_arrow;
            continue;
        }

        let direction = DIR_TABLE[prev_arrow as usize][curr_arrow as usize];
        let (new_foot, should_finalize) = next_facing_foot(prev_foot, curr_arrow);
        if should_finalize {
            finalize_facing(
                state,
                count,
                &mut final_left,
                &mut final_right,
                mono_threshold,
            );
            state = FACE_WAIT;
            count = 0;
        }
        prev_foot = new_foot;
        (state, count) = step_facing(
            state,
            count,
            direction,
            &mut final_left,
            &mut final_right,
            mono_threshold,
        );
        prev_arrow = curr_arrow;
    }

    if prev_arrow != ARROW_NONE {
        finalize_facing(
            state,
            count,
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

#[cfg(test)]
mod tests {
    use super::count_facing_steps;

    #[test]
    fn facing_steps_count_left_and_right_runs() {
        assert_eq!(
            count_facing_steps(&[0b0001, 0b0100, 0b0001, 0b0100], 2),
            (4, 0)
        );
        assert_eq!(
            count_facing_steps(&[0b0001, 0b0010, 0b0001, 0b0010], 2),
            (0, 4)
        );
    }

    #[test]
    fn facing_steps_split_on_empty_and_forced_foot_conflict() {
        assert_eq!(
            count_facing_steps(&[0b0001, 0b0100, 0, 0b0001, 0b0100], 2),
            (4, 0)
        );
        assert_eq!(count_facing_steps(&[0b0001, 0b0100, 0b1000], 2), (2, 0));
    }
}

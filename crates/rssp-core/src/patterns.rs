use std::borrow::Cow;
use std::collections::HashMap;
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CustomPatternSummary {
    pub pattern: String,
    pub count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PatternAnalysis {
    pub detected_patterns: PatternCounts,
    pub anchors: (u32, u32, u32, u32),
    pub facing_steps: (u32, u32),
    pub custom_patterns: Vec<CustomPatternSummary>,
}

#[derive(Debug, Clone)]
struct CompiledPattern {
    pattern: String,
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

const AC_OUTPUT_NONE: u32 = u32::MAX;

fn ac_finalize_output<T: Copy>(
    state: usize,
    fail_target: usize,
    direct_heads: &[u32],
    direct_outputs: &[(T, u32)],
    output_starts: &mut [u32],
    output_lens: &mut [u32],
    flat_outputs: &mut Vec<T>,
) {
    let output_start = flat_outputs.len();
    let mut node = direct_heads[state];
    while node != AC_OUTPUT_NONE {
        let (value, next) = direct_outputs[node as usize];
        flat_outputs.push(value);
        node = next;
    }
    flat_outputs[output_start..].reverse();

    if fail_target != 0 {
        let inherited_start = output_starts[fail_target] as usize;
        let inherited_end = inherited_start + output_lens[fail_target] as usize;
        for index in inherited_start..inherited_end {
            flat_outputs.push(flat_outputs[index]);
        }
    }

    output_starts[state] = output_start as u32;
    output_lens[state] = (flat_outputs.len() - output_start) as u32;
}

fn ac_build<T, P>(
    patterns: impl IntoIterator<Item = (T, P)>,
    mut pattern_symbol: impl FnMut(u8) -> u8,
) -> AcDfa<T>
where
    T: Copy,
    P: AsRef<[u8]>,
{
    let patterns = patterns.into_iter();
    let mut goto: Vec<[u32; AC_ALPHA]> = vec![[u32::MAX; AC_ALPHA]];
    let mut direct_heads = vec![AC_OUTPUT_NONE];
    let mut direct_outputs = Vec::with_capacity(patterns.size_hint().0.min(256));

    for (id, pat) in patterns {
        let pat = pat.as_ref();
        if pat.is_empty() {
            continue;
        }
        let mut state = 0usize;
        for &b in pat {
            let sym = (pattern_symbol(b) & 0x0F) as usize;
            if goto[state][sym] == u32::MAX {
                goto[state][sym] = goto.len() as u32;
                goto.push([u32::MAX; AC_ALPHA]);
                direct_heads.push(AC_OUTPUT_NONE);
            }
            state = goto[state][sym] as usize;
        }
        let output_index = direct_outputs.len() as u32;
        direct_outputs.push((id, direct_heads[state]));
        direct_heads[state] = output_index;
    }

    let n = goto.len();
    if n == 1 {
        goto[0] = [0; AC_ALPHA];
        return AcDfa {
            goto: goto.into_flattened(),
            output_starts: vec![0],
            output_lens: vec![0],
            flat_outputs: Vec::new(),
        };
    }

    let mut fail = vec![0u32; n];
    let mut queue = Vec::with_capacity(n.saturating_sub(1));
    let mut output_starts = vec![0; n];
    let mut output_lens = vec![0; n];
    let mut flat_outputs = Vec::with_capacity(direct_outputs.len());

    for sym in 0..AC_ALPHA {
        let next = goto[0][sym];
        if next == u32::MAX {
            goto[0][sym] = 0;
            continue;
        }
        let next = next as usize;
        queue.push(next);
        ac_finalize_output(
            next,
            0,
            &direct_heads,
            &direct_outputs,
            &mut output_starts,
            &mut output_lens,
            &mut flat_outputs,
        );
    }

    let mut queue_index = 0;
    while let Some(&state) = queue.get(queue_index) {
        queue_index += 1;
        let fail_state = fail[state] as usize;
        let mut row = goto[state];
        for (sym, child) in row.iter_mut().enumerate() {
            if *child == u32::MAX {
                *child = goto[fail_state][sym];
                continue;
            }
            let child_idx = *child as usize;
            queue.push(child_idx);

            let fail_target = goto[fail_state][sym];
            fail[child_idx] = fail_target;
            ac_finalize_output(
                child_idx,
                fail_target as usize,
                &direct_heads,
                &direct_outputs,
                &mut output_starts,
                &mut output_lens,
                &mut flat_outputs,
            );
        }
        goto[state] = row;
    }

    debug_assert!(goto
        .iter()
        .all(|row| row.iter().all(|&child| child != u32::MAX)));

    let flat_goto = goto.into_flattened();

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
    if count == 0 {
        return counts;
    }
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
        goto: Vec::new(),
        output_starts: Vec::new(),
        output_lens: Vec::new(),
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

static PATTERN_DFA: LazyLock<AcDfa<PatternVariant>> =
    LazyLock::new(|| ac_build(ALL_PATTERNS.iter().copied(), |byte| byte));

// ============================================================================
// Pattern Detection Functions
// ============================================================================

#[must_use]
pub fn detect_patterns<B: AsRef<[u8]>>(
    bitmasks: &[u8],
    patterns: &[(PatternVariant, B)],
) -> PatternCounts {
    let dfa = ac_build(
        patterns
            .iter()
            .map(|(variant, bits)| (*variant, bits.as_ref())),
        |byte| byte,
    );
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
    let mut pattern_indexes = HashMap::with_capacity(patterns.len());

    for pattern_str in patterns {
        let upper = if pattern_str.bytes().any(|byte| byte.is_ascii_lowercase()) {
            Cow::Owned(pattern_str.to_ascii_uppercase())
        } else {
            Cow::Borrowed(pattern_str.as_str())
        };
        if pattern_indexes.contains_key(upper.as_ref()) {
            continue;
        }
        let next_index = pattern_indexes.len();
        pattern_indexes.insert(upper.into_owned(), next_index);
    }

    let mut compiled: Vec<_> = std::iter::repeat_with(|| CompiledPattern {
        pattern: String::new(),
    })
    .take(pattern_indexes.len())
    .collect();
    for (pattern, index) in pattern_indexes {
        compiled[index].pattern = pattern;
    }

    let dfa = ac_build(
        compiled
            .iter()
            .enumerate()
            .map(|(index, pattern)| (index, pattern.pattern.as_bytes())),
        pattern_bit,
    );

    CompiledCustomPatterns {
        dfa,
        patterns: compiled,
    }
}

pub fn detect_custom_patterns_compiled(
    bitmasks: &[u8],
    compiled: &CompiledCustomPatterns,
) -> Vec<CustomPatternSummary> {
    let counts = ac_search_vec(bitmasks, &compiled.dfa, compiled.patterns.len());

    custom_pattern_summaries(compiled, &counts)
}

fn custom_pattern_summaries(
    compiled: &CompiledCustomPatterns,
    counts: &[u32],
) -> Vec<CustomPatternSummary> {
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

#[inline(always)]
fn note_mask4(row: &[u8; 4]) -> u8 {
    u8::from(matches!(row[0], b'1' | b'2' | b'4'))
        | (u8::from(matches!(row[1], b'1' | b'2' | b'4')) << 1)
        | (u8::from(matches!(row[2], b'1' | b'2' | b'4')) << 2)
        | (u8::from(matches!(row[3], b'1' | b'2' | b'4')) << 3)
}

#[must_use]
pub fn analyze_patterns_from_rows(
    rows: &[[u8; 4]],
    mono_threshold: usize,
    compiled: &CompiledCustomPatterns,
) -> PatternAnalysis {
    let mut detected_patterns = [0u32; PATTERN_COUNT];
    let mut default_state = 0u32;
    let mut custom_counts = vec![0u32; compiled.patterns.len()];
    let mut custom_state = 0u32;
    let mut anchors = [0u32; 4];
    let mut mask_history = [0u8; 4];
    let mut facing = FacingCounter::new(mono_threshold);

    for (idx, row) in rows.iter().enumerate() {
        let mask = note_mask4(row);
        let sym = (mask & 0x0F) as usize;

        default_state = PATTERN_DFA.goto[default_state as usize * AC_ALPHA + sym];
        for &id in ac_output_slice(&PATTERN_DFA, default_state) {
            detected_patterns[id as usize] += 1;
        }

        if !custom_counts.is_empty() {
            custom_state = compiled.dfa.goto[custom_state as usize * AC_ALPHA + sym];
            for &id in ac_output_slice(&compiled.dfa, custom_state) {
                custom_counts[id] += 1;
            }
        }

        if idx >= 4 {
            let anchor_mask = mask_history[idx & 3] & mask_history[(idx - 2) & 3] & mask;
            for (column, count) in anchors.iter_mut().enumerate() {
                *count += u32::from(anchor_mask & (1 << column) != 0);
            }
        }
        mask_history[idx & 3] = mask;
        facing.push(mask);
    }

    PatternAnalysis {
        detected_patterns,
        anchors: (anchors[0], anchors[1], anchors[2], anchors[3]),
        facing_steps: facing.finish(),
        custom_patterns: custom_pattern_summaries(compiled, &custom_counts),
    }
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

struct FacingCounter {
    final_left: u32,
    final_right: u32,
    state: u8,
    count: usize,
    prev_arrow: u8,
    prev_foot: u8,
    mono_threshold: usize,
}

impl FacingCounter {
    const fn new(mono_threshold: usize) -> Self {
        Self {
            final_left: 0,
            final_right: 0,
            state: FACE_WAIT,
            count: 0,
            prev_arrow: ARROW_NONE,
            prev_foot: FOOT_NONE,
            mono_threshold,
        }
    }

    #[inline(always)]
    fn push(&mut self, mask: u8) {
        let curr_arrow = bitmask_arrow(mask);
        if curr_arrow == ARROW_NONE {
            if self.prev_arrow != ARROW_NONE {
                finalize_facing(
                    self.state,
                    self.count,
                    &mut self.final_left,
                    &mut self.final_right,
                    self.mono_threshold,
                );
                self.state = FACE_WAIT;
                self.count = 0;
                self.prev_arrow = ARROW_NONE;
                self.prev_foot = FOOT_NONE;
            }
            return;
        }

        if self.prev_arrow == ARROW_NONE {
            self.state = FACE_WAIT;
            self.count = 1;
            self.prev_foot = FORCED_FOOT[curr_arrow as usize];
            self.prev_arrow = curr_arrow;
            return;
        }

        let direction = DIR_TABLE[self.prev_arrow as usize][curr_arrow as usize];
        let (new_foot, should_finalize) = next_facing_foot(self.prev_foot, curr_arrow);
        if should_finalize {
            finalize_facing(
                self.state,
                self.count,
                &mut self.final_left,
                &mut self.final_right,
                self.mono_threshold,
            );
            self.state = FACE_WAIT;
            self.count = 0;
        }
        self.prev_foot = new_foot;
        (self.state, self.count) = step_facing(
            self.state,
            self.count,
            direction,
            &mut self.final_left,
            &mut self.final_right,
            self.mono_threshold,
        );
        self.prev_arrow = curr_arrow;
    }

    fn finish(mut self) -> (u32, u32) {
        if self.prev_arrow != ARROW_NONE {
            finalize_facing(
                self.state,
                self.count,
                &mut self.final_left,
                &mut self.final_right,
                self.mono_threshold,
            );
        }
        (self.final_left, self.final_right)
    }
}

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
    let mut counter = FacingCounter::new(mono_threshold);
    for &mask in bitmasks {
        counter.push(mask);
    }
    counter.finish()
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
    use super::{
        ac_build, ac_output_slice, ac_search_vec, analyze_patterns_from_rows,
        compile_custom_patterns, count_anchors, count_facing_steps,
        detect_custom_patterns_compiled, detect_default_patterns, pattern_bit,
        CompiledCustomPatterns, CompiledPattern, CustomPatternSummary, AC_ALPHA,
    };
    use std::collections::HashSet;

    fn row_from_mask(mask: u8) -> [u8; 4] {
        std::array::from_fn(|column| {
            if mask & (1 << column) == 0 {
                b'0'
            } else {
                b'1'
            }
        })
    }

    fn compile_custom_patterns_materialized(patterns: &[String]) -> CompiledCustomPatterns {
        let mut compiled = Vec::with_capacity(patterns.len());
        let mut pattern_bits = Vec::with_capacity(patterns.len());
        let mut seen = HashSet::with_capacity(patterns.len());

        for pattern in patterns {
            let upper = pattern.to_ascii_uppercase();
            if !seen.insert(upper.clone()) {
                continue;
            }
            pattern_bits.push(upper.bytes().map(pattern_bit).collect::<Vec<_>>());
            compiled.push(CompiledPattern { pattern: upper });
        }

        let dfa = ac_build(
            pattern_bits
                .iter()
                .enumerate()
                .map(|(index, bits)| (index, bits.as_slice())),
            |byte| byte,
        );
        CompiledCustomPatterns {
            patterns: compiled,
            dfa,
        }
    }

    fn detect_custom_patterns_materialized(
        bitmasks: &[u8],
        compiled: &CompiledCustomPatterns,
    ) -> Vec<CustomPatternSummary> {
        let mut counts = vec![0u32; compiled.patterns.len()];
        let mut state = 0u32;
        for &bitmask in bitmasks {
            let symbol = (bitmask & 0x0F) as usize;
            state = compiled.dfa.goto[state as usize * AC_ALPHA + symbol];
            for &id in ac_output_slice(&compiled.dfa, state) {
                counts[id] += 1;
            }
        }
        compiled
            .patterns
            .iter()
            .zip(counts)
            .map(|(pattern, count)| CustomPatternSummary {
                pattern: pattern.pattern.clone(),
                count,
            })
            .collect()
    }

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

    #[test]
    fn row_analysis_matches_separate_bitmask_passes() {
        let bitmasks = [
            0b0001, 0b0010, 0b0100, 0b1000, 0, 0b0001, 0b0100, 0b0001, 0b0100, 0b0011, 0b0001,
            0b0010, 0b0100, 0b0001,
        ];
        let rows: Vec<_> = bitmasks.iter().copied().map(row_from_mask).collect();
        let custom = compile_custom_patterns(&["LDU".to_string(), "LUL".to_string()]);

        let combined = analyze_patterns_from_rows(&rows, 2, &custom);

        assert_eq!(
            combined.detected_patterns,
            detect_default_patterns(&bitmasks)
        );
        assert_eq!(combined.anchors, count_anchors(&bitmasks));
        assert_eq!(combined.facing_steps, count_facing_steps(&bitmasks, 2));
        assert_eq!(
            combined.custom_patterns,
            detect_custom_patterns_compiled(&bitmasks, &custom)
        );
    }

    #[test]
    fn empty_custom_patterns_skip_the_automaton() {
        let bitmasks = [0b0001, 0b0010, 0b0100, 0b1000];
        let rows: Vec<_> = bitmasks.iter().copied().map(row_from_mask).collect();
        let empty = super::compiled_custom_empty();

        assert!(detect_custom_patterns_compiled(&bitmasks, &empty).is_empty());
        assert!(
            analyze_patterns_from_rows(&rows, 2, &empty)
                .custom_patterns
                .is_empty()
        );
    }

    #[test]
    fn compact_dfa_outputs_match_naive_overlapping_search() {
        let patterns: &[(usize, &[u8])] = &[
            (0, &[1]),
            (1, &[2, 1]),
            (2, &[1]),
            (3, &[]),
            (4, &[3, 2, 1]),
        ];
        let text = [3, 2, 1, 2, 1, 1, 3, 2, 1];
        let dfa = ac_build(patterns.iter().copied(), |byte| byte);
        let actual = ac_search_vec(&text, &dfa, patterns.len());
        let suffix_state = [3, 2, 1].into_iter().fold(0u32, |state, symbol| {
            dfa.goto[state as usize * AC_ALPHA + symbol]
        });

        let mut expected = vec![0; patterns.len()];
        for &(id, pattern) in patterns {
            if pattern.is_empty() {
                continue;
            }
            expected[id] = text
                .windows(pattern.len())
                .filter(|window| *window == pattern)
                .count() as u32;
        }

        assert_eq!(actual, expected);
        assert_eq!(ac_output_slice(&dfa, suffix_state), &[4, 1, 0, 2]);
    }

    #[test]
    fn custom_pattern_pipeline_matches_materialized_implementation() {
        let patterns = [
            "", "ldu", "LDU", "LuL", "lul", "RDR", "rdr", "éL", "x", "LLLLLLLL",
        ]
        .map(str::to_string);
        let mut state = 0x9e37_79b9_u32;
        let bitmasks: Vec<_> = (0..4_096)
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 17;
                state ^= state << 5;
                state as u8 & 0x0f
            })
            .collect();

        let expected_compiled = compile_custom_patterns_materialized(&patterns);
        let actual_compiled = compile_custom_patterns(&patterns);
        assert_eq!(actual_compiled.dfa.goto, expected_compiled.dfa.goto);
        assert_eq!(
            actual_compiled.dfa.output_starts,
            expected_compiled.dfa.output_starts
        );
        assert_eq!(
            actual_compiled.dfa.output_lens,
            expected_compiled.dfa.output_lens
        );
        assert_eq!(
            actual_compiled.dfa.flat_outputs,
            expected_compiled.dfa.flat_outputs
        );
        assert_eq!(
            actual_compiled
                .patterns
                .iter()
                .map(|pattern| pattern.pattern.as_str())
                .collect::<Vec<_>>(),
            expected_compiled
                .patterns
                .iter()
                .map(|pattern| pattern.pattern.as_str())
                .collect::<Vec<_>>()
        );
        assert_eq!(
            detect_custom_patterns_compiled(&bitmasks, &actual_compiled),
            detect_custom_patterns_materialized(&bitmasks, &expected_compiled)
        );
        assert_eq!(
            detect_custom_patterns_compiled(&[], &actual_compiled),
            detect_custom_patterns_materialized(&[], &expected_compiled)
        );

        let many_patterns: Vec<_> = (0..300)
            .map(|mut value| {
                let mut pattern = String::with_capacity(5);
                for _ in 0..5 {
                    pattern.push(char::from(b"LDUR"[value & 3]));
                    value >>= 2;
                }
                pattern
            })
            .collect();
        let expected_many = compile_custom_patterns_materialized(&many_patterns);
        let actual_many = compile_custom_patterns(&many_patterns);
        assert_eq!(
            detect_custom_patterns_compiled(&bitmasks, &actual_many),
            detect_custom_patterns_materialized(&bitmasks, &expected_many)
        );
    }
}

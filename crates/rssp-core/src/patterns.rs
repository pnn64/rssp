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

fn ac_build_with_capacity<T, P>(
    patterns: impl IntoIterator<Item = (T, P)>,
    state_capacity: usize,
    mut pattern_symbol: impl FnMut(u8) -> u8,
) -> AcDfa<T>
where
    T: Copy,
    P: AsRef<[u8]>,
{
    let patterns = patterns.into_iter();
    let mut goto: Vec<[u32; AC_ALPHA]> = Vec::with_capacity(state_capacity.max(1));
    goto.push([u32::MAX; AC_ALPHA]);
    let mut direct_heads = Vec::with_capacity(state_capacity.max(1));
    direct_heads.push(AC_OUTPUT_NONE);
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

    debug_assert!(
        goto.iter()
            .all(|row| row.iter().all(|&child| child != u32::MAX))
    );

    let flat_goto = goto.into_flattened();

    AcDfa {
        goto: flat_goto,
        output_starts,
        output_lens,
        flat_outputs,
    }
}

fn ac_build<T, P>(
    patterns: impl IntoIterator<Item = (T, P)>,
    pattern_symbol: impl FnMut(u8) -> u8,
) -> AcDfa<T>
where
    T: Copy,
    P: AsRef<[u8]>,
{
    ac_build_with_capacity(patterns, 1, pattern_symbol)
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
        const DEFAULT_PATTERN_VALUES: &[PatternDef] = &[
            $(pattern_def!($default_variant, $default_bits),)*
        ];
        const EXTRA_PATTERN_VALUES: &[PatternDef] = &[
            $(pattern_def!($extra_variant, $extra_bits),)*
        ];
        const ALL_PATTERN_VALUES: &[PatternDef] = &[
            $(pattern_def!($default_variant, $default_bits),)*
            $(pattern_def!($extra_variant, $extra_bits),)*
        ];
        pub static DEFAULT_PATTERNS: &[PatternDef] = DEFAULT_PATTERN_VALUES;
        pub static EXTRA_PATTERNS: &[PatternDef] = EXTRA_PATTERN_VALUES;
        pub static ALL_PATTERNS: &[PatternDef] = ALL_PATTERN_VALUES;
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

const PATTERN_DEF_COUNT: usize = 76;
const PATTERN_STATE_COUNT: usize = 190;
const PATTERN_OUTPUT_COUNT: usize = 167;
const PATTERN_GOTO_LEN: usize = PATTERN_STATE_COUNT * AC_ALPHA;

struct PatternDfa {
    goto: [u8; PATTERN_GOTO_LEN],
    output_ranges: [u16; PATTERN_STATE_COUNT],
    flat_outputs: [u8; PATTERN_OUTPUT_COUNT],
}

const fn pattern_finalize_output(
    state: usize,
    fail_target: usize,
    direct_heads: &[u8; PATTERN_STATE_COUNT],
    direct_values: &[u8; PATTERN_DEF_COUNT],
    direct_next: &[u8; PATTERN_DEF_COUNT],
    output_ranges: &mut [u16; PATTERN_STATE_COUNT],
    flat_outputs: &mut [u8; PATTERN_OUTPUT_COUNT],
    flat_len: &mut usize,
) {
    let output_start = *flat_len;
    let mut node = direct_heads[state];
    while node != u8::MAX {
        flat_outputs[*flat_len] = direct_values[node as usize];
        *flat_len += 1;
        node = direct_next[node as usize];
    }

    let mut left = output_start;
    let mut right = *flat_len;
    while left < right {
        right -= 1;
        if left >= right {
            break;
        }
        let value = flat_outputs[left];
        flat_outputs[left] = flat_outputs[right];
        flat_outputs[right] = value;
        left += 1;
    }

    if fail_target != 0 {
        let inherited = output_ranges[fail_target];
        let inherited_start = (inherited & 0xff) as usize;
        let inherited_len = (inherited >> 8) as usize;
        let mut index = 0usize;
        while index < inherited_len {
            flat_outputs[*flat_len] = flat_outputs[inherited_start + index];
            *flat_len += 1;
            index += 1;
        }
    }

    let len = *flat_len - output_start;
    output_ranges[state] = output_start as u16 | ((len as u16) << 8);
}

const fn build_pattern_dfa(patterns: &[PatternDef]) -> PatternDfa {
    let mut goto = [u8::MAX; PATTERN_GOTO_LEN];
    let mut direct_heads = [u8::MAX; PATTERN_STATE_COUNT];
    let mut direct_values = [0u8; PATTERN_DEF_COUNT];
    let mut direct_next = [u8::MAX; PATTERN_DEF_COUNT];
    let mut state_count = 1usize;
    let mut direct_count = 0usize;

    let mut pattern_index = 0usize;
    while pattern_index < patterns.len() {
        let (variant, pattern) = patterns[pattern_index];
        if !pattern.is_empty() {
            let mut state = 0usize;
            let mut byte_index = 0usize;
            while byte_index < pattern.len() {
                let symbol = (pattern[byte_index] & 0x0f) as usize;
                let transition = state * AC_ALPHA + symbol;
                if goto[transition] == u8::MAX {
                    goto[transition] = state_count as u8;
                    state_count += 1;
                }
                state = goto[transition] as usize;
                byte_index += 1;
            }
            direct_values[direct_count] = variant as u8;
            direct_next[direct_count] = direct_heads[state];
            direct_heads[state] = direct_count as u8;
            direct_count += 1;
        }
        pattern_index += 1;
    }

    let mut fail = [0u8; PATTERN_STATE_COUNT];
    let mut queue = [0u8; PATTERN_STATE_COUNT - 1];
    let mut queue_start = 0usize;
    let mut queue_end = 0usize;
    let mut output_ranges = [0u16; PATTERN_STATE_COUNT];
    let mut flat_outputs = [0u8; PATTERN_OUTPUT_COUNT];
    let mut flat_len = 0usize;

    let mut symbol = 0usize;
    while symbol < AC_ALPHA {
        let next = goto[symbol];
        if next == u8::MAX {
            goto[symbol] = 0;
        } else {
            queue[queue_end] = next;
            queue_end += 1;
            pattern_finalize_output(
                next as usize,
                0,
                &direct_heads,
                &direct_values,
                &direct_next,
                &mut output_ranges,
                &mut flat_outputs,
                &mut flat_len,
            );
        }
        symbol += 1;
    }

    while queue_start < queue_end {
        let state = queue[queue_start] as usize;
        queue_start += 1;
        let fail_state = fail[state] as usize;
        symbol = 0;
        while symbol < AC_ALPHA {
            let transition = state * AC_ALPHA + symbol;
            let child = goto[transition];
            if child == u8::MAX {
                goto[transition] = goto[fail_state * AC_ALPHA + symbol];
            } else {
                queue[queue_end] = child;
                queue_end += 1;
                let fail_target = goto[fail_state * AC_ALPHA + symbol];
                fail[child as usize] = fail_target;
                pattern_finalize_output(
                    child as usize,
                    fail_target as usize,
                    &direct_heads,
                    &direct_values,
                    &direct_next,
                    &mut output_ranges,
                    &mut flat_outputs,
                    &mut flat_len,
                );
            }
            symbol += 1;
        }
    }

    assert!(patterns.len() == PATTERN_DEF_COUNT);
    assert!(direct_count == PATTERN_DEF_COUNT);
    assert!(state_count == PATTERN_STATE_COUNT);
    assert!(flat_len == PATTERN_OUTPUT_COUNT);
    PatternDfa {
        goto,
        output_ranges,
        flat_outputs,
    }
}

#[inline(always)]
fn pattern_output_slice(state: u8) -> &'static [u8] {
    let range = PATTERN_DFA.output_ranges[state as usize];
    let start = (range & 0xff) as usize;
    let len = (range >> 8) as usize;
    debug_assert!(start + len <= PATTERN_OUTPUT_COUNT);
    // SAFETY: build_pattern_dfa creates each packed range from indices into
    // flat_outputs, and its const-evaluated writes enforce the array bound.
    unsafe { std::slice::from_raw_parts(PATTERN_DFA.flat_outputs.as_ptr().add(start), len) }
}

// Immutable process-lifetime table shared lock-free by analysis callers. It is
// const-built (no warmup or miss work), has fixed 190-state/167-output capacity,
// never evicts, and resides in the binary until process teardown. Each input
// byte performs one transition plus at most three output visits.
static PATTERN_DFA: PatternDfa = build_pattern_dfa(ALL_PATTERN_VALUES);

#[inline]
fn pattern_search_array(text: &[u8]) -> PatternCounts {
    let mut counts = [0u32; PATTERN_COUNT];
    let mut state = 0u8;
    for &byte in text {
        let symbol = (byte & 0x0f) as usize;
        state = PATTERN_DFA.goto[state as usize * AC_ALPHA + symbol];
        for &id in pattern_output_slice(state) {
            counts[id as usize] += 1;
        }
    }
    counts
}

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
    pattern_search_array(bitmasks)
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

#[inline(always)]
fn pattern_hash_ci(pattern: &str) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in pattern.bytes() {
        hash ^= u64::from(byte.to_ascii_uppercase());
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash ^ (hash >> 32)
}

fn compile_custom_patterns_impl(patterns: &[String]) -> CompiledCustomPatterns {
    if patterns.is_empty() {
        return compiled_custom_empty();
    }

    let table_len = patterns
        .len()
        .saturating_add(patterns.len() / 3)
        .max(2)
        .next_power_of_two();
    let mut slots = vec![u32::MAX; table_len];
    let mut compiled = Vec::with_capacity(patterns.len().min(256));
    let mask = u64::try_from(table_len - 1).expect("table mask fits u64");

    for pattern in patterns {
        let mut slot = usize::try_from(pattern_hash_ci(pattern) & mask)
            .expect("masked pattern hash fits usize");
        loop {
            let index = slots[slot];
            if index == u32::MAX {
                slots[slot] = u32::try_from(compiled.len())
                    .expect("custom pattern count cannot exceed u32 storage");
                compiled.push(CompiledPattern {
                    pattern: pattern.to_ascii_uppercase(),
                });
                break;
            }
            if compiled[index as usize]
                .pattern
                .eq_ignore_ascii_case(pattern)
            {
                break;
            }
            slot = (slot + 1) & (table_len - 1);
        }
    }
    compiled.shrink_to_fit();

    let state_capacity = compiled.iter().fold(1usize, |capacity, pattern| {
        capacity.saturating_add(pattern.pattern.len())
    });
    let dfa = ac_build_with_capacity(
        compiled
            .iter()
            .enumerate()
            .map(|(index, pattern)| (index, pattern.pattern.as_bytes())),
        state_capacity,
        pattern_bit,
    );

    CompiledCustomPatterns {
        dfa,
        patterns: compiled,
    }
}

pub fn compile_custom_patterns(patterns: &[String]) -> CompiledCustomPatterns {
    compile_custom_patterns_impl(patterns)
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

const NOTE_BYTE: [u8; 256] = {
    let mut table = [0u8; 256];
    table[b'1' as usize] = 1;
    table[b'2' as usize] = 1;
    table[b'4' as usize] = 1;
    table
};

#[inline(always)]
fn note_mask4(row: &[u8; 4]) -> u8 {
    NOTE_BYTE[row[0] as usize]
        | (NOTE_BYTE[row[1] as usize] << 1)
        | (NOTE_BYTE[row[2] as usize] << 2)
        | (NOTE_BYTE[row[3] as usize] << 3)
}

#[must_use]
pub fn analyze_patterns_from_rows(
    rows: &[[u8; 4]],
    mono_threshold: usize,
    compiled: &CompiledCustomPatterns,
) -> PatternAnalysis {
    analyze_patterns_from_rows_with_scratch(rows, mono_threshold, compiled, &mut Vec::new())
}

/// Analyzes rows while reusing caller-owned custom-pattern count storage.
///
/// The buffer is cleared before use and retains capacity for the largest
/// compiled pattern set passed by the caller.
#[must_use]
pub fn analyze_patterns_from_rows_with_scratch(
    rows: &[[u8; 4]],
    mono_threshold: usize,
    compiled: &CompiledCustomPatterns,
    custom_counts: &mut Vec<u32>,
) -> PatternAnalysis {
    let mut detected_patterns = [0u32; PATTERN_COUNT];
    let mut default_state = 0u8;
    custom_counts.clear();
    custom_counts.resize(compiled.patterns.len(), 0);
    let mut custom_state = 0u32;
    let mut anchors = [0u32; 4];
    let mut mask_history = [0u8; 4];
    let mut facing = FacingCounter::new(mono_threshold);

    for (idx, row) in rows.iter().enumerate() {
        let mask = note_mask4(row);
        let sym = (mask & 0x0F) as usize;

        default_state = PATTERN_DFA.goto[default_state as usize * AC_ALPHA + sym];
        for &id in pattern_output_slice(default_state) {
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
        custom_patterns: custom_pattern_summaries(compiled, custom_counts),
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
        AC_ALPHA, ac_build, ac_output_slice, ac_search_vec, analyze_patterns_from_rows,
        compile_custom_patterns, count_anchors, count_facing_steps,
        detect_custom_patterns_compiled, detect_default_patterns, note_mask4,
    };

    fn row_from_mask(mask: u8) -> [u8; 4] {
        std::array::from_fn(|column| {
            if mask & (1 << column) == 0 {
                b'0'
            } else {
                b'1'
            }
        })
    }

    #[test]
    fn note_byte_table_matches_row_classifier() {
        for byte in 0u8..=u8::MAX {
            for column in 0..4 {
                let mut row = [b'0'; 4];
                row[column] = byte;
                let expected = u8::from(matches!(byte, b'1' | b'2' | b'4')) << column;
                assert_eq!(note_mask4(&row), expected, "byte={byte} column={column}");
            }
        }
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
}

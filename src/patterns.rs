use std::collections::HashMap;
use std::sync::LazyLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PatternVariant {
    CandleLeft,
    CandleRight,
    BoxLR,
    BoxUD,
    BoxCornerLD,
    BoxCornerLU,
    BoxCornerRD,
    BoxCornerRU,
    DoritoRight,
    DoritoLeft,
    DoritoInvRight,
    DoritoInvLeft,
    SpiralLeft,
    SpiralRight,
    CopterLeft,
    CopterRight,
    LuchiLeft,
    LuchiRight,
    HipBreakerLeft,
    HipBreakerRight,
    SweepLeft,
    SweepRight,
    SweepInvLeft,
    SweepInvRight,
}


pub static ALL_PATTERNS_NON_ANCHORS: LazyLock<Vec<(PatternVariant, Vec<u8>)>> = LazyLock::new(|| {
    let mut patterns = Vec::new();

    // Candles
    patterns.push((PatternVariant::CandleLeft,  string_to_pattern_bits("ULD")));
    patterns.push((PatternVariant::CandleLeft,  string_to_pattern_bits("DLU")));
    patterns.push((PatternVariant::CandleRight, string_to_pattern_bits("URD")));
    patterns.push((PatternVariant::CandleRight, string_to_pattern_bits("DRU")));

    // Boxes
    patterns.push((PatternVariant::BoxLR,       string_to_pattern_bits("LRLR")));
    patterns.push((PatternVariant::BoxLR,       string_to_pattern_bits("RLRL")));
    patterns.push((PatternVariant::BoxUD,       string_to_pattern_bits("UDUD")));
    patterns.push((PatternVariant::BoxUD,       string_to_pattern_bits("DUDU")));
    patterns.push((PatternVariant::BoxCornerLD, string_to_pattern_bits("LDLD")));
    patterns.push((PatternVariant::BoxCornerLD, string_to_pattern_bits("DLDL")));
    patterns.push((PatternVariant::BoxCornerLU, string_to_pattern_bits("LULU")));
    patterns.push((PatternVariant::BoxCornerLU, string_to_pattern_bits("ULUL")));
    patterns.push((PatternVariant::BoxCornerRD, string_to_pattern_bits("RDRD")));
    patterns.push((PatternVariant::BoxCornerRD, string_to_pattern_bits("DRDR")));
    patterns.push((PatternVariant::BoxCornerRU, string_to_pattern_bits("RURU")));
    patterns.push((PatternVariant::BoxCornerRU, string_to_pattern_bits("URUR")));

    // Doritos
    patterns.push((PatternVariant::DoritoLeft,     string_to_pattern_bits("LDUDL")));
    patterns.push((PatternVariant::DoritoRight,    string_to_pattern_bits("RUDUR")));
    patterns.push((PatternVariant::DoritoInvLeft,  string_to_pattern_bits("LUDUL")));
    patterns.push((PatternVariant::DoritoInvRight, string_to_pattern_bits("RDUDR")));

    // Spirals
    patterns.push((PatternVariant::SpiralLeft,  string_to_pattern_bits("LDURDR")));
    patterns.push((PatternVariant::SpiralRight, string_to_pattern_bits("RUDLUL")));

    // Copters
    patterns.push((PatternVariant::CopterLeft,  string_to_pattern_bits("LDURDULDURDU")));
    patterns.push((PatternVariant::CopterLeft,  string_to_pattern_bits("DULDURDULDUR")));
    patterns.push((PatternVariant::CopterRight, string_to_pattern_bits("RUDLUDRUDLUD")));
    patterns.push((PatternVariant::CopterRight, string_to_pattern_bits("UDRUDLUDRUDL")));

    // Luchi
    patterns.push((PatternVariant::LuchiLeft,  string_to_pattern_bits("LDLRURDRLULD")));
    patterns.push((PatternVariant::LuchiRight, string_to_pattern_bits("RURLDLULRDRU")));

    // Hip-Breakers
    patterns.push((PatternVariant::HipBreakerLeft,  string_to_pattern_bits("LDUDLUDULDUDL")));
    patterns.push((PatternVariant::HipBreakerRight, string_to_pattern_bits("RUDURDUDRUDUR")));

    // Sweeps
    patterns.push((PatternVariant::SweepLeft,     string_to_pattern_bits("LDURUDL")));
    patterns.push((PatternVariant::SweepRight,    string_to_pattern_bits("RUDLDUR")));
    patterns.push((PatternVariant::SweepInvLeft,  string_to_pattern_bits("LUDRDUL")));
    patterns.push((PatternVariant::SweepInvRight, string_to_pattern_bits("RDULUDR")));

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
            _ => 0b0000,
        };
        result.push(mask);
    }
    result
}

pub fn detect_all_patterns_non_anchors(bitmasks: &[u8]) -> HashMap<PatternVariant, u32> {
    let mut results: HashMap<PatternVariant, u32> = HashMap::new();
    let defs: &[(PatternVariant, Vec<u8>)] = ALL_PATTERNS_NON_ANCHORS.as_ref();

    let mut i = 0;
    while i < bitmasks.len() {
        let mut matched_any = false;
        for (variant, pat_bits) in defs.iter() {
            let plen = pat_bits.len();
            if i + plen <= bitmasks.len() {
                if bitmasks[i..i + plen] == pat_bits[..] {
                    *results.entry(*variant).or_insert(0) += 1;
                    i += plen; 
                    matched_any = true;
                    break;
                }
            }
        }
        if !matched_any {
            i += 1;
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

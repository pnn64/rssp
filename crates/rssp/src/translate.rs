use memchr::memchr2;

const INTERNAL_CODEPOINT: u32 = 0xE000;
const INVALID_CODEPOINT: u32 = 0xFFFD;

static ALIAS_ENTRIES: &[(&str, u32)] = &[
    ("ha", 0x3042),
    ("hi", 0x3044),
    ("hu", 0x3046),
    ("he", 0x3048),
    ("ho", 0x304a),
    ("hka", 0x304b),
    ("hki", 0x304d),
    ("hku", 0x304f),
    ("hke", 0x3051),
    ("hko", 0x3053),
    ("hga", 0x304c),
    ("hgi", 0x304e),
    ("hgu", 0x3050),
    ("hge", 0x3052),
    ("hgo", 0x3054),
    ("hza", 0x3056),
    ("hzi", 0x3058),
    ("hzu", 0x305a),
    ("hze", 0x305c),
    ("hzo", 0x305e),
    ("hta", 0x305f),
    ("hti", 0x3061),
    ("htu", 0x3064),
    ("hte", 0x3066),
    ("hto", 0x3068),
    ("hda", 0x3060),
    ("hdi", 0x3062),
    ("hdu", 0x3065),
    ("hde", 0x3067),
    ("hdo", 0x3069),
    ("hna", 0x306a),
    ("hni", 0x306b),
    ("hnu", 0x306c),
    ("hne", 0x306d),
    ("hno", 0x306e),
    ("hha", 0x306f),
    ("hhi", 0x3072),
    ("hhu", 0x3075),
    ("hhe", 0x3078),
    ("hho", 0x307b),
    ("hba", 0x3070),
    ("hbi", 0x3073),
    ("hbu", 0x3076),
    ("hbe", 0x3079),
    ("hbo", 0x307c),
    ("hpa", 0x3071),
    ("hpi", 0x3074),
    ("hpu", 0x3077),
    ("hpe", 0x307a),
    ("hpo", 0x307d),
    ("hma", 0x307e),
    ("hmi", 0x307f),
    ("hmu", 0x3080),
    ("hme", 0x3081),
    ("hmo", 0x3082),
    ("hya", 0x3084),
    ("hyu", 0x3086),
    ("hyo", 0x3088),
    ("hra", 0x3089),
    ("hri", 0x308a),
    ("hru", 0x308b),
    ("hre", 0x308c),
    ("hro", 0x308d),
    ("hwa", 0x308f),
    ("hwi", 0x3090),
    ("hwe", 0x3091),
    ("hwo", 0x3092),
    ("hn", 0x3093),
    ("hvu", 0x3094),
    ("has", 0x3041),
    ("his", 0x3043),
    ("hus", 0x3045),
    ("hes", 0x3047),
    ("hos", 0x3049),
    ("hkas", 0x3095),
    ("hkes", 0x3096),
    ("hsa", 0x3055),
    ("hsi", 0x3057),
    ("hsu", 0x3059),
    ("hse", 0x305b),
    ("hso", 0x305d),
    ("hyas", 0x3083),
    ("hyus", 0x3085),
    ("hyos", 0x3087),
    ("hwas", 0x308e),
    ("hq", 0x3063),
    ("ka", 0x30a2),
    ("ki", 0x30a4),
    ("ku", 0x30a6),
    ("ke", 0x30a8),
    ("ko", 0x30aa),
    ("kka", 0x30ab),
    ("kki", 0x30ad),
    ("kku", 0x30af),
    ("kke", 0x30b1),
    ("kko", 0x30b3),
    ("kga", 0x30ac),
    ("kgi", 0x30ae),
    ("kgu", 0x30b0),
    ("kge", 0x30b2),
    ("kgo", 0x30b4),
    ("kza", 0x30b6),
    ("kzi", 0x30b8),
    ("kji", 0x30b8),
    ("kzu", 0x30ba),
    ("kze", 0x30bc),
    ("kzo", 0x30be),
    ("kta", 0x30bf),
    ("kti", 0x30c1),
    ("ktu", 0x30c4),
    ("kte", 0x30c6),
    ("kto", 0x30c8),
    ("kda", 0x30c0),
    ("kdi", 0x30c2),
    ("kdu", 0x30c5),
    ("kde", 0x30c7),
    ("kdo", 0x30c9),
    ("kna", 0x30ca),
    ("kni", 0x30cb),
    ("knu", 0x30cc),
    ("kne", 0x30cd),
    ("kno", 0x30ce),
    ("kha", 0x30cf),
    ("khi", 0x30d2),
    ("khu", 0x30d5),
    ("khe", 0x30d8),
    ("kho", 0x30db),
    ("kba", 0x30d0),
    ("kbi", 0x30d3),
    ("kbu", 0x30d6),
    ("kbe", 0x30d9),
    ("kbo", 0x30dc),
    ("kpa", 0x30d1),
    ("kpi", 0x30d4),
    ("kpu", 0x30d7),
    ("kpe", 0x30da),
    ("kpo", 0x30dd),
    ("kma", 0x30de),
    ("kmi", 0x30df),
    ("kmu", 0x30e0),
    ("kme", 0x30e1),
    ("kmo", 0x30e2),
    ("kya", 0x30e4),
    ("kyu", 0x30e6),
    ("kyo", 0x30e8),
    ("kra", 0x30e9),
    ("kri", 0x30ea),
    ("kru", 0x30eb),
    ("kre", 0x30ec),
    ("kro", 0x30ed),
    ("kwa", 0x30ef),
    ("kwi", 0x30f0),
    ("kwe", 0x30f1),
    ("kwo", 0x30f2),
    ("kn", 0x30f3),
    ("kvu", 0x30f4),
    ("kas", 0x30a1),
    ("kis", 0x30a3),
    ("kus", 0x30a5),
    ("kes", 0x30a7),
    ("kos", 0x30a9),
    ("kkas", 0x30f5),
    ("kkes", 0x30f6),
    ("ksa", 0x30b5),
    ("ksi", 0x30b7),
    ("ksu", 0x30b9),
    ("kse", 0x30bb),
    ("kso", 0x30bd),
    ("kyas", 0x30e3),
    ("kyus", 0x30e5),
    ("kyos", 0x30e7),
    ("kwas", 0x30ee),
    ("kq", 0x30c3),
    ("kdot", 0x30FB),
    ("kdash", 0x30FC),
    ("nbsp", 0x00a0),
    ("delta", 0x0394),
    ("sigma", 0x03a3),
    ("omega", 0x03a9),
    ("angle", 0x2220),
    ("whiteheart", 0x2661),
    ("blackstar", 0x2605),
    ("whitestar", 0x2606),
    ("flipped-a", 0x2200),
    ("squared", 0x00b2),
    ("cubed", 0x00b3),
    ("oq", 0x201c),
    ("cq", 0x201d),
    ("leftarrow", 0x2190),
    ("uparrow", 0x2191),
    ("rightarrow", 0x2192),
    ("downarrow", 0x2193),
    ("4thnote", 0x2669),
    ("8thnote", 0x266A),
    ("b8thnote", 0x266B),
    ("b16thnote", 0x266C),
    ("flat", 0x266D),
    ("natural", 0x266E),
    ("sharp", 0x266F),
    ("up", INTERNAL_CODEPOINT),
    ("down", INTERNAL_CODEPOINT),
    ("left", INTERNAL_CODEPOINT),
    ("right", INTERNAL_CODEPOINT),
    ("downleft", INTERNAL_CODEPOINT),
    ("downright", INTERNAL_CODEPOINT),
    ("upleft", INTERNAL_CODEPOINT),
    ("upright", INTERNAL_CODEPOINT),
    ("center", INTERNAL_CODEPOINT),
    ("menuup", INTERNAL_CODEPOINT),
    ("menudown", INTERNAL_CODEPOINT),
    ("menuleft", INTERNAL_CODEPOINT),
    ("menuright", INTERNAL_CODEPOINT),
    ("start", INTERNAL_CODEPOINT),
    ("doublezeta", INTERNAL_CODEPOINT),
    ("planet", INTERNAL_CODEPOINT),
    ("back", INTERNAL_CODEPOINT),
    ("ok", INTERNAL_CODEPOINT),
    ("nextrow", INTERNAL_CODEPOINT),
    ("select", INTERNAL_CODEPOINT),
    ("auxx", INTERNAL_CODEPOINT),
    ("auxtriangle", INTERNAL_CODEPOINT),
    ("auxsquare", INTERNAL_CODEPOINT),
    ("auxcircle", INTERNAL_CODEPOINT),
    ("auxl1", INTERNAL_CODEPOINT),
    ("auxl2", INTERNAL_CODEPOINT),
    ("auxl3", INTERNAL_CODEPOINT),
    ("auxr1", INTERNAL_CODEPOINT),
    ("auxr2", INTERNAL_CODEPOINT),
    ("auxr3", INTERNAL_CODEPOINT),
    ("auxselect", INTERNAL_CODEPOINT),
    ("auxstart", INTERNAL_CODEPOINT),
    ("auxa", INTERNAL_CODEPOINT),
    ("auxb", INTERNAL_CODEPOINT),
    ("auxc", INTERNAL_CODEPOINT),
    ("auxd", INTERNAL_CODEPOINT),
    ("auxy", INTERNAL_CODEPOINT),
    ("auxz", INTERNAL_CODEPOINT),
    ("auxl", INTERNAL_CODEPOINT),
    ("auxr", INTERNAL_CODEPOINT),
    ("auxwhite", INTERNAL_CODEPOINT),
    ("auxblack", INTERNAL_CODEPOINT),
    ("auxlb", INTERNAL_CODEPOINT),
    ("auxrb", INTERNAL_CODEPOINT),
    ("auxlt", INTERNAL_CODEPOINT),
    ("auxrt", INTERNAL_CODEPOINT),
    ("auxback", INTERNAL_CODEPOINT),
];

#[inline(always)]
const fn lower_byte(b: u8) -> u8 {
    if b'A' <= b && b <= b'Z' { b + 32 } else { b }
}

const ALIAS_TABLE_LEN: usize = 512;

const fn alias_lengths_by_initial() -> [u16; 256] {
    let mut lengths = [0u16; 256];
    let mut index = 0;
    while index < ALIAS_ENTRIES.len() {
        let bytes = ALIAS_ENTRIES[index].0.as_bytes();
        if !bytes.is_empty() {
            lengths[lower_byte(bytes[0]) as usize] |= 1 << bytes.len();
        }
        index += 1;
    }
    lengths
}

const ALIAS_LENGTHS_BY_INITIAL: [u16; 256] = alias_lengths_by_initial();

#[inline(always)]
const fn alias_hash(bytes: &[u8]) -> usize {
    let mut hash = 2_166_136_261u32;
    let mut index = 0usize;
    while index < bytes.len() {
        hash ^= lower_byte(bytes[index]) as u32;
        hash = hash.wrapping_mul(16_777_619);
        index += 1;
    }
    hash as usize & (ALIAS_TABLE_LEN - 1)
}

const ALIAS_ENTRY_COUNT: usize = ALIAS_ENTRIES.len();
const ALIAS_EMPTY: u8 = u8::MAX;
const ALIAS_INDEX_COUNT: usize = ALIAS_EMPTY as usize;
const ALIAS_KEY_RADIX: u64 = 38;
const ALIAS_KEY_MAX_LEN: usize = 11;

struct AliasTable {
    slots: [u8; ALIAS_TABLE_LEN],
    keys: [u64; ALIAS_INDEX_COUNT],
    values: [u16; ALIAS_INDEX_COUNT],
}

const fn alias_eq(left: &str, right: &str) -> bool {
    let left = left.as_bytes();
    let right = right.as_bytes();
    if left.len() != right.len() {
        return false;
    }
    let mut index = 0usize;
    while index < left.len() {
        if left[index] != right[index] {
            return false;
        }
        index += 1;
    }
    true
}

#[inline(always)]
const fn alias_digit(byte: u8) -> u64 {
    let byte = lower_byte(byte);
    match byte {
        b'a'..=b'z' => (byte - b'a' + 1) as u64,
        b'0'..=b'9' => (byte - b'0' + 27) as u64,
        b'-' => 37,
        _ => 0,
    }
}

const fn alias_key(bytes: &[u8]) -> u64 {
    assert!(!bytes.is_empty() && bytes.len() <= ALIAS_KEY_MAX_LEN);
    let mut key = 0u64;
    let mut index = 0usize;
    while index < bytes.len() {
        let digit = alias_digit(bytes[index]);
        assert!(digit != 0);
        key = key * ALIAS_KEY_RADIX + digit;
        index += 1;
    }
    key
}

const fn alias_char(value: u32) -> char {
    match char::from_u32(value) {
        Some(value) => value,
        None => char::REPLACEMENT_CHARACTER,
    }
}

const fn build_alias_table() -> AliasTable {
    const { assert!(ALIAS_ENTRY_COUNT <= ALIAS_INDEX_COUNT) };
    let mut slots = [ALIAS_EMPTY; ALIAS_TABLE_LEN];
    let mut keys = [0u64; ALIAS_INDEX_COUNT];
    let mut values = [0u16; ALIAS_INDEX_COUNT];
    let mut next_internal = INTERNAL_CODEPOINT;
    let mut entry_index = 0usize;
    while entry_index < ALIAS_ENTRY_COUNT {
        let (alias, codepoint) = ALIAS_ENTRIES[entry_index];
        let value = if codepoint == INTERNAL_CODEPOINT {
            let current = next_internal;
            next_internal += 1;
            current
        } else {
            codepoint
        };
        let value = alias_char(value);
        assert!(value as u32 <= u16::MAX as u32);
        keys[entry_index] = alias_key(alias.as_bytes());
        values[entry_index] = value as u16;

        let mut slot = alias_hash(alias.as_bytes());
        let mut probes = 0usize;
        while probes < ALIAS_TABLE_LEN {
            let existing = slots[slot];
            if existing == ALIAS_EMPTY {
                slots[slot] = entry_index as u8;
                break;
            }
            if alias_eq(ALIAS_ENTRIES[existing as usize].0, alias) {
                values[existing as usize] = value as u16;
                break;
            }
            slot = (slot + 1) & (ALIAS_TABLE_LEN - 1);
            probes += 1;
        }
        assert!(probes < ALIAS_TABLE_LEN);
        entry_index += 1;
    }
    AliasTable {
        slots,
        keys,
        values,
    }
}

// Immutable process-lifetime table shared lock-free by translation callers. It
// is const-built (no warmup or miss work), uses fixed index/key/value arrays,
// never evicts, and resides in the binary until process teardown. Each lookup
// is bounded to 512 linear probes. The alias-table allocation and cycle
// benchmarks cover construction, storage, misses, and successful lookup.
static ALIAS_TABLE: AliasTable = build_alias_table();

#[inline(always)]
fn alias_hash_key(bytes: &[u8]) -> Option<(usize, u64)> {
    let mut hash = 2_166_136_261u32;
    let mut key = 0u64;
    for &byte in bytes {
        let byte = lower_byte(byte);
        let digit = alias_digit(byte);
        if digit == 0 {
            return None;
        }
        hash ^= u32::from(byte);
        hash = hash.wrapping_mul(16_777_619);
        key = key * ALIAS_KEY_RADIX + digit;
    }
    Some((hash as usize & (ALIAS_TABLE_LEN - 1), key))
}

#[inline(always)]
fn alias_slot_value(entry: usize) -> char {
    let value = u32::from(ALIAS_TABLE.values[entry]);
    debug_assert!(char::from_u32(value).is_some());
    // SAFETY: `build_alias_table` stores only valid BMP `char` values, and the
    // table is immutable, so every occupied slot indexes a valid encoded char.
    unsafe { char::from_u32_unchecked(value) }
}

#[inline(always)]
fn alias_lookup(element: &str) -> Option<char> {
    let bytes = element.as_bytes();
    if bytes.is_empty() {
        return None;
    }
    if bytes.len() > ALIAS_KEY_MAX_LEN
        || ALIAS_LENGTHS_BY_INITIAL[usize::from(lower_byte(bytes[0]))] & (1 << bytes.len()) == 0
    {
        return None;
    }
    let (mut index, key) = alias_hash_key(bytes)?;
    for _ in 0..ALIAS_TABLE_LEN {
        let entry = ALIAS_TABLE.slots[index];
        if entry == ALIAS_EMPTY {
            return None;
        }
        let entry = entry as usize;
        if ALIAS_TABLE.keys[entry] == key {
            return Some(alias_slot_value(entry));
        }
        index = (index + 1) & (ALIAS_TABLE_LEN - 1);
    }
    None
}

#[inline(always)]
fn marker_end(remaining: &[u8]) -> Option<usize> {
    memchr2(b'&', b';', remaining).filter(|&index| remaining[index] == b';')
}

#[inline(always)]
fn parse_numeric_marker(element: &str, invalid: char) -> Option<char> {
    let bytes = element.as_bytes();
    if bytes.is_empty() {
        return None;
    }
    let (hex, digits_start) = match bytes[0] {
        b'#' => {
            if bytes.len() < 2 {
                return None;
            }
            if bytes[1] == b'x' || bytes[1] == b'X' {
                (true, 2)
            } else {
                (false, 1)
            }
        }
        b'x' | b'X' => (true, 1),
        _ => return None,
    };
    if digits_start >= bytes.len() {
        return None;
    }

    let mut value = 0u32;
    let mut overflow = false;
    if hex {
        for &b in &bytes[digits_start..] {
            let digit = match b {
                b'0'..=b'9' => u32::from(b - b'0'),
                b'a'..=b'f' => u32::from(b - b'a' + 10),
                b'A'..=b'F' => u32::from(b - b'A' + 10),
                _ => return None,
            };
            if !overflow {
                if let Some(next) = value.checked_mul(16).and_then(|v| v.checked_add(digit)) {
                    value = next;
                } else {
                    overflow = true;
                }
            }
        }
    } else {
        for &b in &bytes[digits_start..] {
            if !b.is_ascii_digit() {
                return None;
            }
            let digit = u32::from(b - b'0');
            if !overflow {
                if let Some(next) = value.checked_mul(10).and_then(|v| v.checked_add(digit)) {
                    value = next;
                } else {
                    overflow = true;
                }
            }
        }
    }

    if overflow || value > 0xFFFF {
        value = INVALID_CODEPOINT;
    }
    Some(char::from_u32(value).unwrap_or(invalid))
}

/// Replace &alias; markers and unicode markers in place, matching `ITGmania` behavior.
// Keep the scan/copy state machine inline so its cursors cannot diverge across buffer helpers.
pub fn replace_markers_in_place(text: &mut String) {
    if !text.contains('&') {
        return;
    }
    let mut bytes = std::mem::take(text).into_bytes();
    let len = bytes.len();
    let invalid = char::REPLACEMENT_CHARACTER;
    let mut output_len = None;
    let mut scan = 0usize;
    let mut copy_from = 0usize;

    while scan < len {
        let Some(start) = memchr::memchr(b'&', &bytes[scan..len]).map(|pos| scan + pos) else {
            break;
        };
        let after_amp = start + 1;
        if after_amp >= len {
            break;
        }
        let Some(end_idx) = marker_end(&bytes[after_amp..len]).map(|end| after_amp + end) else {
            scan = after_amp;
            continue;
        };
        // SAFETY: `bytes` came from a valid `String`, and ASCII marker delimiters
        // can only occur at UTF-8 code-point boundaries.
        let element = unsafe { std::str::from_utf8_unchecked(&bytes[after_amp..end_idx]) };
        let replacement = match element.as_bytes().first() {
            Some(b'#' | b'x' | b'X') => parse_numeric_marker(element, invalid),
            _ => alias_lookup(element),
        };
        let Some(replacement) = replacement else {
            scan = end_idx + 1;
            continue;
        };

        let write = output_len.get_or_insert(start);
        if copy_from != 0 {
            let pending = start - copy_from;
            bytes.copy_within(copy_from..start, *write);
            *write += pending;
        }
        let mut encoded = [0; 4];
        let replacement = replacement.encode_utf8(&mut encoded).as_bytes();
        debug_assert!(
            replacement.len() <= end_idx + 1 - start,
            "marker replacement cannot exceed its encoded marker"
        );
        bytes[*write..*write + replacement.len()].copy_from_slice(replacement);
        *write += replacement.len();
        copy_from = end_idx + 1;
        scan = copy_from;
    }

    if let Some(mut output_len) = output_len {
        bytes.copy_within(copy_from..len, output_len);
        output_len += len - copy_from;
        bytes.truncate(output_len);
    }
    // SAFETY: compaction copies slices from the original valid `String` and
    // inserts only bytes produced by `char::encode_utf8`.
    *text = unsafe { String::from_utf8_unchecked(bytes) };
}

/// Replace &alias; markers and unicode markers, returning an updated string.
#[must_use]
pub fn replace_markers(text: &str) -> String {
    let mut out = text.to_string();
    replace_markers_in_place(&mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::replace_markers_in_place;

    #[test]
    fn numeric_markers_are_replaced_in_place() {
        let mut text = "before &#65; and &#x266F; after".to_string();
        replace_markers_in_place(&mut text);
        assert_eq!(text, "before A and ♯ after");
    }

    #[test]
    fn unknown_markers_preserve_the_existing_buffer() {
        let mut text = String::with_capacity(1_024);
        text.push_str("prefix &unknown; suffix && trailing &");
        let pointer = text.as_ptr();
        let capacity = text.capacity();

        replace_markers_in_place(&mut text);

        assert_eq!(text, "prefix &unknown; suffix && trailing &");
        assert_eq!(text.as_ptr(), pointer);
        assert_eq!(text.capacity(), capacity);
    }
}

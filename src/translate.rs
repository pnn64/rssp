use std::collections::HashMap;
use std::sync::OnceLock;

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

fn alias_map() -> &'static HashMap<String, String> {
    static MAP: OnceLock<HashMap<String, String>> = OnceLock::new();
    MAP.get_or_init(|| {
        let mut map = HashMap::with_capacity(ALIAS_ENTRIES.len());
        let mut next_internal = INTERNAL_CODEPOINT;
        for (alias, codepoint) in ALIAS_ENTRIES {
            let value = if *codepoint == INTERNAL_CODEPOINT {
                let current = next_internal;
                next_internal += 1;
                current
            } else {
                *codepoint
            };
            let key = alias.to_ascii_lowercase();
            let ch = char::from_u32(value).unwrap_or(char::from_u32(INVALID_CODEPOINT).unwrap());
            map.insert(key, ch.to_string());
        }
        map
    })
}

fn replace_entity_text(text: &mut String) {
    let aliases = alias_map();
    if !text.contains('&') {
        return;
    }
    let mut out = String::with_capacity(text.len());
    let mut offset = 0;
    while offset < text.len() {
        let start = match text[offset..].find('&') {
            Some(pos) => offset + pos,
            None => {
                out.push_str(&text[offset..]);
                *text = out;
                return;
            }
        };
        out.push_str(&text[offset..start]);
        let rest = &text[start + 1..];
        let next_amp = rest.find('&');
        let next_semi = rest.find(';');
        let end = match (next_amp, next_semi) {
            (Some(a), Some(s)) => if a < s { None } else { Some(start + 1 + s) },
            (Some(_), None) => None,
            (None, Some(s)) => Some(start + 1 + s),
            (None, None) => None,
        };
        if let Some(end_idx) = end {
            let element = &text[start + 1..end_idx];
            let key = element.to_ascii_lowercase();
            if let Some(repl) = aliases.get(&key) {
                out.push_str(repl);
            } else {
                out.push_str(&text[start..=end_idx]);
            }
            offset = end_idx + 1;
        } else {
            out.push('&');
            offset = start + 1;
        }
    }
    *text = out;
}

fn replace_unicode_markers(text: &mut String) {
    let mut start = 0;
    loop {
        if start >= text.len() {
            break;
        }
        let dec_pos = text[start..].find("&#");
        let (pos, hex) = if let Some(d) = dec_pos {
            (start + d, false)
        } else if let Some(h) = text[start..].find("&x") {
            (start + h, true)
        } else {
            break;
        };
        start = pos + 1;
        let mut p = pos + 2;
        let bytes = text.as_bytes();
        let mut digits = 0;
        while p < text.len() {
            let b = bytes[p];
            let ok = if hex {
                b.is_ascii_hexdigit()
            } else {
                b.is_ascii_digit()
            };
            if !ok {
                break;
            }
            p += 1;
            digits += 1;
        }
        if digits == 0 || p >= text.len() || bytes[p] != b';' {
            continue;
        }
        let num_str = &text[pos + 2..p];
        let mut value = if hex {
            u32::from_str_radix(num_str, 16).unwrap_or(INVALID_CODEPOINT)
        } else {
            num_str.parse::<u32>().unwrap_or(INVALID_CODEPOINT)
        };
        if value > 0xFFFF {
            value = INVALID_CODEPOINT;
        }
        let ch = char::from_u32(value).unwrap_or(char::from_u32(INVALID_CODEPOINT).unwrap());
        text.replace_range(pos..=p, ch.to_string().as_str());
    }
}

/// Replace &alias; markers and unicode markers in place, matching ITGmania behavior.
pub fn replace_markers_in_place(text: &mut String) {
    replace_entity_text(text);
    replace_unicode_markers(text);
}

/// Replace &alias; markers and unicode markers, returning an updated string.
pub fn replace_markers(text: &str) -> String {
    let mut out = text.to_string();
    replace_markers_in_place(&mut out);
    out
}

use std::borrow::Cow;
use std::io;

use crate::timing::{STEPFILE_VERSION_NUMBER, TimingFormat};

#[must_use]
pub fn strip_title_tags(mut title: &str) -> Cow<'_, str> {
    loop {
        let original = title;
        title = title.trim_start();

        if let Some(rest) = title.strip_prefix('[').and_then(|s| s.split_once(']')) {
            title = rest.1.trim_start();
            continue;
        }

        if let Some(pos) = title.find("- ")
            && title[..pos].chars().all(|c| c.is_ascii_digit() || c == '.')
        {
            title = title[pos + 2..].trim_start();
            continue;
        }

        if title == original {
            break;
        }
    }
    Cow::Borrowed(title)
}

#[must_use]
pub fn clean_tag(tag: &str) -> Cow<'_, str> {
    let mut iter = tag.char_indices();
    while let Some((i, c)) = iter.next() {
        if c.is_control() {
            let mut out = String::with_capacity(tag.len());
            out.push_str(&tag[..i]);
            for (_, ch) in iter {
                if !ch.is_control() {
                    out.push(ch);
                }
            }
            return Cow::Owned(out);
        }
    }
    Cow::Borrowed(tag)
}

#[must_use]
pub fn unescape_tag(tag: &str) -> Cow<'_, str> {
    if !tag.as_bytes().contains(&b'\\') {
        return Cow::Borrowed(tag);
    }
    let mut out = String::with_capacity(tag.len());
    let mut chars = tag.chars();
    while let Some(c) = chars.next() {
        out.push(if c == '\\' {
            chars.next().unwrap_or(c)
        } else {
            c
        });
    }
    Cow::Owned(out)
}

fn unescape_owned(mut value: String) -> String {
    if !value.as_bytes().contains(&b'\\') {
        return value;
    }
    let mut escaped = false;
    value.retain(|ch| {
        if escaped {
            escaped = false;
            true
        } else if ch == '\\' {
            escaped = true;
            false
        } else {
            true
        }
    });
    if escaped {
        value.push('\\');
    }
    value
}

#[must_use]
pub fn decode_unescape(bytes: &[u8]) -> Cow<'_, str> {
    match decode_bytes(bytes) {
        Cow::Borrowed(value) => unescape_tag(value),
        Cow::Owned(value) => Cow::Owned(unescape_owned(value)),
    }
}

#[must_use]
pub fn unescape_trim_cow(tag: &str) -> Cow<'_, str> {
    match unescape_tag(tag) {
        Cow::Borrowed(value) => Cow::Borrowed(value.trim()),
        Cow::Owned(mut value) => {
            trim_string_in_place(&mut value);
            Cow::Owned(value)
        }
    }
}

#[must_use]
pub fn unescape_trim(tag: &str) -> String {
    unescape_trim_cow(tag).into_owned()
}

#[must_use]
pub fn decode_unescape_trim(bytes: &[u8]) -> Cow<'_, str> {
    match decode_unescape(bytes) {
        Cow::Borrowed(value) => Cow::Borrowed(value.trim()),
        Cow::Owned(mut value) => {
            trim_string_in_place(&mut value);
            Cow::Owned(value)
        }
    }
}

fn trim_string_in_place(value: &mut String) {
    let trimmed = value.trim();
    let start = trimmed.as_ptr() as usize - value.as_ptr() as usize;
    let end = start + trimmed.len();
    value.truncate(end);
    if start != 0 {
        value.drain(..start);
    }
}

const CP1252_MAP: [u16; 32] = [
    0x20AC, 0xFFFD, 0x201A, 0x0192, 0x201E, 0x2026, 0x2020, 0x2021, 0x02C6, 0x2030, 0x0160, 0x2039,
    0x0152, 0xFFFD, 0x017D, 0xFFFD, 0xFFFD, 0x2018, 0x2019, 0x201C, 0x201D, 0x2022, 0x2013, 0x2014,
    0x02DC, 0x2122, 0x0161, 0x203A, 0x0153, 0xFFFD, 0x017E, 0x0178,
];

const CP1252_UTF8_EXTRA: [u8; 128] = {
    let mut extra = [1u8; 128];
    let mut idx = 0usize;
    while idx < CP1252_MAP.len() {
        extra[idx] = if CP1252_MAP[idx] <= 0x07ff { 1 } else { 2 };
        idx += 1;
    }
    extra
};

const TAG_CH_BS: u8 = 1;
const TAG_CH_SEMI: u8 = 1 << 1;
const TAG_CH_COLON: u8 = 1 << 2;
const TAG_CH_NL: u8 = 1 << 3;

const TAG_CHAR_CLASS: [u8; 256] = {
    let mut t = [0u8; 256];
    t[b'\\' as usize] = TAG_CH_BS;
    t[b';' as usize] = TAG_CH_SEMI;
    t[b':' as usize] = TAG_CH_COLON;
    t[b'\n' as usize] = TAG_CH_NL;
    t[b'\r' as usize] = TAG_CH_NL;
    t
};

#[inline(always)]
fn cp1252_char(byte: u8) -> char {
    match byte {
        0x00..=0x7F => byte as char,
        0x80..=0x9F => {
            char::from_u32(u32::from(CP1252_MAP[(byte - 0x80) as usize])).unwrap_or('\u{FFFD}')
        }
        _ => char::from_u32(u32::from(byte)).unwrap_or('\u{FFFD}'),
    }
}

#[inline]
fn cp1252_utf8_len(bytes: &[u8]) -> usize {
    const HIGH_BITS: u64 = 0x8080_8080_8080_8080;

    let mut utf8_len = bytes.len();
    let (chunks, remainder) = bytes.as_chunks::<8>();
    for chunk in chunks {
        if u64::from_ne_bytes(*chunk) & HIGH_BITS == 0 {
            continue;
        }
        for &byte in chunk {
            if byte >= 0x80 {
                utf8_len += usize::from(CP1252_UTF8_EXTRA[(byte - 0x80) as usize]);
            }
        }
    }
    for &byte in remainder {
        if byte >= 0x80 {
            utf8_len += usize::from(CP1252_UTF8_EXTRA[(byte - 0x80) as usize]);
        }
    }
    utf8_len
}

fn decode_cp1252(bytes: &[u8]) -> String {
    let mut decoded = String::with_capacity(cp1252_utf8_len(bytes));
    let mut ascii_start = 0usize;
    for (idx, &byte) in bytes.iter().enumerate() {
        if byte < 0x80 {
            continue;
        }
        if ascii_start < idx {
            // Every byte in this run was checked by the branch above.
            let ascii = unsafe { std::str::from_utf8_unchecked(&bytes[ascii_start..idx]) };
            decoded.push_str(ascii);
        }
        decoded.push(cp1252_char(byte));
        ascii_start = idx + 1;
    }
    if ascii_start < bytes.len() {
        // The loop found no high byte in the remaining run.
        let ascii = unsafe { std::str::from_utf8_unchecked(&bytes[ascii_start..]) };
        decoded.push_str(ascii);
    }
    decoded
}

pub fn decode_bytes(bytes: &[u8]) -> Cow<'_, str> {
    std::str::from_utf8(bytes).map_or_else(|_| Cow::Owned(decode_cp1252(bytes)), Cow::Borrowed)
}

#[must_use]
pub fn parse_offset_seconds(offset: Option<&[u8]>) -> f64 {
    offset
        .and_then(|b| std::str::from_utf8(b).ok())
        .and_then(|s| s.parse::<f64>().ok())
        .map_or(0.0, |f| f64::from(f as f32))
}

pub(crate) fn parse_float_prefix(s: &str) -> Option<f64> {
    let b = s.trim_start().as_bytes();
    let mut i = usize::from(b.first().is_some_and(|&c| c == b'+' || c == b'-'));

    let start = i;
    while i < b.len() && b[i].is_ascii_digit() {
        i += 1;
    }
    if i < b.len() && b[i] == b'.' {
        i += 1;
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
    }
    if i == start || (i == start + 1 && !b[start].is_ascii_digit()) {
        return None;
    }
    if i < b.len() && matches!(b[i], b'e' | b'E') {
        let e = i;
        i += 1;
        if i < b.len() && matches!(b[i], b'+' | b'-') {
            i += 1;
        }
        let exponent = i;
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
        if exponent == i {
            i = e;
        }
    }
    std::str::from_utf8(&b[..i])
        .ok()?
        .parse()
        .ok()
        .map(|value: f64| if value.is_finite() { value } else { 0.0 })
}

#[must_use]
pub fn parse_version(version: Option<&[u8]>, fmt: TimingFormat) -> f32 {
    version
        .and_then(|b| std::str::from_utf8(b).ok())
        .and_then(parse_float_prefix)
        .map_or(
            if fmt == TimingFormat::Ssc {
                f32::NAN
            } else {
                STEPFILE_VERSION_NUMBER
            },
            |version| version as f32,
        )
}

pub const SSC_VERSION_CHART_NAME_TAG: f32 = 0.74;

#[must_use]
pub fn normalize_chart_desc(desc: String, fmt: TimingFormat, ver: f32) -> String {
    if normalize_chart_desc_ref(&desc, fmt, ver).is_empty() && !desc.is_empty() {
        String::new()
    } else {
        desc
    }
}

#[must_use]
pub fn normalize_chart_desc_ref(desc: &str, fmt: TimingFormat, ver: f32) -> &str {
    if fmt == TimingFormat::Ssc && ver < SSC_VERSION_CHART_NAME_TAG {
        ""
    } else {
        desc
    }
}

#[must_use]
pub fn normalize_chart_name(chart_name: String, desc: &str, fmt: TimingFormat, ver: f32) -> String {
    if fmt == TimingFormat::Ssc && ver < SSC_VERSION_CHART_NAME_TAG {
        desc.to_string()
    } else {
        chart_name
    }
}

type TagBytes<'a> = Cow<'a, [u8]>;

#[derive(Default)]
pub struct ParsedChartEntry<'a> {
    pub field_count: u8,
    pub fields: [&'a [u8]; 5],
    pub chart_name: Option<&'a [u8]>,
    pub chart_style: Option<&'a [u8]>,
    pub note_data: &'a [u8],
    pub chart_music: Option<TagBytes<'a>>,
    pub chart_attacks: Option<TagBytes<'a>>,
    pub chart_bpms: Option<TagBytes<'a>>,
    pub chart_stops: Option<TagBytes<'a>>,
    pub chart_delays: Option<TagBytes<'a>>,
    pub chart_warps: Option<TagBytes<'a>>,
    pub chart_speeds: Option<TagBytes<'a>>,
    pub chart_scrolls: Option<TagBytes<'a>>,
    pub chart_fakes: Option<TagBytes<'a>>,
    pub chart_offset: Option<TagBytes<'a>>,
    pub chart_display_bpm: Option<TagBytes<'a>>,
    pub chart_time_signatures: Option<TagBytes<'a>>,
    pub chart_labels: Option<TagBytes<'a>>,
    pub chart_tickcounts: Option<TagBytes<'a>>,
    pub chart_combos: Option<TagBytes<'a>>,
    pub chart_radar_values: Option<TagBytes<'a>>,
}

#[derive(Default)]
pub struct ParsedSimfileData<'a> {
    pub title: Option<&'a [u8]>,
    pub subtitle: Option<&'a [u8]>,
    pub artist: Option<&'a [u8]>,
    pub genre: Option<&'a [u8]>,
    pub title_translit: Option<&'a [u8]>,
    pub subtitle_translit: Option<&'a [u8]>,
    pub artist_translit: Option<&'a [u8]>,
    pub version: Option<&'a [u8]>,
    pub offset: Option<&'a [u8]>,
    pub origin: Option<&'a [u8]>,
    pub credit: Option<&'a [u8]>,
    pub attacks: Option<TagBytes<'a>>,
    pub bpms: Option<&'a [u8]>,
    pub stops: Option<&'a [u8]>,
    pub delays: Option<&'a [u8]>,
    pub warps: Option<&'a [u8]>,
    pub speeds: Option<&'a [u8]>,
    pub scrolls: Option<&'a [u8]>,
    pub fakes: Option<&'a [u8]>,
    pub time_signatures: Option<&'a [u8]>,
    pub labels: Option<&'a [u8]>,
    pub tickcounts: Option<&'a [u8]>,
    pub combos: Option<&'a [u8]>,
    pub banner: Option<&'a [u8]>,
    pub background: Option<&'a [u8]>,
    pub cdtitle: Option<&'a [u8]>,
    pub jacket: Option<&'a [u8]>,
    pub music: Option<&'a [u8]>,
    pub sample_start: Option<&'a [u8]>,
    pub sample_length: Option<&'a [u8]>,
    pub display_bpm: Option<&'a [u8]>,
    pub selectable: Option<&'a [u8]>,
    pub lyricspath: Option<&'a [u8]>,
    pub previewvid: Option<&'a [u8]>,
    pub cdimage: Option<&'a [u8]>,
    pub discimage: Option<&'a [u8]>,
    pub bgchanges: Option<&'a [u8]>,
    pub fgchanges: Option<&'a [u8]>,
    pub keysounds: Option<&'a [u8]>,
    pub last_second_hint: Option<&'a [u8]>,
    pub notes_list: Vec<ParsedChartEntry<'a>>,
}

#[derive(Default)]
struct NotedataFields<'a> {
    step_type: Option<&'a [u8]>,
    chart_name: Option<&'a [u8]>,
    chart_style: Option<&'a [u8]>,
    description: Option<&'a [u8]>,
    credit: Option<&'a [u8]>,
    difficulty: Option<&'a [u8]>,
    meter: Option<&'a [u8]>,
    notes: Option<&'a [u8]>,
    notes2: Option<&'a [u8]>,
    chart_music: Option<&'a [u8]>,
    chart_attacks: Option<TagBytes<'a>>,
    chart_bpms: Option<&'a [u8]>,
    chart_stops: Option<&'a [u8]>,
    chart_freezes: Option<&'a [u8]>,
    chart_delays: Option<&'a [u8]>,
    chart_warps: Option<&'a [u8]>,
    chart_speeds: Option<&'a [u8]>,
    chart_scrolls: Option<&'a [u8]>,
    chart_fakes: Option<&'a [u8]>,
    chart_offset: Option<&'a [u8]>,
    chart_display_bpm: Option<&'a [u8]>,
    chart_time_signatures: Option<&'a [u8]>,
    chart_labels: Option<&'a [u8]>,
    chart_tickcounts: Option<&'a [u8]>,
    chart_combos: Option<&'a [u8]>,
    chart_radar_values: Option<&'a [u8]>,
}

#[inline(always)]
fn starts_with_ci(slice: &[u8], tag: &[u8]) -> bool {
    slice
        .get(..tag.len())
        .is_some_and(|head| head.eq_ignore_ascii_case(tag))
}

#[inline(always)]
fn find_byte(slice: &[u8], needle: u8) -> Option<usize> {
    let mut i = 0usize;
    let (chunks, rem) = slice.as_chunks::<8>();
    for chunk in chunks {
        let word = u64::from_le_bytes(*chunk);
        let hits = byte_hits(word, needle);
        if hits != 0 {
            return Some(i + hits.trailing_zeros() as usize / 8);
        }
        i += 8;
    }
    for (j, &b) in rem.iter().enumerate() {
        if b == needle {
            return Some(i + j);
        }
    }
    None
}

#[inline(always)]
fn find_either_byte(slice: &[u8], a: u8, b: u8) -> Option<usize> {
    let mut i = 0usize;
    let (chunks, rem) = slice.as_chunks::<8>();
    for chunk in chunks {
        let word = u64::from_le_bytes(*chunk);
        let hits = byte_hits(word, a) | byte_hits(word, b);
        if hits != 0 {
            return Some(i + hits.trailing_zeros() as usize / 8);
        }
        i += 8;
    }
    for (j, &x) in rem.iter().enumerate() {
        if x == a || x == b {
            return Some(i + j);
        }
    }
    None
}

#[inline(always)]
fn find_three_byte(slice: &[u8], a: u8, b: u8, c: u8) -> Option<usize> {
    let mut i = 0usize;
    let (chunks, rem) = slice.as_chunks::<8>();
    for chunk in chunks {
        let word = u64::from_le_bytes(*chunk);
        let hits = byte_hits(word, a) | byte_hits(word, b) | byte_hits(word, c);
        if hits != 0 {
            return Some(i + hits.trailing_zeros() as usize / 8);
        }
        i += 8;
    }
    for (j, &x) in rem.iter().enumerate() {
        if x == a || x == b || x == c {
            return Some(i + j);
        }
    }
    None
}

#[inline(always)]
fn byte_hits(word: u64, byte: u8) -> u64 {
    // Marks the high bit of each byte lane equal to `byte`.
    const LO: u64 = 0x0101_0101_0101_0101;
    const HI: u64 = 0x8080_8080_8080_8080;
    let x = word ^ (u64::from(byte) * LO);
    x.wrapping_sub(LO) & !x & HI
}

/// Finds the first unescaped `;` in `slice` when there is no `#` before it.
#[inline(always)]
fn find_unescaped_semi_no_hash(slice: &[u8]) -> Option<usize> {
    let mut off = 0usize;
    let mut has_hash = false;
    while off < slice.len() {
        let rel = find_either_byte(&slice[off..], b';', b'#')?;
        let idx = off + rel;
        let b = slice[idx];
        if b == b'#' {
            has_hash = true;
            off = idx + 1;
            continue;
        }
        let mut bs = 0usize;
        let mut i = idx;
        while i > 0 && slice[i - 1] == b'\\' {
            bs += 1;
            i -= 1;
        }
        if bs & 1 == 0 {
            return (!has_hash).then_some(idx);
        }
        off = idx + 1;
    }
    None
}

/// Returns (`value_end`, `next_position`) if terminator found.
#[inline(always)]
fn scan_tag_end(slice: &[u8], allow_nl: bool) -> Option<(usize, usize)> {
    if allow_nl && let Some(end) = find_unescaped_semi_no_hash(slice) {
        return Some((end, end + 1));
    }

    let mut i = 0;
    let mut bs_odd = false;
    while i < slice.len() {
        let b = slice[i];
        let class = TAG_CHAR_CLASS[b as usize];
        if class == 0 || (allow_nl && class == TAG_CH_COLON) {
            bs_odd = false;
            i += 1;
            continue;
        }

        if class & TAG_CH_BS != 0 {
            bs_odd = !bs_odd;
            i += 1;
            continue;
        }

        let escaped = bs_odd;
        bs_odd = false;

        if class & TAG_CH_SEMI != 0 {
            if !escaped {
                return Some((i, i + 1));
            }
            i += 1;
            continue;
        }

        if class & TAG_CH_COLON != 0 {
            if !escaped {
                return Some((i, i + 1));
            }
            i += 1;
            continue;
        }

        let mut j = i + 1;
        if b == b'\r' && slice.get(j) == Some(&b'\n') {
            j += 1;
        }
        while j < slice.len()
            && slice[j].is_ascii_whitespace()
            && !matches!(slice[j], b'\n' | b'\r')
        {
            j += 1;
        }
        if slice.get(j) == Some(&b'#') {
            return Some((i, j));
        }
        if !allow_nl && slice.get(j) != Some(&b';') {
            return None;
        }
        i += 1;
    }
    None
}

/// Unified tag parser: returns value slice and advance amount.
#[inline(always)]
fn parse_tag_val(data: &[u8], tag_len: usize, allow_nl: bool) -> Option<(&[u8], usize)> {
    let slice = data.get(tag_len..)?;
    let (end, next) = scan_tag_end(slice, allow_nl)?;
    Some((&slice[..end], tag_len + next))
}

#[inline(always)]
fn try_tag_step<'a>(
    s: &'a [u8],
    tag: &[u8],
    out: &mut Option<&'a [u8]>,
    enabled: bool,
) -> Option<usize> {
    if !enabled || !starts_with_ci(s, tag) {
        return None;
    }
    if let Some((v, adv)) = parse_tag_val(s, tag.len(), true) {
        *out = Some(v);
        Some(adv)
    } else {
        Some(1)
    }
}

#[inline(always)]
fn try_tag_adv<'a>(s: &'a [u8], tag: &[u8], nl: bool, out: &mut Option<&'a [u8]>) -> Option<usize> {
    if !starts_with_ci(s, tag) {
        return None;
    }
    let (val, adv) = parse_tag_val(s, tag.len(), nl)?;
    *out = Some(val);
    Some(adv)
}

#[inline]
fn try_tag_append<'a>(
    s: &'a [u8],
    tag: &[u8],
    nl: bool,
    out: &mut Option<TagBytes<'a>>,
) -> Option<usize> {
    if !starts_with_ci(s, tag) {
        return None;
    }
    let (value, adv) = parse_tag_val(s, tag.len(), nl)?;
    *out = Some(match out.take() {
        None => Cow::Borrowed(value),
        Some(Cow::Borrowed(previous)) => {
            let mut joined = Vec::with_capacity(previous.len() + 1 + value.len());
            joined.extend_from_slice(previous);
            joined.push(b':');
            joined.extend_from_slice(value);
            Cow::Owned(joined)
        }
        Some(previous) => {
            let mut joined = previous.into_owned();
            joined.reserve(1 + value.len());
            joined.push(b':');
            joined.extend_from_slice(value);
            Cow::Owned(joined)
        }
    });
    Some(adv)
}

macro_rules! try_tags {
    ($s:expr, $i:expr, $o:expr, [ $( ($tag:expr, $field:ident, $nl:expr) ),* $(,)? ]) => {{
        $( if let Some(a) = try_tag_adv($s, $tag, $nl, &mut $o.$field) { $i += a; continue; } )*
    }};
}

macro_rules! return_tags {
    ($s:expr, $o:expr, [ $( ($tag:expr, $field:ident, $nl:expr) ),* $(,)? ]) => {{
        $( if let Some(advance) = try_tag_adv($s, $tag, $nl, &mut $o.$field) {
            return Some(advance);
        } )*
    }};
}

macro_rules! return_header_tags {
    ($s:expr, $o:expr, [ $( ($tag:expr, $field:ident, $on:expr) ),* $(,)? ]) => {{
        $( if let Some(advance) = try_tag_step($s, $tag, &mut $o.$field, $on) {
            return Some(advance);
        } )*
    }};
}

#[inline(never)]
fn dispatch_notedata_tag<'a>(s: &'a [u8], out: &mut NotedataFields<'a>) -> Option<usize> {
    match s.get(1).map_or(0, u8::to_ascii_uppercase) {
        b'A' => {
            if let Some(advance) = try_tag_append(s, b"#ATTACKS:", true, &mut out.chart_attacks) {
                return Some(advance);
            }
        }
        b'B' => return_tags!(s, out, [(b"#BPMS:", chart_bpms, true)]),
        b'C' => return_tags!(
            s,
            out,
            [
                (b"#CHARTNAME:", chart_name, false),
                (b"#CHARTSTYLE:", chart_style, false),
                (b"#CREDIT:", credit, false),
                (b"#COMBOS:", chart_combos, true),
            ]
        ),
        b'D' => return_tags!(
            s,
            out,
            [
                (b"#DESCRIPTION:", description, false),
                (b"#DIFFICULTY:", difficulty, false),
                (b"#DELAYS:", chart_delays, true),
                (b"#DISPLAYBPM:", chart_display_bpm, true),
            ]
        ),
        b'F' => return_tags!(
            s,
            out,
            [
                (b"#FREEZES:", chart_freezes, true),
                (b"#FAKES:", chart_fakes, true),
            ]
        ),
        b'L' => return_tags!(s, out, [(b"#LABELS:", chart_labels, true)]),
        b'M' => return_tags!(
            s,
            out,
            [(b"#METER:", meter, false), (b"#MUSIC:", chart_music, true),]
        ),
        b'O' => return_tags!(s, out, [(b"#OFFSET:", chart_offset, true)]),
        b'R' => return_tags!(s, out, [(b"#RADARVALUES:", chart_radar_values, true)]),
        b'S' => return_tags!(
            s,
            out,
            [
                (b"#STEPSTYPE:", step_type, false),
                (b"#STOPS:", chart_stops, true),
                (b"#SPEEDS:", chart_speeds, true),
                (b"#SCROLLS:", chart_scrolls, true),
            ]
        ),
        b'T' => return_tags!(
            s,
            out,
            [
                (b"#TIMESIGNATURES:", chart_time_signatures, true),
                (b"#TICKCOUNTS:", chart_tickcounts, true),
            ]
        ),
        b'W' => return_tags!(s, out, [(b"#WARPS:", chart_warps, true)]),
        _ => {}
    }
    None
}

#[inline(never)]
fn dispatch_header_tag<'a>(
    s: &'a [u8],
    ssc: bool,
    out: &mut ParsedSimfileData<'a>,
) -> Option<usize> {
    match s.get(1).map_or(0, u8::to_ascii_uppercase) {
        b'A' => {
            if let Some(advance) = try_tag_append(s, b"#ATTACKS:", true, &mut out.attacks) {
                return Some(advance);
            }
            return_header_tags!(
                s,
                out,
                [
                    (b"#ARTIST:", artist, true),
                    (b"#ARTISTTRANSLIT:", artist_translit, true),
                ]
            );
        }
        b'B' => return_header_tags!(
            s,
            out,
            [
                (b"#BPMS:", bpms, true),
                (b"#BANNER:", banner, true),
                (b"#BACKGROUND:", background, true),
                (b"#BGCHANGES:", bgchanges, true),
            ]
        ),
        b'C' => return_header_tags!(
            s,
            out,
            [
                (b"#CREDIT:", credit, true),
                (b"#CDTITLE:", cdtitle, true),
                (b"#CDIMAGE:", cdimage, ssc),
                (b"#COMBOS:", combos, ssc),
            ]
        ),
        b'D' => return_header_tags!(
            s,
            out,
            [
                (b"#DELAYS:", delays, true),
                (b"#DISPLAYBPM:", display_bpm, true),
                (b"#DISCIMAGE:", discimage, ssc),
            ]
        ),
        b'F' => return_header_tags!(
            s,
            out,
            [
                (b"#FREEZES:", stops, true),
                (b"#FGCHANGES:", fgchanges, true),
                (b"#FAKES:", fakes, ssc),
            ]
        ),
        b'G' => return_header_tags!(s, out, [(b"#GENRE:", genre, true)]),
        b'J' => return_header_tags!(s, out, [(b"#JACKET:", jacket, true)]),
        b'K' => return_header_tags!(s, out, [(b"#KEYSOUNDS:", keysounds, true)]),
        b'L' => return_header_tags!(
            s,
            out,
            [
                (b"#LYRICSPATH:", lyricspath, true),
                (b"#LABELS:", labels, ssc),
                (b"#LASTSECONDHINT:", last_second_hint, ssc),
            ]
        ),
        b'M' => return_header_tags!(s, out, [(b"#MUSIC:", music, true)]),
        b'O' => return_header_tags!(
            s,
            out,
            [(b"#OFFSET:", offset, true), (b"#ORIGIN:", origin, ssc)]
        ),
        b'P' => return_header_tags!(s, out, [(b"#PREVIEWVID:", previewvid, ssc)]),
        b'S' => return_header_tags!(
            s,
            out,
            [
                (b"#SUBTITLE:", subtitle, true),
                (b"#SUBTITLETRANSLIT:", subtitle_translit, true),
                (b"#STOPS:", stops, true),
                (b"#SPEEDS:", speeds, ssc),
                (b"#SCROLLS:", scrolls, ssc),
                (b"#SAMPLESTART:", sample_start, true),
                (b"#SAMPLELENGTH:", sample_length, true),
                (b"#SELECTABLE:", selectable, true),
            ]
        ),
        b'T' => return_header_tags!(
            s,
            out,
            [
                (b"#TITLE:", title, true),
                (b"#TITLETRANSLIT:", title_translit, true),
                (b"#TIMESIGNATURES:", time_signatures, true),
                (b"#TICKCOUNTS:", tickcounts, true),
            ]
        ),
        b'V' => return_header_tags!(s, out, [(b"#VERSION:", version, true)]),
        b'W' => return_header_tags!(s, out, [(b"#WARPS:", warps, ssc)]),
        _ => {}
    }
    None
}

fn parse_notedata_entry(data: &[u8], start: usize) -> (Option<ParsedChartEntry<'_>>, usize) {
    let mut out = NotedataFields::default();
    let mut i = start;

    while i < data.len() {
        let Some(pos) = find_byte(&data[i..], b'#') else {
            return (finalize_notedata_entry(out), data.len());
        };
        i += pos;
        let s = &data[i..];

        if starts_with_ci(s, b"#NOTEDATA:") {
            if i != start {
                break;
            }
            if let Some((_, next)) = scan_tag_end(&s[10..], true) {
                i += 10 + next;
                continue;
            }
            i += 10;
            continue;
        }

        if s.get(1)
            .is_some_and(|byte| byte.eq_ignore_ascii_case(&b'N'))
        {
            try_tags!(
                s,
                i,
                out,
                [(b"#NOTES:", notes, true), (b"#NOTES2:", notes2, true)]
            );
        }
        if let Some(advance) = dispatch_notedata_tag(s, &mut out) {
            i += advance;
            continue;
        }
        i += 1;
    }

    (finalize_notedata_entry(out), i)
}

fn build_chart_entry(f: NotedataFields<'_>) -> ParsedChartEntry<'_> {
    ParsedChartEntry {
        field_count: 5,
        fields: [
            f.step_type.unwrap_or_default(),
            f.description.unwrap_or_default(),
            f.difficulty.unwrap_or_default(),
            f.meter.unwrap_or_default(),
            f.credit.unwrap_or_default(),
        ],
        chart_name: f.chart_name,
        chart_style: f.chart_style,
        note_data: f.notes.or(f.notes2).unwrap_or_default(),
        chart_music: f.chart_music.map(Cow::Borrowed),
        chart_attacks: f.chart_attacks,
        chart_bpms: f.chart_bpms.map(Cow::Borrowed),
        chart_stops: f.chart_stops.or(f.chart_freezes).map(Cow::Borrowed),
        chart_delays: f.chart_delays.map(Cow::Borrowed),
        chart_warps: f.chart_warps.map(Cow::Borrowed),
        chart_speeds: f.chart_speeds.map(Cow::Borrowed),
        chart_scrolls: f.chart_scrolls.map(Cow::Borrowed),
        chart_fakes: f.chart_fakes.map(Cow::Borrowed),
        chart_offset: f.chart_offset.map(Cow::Borrowed),
        chart_display_bpm: f.chart_display_bpm.map(Cow::Borrowed),
        chart_time_signatures: f.chart_time_signatures.map(Cow::Borrowed),
        chart_labels: f.chart_labels.map(Cow::Borrowed),
        chart_tickcounts: f.chart_tickcounts.map(Cow::Borrowed),
        chart_combos: f.chart_combos.map(Cow::Borrowed),
        chart_radar_values: f.chart_radar_values.map(Cow::Borrowed),
    }
}

#[inline(always)]
fn finalize_notedata_entry(f: NotedataFields<'_>) -> Option<ParsedChartEntry<'_>> {
    (f.notes.is_some() || f.notes2.is_some()).then(|| build_chart_entry(f))
}

const MAX_CHART_RESERVE: usize = 32;

fn chart_reserve_len(data_len: usize, start: usize, next: usize) -> usize {
    let block_len = next.saturating_sub(start).max(1);
    data_len
        .saturating_sub(start)
        .div_ceil(block_len)
        .clamp(1, MAX_CHART_RESERVE)
}

/// Extracts global metadata and chart sections without copying their contents.
///
/// # Errors
///
/// Returns `InvalidInput` when `ext` is not `sm` or `ssc`.
pub fn extract_sections<'a>(data: &'a [u8], ext: &str) -> io::Result<ParsedSimfileData<'a>> {
    extract_sections_impl(data, ext)
}

fn extract_sections_impl<'a>(data: &'a [u8], ext: &str) -> io::Result<ParsedSimfileData<'a>> {
    let ssc = extension_is_ssc(ext)?;

    let mut r = ParsedSimfileData::default();
    let mut i = 0;

    while i < data.len() {
        let Some(pos) = find_byte(&data[i..], b'#') else {
            break;
        };
        i += pos;
        let s = &data[i..];

        // SSC notedata block
        if ssc && starts_with_ci(s, b"#NOTEDATA:") {
            let (entry, next) = parse_notedata_entry(data, i);
            if let Some(entry) = entry {
                if r.notes_list.capacity() == 0 {
                    r.notes_list
                        .reserve_exact(chart_reserve_len(data.len(), i, next));
                }
                r.notes_list.push(entry);
            }
            i = next;
            continue;
        }

        // SM notes block
        if !ssc {
            let tag_len = if starts_with_ci(s, b"#NOTES2:") {
                8
            } else if starts_with_ci(s, b"#NOTES:") {
                7
            } else {
                0
            };
            if tag_len != 0 {
                let start = i + tag_len;
                let (field_count, fields, note_data, next) = split_sm_notes(data, start);
                if field_count == 5 {
                    if r.notes_list.capacity() == 0 {
                        r.notes_list
                            .reserve_exact(chart_reserve_len(data.len(), i, next));
                    }
                    r.notes_list.push(ParsedChartEntry {
                        field_count,
                        fields,
                        note_data,
                        ..Default::default()
                    });
                }
                i = next;
                continue;
            }
        }

        if let Some(advance) = dispatch_header_tag(s, ssc, &mut r) {
            i += advance;
            continue;
        }
        i += 1;
    }
    Ok(r)
}

/// Reports whether a supported simfile extension selects the SSC format.
///
/// # Errors
///
/// Returns `InvalidInput` unless `ext` is `sm` or `ssc`, ignoring ASCII case.
pub fn extension_is_ssc(ext: &str) -> io::Result<bool> {
    let ssc = if ext.eq_ignore_ascii_case("ssc") {
        true
    } else if ext.eq_ignore_ascii_case("sm") {
        false
    } else {
        return io::Result::Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Unsupported file extension (must be .sm or .ssc)",
        ));
    };
    io::Result::Ok(ssc)
}

#[inline(always)]
fn bgchanges_tag_len(s: &[u8]) -> Option<usize> {
    if starts_with_ci(s, b"#ANIMATIONS:") {
        return Some(12);
    }
    if !starts_with_ci(s, b"#BGCHANGES") {
        return None;
    }
    let mut i = 10usize;
    while matches!(s.get(i), Some(b'0'..=b'9')) {
        i += 1;
    }
    if s.get(i) != Some(&b':') {
        return None;
    }
    let layer = &s[10..i];
    (layer.is_empty() || layer == b"1").then_some(i + 1)
}

pub fn bgchanges_values(data: &[u8]) -> impl Iterator<Item = &[u8]> {
    let mut i = 0usize;
    std::iter::from_fn(move || {
        while i < data.len() {
            let pos = find_byte(&data[i..], b'#')?;
            i += pos;
            let s = &data[i..];
            let Some(tag_len) = bgchanges_tag_len(s) else {
                i += 1;
                continue;
            };
            if let Some((value, adv)) = parse_tag_val(s, tag_len, true) {
                i += adv;
                return Some(value);
            }
            i += 1;
        }
        None
    })
}

#[must_use]
pub fn split_notes_fields(block: &[u8]) -> (Vec<&[u8]>, &[u8]) {
    let (n, parts, note_data) = split_notes6(block);
    let mut fields = Vec::with_capacity(n as usize);
    fields.extend(parts.iter().take(n as usize).copied());
    (fields, note_data)
}

#[inline(always)]
fn find_unescaped_colon(slice: &[u8]) -> Option<usize> {
    let mut off = 0usize;
    while off < slice.len() {
        let rel = find_either_byte(&slice[off..], b'\\', b':')?;
        let idx = off + rel;
        if slice[idx] == b':' {
            return Some(idx);
        }

        let start = idx;
        off = idx + 1;
        while off < slice.len() && slice[off] == b'\\' {
            off += 1;
        }
        if slice.get(off) == Some(&b':') {
            if (off - start) & 1 == 0 {
                return Some(off);
            }
            off += 1;
        }
    }
    None
}

#[inline(always)]
fn next_after_semi(data: &[u8], start: usize) -> usize {
    find_byte(data.get(start..).unwrap_or(&[]), b';').map_or(data.len() + 1, |i| start + i + 1)
}

#[inline(always)]
fn scan_sm_note_data(data: &[u8], start: usize) -> (usize, usize) {
    let mut off = start;
    while off < data.len() {
        let Some(rel) = find_three_byte(&data[off..], b';', b'\\', b':') else {
            return (data.len(), data.len() + 1);
        };
        let idx = off + rel;
        match data[idx] {
            b';' => return (idx, idx + 1),
            b':' => {
                let mut bs = 0usize;
                let mut i = idx;
                while i > start && data[i - 1] == b'\\' {
                    bs += 1;
                    i -= 1;
                }
                if bs & 1 == 0 {
                    return (idx, next_after_semi(data, idx + 1));
                }
                off = idx + 1;
            }
            b'\\' => {
                let run_start = idx;
                let mut i = idx + 1;
                while i < data.len() && data[i] == b'\\' {
                    i += 1;
                }
                match data.get(i) {
                    Some(b';') => return (i, i + 1),
                    Some(b':') if (i - run_start) & 1 == 0 => {
                        return (i, next_after_semi(data, i + 1));
                    }
                    Some(b':') => off = i + 1,
                    _ => off = i,
                }
            }
            _ => unreachable!(),
        }
    }
    (data.len(), data.len() + 1)
}

fn split_sm_notes(data: &[u8], start: usize) -> (u8, [&[u8]; 5], &[u8], usize) {
    let mut fields: [&[u8]; 5] = [&[]; 5];
    let mut count = 0u8;
    let mut field_start = start;
    let mut bs_run = 0usize;
    let mut i = start;

    while i < data.len() {
        let b = data[i];
        if b == b';' {
            return (count, fields, &[], i + 1);
        }
        if b == b'\\' {
            bs_run += 1;
            i += 1;
            continue;
        }
        if b == b':' && bs_run & 1 == 0 {
            fields[count as usize] = data.get(field_start..i).unwrap_or(&[]);
            count += 1;
            field_start = i + 1;
            if count == 5 {
                let (end, next) = scan_sm_note_data(data, field_start);
                return (count, fields, &data[field_start..end], next);
            }
        }
        bs_run = 0;
        i += 1;
    }

    (count, fields, &[], data.len() + 1)
}

#[inline(always)]
fn split_notes6(block: &[u8]) -> (u8, [&[u8]; 5], &[u8]) {
    let mut fields: [&[u8]; 5] = [&[]; 5];
    let mut count = 0u8;
    let mut start = 0usize;
    let mut bs_run = 0usize;

    for (i, &b) in block.iter().enumerate() {
        if b == b'\\' {
            bs_run += 1;
            continue;
        }
        if b == b':' && bs_run & 1 == 0 && count < 5 {
            fields[count as usize] = block.get(start..i).unwrap_or(&[]);
            count += 1;
            start = i + 1;
            if count == 5 {
                break;
            }
        }
        bs_run = 0;
    }

    let rest = block.get(start..).unwrap_or(&[]);
    let end = find_unescaped_colon(rest).unwrap_or(rest.len());

    (count, fields, &rest[..end])
}

#[cfg(test)]
mod tests {
    use std::borrow::Cow;

    use super::{
        decode_cp1252, decode_unescape_trim, extract_sections, parse_version, unescape_trim_cow,
    };
    use crate::timing::{STEPFILE_VERSION_NUMBER, TimingFormat};

    #[test]
    fn indexed_tag_dispatch_preserves_mixed_case_tags() {
        let data = concat!(
            "#tItLe:Mixed Case;\n",
            "#aTtAcKs:first;\n",
            "#AtTaCkS:second;\n",
            "#unknown:ignored;\n",
            "#nOtEdAtA:;\n",
            "#sTePsTyPe:dance-single;\n",
            "#dEsCrIpTiOn:dispatch;\n",
            "#dIfFiCuLtY:Challenge;\n",
            "#mEtEr:12;\n",
            "#cReDiT:Author;\n",
            "#dIsPlAyBpM:120:180;\n",
            "#nOtEs:\n1000\n0100\n0010\n0001\n;\n",
        )
        .as_bytes();
        let current = extract_sections(data, "ssc").expect("fixture should parse");
        assert_eq!(current.title, Some(&b"Mixed Case"[..]));
        assert_eq!(current.attacks.as_deref(), Some(&b"first:second"[..]));
        assert_eq!(current.notes_list.len(), 1);
        assert_eq!(
            current.notes_list[0].fields,
            [
                &b"dance-single"[..],
                &b"dispatch"[..],
                &b"Challenge"[..],
                &b"12"[..],
                &b"Author"[..],
            ]
        );
        assert_eq!(
            current.notes_list[0].chart_display_bpm.as_deref(),
            Some(&b"120:180"[..])
        );
        assert_eq!(
            current.notes_list[0].note_data,
            b"\n1000\n0100\n0010\n0001\n"
        );
    }

    #[test]
    fn version_parses_itg_numeric_prefix() {
        assert_eq!(
            parse_version(Some(b"0.83 StepPrime"), TimingFormat::Ssc),
            0.83
        );
        assert_eq!(
            parse_version(Some(b"  +.74custom"), TimingFormat::Ssc),
            0.74
        );
        assert!(parse_version(Some(b"StepPrime"), TimingFormat::Ssc).is_nan());
        assert_eq!(
            parse_version(Some(b"StepPrime"), TimingFormat::Sm),
            STEPFILE_VERSION_NUMBER
        );
    }

    #[test]
    fn decoded_unescaped_trim_borrows_clean_utf8() {
        let value = decode_unescape_trim(b"  dance-single  ");
        assert!(matches!(value, Cow::Borrowed(_)));
        assert_eq!(value, "dance-single");
    }

    #[test]
    fn decoded_unescaped_trim_owns_transformed_values() {
        let escaped = decode_unescape_trim(b"  Fixture\\ Artist  ");
        assert!(matches!(escaped, Cow::Owned(_)));
        assert_eq!(escaped, "Fixture Artist");

        let cp1252 = decode_unescape_trim(&[b' ', 0x80, b' ']);
        assert!(matches!(cp1252, Cow::Owned(_)));
        assert_eq!(cp1252, "\u{20ac}");
    }

    #[test]
    fn cp1252_decoding_preserves_byte_classes_with_exact_capacity() {
        let decoded = decode_cp1252(&[0x00, 0x7f, 0x80, 0x81, 0x83, 0x93, 0x9f, 0xa0, 0xff]);

        assert_eq!(
            decoded,
            "\0\u{7f}\u{20ac}\u{fffd}\u{0192}\u{201c}\u{0178}\u{00a0}\u{00ff}"
        );
        assert_eq!(decoded.capacity(), decoded.len());

        let all_bytes: Vec<_> = (u8::MIN..=u8::MAX).collect();
        let expected: String = all_bytes
            .iter()
            .map(|&byte| super::cp1252_char(byte))
            .collect();
        let decoded = decode_cp1252(&all_bytes);
        assert_eq!(decoded, expected);
        assert_eq!(decoded.capacity(), decoded.len());
    }

    #[test]
    fn unescaped_trim_cow_preserves_unicode_trim_behavior() {
        assert_eq!(unescape_trim_cow("\u{2003}Title\u{2003}"), "Title");
        assert_eq!(unescape_trim_cow(r" A\ B "), "A B");
    }

    #[test]
    fn repeated_song_attacks_append_in_file_order() {
        let parsed = extract_sections(
            b"#ATTACKS:TIME=0:END=9999:MODS=overhead;\n\
              #ATTACKS:TIME=0.241:END=0.438:MODS=*1.875 15% invert;\n\
              #ATTACKS:TIME=0.338:END=0.515:MODS=*1.946 no invert;",
            "sm",
        )
        .expect("SM extraction should succeed");
        let attacks = parsed.attacks.expect("attacks should be present");

        assert!(matches!(attacks, Cow::Owned(_)));
        assert_eq!(
            attacks.as_ref(),
            b"TIME=0:END=9999:MODS=overhead:\
              TIME=0.241:END=0.438:MODS=*1.875 15% invert:\
              TIME=0.338:END=0.515:MODS=*1.946 no invert"
        );
    }

    #[test]
    fn repeated_step_attacks_append_in_file_order() {
        let parsed = extract_sections(
            b"#VERSION:0.83;\n\
              #NOTEDATA:;\n\
              #STEPSTYPE:dance-single;\n\
              #ATTACKS:TIME=1:LEN=2:MODS=mirror;\n\
              #ATTACKS:TIME=4:LEN=1:MODS=invert;\n\
              #NOTES:\n0000\n;",
            "ssc",
        )
        .expect("SSC extraction should succeed");
        let attacks = parsed.notes_list[0]
            .chart_attacks
            .as_deref()
            .expect("step attacks should be present");

        assert_eq!(
            attacks,
            b"TIME=1:LEN=2:MODS=mirror:TIME=4:LEN=1:MODS=invert"
        );
    }
}

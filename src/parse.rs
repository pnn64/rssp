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
            && title[..pos].chars().all(|c| c.is_ascii_digit() || c == '.') {
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

#[must_use] 
pub fn unescape_trim(tag: &str) -> String {
    let s = unescape_tag(tag);
    let t = s.trim();
    if t.len() == s.len() {
        s.into_owned()
    } else {
        t.to_string()
    }
}

const CP1252_MAP: [u16; 32] = [
    0x20AC, 0xFFFD, 0x201A, 0x0192, 0x201E, 0x2026, 0x2020, 0x2021, 0x02C6, 0x2030, 0x0160, 0x2039,
    0x0152, 0xFFFD, 0x017D, 0xFFFD, 0xFFFD, 0x2018, 0x2019, 0x201C, 0x201D, 0x2022, 0x2013, 0x2014,
    0x02DC, 0x2122, 0x0161, 0x203A, 0x0153, 0xFFFD, 0x017E, 0x0178,
];

fn decode_cp1252(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|&b| match b {
            0x00..=0x7F => b as char,
            0x80..=0x9F => {
                char::from_u32(u32::from(CP1252_MAP[(b - 0x80) as usize])).unwrap_or('\u{FFFD}')
            }
            _ => char::from_u32(u32::from(b)).unwrap_or('\u{FFFD}'),
        })
        .collect()
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

#[must_use] 
pub fn parse_version(version: Option<&[u8]>, fmt: TimingFormat) -> f32 {
    version
        .and_then(|b| std::str::from_utf8(b).ok())
        .and_then(|s| s.parse().ok())
        .unwrap_or(if fmt == TimingFormat::Ssc {
            f32::NAN
        } else {
            STEPFILE_VERSION_NUMBER
        })
}

pub const SSC_VERSION_CHART_NAME_TAG: f32 = 0.74;

#[must_use] 
pub fn normalize_chart_desc(desc: String, fmt: TimingFormat, ver: f32) -> String {
    if fmt == TimingFormat::Ssc && ver < SSC_VERSION_CHART_NAME_TAG {
        String::new()
    } else {
        desc
    }
}

type TagBytes<'a> = Cow<'a, [u8]>;

#[derive(Default)]
pub struct ParsedChartEntry<'a> {
    pub field_count: u8,
    pub fields: [&'a [u8]; 5],
    pub note_data: &'a [u8],
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
    pub title_translit: Option<&'a [u8]>,
    pub subtitle_translit: Option<&'a [u8]>,
    pub artist_translit: Option<&'a [u8]>,
    pub version: Option<&'a [u8]>,
    pub offset: Option<&'a [u8]>,
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
    pub music: Option<&'a [u8]>,
    pub sample_start: Option<&'a [u8]>,
    pub sample_length: Option<&'a [u8]>,
    pub display_bpm: Option<&'a [u8]>,
    pub notes_list: Vec<ParsedChartEntry<'a>>,
}

#[derive(Default)]
struct NotedataFields<'a> {
    step_type: Option<&'a [u8]>,
    description: Option<&'a [u8]>,
    credit: Option<&'a [u8]>,
    difficulty: Option<&'a [u8]>,
    meter: Option<&'a [u8]>,
    notes: Option<&'a [u8]>,
    notes2: Option<&'a [u8]>,
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
    slice.len() >= tag.len()
        && slice
            .iter()
            .zip(tag)
            .all(|(a, b)| a.eq_ignore_ascii_case(b))
}

/// Returns (`value_end`, `next_position`) if terminator found.
#[inline(always)]
fn scan_tag_end(slice: &[u8], allow_nl: bool) -> Option<(usize, usize)> {
    let mut i = 0;
    let mut bs = 0usize;
    while i < slice.len() {
        let b = slice[i];
        let escaped = bs & 1 != 0;
        match b {
            b';' if !escaped => return Some((i, i + 1)),
            b':' if !allow_nl && !escaped => return Some((i, i + 1)),
            b'\n' | b'\r' => {
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
            }
            _ => {}
        }
        bs = if b == b'\\' { bs + 1 } else { 0 };
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
fn try_tag<'a>(s: &'a [u8], tag: &[u8], out: &mut Option<&'a [u8]>, enabled: bool) -> bool {
    if enabled && starts_with_ci(s, tag) {
        if let Some((v, _)) = parse_tag_val(s, tag.len(), true) {
            *out = Some(v);
        }
        true
    } else {
        false
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

macro_rules! try_tags {
    ($s:expr, $i:expr, $o:expr, [ $( ($tag:expr, $field:ident, $nl:expr) ),* $(,)? ]) => {
        $( if let Some(a) = try_tag_adv($s, $tag, $nl, &mut $o.$field) { $i += a; continue; } )*
    };
}

fn parse_notedata_entry(data: &[u8], start: usize) -> (Option<ParsedChartEntry<'_>>, usize) {
    let mut out = NotedataFields::default();
    let mut i = start;

    while i < data.len() {
        let Some(pos) = data[i..].iter().position(|&b| b == b'#') else {
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

        try_tags!(
            s,
            i,
            out,
            [
                (b"#STEPSTYPE:", step_type, false),
                (b"#DESCRIPTION:", description, false),
                (b"#CREDIT:", credit, false),
                (b"#DIFFICULTY:", difficulty, false),
                (b"#METER:", meter, false),
                (b"#NOTES:", notes, true),
                (b"#NOTES2:", notes2, true),
                (b"#BPMS:", chart_bpms, true),
                (b"#STOPS:", chart_stops, true),
                (b"#FREEZES:", chart_freezes, true),
                (b"#DELAYS:", chart_delays, true),
                (b"#WARPS:", chart_warps, true),
                (b"#SPEEDS:", chart_speeds, true),
                (b"#SCROLLS:", chart_scrolls, true),
                (b"#FAKES:", chart_fakes, true),
                (b"#OFFSET:", chart_offset, true),
                (b"#DISPLAYBPM:", chart_display_bpm, true),
                (b"#TIMESIGNATURES:", chart_time_signatures, true),
                (b"#LABELS:", chart_labels, true),
                (b"#TICKCOUNTS:", chart_tickcounts, true),
                (b"#COMBOS:", chart_combos, true),
                (b"#RADARVALUES:", chart_radar_values, true),
            ]
        );
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
        note_data: f.notes.or(f.notes2).unwrap_or_default(),
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

pub fn extract_sections<'a>(data: &'a [u8], ext: &str) -> io::Result<ParsedSimfileData<'a>> {
    let ext_lower = ext.to_lowercase();
    if !matches!(ext_lower.as_str(), "sm" | "ssc") {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Unsupported file extension (must be .sm or .ssc)",
        ));
    }

    let mut r = ParsedSimfileData::default();
    let mut i = 0;
    let ssc = ext_lower == "ssc";

    while i < data.len() {
        let Some(pos) = data[i..].iter().position(|&b| b == b'#') else {
            break;
        };
        i += pos;
        let s = &data[i..];

        // SSC notedata block
        if ssc && starts_with_ci(s, b"#NOTEDATA:") {
            let (entry, next) = parse_notedata_entry(data, i);
            if let Some(entry) = entry {
                r.notes_list.push(entry);
            }
            i = next;
            continue;
        }

        // SM notes block
        if !ssc && (starts_with_ci(s, b"#NOTES:") || starts_with_ci(s, b"#NOTES2:")) {
            let tag_len = if starts_with_ci(s, b"#NOTES2:") { 8 } else { 7 };
            let start = i + tag_len;
            let end = data[start..]
                .iter()
                .position(|&b| b == b';')
                .map_or(data.len(), |e| start + e);
            let (field_count, fields, note_data) = split_notes6(&data[start..end]);
            if field_count == 5 {
                r.notes_list.push(ParsedChartEntry {
                    field_count,
                    fields,
                    note_data,
                    ..Default::default()
                });
            }
            i = end + 1;
            continue;
        }

        // Header tags (chained for short-circuit evaluation)
        let _ = try_tag(s, b"#TITLE:", &mut r.title, true)
            || try_tag(s, b"#SUBTITLE:", &mut r.subtitle, true)
            || try_tag(s, b"#ARTIST:", &mut r.artist, true)
            || try_tag(s, b"#TITLETRANSLIT:", &mut r.title_translit, true)
            || try_tag(s, b"#SUBTITLETRANSLIT:", &mut r.subtitle_translit, true)
            || try_tag(s, b"#ARTISTTRANSLIT:", &mut r.artist_translit, true)
            || try_tag(s, b"#VERSION:", &mut r.version, true)
            || try_tag(s, b"#OFFSET:", &mut r.offset, true)
            || try_tag(s, b"#BPMS:", &mut r.bpms, true)
            || try_tag(s, b"#STOPS:", &mut r.stops, true)
            || try_tag(s, b"#FREEZES:", &mut r.stops, true)
            || try_tag(s, b"#DELAYS:", &mut r.delays, true)
            || try_tag(s, b"#TIMESIGNATURES:", &mut r.time_signatures, true)
            || try_tag(s, b"#TICKCOUNTS:", &mut r.tickcounts, true)
            || try_tag(s, b"#BANNER:", &mut r.banner, true)
            || try_tag(s, b"#BACKGROUND:", &mut r.background, true)
            || try_tag(s, b"#MUSIC:", &mut r.music, true)
            || try_tag(s, b"#SAMPLESTART:", &mut r.sample_start, true)
            || try_tag(s, b"#SAMPLELENGTH:", &mut r.sample_length, true)
            || try_tag(s, b"#DISPLAYBPM:", &mut r.display_bpm, true)
            || try_tag(s, b"#FAKES:", &mut r.fakes, ssc)
            || try_tag(s, b"#WARPS:", &mut r.warps, ssc)
            || try_tag(s, b"#SPEEDS:", &mut r.speeds, ssc)
            || try_tag(s, b"#SCROLLS:", &mut r.scrolls, ssc)
            || try_tag(s, b"#LABELS:", &mut r.labels, ssc)
            || try_tag(s, b"#COMBOS:", &mut r.combos, ssc);
        i += 1;
    }
    Ok(r)
}

#[must_use] 
pub fn split_notes_fields(block: &[u8]) -> (Vec<&[u8]>, &[u8]) {
    let (n, parts, note_data) = split_notes6(block);
    let mut fields = Vec::with_capacity(n as usize);
    fields.extend(parts.iter().take(n as usize).copied());
    (fields, note_data)
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
    let mut end = rest.len();
    bs_run = 0;
    for (i, &b) in rest.iter().enumerate() {
        if b == b'\\' {
            bs_run += 1;
            continue;
        }
        if b == b':' && bs_run & 1 == 0 {
            end = i;
            break;
        }
        bs_run = 0;
    }

    (count, fields, &rest[..end])
}

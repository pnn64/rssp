use std::borrow::Cow;
use std::collections::{HashMap, HashSet, hash_map::Entry};
use std::path::{Path, PathBuf};
use std::sync::Arc;
#[cfg(not(target_arch = "wasm32"))]
use std::time::{Duration, Instant};
#[cfg(target_arch = "wasm32")]
use web_time::{Duration, Instant};

use crate::analysis::{AnalysisOptions, AnalysisScratch, PreparedAnalysis, analyze_prepared_in};
use crate::assets;
use crate::math::{round_dp, round_sig_figs_6};
use crate::nps::get_nps_stats;
use crate::pack;
use crate::parse::{clean_tag, decode_bytes, extract_sections, unescape_tag};
use crate::patterns::PATTERN_COUNT;
use crate::report::{ChartSummary, CourseEntrySummary, CourseSummary, SimfileSummary};
use crate::simfile;
use crate::timing::TimingSegments;

#[derive(Debug, Clone)]
pub struct CourseFile {
    pub name: String,
    pub name_translit: String,
    pub scripter: String,
    pub description: String,
    pub banner: String,
    pub background: String,
    pub repeat: bool,
    pub lives: i32,
    pub meters: [Option<i32>; 6],
    pub entries: Vec<CourseEntry>,
}

const COURSE_BANNER_EXTS: [&str; 5] = ["png", "jpg", "jpeg", "bmp", "gif"];
const MAX_CACHED_SIMS: usize = 128;

#[derive(Debug, Clone, PartialEq)]
pub struct CourseEntry {
    pub song: CourseSong,
    pub steps: StepsSpec,
    pub modifiers: String,
    pub secret: bool,
    pub no_difficult: bool,
    pub gain_seconds: f32,
    pub gain_lives: i32,
}

#[derive(Debug, Clone, PartialEq)]
pub enum CourseSong {
    Fixed { group: Option<String>, song: String },
    RandomAny,
    RandomWithinGroup { group: String },
    SortPick { sort: SongSort, index: i32 },
    Select(SongSelect),
    Unknown { raw: String },
}

#[derive(Debug, Clone, PartialEq)]
pub struct SongSelect {
    pub titles: Vec<String>,
    pub groups: Vec<String>,
    pub artists: Vec<String>,
    pub genres: Vec<String>,
    pub difficulties: Vec<Difficulty>,
    pub meter_range: Option<(i32, i32)>,
    pub bpm_range: Option<(f64, f64)>,
    pub duration_range: Option<(f32, f32)>,
    pub sort: Option<SongSort>,
    pub index: i32,
}

impl Default for SongSelect {
    fn default() -> Self {
        Self {
            titles: Vec::new(),
            groups: Vec::new(),
            artists: Vec::new(),
            genres: Vec::new(),
            difficulties: Vec::new(),
            meter_range: None,
            bpm_range: None,
            duration_range: None,
            sort: None,
            index: 0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SongSort {
    MostPlays,
    FewestPlays,
    TopGrades,
    LowestGrades,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Difficulty {
    Beginner = 0,
    Easy = 1,
    Medium = 2,
    Hard = 3,
    Challenge = 4,
    Edit = 5,
}

#[must_use]
pub const fn difficulty_label(d: Difficulty) -> &'static str {
    match d {
        Difficulty::Beginner => "Beginner",
        Difficulty::Easy => "Easy",
        Difficulty::Medium => "Medium",
        Difficulty::Hard => "Hard",
        Difficulty::Challenge => "Challenge",
        Difficulty::Edit => "Edit",
    }
}

#[inline(always)]
const fn course_meter(meters: &[Option<i32>; 6], difficulty: Difficulty) -> Option<i32> {
    meters[difficulty as usize]
}

#[derive(Debug, Clone, PartialEq)]
pub enum StepsSpec {
    Difficulty(Difficulty),
    MeterRange { low: i32, high: i32 },
    Unknown { raw: String },
}

fn parse_course_difficulty(raw: &str) -> Option<Difficulty> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "beginner" => Some(Difficulty::Beginner),
        "easy" | "basic" | "light" => Some(Difficulty::Easy),
        "regular" | "medium" | "another" | "trick" | "standard" => Some(Difficulty::Medium),
        "difficult" | "hard" | "ssr" | "maniac" | "heavy" => Some(Difficulty::Hard),
        "challenge" | "expert" | "oni" | "smaniac" => Some(Difficulty::Challenge),
        "edit" => Some(Difficulty::Edit),
        _ => None,
    }
}

fn normalize_stepstype(raw: &str) -> String {
    raw.trim().to_ascii_lowercase().replace('_', "-")
}

#[inline(always)]
const fn norm_step_byte(byte: u8) -> u8 {
    if byte == b'_' {
        b'-'
    } else {
        byte.to_ascii_lowercase()
    }
}

#[inline(always)]
fn stepstype_eq(raw: &str, normalized: &str) -> bool {
    raw.trim()
        .bytes()
        .map(norm_step_byte)
        .eq(normalized.bytes())
}

#[cfg(feature = "profile")]
#[doc(hidden)]
#[must_use]
pub fn profile_stepstype_eq_legacy(raw: &str, normalized: &str) -> bool {
    normalize_stepstype(raw) == normalized
}

#[cfg(feature = "profile")]
#[doc(hidden)]
#[must_use]
pub fn profile_stepstype_eq(raw: &str, normalized: &str) -> bool {
    stepstype_eq(raw, normalized)
}

const fn diff_from_idx(idx: i32) -> Difficulty {
    match idx {
        i if i <= Difficulty::Beginner as i32 => Difficulty::Beginner,
        1 => Difficulty::Easy,
        2 => Difficulty::Medium,
        3 => Difficulty::Hard,
        4 => Difficulty::Challenge,
        _ => Difficulty::Edit,
    }
}

fn shift_diff(base: Difficulty, course: Difficulty) -> Difficulty {
    let base = base as i32;
    let delta = (course as i32) - (Difficulty::Medium as i32);
    diff_from_idx((base + delta).clamp(0, Difficulty::Challenge as i32))
}

#[inline(always)]
fn scan_term(slice: &[u8]) -> Option<(usize, usize)> {
    let mut bs = 0usize;
    for (i, &b) in slice.iter().enumerate() {
        let escaped = bs & 1 != 0;
        if b == b';' && !escaped {
            return Some((i, i + 1));
        }
        bs = if b == b'\\' { bs + 1 } else { 0 };
    }
    None
}

#[inline(always)]
fn split_unescaped(block: &[u8], delim: u8) -> Vec<&[u8]> {
    if block.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    let (mut start, mut bs) = (0usize, 0usize);
    for (i, &b) in block.iter().enumerate() {
        if b == b'\\' {
            bs += 1;
            continue;
        }
        if b == delim && bs & 1 == 0 {
            out.push(block.get(start..i).unwrap_or(&[]));
            start = i + 1;
        }
        bs = 0;
    }
    out.push(block.get(start..).unwrap_or(&[]));
    out
}

fn split_entry_fields(block: &[u8]) -> [&[u8]; 3] {
    let mut fields = [&[][..]; 3];
    if block.is_empty() {
        return fields;
    }

    let (mut field, mut start, mut bs) = (0usize, 0usize, 0usize);
    for (i, &byte) in block.iter().enumerate() {
        if byte == b'\\' {
            bs += 1;
            continue;
        }
        if byte == b':' && bs & 1 == 0 {
            fields[field] = &block[start..i];
            if field == fields.len() - 1 {
                return fields;
            }
            field += 1;
            start = i + 1;
        }
        bs = 0;
    }
    fields[field] = &block[start..];
    fields
}

#[inline(always)]
const fn trim_ascii(mut s: &[u8]) -> &[u8] {
    while let Some((&b, rest)) = s.split_first() {
        if !b.is_ascii_whitespace() {
            break;
        }
        s = rest;
    }
    while let Some((&b, rest)) = s.split_last() {
        if !b.is_ascii_whitespace() {
            break;
        }
        s = rest;
    }
    s
}

fn decode_trim(bytes: &[u8]) -> String {
    decode_trimmed(bytes).into_owned()
}

fn decode_trimmed(bytes: &[u8]) -> Cow<'_, str> {
    match decode_bytes(trim_ascii(bytes)) {
        Cow::Borrowed(value) => Cow::Borrowed(value.trim()),
        Cow::Owned(mut value) => {
            let start = value.len() - value.trim_start().len();
            let len = value.trim().len();
            value.replace_range(..start, "");
            value.truncate(len);
            Cow::Owned(value)
        }
    }
}

fn decode_unescape_trim(bytes: &[u8]) -> String {
    let decoded = decode_bytes(trim_ascii(bytes));
    unescape_tag(decoded.as_ref()).trim().to_string()
}

fn parse_repeat(s: &str) -> bool {
    s.to_ascii_lowercase().contains("yes")
}

fn parse_sort_pick(raw: &str) -> Option<(SongSort, i32)> {
    let raw = raw.trim();
    let (sort, rest) = if let Some(s) = raw.strip_prefix("BEST") {
        (SongSort::MostPlays, s)
    } else if let Some(s) = raw.strip_prefix("WORST") {
        (SongSort::FewestPlays, s)
    } else if let Some(s) = raw.strip_prefix("GRADEBEST") {
        (SongSort::TopGrades, s)
    } else if let Some(s) = raw.strip_prefix("GRADEWORST") {
        (SongSort::LowestGrades, s)
    } else {
        return None;
    };
    let index = rest.trim().parse::<i32>().ok()? - 1;
    Some((sort, index))
}

fn parse_song(raw: &str) -> (CourseSong, bool) {
    let raw = raw.trim();
    if raw == "*" {
        return (CourseSong::RandomAny, true);
    }
    if let Some((sort, index)) = parse_sort_pick(raw) {
        return (CourseSong::SortPick { sort, index }, false);
    }

    if let Some(group) = raw
        .strip_suffix("/*")
        .or_else(|| raw.strip_suffix("\\*"))
        .map(str::trim)
        && !group.is_empty()
    {
        return (
            CourseSong::RandomWithinGroup {
                group: group.replace('\\', "/"),
            },
            true,
        );
    }

    let mut parts = raw
        .split(['/', '\\'])
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let first = parts.next().unwrap_or_default();
    let second = parts.next();
    if parts.next().is_some() {
        return (
            CourseSong::Unknown {
                raw: raw.to_string(),
            },
            false,
        );
    }
    let song = second.map_or_else(|| first.to_string(), str::to_string);
    let group = second.map(|_| first.to_string());
    (CourseSong::Fixed { group, song }, false)
}

fn parse_difficulty_label(label: &str) -> Option<Difficulty> {
    let label = label.trim();
    if label.eq_ignore_ascii_case("beginner") {
        Some(Difficulty::Beginner)
    } else if ["easy", "basic", "light"]
        .iter()
        .any(|value| label.eq_ignore_ascii_case(value))
    {
        Some(Difficulty::Easy)
    } else if ["medium", "another", "trick", "standard", "difficult"]
        .iter()
        .any(|value| label.eq_ignore_ascii_case(value))
    {
        Some(Difficulty::Medium)
    } else if ["hard", "ssr", "maniac", "heavy"]
        .iter()
        .any(|value| label.eq_ignore_ascii_case(value))
    {
        Some(Difficulty::Hard)
    } else if ["challenge", "expert", "oni", "smaniac"]
        .iter()
        .any(|value| label.eq_ignore_ascii_case(value))
    {
        Some(Difficulty::Challenge)
    } else if label.eq_ignore_ascii_case("edit") {
        Some(Difficulty::Edit)
    } else {
        None
    }
}

fn parse_meter_range(raw: &str) -> Option<(i32, i32)> {
    let raw = raw.trim();
    let (a, b) = raw.split_once("..").unwrap_or((raw, raw));
    let low = a.trim().parse::<i32>().ok()?;
    let high = b.trim().parse::<i32>().ok()?;
    Some((low, high))
}

fn parse_steps(raw: &str) -> StepsSpec {
    let raw = raw.trim();
    if let Some(diff) = parse_difficulty_label(raw) {
        return StepsSpec::Difficulty(diff);
    }
    if let Some((low, high)) = parse_meter_range(raw) {
        return StepsSpec::MeterRange { low, high };
    }
    StepsSpec::Unknown {
        raw: raw.to_string(),
    }
}

fn apply_song_mods(mut secret: bool, mods_raw: &str) -> (bool, bool, i32, String) {
    let mut out_mods = String::new();
    let mut no_difficult = false;
    let mut gain_lives = -1;

    for raw in mods_raw.split(',') {
        let mod_str = raw.trim();
        if mod_str.is_empty() {
            continue;
        }
        if mod_str.eq_ignore_ascii_case("showcourse") {
            secret = false;
            continue;
        }
        if mod_str.eq_ignore_ascii_case("noshowcourse") {
            secret = true;
            continue;
        }
        if mod_str.eq_ignore_ascii_case("nodifficult") {
            no_difficult = true;
            continue;
        }
        let mod_bytes = mod_str.as_bytes();
        if mod_bytes.len() > 5 && mod_bytes[..5].eq_ignore_ascii_case(b"award") {
            let rest = mod_str[5..].trim();
            if let Ok(v) = rest.parse::<i32>() {
                gain_lives = v;
            }
            continue;
        }
        if out_mods.is_empty() {
            out_mods.reserve(mods_raw.len());
        } else {
            out_mods.push(',');
        }
        out_mods.push_str(mod_str);
    }

    (secret, no_difficult, gain_lives, out_mods)
}

#[cfg(feature = "profile")]
#[doc(hidden)]
pub fn profile_song_mods(secret: bool, mods_raw: &str) -> (bool, bool, i32, String) {
    apply_song_mods(secret, mods_raw)
}

fn parse_song_entry(value: &[u8]) -> CourseEntry {
    let [song_raw, diff_raw, mods_raw] = split_entry_fields(value);
    let song_text = decode_trimmed(song_raw);
    let diff_text = decode_trimmed(diff_raw);
    let mods_text = decode_trimmed(mods_raw);
    let (song, secret_default) = parse_song(&song_text);
    let steps = parse_steps(&diff_text);
    let (secret, no_difficult, gain_lives, modifiers) = apply_song_mods(secret_default, &mods_text);

    CourseEntry {
        song,
        steps,
        modifiers,
        secret,
        no_difficult,
        gain_seconds: 0.0,
        gain_lives,
    }
}

fn parse_select_list(raw: &[u8], out: &mut Vec<String>) -> bool {
    let items = split_unescaped(raw, b',');
    if items.is_empty() {
        return false;
    }
    out.extend(items.into_iter().map(decode_unescape_trim));
    true
}

fn parse_select_range(raw: &[u8]) -> Option<(f64, f64)> {
    let raw = decode_trim(raw);
    let mut values = raw.split('-');
    let first = values.next()?.trim().parse::<f64>().ok()?;
    let last = values
        .next()
        .map(str::trim)
        .map_or(Ok(first), str::parse)
        .ok()?;
    if values.next().is_some() || first > last {
        return None;
    }
    Some((first, last))
}

fn parse_select_sort(raw: &[u8]) -> Option<(Option<SongSort>, i32)> {
    let parts = split_unescaped(raw, b',');
    if parts.len() != 2 {
        return None;
    }
    let sort = match decode_trim(parts[0]).to_ascii_lowercase().as_str() {
        "randomize" => None,
        "mostplays" | "best" => Some(SongSort::MostPlays),
        "fewestplays" | "worst" => Some(SongSort::FewestPlays),
        "topgrades" | "gradebest" => Some(SongSort::TopGrades),
        "lowestgrades" | "gradeworst" => Some(SongSort::LowestGrades),
        _ => return None,
    };
    let index = (decode_trim(parts[1]).parse::<i32>().unwrap_or(0) - 1).clamp(0, 500);
    Some((sort, index))
}

fn apply_select_mods(entry: &mut CourseEntry, raw: &[u8]) {
    let mut modifiers = Vec::new();
    for item in split_unescaped(raw, b',') {
        let value = decode_unescape_trim(item);
        if value.eq_ignore_ascii_case("showcourse") {
            entry.secret = false;
        } else if value.eq_ignore_ascii_case("noshowcourse") {
            entry.secret = true;
        } else if value.eq_ignore_ascii_case("nodifficult") {
            entry.no_difficult = true;
        } else if !value.is_empty() {
            modifiers.push(value);
        }
    }
    entry.modifiers = modifiers.join(",");
}

fn parse_song_select(params: &[&[u8]]) -> Option<CourseEntry> {
    let mut entry = CourseEntry {
        song: CourseSong::Select(SongSelect::default()),
        steps: StepsSpec::Unknown { raw: String::new() },
        modifiers: String::new(),
        secret: false,
        no_difficult: false,
        gain_seconds: 0.0,
        gain_lives: -1,
    };

    for param in params {
        let parts = split_unescaped(param, b'=');
        if parts.len() != 2 {
            return None;
        }
        let name = decode_trim(parts[0]);
        let value = parts[1];
        let CourseSong::Select(select) = &mut entry.song else {
            unreachable!("SONGSELECT parser always constructs selection criteria");
        };
        if name.eq_ignore_ascii_case("TITLE") {
            if !parse_select_list(value, &mut select.titles) {
                return None;
            }
        } else if name.eq_ignore_ascii_case("GROUP") {
            if !parse_select_list(value, &mut select.groups) {
                return None;
            }
        } else if name.eq_ignore_ascii_case("ARTIST") {
            if !parse_select_list(value, &mut select.artists) {
                return None;
            }
        } else if name.eq_ignore_ascii_case("GENRE") {
            if !parse_select_list(value, &mut select.genres) {
                return None;
            }
        } else if name.eq_ignore_ascii_case("DIFFICULTY") {
            for raw in split_unescaped(value, b',') {
                if let Some(diff) = parse_course_difficulty(&decode_trim(raw)) {
                    select.difficulties.push(diff);
                }
            }
        } else if name.eq_ignore_ascii_case("METER") {
            let (low, high) = parse_select_range(value)?;
            select.meter_range = Some((low as i32, high as i32));
        } else if name.eq_ignore_ascii_case("BPMRANGE") {
            select.bpm_range = Some(parse_select_range(value)?);
        } else if name.eq_ignore_ascii_case("DURATION") {
            let (low, high) = parse_select_range(value)?;
            select.duration_range = Some((low as f32, high as f32));
        } else if name.eq_ignore_ascii_case("SORT") {
            let (sort, index) = parse_select_sort(value)?;
            select.sort = sort;
            select.index = index;
        } else if name.eq_ignore_ascii_case("GAINLIVES") {
            entry.gain_lives = decode_trim(value).parse().unwrap_or(0);
        } else if name.eq_ignore_ascii_case("GAINSECONDS") {
            entry.gain_seconds = decode_trim(value).parse::<i32>().unwrap_or(0) as f32;
        } else if name.eq_ignore_ascii_case("MODS") {
            apply_select_mods(&mut entry, value);
        }
    }
    Some(entry)
}

fn parse_course_meter_tag(value: &[u8], meters: &mut [Option<i32>; 6]) {
    let params = split_unescaped(value, b':');
    if params.is_empty() {
        return;
    }

    if params.len() == 1 {
        let meter = decode_trim(params[0]).parse::<i32>().unwrap_or(0).max(0);
        meters[Difficulty::Medium as usize] = Some(meter);
        return;
    }

    let mut i = 0usize;
    while i + 1 < params.len() {
        let diff_raw = decode_trim(params[i]);
        let meter_raw = decode_trim(params[i + 1]);
        if let Some(diff) = parse_course_difficulty(&diff_raw)
            && let Ok(meter) = meter_raw.parse::<i32>()
        {
            meters[diff as usize] = Some(meter.max(0));
        }
        i += 2;
    }
}

#[inline(always)]
fn has_banner_prefix(path: &Path, stem_lc: &str, ext: &str) -> bool {
    if !path.is_file() {
        return false;
    }
    let Some(path_ext) = path.extension().and_then(|s| s.to_str()) else {
        return false;
    };
    if !path_ext.eq_ignore_ascii_case(ext) {
        return false;
    }
    let Some(file_stem) = path.file_stem().and_then(|s| s.to_str()) else {
        return false;
    };
    file_stem.to_ascii_lowercase().starts_with(stem_lc)
}

fn push_banner_ext_matches(dir: &Path, stem_lc: &str, ext: &str, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut matches: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| has_banner_prefix(p, stem_lc, ext))
        .collect();
    matches.sort_by_cached_key(|p| assets::lc_name(p));
    out.extend(matches);
}

#[must_use]
pub fn resolve_course_banner_path(course_path: &Path, banner_tag: &str) -> Option<PathBuf> {
    let banner_tag = banner_tag.trim();
    if !banner_tag.is_empty() {
        let tag_path = Path::new(banner_tag);
        if tag_path.is_absolute() {
            return tag_path.is_file().then_some(tag_path.to_path_buf());
        }
        let parent = course_path.parent().unwrap_or_else(|| Path::new(""));
        let joined = parent.join(tag_path);
        return joined.is_file().then_some(joined);
    }

    let parent = course_path.parent().unwrap_or_else(|| Path::new(""));
    let stem_lc = course_path
        .file_stem()?
        .to_string_lossy()
        .to_ascii_lowercase();
    if stem_lc.is_empty() {
        return None;
    }

    let mut possible = Vec::new();
    for ext in COURSE_BANNER_EXTS {
        push_banner_ext_matches(parent, &stem_lc, ext, &mut possible);
    }
    possible.into_iter().next()
}

pub fn parse_crs(data: &[u8]) -> Result<CourseFile, String> {
    let mut name = String::new();
    let mut name_translit = String::new();
    let mut scripter = String::new();
    let mut description = String::new();
    let mut banner = String::new();
    let mut background = String::new();
    let mut repeat = false;
    let mut lives = -1;
    let mut meters = [None; 6];
    let mut entries = Vec::new();

    let mut i = 0usize;
    while i < data.len() {
        let Some(pos) = data[i..].iter().position(|&b| b == b'#') else {
            break;
        };
        i += pos;
        let s = &data[i..];
        let Some(name_end) = s.iter().position(|&b| b == b':') else {
            i += 1;
            continue;
        };

        let name_bytes = &s[1..name_end];
        let value_start = name_end + 1;
        let (value_end, adv) =
            scan_term(&s[value_start..]).unwrap_or((s.len() - value_start, s.len() - value_start));
        let value = &s[value_start..value_start + value_end];
        i += value_start + adv;

        if name_bytes.eq_ignore_ascii_case(b"COURSE") {
            name = decode_trim(value);
            continue;
        }
        if name_bytes.eq_ignore_ascii_case(b"COURSETRANSLIT") {
            name_translit = decode_trim(value);
            continue;
        }
        if name_bytes.eq_ignore_ascii_case(b"SCRIPTER") {
            scripter = decode_trim(value);
            continue;
        }
        if name_bytes.eq_ignore_ascii_case(b"DESCRIPTION") {
            description = decode_trim(value);
            continue;
        }
        if name_bytes.eq_ignore_ascii_case(b"REPEAT") {
            repeat = parse_repeat(&decode_trim(value));
            continue;
        }
        if name_bytes.eq_ignore_ascii_case(b"BANNER") {
            banner = decode_trim(value);
            continue;
        }
        if name_bytes.eq_ignore_ascii_case(b"BACKGROUND") {
            background = decode_trim(value);
            continue;
        }
        if name_bytes.eq_ignore_ascii_case(b"LIVES") {
            lives = decode_trim(value).parse::<i32>().unwrap_or(0).max(0);
            continue;
        }
        if name_bytes.eq_ignore_ascii_case(b"METER") {
            parse_course_meter_tag(value, &mut meters);
            continue;
        }
        if name_bytes.eq_ignore_ascii_case(b"SONG") {
            entries.push(parse_song_entry(value));
            continue;
        }
        if name_bytes.eq_ignore_ascii_case(b"SONGSELECT") {
            let params = split_unescaped(value, b':');
            if let Some(entry) = parse_song_select(&params) {
                entries.push(entry);
            }
        }
    }

    if name.is_empty() {
        return Err("Missing #COURSE tag".to_string());
    }

    Ok(CourseFile {
        name,
        name_translit,
        scripter,
        description,
        banner,
        background,
        repeat,
        lives,
        meters,
        entries,
    })
}

const fn empty_timing_segments() -> TimingSegments {
    TimingSegments {
        beat0_offset_adjust: 0.0,
        bpms: Vec::new(),
        stops: Vec::new(),
        delays: Vec::new(),
        warps: Vec::new(),
        speeds: Vec::new(),
        scrolls: Vec::new(),
        fakes: Vec::new(),
    }
}

fn empty_course_chart(step_type: &str, course_difficulty: Difficulty, meter: i32) -> ChartSummary {
    ChartSummary {
        step_type_str: step_type.to_string(),
        step_artist_str: "course total".to_string(),
        description_str: String::new(),
        chart_name_str: String::new(),
        chart_style_str: String::new(),
        difficulty_str: difficulty_label(course_difficulty).to_string(),
        rating_str: meter.to_string(),
        matrix_rating: 0.0,
        matrix_profile: crate::matrix::MatrixProfile::default(),
        tech_notation_str: String::new(),
        tier_bpm: 0.0,
        stats: crate::stats::ArrowStats::default(),
        stream_counts: crate::stats::StreamCounts::default(),
        total_measures: 0,
        total_streams: 0,
        mines_nonfake: 0,
        sn_detailed_breakdown: String::new(),
        sn_partial_breakdown: String::new(),
        sn_simple_breakdown: String::new(),
        detailed_breakdown: String::new(),
        partial_breakdown: String::new(),
        simple_breakdown: String::new(),
        max_nps: 0.0,
        median_nps: 0.0,
        duration_seconds: 0.0,
        detected_patterns: [0; PATTERN_COUNT],
        anchor_left: 0,
        anchor_down: 0,
        anchor_up: 0,
        anchor_right: 0,
        facing_left: 0,
        facing_right: 0,
        mono_total: 0,
        mono_percent: 0.0,
        candle_total: 0,
        candle_percent: 0.0,
        tech_counts: crate::step_parity::TechCounts::default(),
        note_annotations: None,
        custom_patterns: Vec::new(),
        short_hash: String::new(),
        bpm_neutral_hash: String::new(),
        elapsed: Duration::ZERO,
        measure_densities: Vec::new(),
        measure_nps_vec: Vec::new(),
        row_to_beat: Vec::new(),
        timing_segments: Arc::new(empty_timing_segments()),
        chart_offset_seconds: 0.0,
        chart_has_own_timing: false,
        minimized_note_data: Vec::new(),
        music_path: String::new(),
        chart_attacks: None,
        chart_has_own_attacks: false,
        chart_stops: None,
        chart_speeds: None,
        chart_scrolls: None,
        chart_bpms: None,
        chart_bpms_norm: None,
        chart_delays: None,
        chart_warps: None,
        chart_fakes: None,
        chart_display_bpm: None,
        chart_time_signatures: None,
        chart_labels: None,
        chart_tickcounts: None,
        chart_combos: None,
        cached_radar_values: None,
    }
}

fn merge_custom_patterns(
    total: &mut Vec<crate::patterns::CustomPatternSummary>,
    chart: &[crate::patterns::CustomPatternSummary],
) {
    // `total` starts empty and this function preserves its sorted invariant.
    for custom in chart {
        match total.binary_search_by(|entry| entry.pattern.cmp(&custom.pattern)) {
            Ok(index) => total[index].count += custom.count,
            Err(index) => total.insert(index, custom.clone()),
        }
    }
}

#[cfg(feature = "profile")]
pub(crate) fn profile_merge_custom_patterns_legacy(
    total: &mut Vec<crate::patterns::CustomPatternSummary>,
    chart: &[crate::patterns::CustomPatternSummary],
) {
    for custom in chart {
        if let Some(existing) = total
            .iter_mut()
            .find(|entry| entry.pattern == custom.pattern)
        {
            existing.count += custom.count;
        } else {
            total.push(custom.clone());
        }
    }
    total.sort_by(|left, right| left.pattern.cmp(&right.pattern));
}

#[cfg(feature = "profile")]
pub(crate) fn profile_merge_custom_patterns(
    total: &mut Vec<crate::patterns::CustomPatternSummary>,
    chart: &[crate::patterns::CustomPatternSummary],
) {
    merge_custom_patterns(total, chart);
}

fn add_course_chart(total: &mut ChartSummary, chart: &ChartSummary) {
    total.stats.total_arrows += chart.stats.total_arrows;
    total.stats.left += chart.stats.left;
    total.stats.down += chart.stats.down;
    total.stats.up += chart.stats.up;
    total.stats.right += chart.stats.right;
    total.stats.total_steps += chart.stats.total_steps;
    total.stats.jumps += chart.stats.jumps;
    total.stats.hands += chart.stats.hands;
    total.stats.mines += chart.stats.mines;
    total.stats.holds += chart.stats.holds;
    total.stats.rolls += chart.stats.rolls;
    total.stats.lifts += chart.stats.lifts;
    total.stats.fakes += chart.stats.fakes;

    total.stream_counts.run16_streams += chart.stream_counts.run16_streams;
    total.stream_counts.run20_streams += chart.stream_counts.run20_streams;
    total.stream_counts.run24_streams += chart.stream_counts.run24_streams;
    total.stream_counts.run32_streams += chart.stream_counts.run32_streams;
    total.stream_counts.total_breaks += chart.stream_counts.total_breaks;
    total.stream_counts.sn_breaks += chart.stream_counts.sn_breaks;

    total.total_measures += chart.total_measures;
    total.total_streams += chart.total_streams;
    total.mines_nonfake += chart.mines_nonfake;
    total.duration_seconds += chart.duration_seconds;

    total.anchor_left += chart.anchor_left;
    total.anchor_down += chart.anchor_down;
    total.anchor_up += chart.anchor_up;
    total.anchor_right += chart.anchor_right;
    total.facing_left += chart.facing_left;
    total.facing_right += chart.facing_right;
    total.candle_total += chart.candle_total;

    total.tech_counts.crossovers += chart.tech_counts.crossovers;
    total.tech_counts.half_crossovers += chart.tech_counts.half_crossovers;
    total.tech_counts.full_crossovers += chart.tech_counts.full_crossovers;
    total.tech_counts.footswitches += chart.tech_counts.footswitches;
    total.tech_counts.up_footswitches += chart.tech_counts.up_footswitches;
    total.tech_counts.down_footswitches += chart.tech_counts.down_footswitches;
    total.tech_counts.sideswitches += chart.tech_counts.sideswitches;
    total.tech_counts.jacks += chart.tech_counts.jacks;
    total.tech_counts.brackets += chart.tech_counts.brackets;
    total.tech_counts.doublesteps += chart.tech_counts.doublesteps;

    for i in 0..PATTERN_COUNT {
        total.detected_patterns[i] += chart.detected_patterns[i];
    }

    merge_custom_patterns(&mut total.custom_patterns, &chart.custom_patterns);
}

fn course_title_from_simfile(sim: &SimfileSummary) -> String {
    if sim.subtitle_str.is_empty() {
        sim.title_str.clone()
    } else {
        format!("{} {}", sim.title_str, sim.subtitle_str)
    }
}

fn simfile_translit_full_title(data: &[u8], ext: &str) -> Option<String> {
    let parsed = extract_sections(data, ext).ok()?;
    let title = parsed
        .title_translit
        .or(parsed.title)
        .map(|b| {
            let decoded = decode_bytes(b);
            let unescaped = unescape_tag(decoded.as_ref());
            clean_tag(unescaped.as_ref()).into_owned()
        })
        .unwrap_or_default();
    let subtitle = parsed
        .subtitle_translit
        .or(parsed.subtitle)
        .map(|b| unescape_tag(decode_bytes(b).as_ref()).into_owned())
        .unwrap_or_default();

    let title = title.trim();
    let subtitle = subtitle.trim();
    if subtitle.is_empty() {
        Some(title.to_string())
    } else {
        Some(format!("{title} {subtitle}"))
    }
}

fn song_dir_name(dir: &Path) -> String {
    dir.file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default()
}

fn resolve_song_dir(songs_dir: &Path, group: Option<&str>, song: &str) -> Option<PathBuf> {
    let song = song.trim();
    if song.is_empty() {
        return None;
    }

    if let Some(group) = group.map(str::trim).filter(|g| !g.is_empty()) {
        let group_dir = assets::is_dir_ci(songs_dir, group).or_else(|| {
            let p = songs_dir.join(group);
            p.is_dir().then_some(p)
        })?;

        let direct = assets::is_dir_ci(&group_dir, song).or_else(|| {
            let p = group_dir.join(song);
            p.is_dir().then_some(p)
        });
        if direct.is_some() {
            return direct;
        }

        let Ok(entries) = std::fs::read_dir(&group_dir) else {
            return None;
        };
        let mut subdirs: Vec<PathBuf> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_dir())
            .collect();
        subdirs.sort_by_cached_key(|p| {
            p.file_name()
                .map(|s| s.to_string_lossy().to_ascii_lowercase())
        });

        for dir in subdirs {
            let scan = pack::scan_song_dir(&dir, pack::ScanOpt::default()).ok()??;
            let sim = simfile::open(&scan.simfile).ok()?;
            let title = simfile_translit_full_title(&sim.data, sim.extension)?;
            if title.eq_ignore_ascii_case(song) {
                return Some(dir);
            }
        }
        return None;
    }

    let Ok(entries) = std::fs::read_dir(songs_dir) else {
        return None;
    };
    let mut groups: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    groups.sort_by_cached_key(|p| {
        p.file_name()
            .map(|s| s.to_string_lossy().to_ascii_lowercase())
    });

    for group_dir in groups {
        if let Some(dir) = assets::is_dir_ci(&group_dir, song).or_else(|| {
            let p = group_dir.join(song);
            p.is_dir().then_some(p)
        }) {
            return Some(dir);
        }
    }
    None
}

fn guess_songs_dir(course_path: &Path) -> Option<PathBuf> {
    let mut cur = course_path.parent();
    while let Some(dir) = cur {
        if dir
            .file_name()
            .is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case("Courses"))
        {
            let root = dir.parent()?;
            let songs = root.join("Songs");
            if songs.is_dir() {
                return Some(songs);
            }
        }
        cur = dir.parent();
    }
    None
}

fn select_chart<'a>(
    sim: &'a SimfileSummary,
    step_type: &str,
    difficulty: Difficulty,
) -> Option<&'a ChartSummary> {
    sim.charts.iter().find(|c| {
        stepstype_eq(&c.step_type_str, step_type)
            && c.difficulty_str
                .eq_ignore_ascii_case(difficulty_label(difficulty))
    })
}

fn parse_meter(meter: &str) -> i32 {
    meter.trim().parse::<i32>().unwrap_or(0)
}

fn avg_meter(meters: &[i32]) -> i32 {
    if meters.is_empty() {
        return 0;
    }
    let sum: i32 = meters.iter().sum();
    (f64::from(sum) / (meters.len() as f64)).round() as i32
}

#[derive(Debug, Hash, PartialEq, Eq)]
enum CourseHashKey {
    Short([u8; 16]),
    Other(String),
}

impl CourseHashKey {
    fn from_str(value: &str) -> Self {
        <[u8; 16]>::try_from(value.as_bytes())
            .map_or_else(|_| Self::Other(value.to_string()), Self::Short)
    }
}

fn dedup_push(vec: &mut Vec<String>, seen: &mut HashSet<CourseHashKey>, value: &str) {
    if value.is_empty() {
        return;
    }
    if seen.insert(CourseHashKey::from_str(value)) {
        vec.push(value.to_string());
    }
}

fn analyze_course_song(
    path: &Path,
    prepared: &PreparedAnalysis,
    scratch: &mut AnalysisScratch,
) -> Result<SimfileSummary, String> {
    let opened = simfile::open(path).map_err(|e| e.to_string())?;
    analyze_prepared_in(&opened.data, opened.extension, prepared, scratch)
}

pub fn analyze_crs_path(
    course_path: &Path,
    songs_dir: Option<&Path>,
    target_step_type: &str,
    course_difficulty: &str,
    options: AnalysisOptions,
) -> Result<CourseSummary, String> {
    analyze_crs_path_impl(
        course_path,
        songs_dir,
        target_step_type,
        course_difficulty,
        options,
        false,
    )
}

#[cfg(feature = "profile")]
#[doc(hidden)]
pub fn analyze_crs_path_cache_all_for_bench(
    course_path: &Path,
    songs_dir: Option<&Path>,
    target_step_type: &str,
    course_difficulty: &str,
    options: AnalysisOptions,
) -> Result<CourseSummary, String> {
    analyze_crs_path_impl(
        course_path,
        songs_dir,
        target_step_type,
        course_difficulty,
        options,
        true,
    )
}

fn analyze_crs_path_impl(
    course_path: &Path,
    songs_dir: Option<&Path>,
    target_step_type: &str,
    course_difficulty: &str,
    options: AnalysisOptions,
    cache_all: bool,
) -> Result<CourseSummary, String> {
    let start = Instant::now();
    let data = std::fs::read(course_path).map_err(|e| e.to_string())?;
    let course = parse_crs(&data)?;

    let base_songs_dir = songs_dir
        .map(PathBuf::from)
        .or_else(|| guess_songs_dir(course_path))
        .ok_or_else(|| "Unable to locate Songs/ directory (pass --songs-dir)".to_string())?;

    let course_diff = parse_course_difficulty(course_difficulty)
        .ok_or_else(|| format!("Invalid course difficulty: {course_difficulty}"))?;
    let step_type = normalize_stepstype(target_step_type);
    let prepared = PreparedAnalysis::new(options);
    let options = prepared.options();

    let entry_count = course.entries.len();
    let mut song_uses: HashMap<(Option<&str>, &str), usize> = HashMap::new();
    if !cache_all {
        song_uses.reserve(entry_count);
        for entry in &course.entries {
            if let CourseSong::Fixed { group, song } = &entry.song {
                *song_uses.entry((group.as_deref(), song)).or_default() += 1;
            }
        }
    }
    let repeated_songs = song_uses.values().filter(|&&uses| uses > 1).count();

    // Function-local course cache documentation:
    // - Owner/thread safety: the calling worker owns it; it is never shared.
    // - Lifetime/warmup: one course analysis; populated lazily on repeated songs.
    // - Capacity: at most 128 simfile summaries; insertion saturates at the cap.
    // - Miss/overflow: analyze at this load-time boundary; overflow bypasses insertion.
    // - Eviction/destruction: no eviction; entries drop on return, off gameplay frames.
    // - Instrumentation: allocation_perf tracks peak heap; no persistent counters needed.
    // - Worst-frame cost: none during gameplay; a miss costs one simfile analysis here.
    let cache_capacity = if cache_all {
        entry_count
    } else {
        repeated_songs.min(MAX_CACHED_SIMS)
    };
    let mut sim_cache: HashMap<PathBuf, SimfileSummary> = HashMap::with_capacity(cache_capacity);
    let mut entries = Vec::with_capacity(entry_count);
    let mut hash_list = Vec::new();
    let mut hash_seen = HashSet::new();
    let mut bpm_neutral_hash_list = Vec::new();
    let mut bpm_neutral_hash_seen = HashSet::new();

    let mut meters = Vec::with_capacity(entry_count);
    let mut measure_nps_all = Vec::new();
    let mut analysis_scratch = AnalysisScratch::default();

    let mut total = empty_course_chart(&step_type, course_diff, 0);

    for entry in &course.entries {
        let CourseSong::Fixed { group, song } = &entry.song else {
            return Err(
                "Only fixed #SONG entries are supported (no RANDOM/BEST/WORST/SONGSELECT yet)"
                    .to_string(),
            );
        };
        let StepsSpec::Difficulty(base_diff) = entry.steps else {
            return Err(
                "Only difficulty-based #SONG entries are supported (no meter ranges yet)"
                    .to_string(),
            );
        };

        let song_dir = resolve_song_dir(&base_songs_dir, group.as_deref(), song)
            .ok_or_else(|| format!("Song not found: {song}"))?;
        let scan = pack::scan_song_dir(&song_dir, pack::ScanOpt::default())
            .map_err(|e| format!("Failed scanning {}: {e:?}", song_dir.display()))?;
        let scan = scan.ok_or_else(|| format!("No simfile in {}", song_dir.display()))?;

        let cache_song = cache_all
            || song_uses
                .get(&(group.as_deref(), song.as_str()))
                .is_some_and(|&uses| uses > 1);
        let cache_has_room = cache_all || sim_cache.len() < MAX_CACHED_SIMS;
        let uncached_sim;
        let sim: &SimfileSummary = if cache_song {
            match sim_cache.entry(scan.simfile) {
                Entry::Occupied(entry) => entry.into_mut(),
                Entry::Vacant(entry) if cache_has_room => {
                    let summary =
                        analyze_course_song(entry.key(), &prepared, &mut analysis_scratch)?;
                    entry.insert(summary)
                }
                Entry::Vacant(entry) => {
                    let path = entry.into_key();
                    uncached_sim = analyze_course_song(&path, &prepared, &mut analysis_scratch)?;
                    &uncached_sim
                }
            }
        } else {
            uncached_sim = analyze_course_song(&scan.simfile, &prepared, &mut analysis_scratch)?;
            &uncached_sim
        };

        let base_chart = select_chart(sim, &step_type, base_diff).ok_or_else(|| {
            format!(
                "Chart not found for {} {} {}",
                song,
                step_type,
                difficulty_label(base_diff)
            )
        })?;
        let chart = if course_diff != Difficulty::Medium && !entry.no_difficult {
            let shifted = shift_diff(base_diff, course_diff);
            select_chart(sim, &step_type, shifted).unwrap_or(base_chart)
        } else {
            base_chart
        };

        dedup_push(&mut hash_list, &mut hash_seen, &chart.short_hash);
        dedup_push(
            &mut bpm_neutral_hash_list,
            &mut bpm_neutral_hash_seen,
            &chart.bpm_neutral_hash,
        );

        meters.push(parse_meter(&chart.rating_str));
        measure_nps_all.extend_from_slice(&chart.measure_nps_vec);

        entries.push(CourseEntrySummary {
            song: course_title_from_simfile(sim),
            song_dir: song_dir_name(&song_dir),
            step_type: chart.step_type_str.clone(),
            difficulty: chart.difficulty_str.clone(),
            rating: chart.rating_str.clone(),
            sha1: chart.short_hash.clone(),
            bpm_neutral_sha1: chart.bpm_neutral_hash.clone(),
        });
        add_course_chart(&mut total, chart);
    }

    if let Some(meter) = course_meter(&course.meters, course_diff) {
        total.rating_str = meter.to_string();
    } else {
        total.rating_str = avg_meter(&meters).to_string();
    }
    total.mono_total = total.facing_left + total.facing_right;
    total.mono_percent = if total.stats.total_steps > 0 {
        (f64::from(total.mono_total) / f64::from(total.stats.total_steps)) * 100.0
    } else {
        0.0
    };
    total.mono_percent = round_dp(total.mono_percent, 2);
    let max_candles = (total.stats.total_steps.saturating_sub(1)) / 2;
    total.candle_percent = if max_candles > 0 {
        (f64::from(total.candle_total) / f64::from(max_candles)) * 100.0
    } else {
        0.0
    };
    total.candle_percent = round_dp(total.candle_percent, 2);

    let (max_nps_raw, median_nps_raw) = get_nps_stats(&measure_nps_all);
    total.max_nps = round_sig_figs_6(max_nps_raw);
    total.median_nps = round_dp(median_nps_raw, 2);
    total.short_hash = hash_list.join(", ");
    total.bpm_neutral_hash = bpm_neutral_hash_list.join(", ");
    drop(song_uses);

    let elapsed = start.elapsed();
    let total_length = total.duration_seconds.floor().max(0.0) as i32;

    Ok(CourseSummary {
        course: course.name,
        course_difficulty: difficulty_label(course_diff).to_string(),
        step_type,
        total_length,
        entries,
        chart: total,
        sha1_hashes: hash_list,
        bpm_neutral_sha1_hashes: bpm_neutral_hash_list,
        pattern_counts_enabled: options.compute_pattern_counts,
        tech_counts_enabled: options.compute_tech_counts,
        total_elapsed: elapsed,
    })
}

#[cfg(test)]
mod tests {
    use super::{
        CourseHashKey, CourseSong, Difficulty, SongSort, analyze_crs_path, analyze_crs_path_impl,
        dedup_push, merge_custom_patterns, normalize_stepstype, parse_crs, stepstype_eq,
    };
    use std::collections::HashSet;
    use std::path::{Path, PathBuf};

    #[test]
    fn custom_pattern_merge_is_sorted_and_accumulates() {
        let mut total = vec![
            crate::patterns::CustomPatternSummary {
                pattern: "LDR".to_string(),
                count: 2,
            },
            crate::patterns::CustomPatternSummary {
                pattern: "RDL".to_string(),
                count: 3,
            },
        ];
        let chart = [
            crate::patterns::CustomPatternSummary {
                pattern: "RDL".to_string(),
                count: 5,
            },
            crate::patterns::CustomPatternSummary {
                pattern: "DLR".to_string(),
                count: 7,
            },
        ];

        merge_custom_patterns(&mut total, &chart);

        assert_eq!(
            total,
            [
                crate::patterns::CustomPatternSummary {
                    pattern: "DLR".to_string(),
                    count: 7,
                },
                crate::patterns::CustomPatternSummary {
                    pattern: "LDR".to_string(),
                    count: 2,
                },
                crate::patterns::CustomPatternSummary {
                    pattern: "RDL".to_string(),
                    count: 8,
                },
            ]
        );
    }
    use std::sync::atomic::{AtomicU64, Ordering};

    const SIMFILE: &[u8] = include_bytes!("../benches/fixtures/hash_fixture.ssc");
    static NEXT_TEMP_ID: AtomicU64 = AtomicU64::new(0);

    struct TempRoot(PathBuf);

    impl TempRoot {
        fn new() -> Self {
            let id = NEXT_TEMP_ID.fetch_add(1, Ordering::Relaxed);
            let path =
                std::env::temp_dir().join(format!("rssp-course-test-{}-{id}", std::process::id()));
            std::fs::create_dir(&path).expect("test root should be creatable");
            Self(path)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempRoot {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn dedup_push_materialized(output: &mut Vec<String>, seen: &mut HashSet<String>, value: &str) {
        if !value.is_empty() && seen.insert(value.to_string()) {
            output.push(value.to_string());
        }
    }

    #[test]
    fn compact_hash_dedup_matches_materialized_strings() {
        let values = [
            "",
            "0123456789abcdef",
            "0123456789abcdef",
            "fedcba9876543210",
            "short",
            "short",
            "é234567890abcdef",
        ];
        let mut expected = Vec::new();
        let mut expected_seen = HashSet::new();
        let mut actual = Vec::new();
        let mut actual_seen: HashSet<CourseHashKey> = HashSet::new();

        for value in values {
            dedup_push_materialized(&mut expected, &mut expected_seen, value);
            dedup_push(&mut actual, &mut actual_seen, value);
        }

        assert_eq!(actual, expected);
    }

    #[test]
    fn allocation_free_stepstype_match_preserves_normalization() {
        let cases = [
            "dance-single",
            " DANCE_SINGLE ",
            "pump_Double",
            "lights-cabinet",
            "dance_solo",
            "非ASCII-single",
            "",
        ];
        let targets = ["dance-single", "pump-double", "lights-cabinet", ""];

        for raw in cases {
            for normalized in targets {
                assert_eq!(
                    stepstype_eq(raw, normalized),
                    normalize_stepstype(raw) == normalized,
                    "raw={raw:?} normalized={normalized:?}"
                );
            }
        }
    }

    #[test]
    fn songselect_parses_itgmania_criteria() {
        let course = parse_crs(
            br#"
#COURSE:[Per-Song MMod Scale] ITL Random 8s;
#METER:Hard:8;
#SONGSELECT:GROUP=ITL Online 2022,ITL Online 2023:METER=8-8;
#SONGSELECT:TITLE=thank u\, next:ARTIST=Artist\=Name:GENRE=J-Pop,Black Metal:DIFFICULTY=Medium,Hard:BPMRANGE=120-160:DURATION=90-125:SORT=FewestPlays,4:GAINSECONDS=5:GAINLIVES=2:MODS=2x,noshowcourse,nodifficult;
"#,
        )
        .expect("ITGmania SONGSELECT course should parse");

        assert_eq!(course.entries.len(), 2);
        let CourseSong::Select(first) = &course.entries[0].song else {
            panic!("first entry should preserve SONGSELECT criteria");
        };
        assert_eq!(first.groups, ["ITL Online 2022", "ITL Online 2023"]);
        assert_eq!(first.meter_range, Some((8, 8)));

        let second_entry = &course.entries[1];
        let CourseSong::Select(second) = &second_entry.song else {
            panic!("second entry should preserve SONGSELECT criteria");
        };
        assert_eq!(second.titles, ["thank u, next"]);
        assert_eq!(second.artists, ["Artist=Name"]);
        assert_eq!(second.genres, ["J-Pop", "Black Metal"]);
        assert_eq!(second.difficulties, [Difficulty::Medium, Difficulty::Hard]);
        assert_eq!(second.bpm_range, Some((120.0, 160.0)));
        assert_eq!(second.duration_range, Some((90.0, 125.0)));
        assert_eq!(second.sort, Some(SongSort::FewestPlays));
        assert_eq!(second.index, 3);
        assert_eq!(second_entry.gain_seconds, 5.0);
        assert_eq!(second_entry.gain_lives, 2);
        assert_eq!(second_entry.modifiers, "2x");
        assert!(second_entry.secret);
        assert!(second_entry.no_difficult);
    }

    #[test]
    fn songselect_skips_invalid_entry_like_itgmania() {
        let course =
            parse_crs(b"#COURSE:Skip Invalid;\n#SONGSELECT:METER=12-8;\n#SONGSELECT:METER=8-8;")
                .expect("course should remain valid when one SONGSELECT is invalid");

        assert_eq!(course.entries.len(), 1);
        let CourseSong::Select(select) = &course.entries[0].song else {
            panic!("valid SONGSELECT should remain");
        };
        assert_eq!(select.meter_range, Some((8, 8)));
    }

    #[test]
    fn course_analysis_caches_songs_and_deduplicates_hashes() {
        let root = TempRoot::new();
        let songs_dir = root.path().join("Songs");
        let group_dir = songs_dir.join("Group");
        std::fs::create_dir_all(&group_dir).expect("group directory should be creatable");

        for song in ["SongA", "SongB"] {
            let song_dir = group_dir.join(song);
            std::fs::create_dir(&song_dir).expect("song directory should be creatable");
            std::fs::write(song_dir.join(format!("{song}.ssc")), SIMFILE)
                .expect("simfile should be writable");
        }
        let course_path = root.path().join("test.crs");
        std::fs::write(
            &course_path,
            concat!(
                "#COURSE:Optimization Test;\n",
                "#SONG:Group/SongA:Challenge:;\n",
                "#SONG:Group/SongB:Challenge:;\n",
                "#SONG:Group/SongA:Challenge:;\n",
            ),
        )
        .expect("course should be writable");

        let options = crate::AnalysisOptions {
            custom_patterns: vec!["LDU".to_string(), "RUR".to_string()],
            compute_pattern_counts: false,
            compute_tech_counts: false,
            ..crate::AnalysisOptions::default()
        };
        let cached_all = analyze_crs_path_impl(
            &course_path,
            Some(&songs_dir),
            "dance-single",
            "Medium",
            options.clone(),
            true,
        )
        .expect("legacy cache policy should analyze");
        let summary = analyze_crs_path(
            &course_path,
            Some(&songs_dir),
            "dance-single",
            "Medium",
            options,
        )
        .expect("course should analyze");

        assert_eq!(summary.course, "Optimization Test");
        assert_eq!(summary.entries.len(), 3);
        assert_eq!(summary.entries[0].song_dir, "SongA");
        assert_eq!(summary.entries[1].song_dir, "SongB");
        assert_eq!(summary.entries[2].song_dir, "SongA");
        assert_eq!(summary.sha1_hashes.len(), 1);
        assert_eq!(summary.bpm_neutral_sha1_hashes.len(), 1);
        assert_eq!(summary.chart.short_hash, summary.sha1_hashes[0]);
        assert_eq!(
            summary.chart.bpm_neutral_hash,
            summary.bpm_neutral_sha1_hashes[0]
        );
        assert!(summary.chart.custom_patterns.is_empty());
        assert!(!summary.pattern_counts_enabled);
        assert!(!summary.tech_counts_enabled);

        let mut expected_json = Vec::new();
        let mut actual_json = Vec::new();
        crate::report::write_course_reports(
            &cached_all,
            crate::report::OutputMode::JSON,
            &mut expected_json,
        )
        .expect("legacy cache summary should serialize");
        crate::report::write_course_reports(
            &summary,
            crate::report::OutputMode::JSON,
            &mut actual_json,
        )
        .expect("repeated-only cache summary should serialize");
        assert_eq!(actual_json, expected_json);
    }
}

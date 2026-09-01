use std::borrow::Cow;
use std::collections::{HashMap, HashSet, hash_map::Entry};
use std::ffi::{OsStr, OsString};
use std::hash::BuildHasher;
use std::path::{Path, PathBuf};
use std::sync::Arc;
#[cfg(not(target_arch = "wasm32"))]
use std::time::{Duration, Instant};
#[cfg(target_arch = "wasm32")]
use web_time::{Duration, Instant};

use crate::analysis::{AnalysisOptions, AnalysisScratch, PreparedAnalysis, analyze_prepared_in};
use crate::assets;
use crate::math::{round_dp, round_sig_figs_6};
use crate::nps::get_nps_stats_in_place;
use crate::pack;
use crate::parse::{clean_tag, decode_bytes, decode_unescape_trim, extract_sections, unescape_tag};
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
    let raw = raw.trim();
    if raw.eq_ignore_ascii_case("beginner") {
        Some(Difficulty::Beginner)
    } else if ["easy", "basic", "light"]
        .iter()
        .any(|value| raw.eq_ignore_ascii_case(value))
    {
        Some(Difficulty::Easy)
    } else if ["regular", "medium", "another", "trick", "standard"]
        .iter()
        .any(|value| raw.eq_ignore_ascii_case(value))
    {
        Some(Difficulty::Medium)
    } else if ["difficult", "hard", "ssr", "maniac", "heavy"]
        .iter()
        .any(|value| raw.eq_ignore_ascii_case(value))
    {
        Some(Difficulty::Hard)
    } else if ["challenge", "expert", "oni", "smaniac"]
        .iter()
        .any(|value| raw.eq_ignore_ascii_case(value))
    {
        Some(Difficulty::Challenge)
    } else if raw.eq_ignore_ascii_case("edit") {
        Some(Difficulty::Edit)
    } else {
        None
    }
}

fn normalize_stepstype(raw: &str) -> Cow<'_, str> {
    let raw = raw.trim();
    if !raw
        .as_bytes()
        .iter()
        .any(|byte| *byte == b'_' || byte.is_ascii_uppercase())
    {
        return Cow::Borrowed(raw);
    }

    let mut normalized = String::with_capacity(raw.len());
    for character in raw.chars() {
        normalized.push(if character == '_' {
            '-'
        } else {
            character.to_ascii_lowercase()
        });
    }
    Cow::Owned(normalized)
}

#[cfg(feature = "profile")]
fn normalize_stepstype_legacy(raw: &str) -> String {
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
    normalize_stepstype_legacy(raw) == normalized
}

#[cfg(feature = "profile")]
#[doc(hidden)]
#[must_use]
pub fn profile_stepstype_eq(raw: &str, normalized: &str) -> bool {
    stepstype_eq(raw, normalized)
}

#[cfg(feature = "profile")]
#[doc(hidden)]
#[must_use]
pub fn profile_normalize_stepstype(raw: &str, legacy: bool) -> Cow<'_, str> {
    if legacy {
        Cow::Owned(normalize_stepstype_legacy(raw))
    } else {
        normalize_stepstype(raw)
    }
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

fn visit_unescaped<'a>(block: &'a [u8], delim: u8, mut visit: impl FnMut(&'a [u8])) {
    if block.is_empty() {
        return;
    }
    let (mut start, mut bs) = (0usize, 0usize);
    for (i, &b) in block.iter().enumerate() {
        if b == b'\\' {
            bs += 1;
            continue;
        }
        if b == delim && bs & 1 == 0 {
            visit(&block[start..i]);
            start = i + 1;
        }
        bs = 0;
    }
    visit(&block[start..]);
}

fn list_capacity(block: &[u8], delim: u8) -> usize {
    if block.is_empty() {
        return 0;
    }
    1 + block.iter().filter(|&&byte| byte == delim).count()
}

#[cfg(feature = "profile")]
#[inline(always)]
fn split_unescaped(block: &[u8], delim: u8) -> Vec<&[u8]> {
    let mut out = Vec::new();
    visit_unescaped(block, delim, |item| out.push(item));
    out
}

fn split_pair(block: &[u8], delim: u8) -> Option<(&[u8], &[u8])> {
    let (mut split, mut bs) = (None, 0usize);
    for (i, &byte) in block.iter().enumerate() {
        if byte == b'\\' {
            bs += 1;
            continue;
        }
        if byte == delim && bs & 1 == 0 {
            if split.is_some() {
                return None;
            }
            split = Some(i);
        }
        bs = 0;
    }
    let split = split?;
    Some((&block[..split], &block[split + 1..]))
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

fn parse_repeat(raw: &[u8]) -> bool {
    raw.windows(3)
        .any(|window| window.eq_ignore_ascii_case(b"yes"))
}

#[cfg(feature = "profile")]
fn parse_repeat_legacy(raw: &[u8]) -> bool {
    decode_trim(raw).to_ascii_lowercase().contains("yes")
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

fn parse_select_list<const TIGHT_CAPACITY: bool>(raw: &[u8], out: &mut Vec<String>) -> bool {
    if raw.is_empty() {
        return false;
    }
    if TIGHT_CAPACITY {
        out.reserve_exact(list_capacity(raw, b','));
    }
    visit_unescaped(raw, b',', |item| {
        out.push(decode_unescape_trim(item).into_owned());
    });
    true
}

fn parse_select_range(raw: &[u8]) -> Option<(f64, f64)> {
    let raw = decode_trimmed(raw);
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
    let (sort_raw, index_raw) = split_pair(raw, b',')?;
    let sort_raw = decode_trimmed(sort_raw);
    let sort = if sort_raw.eq_ignore_ascii_case("randomize") {
        None
    } else if sort_raw.eq_ignore_ascii_case("mostplays") || sort_raw.eq_ignore_ascii_case("best") {
        Some(SongSort::MostPlays)
    } else if sort_raw.eq_ignore_ascii_case("fewestplays") || sort_raw.eq_ignore_ascii_case("worst")
    {
        Some(SongSort::FewestPlays)
    } else if sort_raw.eq_ignore_ascii_case("topgrades")
        || sort_raw.eq_ignore_ascii_case("gradebest")
    {
        Some(SongSort::TopGrades)
    } else if sort_raw.eq_ignore_ascii_case("lowestgrades")
        || sort_raw.eq_ignore_ascii_case("gradeworst")
    {
        Some(SongSort::LowestGrades)
    } else {
        return None;
    };
    let index = (decode_trimmed(index_raw).parse::<i32>().unwrap_or(0) - 1).clamp(0, 500);
    Some((sort, index))
}

fn apply_select_mods(entry: &mut CourseEntry, raw: &[u8]) {
    let mut modifiers = String::new();
    visit_unescaped(raw, b',', |item| {
        let value = decode_unescape_trim(item);
        if value.eq_ignore_ascii_case("showcourse") {
            entry.secret = false;
        } else if value.eq_ignore_ascii_case("noshowcourse") {
            entry.secret = true;
        } else if value.eq_ignore_ascii_case("nodifficult") {
            entry.no_difficult = true;
        } else if !value.is_empty() {
            if modifiers.is_empty() {
                modifiers.reserve(raw.len());
            } else {
                modifiers.push(',');
            }
            modifiers.push_str(&value);
        }
    });
    entry.modifiers = modifiers;
}

#[cfg(feature = "profile")]
#[doc(hidden)]
pub fn profile_select_mods(raw: &[u8]) -> (bool, bool, String) {
    let mut entry = CourseEntry {
        song: CourseSong::RandomAny,
        steps: StepsSpec::Unknown { raw: String::new() },
        modifiers: String::new(),
        secret: false,
        no_difficult: false,
        gain_seconds: 0.0,
        gain_lives: -1,
    };
    apply_select_mods(&mut entry, raw);
    (entry.secret, entry.no_difficult, entry.modifiers)
}

fn apply_select_param<const TIGHT_CAPACITY: bool>(
    entry: &mut CourseEntry,
    param: &[u8],
) -> Option<()> {
    let (name_raw, value) = split_pair(param, b'=')?;
    let name = decode_trimmed(name_raw);
    let CourseSong::Select(select) = &mut entry.song else {
        unreachable!("SONGSELECT parser always constructs selection criteria");
    };
    if name.eq_ignore_ascii_case("TITLE") {
        parse_select_list::<TIGHT_CAPACITY>(value, &mut select.titles).then_some(())?;
    } else if name.eq_ignore_ascii_case("GROUP") {
        parse_select_list::<TIGHT_CAPACITY>(value, &mut select.groups).then_some(())?;
    } else if name.eq_ignore_ascii_case("ARTIST") {
        parse_select_list::<TIGHT_CAPACITY>(value, &mut select.artists).then_some(())?;
    } else if name.eq_ignore_ascii_case("GENRE") {
        parse_select_list::<TIGHT_CAPACITY>(value, &mut select.genres).then_some(())?;
    } else if name.eq_ignore_ascii_case("DIFFICULTY") {
        if TIGHT_CAPACITY && !value.is_empty() {
            select
                .difficulties
                .reserve_exact(list_capacity(value, b','));
        }
        visit_unescaped(value, b',', |raw| {
            if let Some(diff) = parse_course_difficulty(&decode_trimmed(raw)) {
                select.difficulties.push(diff);
            }
        });
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
        entry.gain_lives = decode_trimmed(value).parse().unwrap_or(0);
    } else if name.eq_ignore_ascii_case("GAINSECONDS") {
        entry.gain_seconds = decode_trimmed(value).parse::<i32>().unwrap_or(0) as f32;
    } else if name.eq_ignore_ascii_case("MODS") {
        apply_select_mods(entry, value);
    }
    Some(())
}

fn parse_song_select<const TIGHT_CAPACITY: bool>(raw: &[u8]) -> Option<CourseEntry> {
    let mut entry = CourseEntry {
        song: CourseSong::Select(SongSelect::default()),
        steps: StepsSpec::Unknown { raw: String::new() },
        modifiers: String::new(),
        secret: false,
        no_difficult: false,
        gain_seconds: 0.0,
        gain_lives: -1,
    };
    let mut valid = true;
    visit_unescaped(raw, b':', |param| {
        if valid {
            valid = apply_select_param::<TIGHT_CAPACITY>(&mut entry, param).is_some();
        }
    });
    valid.then_some(entry)
}

fn set_course_meter(diff_raw: &[u8], meter_raw: &[u8], meters: &mut [Option<i32>; 6]) {
    let diff_raw = decode_trimmed(diff_raw);
    let meter_raw = decode_trimmed(meter_raw);
    if let Some(diff) = parse_course_difficulty(&diff_raw)
        && let Ok(meter) = meter_raw.parse::<i32>()
    {
        meters[diff as usize] = Some(meter.max(0));
    }
}

fn parse_course_meter_tag(value: &[u8], meters: &mut [Option<i32>; 6]) {
    let mut field_count = 0usize;
    let mut pending = None;
    visit_unescaped(value, b':', |field| {
        field_count += 1;
        if let Some(diff) = pending.take() {
            set_course_meter(diff, field, meters);
        } else {
            pending = Some(field);
        }
    });

    if field_count == 1 {
        let meter = decode_trimmed(pending.expect("one field was visited"))
            .parse::<i32>()
            .unwrap_or(0)
            .max(0);
        meters[Difficulty::Medium as usize] = Some(meter);
    }
}

#[cfg(feature = "profile")]
fn parse_course_meter_tag_legacy(value: &[u8], meters: &mut [Option<i32>; 6]) {
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

#[cfg(any(test, feature = "profile"))]
#[inline(always)]
fn has_banner_prefix_old(path: &Path, stem_lc: &str, ext: &str) -> bool {
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

#[cfg(any(test, feature = "profile"))]
fn push_banner_matches_old(dir: &Path, stem_lc: &str, ext: &str, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut matches: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| has_banner_prefix_old(p, stem_lc, ext))
        .collect();
    matches.sort_by_cached_key(|p| assets::lc_name(p));
    out.extend(matches);
}

#[inline(always)]
fn starts_ascii_ci(actual: &str, expected: &str) -> bool {
    actual
        .as_bytes()
        .get(..expected.len())
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case(expected.as_bytes()))
}

#[cfg(feature = "profile")]
fn banner_rank(path: &Path, course_stem: &str) -> Option<usize> {
    if !path.is_file() {
        return None;
    }
    let path_ext = path.extension()?.to_str()?;
    let rank = COURSE_BANNER_EXTS
        .iter()
        .position(|ext| path_ext.eq_ignore_ascii_case(ext))?;
    let file_stem = path.file_stem()?.to_str()?;
    starts_ascii_ci(file_stem, course_stem).then_some(rank)
}

fn banner_name_rank(name: &OsStr, course_stem: &str) -> Option<usize> {
    let path = Path::new(name);
    let path_ext = path.extension()?.to_str()?;
    let rank = COURSE_BANNER_EXTS
        .iter()
        .position(|ext| path_ext.eq_ignore_ascii_case(ext))?;
    let file_stem = path.file_stem()?.to_str()?;
    starts_ascii_ci(file_stem, course_stem).then_some(rank)
}

#[cfg(any(test, feature = "profile"))]
fn resolve_banner_old(course_path: &Path, banner_tag: &str) -> Option<PathBuf> {
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
        push_banner_matches_old(parent, &stem_lc, ext, &mut possible);
    }
    possible.into_iter().next()
}

#[cfg(feature = "profile")]
fn resolve_banner_full_paths(course_path: &Path, banner_tag: &str) -> Option<PathBuf> {
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
    let course_stem = course_path.file_stem()?.to_string_lossy();
    if course_stem.is_empty() {
        return None;
    }

    let entries = std::fs::read_dir(parent).ok()?;
    let mut possible: [Option<PathBuf>; COURSE_BANNER_EXTS.len()] = Default::default();
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(rank) = banner_rank(&path, &course_stem) else {
            continue;
        };
        if possible[rank]
            .as_deref()
            .is_none_or(|current| assets::cmp_name_ci(&path, current).is_lt())
        {
            possible[rank] = Some(path);
        }
    }
    possible.into_iter().flatten().next()
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
    let course_stem = course_path.file_stem()?.to_string_lossy();
    if course_stem.is_empty() {
        return None;
    }

    let entries = std::fs::read_dir(parent).ok()?;
    let mut possible: [Option<OsString>; COURSE_BANNER_EXTS.len()] = Default::default();
    for entry in entries.flatten() {
        if !assets::entry_is_file(&entry) {
            continue;
        }
        let name = entry.file_name();
        let Some(rank) = banner_name_rank(&name, &course_stem) else {
            continue;
        };
        if possible[rank]
            .as_deref()
            .is_none_or(|current| assets::cmp_os_ci(&name, current).is_lt())
        {
            possible[rank] = Some(name);
        }
    }
    possible
        .into_iter()
        .flatten()
        .next()
        .map(|name| parent.join(name))
}

#[cfg(feature = "profile")]
#[doc(hidden)]
#[must_use]
pub fn profile_course_banner(
    course_path: &Path,
    banner_tag: &str,
    legacy: bool,
) -> Option<PathBuf> {
    if legacy {
        resolve_banner_old(course_path, banner_tag)
    } else {
        resolve_course_banner_path(course_path, banner_tag)
    }
}

#[cfg(feature = "profile")]
#[doc(hidden)]
#[must_use]
pub fn profile_course_banner_full_paths(course_path: &Path, banner_tag: &str) -> Option<PathBuf> {
    resolve_banner_full_paths(course_path, banner_tag)
}

// Bound speculative storage derived from untrusted course text.
const MAX_COURSE_RESERVE: usize = 64;

fn course_reserve_len(data_len: usize, start: usize, next: usize) -> usize {
    let entry_len = next.saturating_sub(start).max(1);
    data_len
        .saturating_sub(start)
        .div_ceil(entry_len)
        .clamp(1, MAX_COURSE_RESERVE)
}

fn push_course_entry<const RESERVE_ENTRIES: bool>(
    entries: &mut Vec<CourseEntry>,
    entry: CourseEntry,
    data_len: usize,
    start: usize,
    next: usize,
) {
    if RESERVE_ENTRIES && entries.capacity() == 0 {
        entries.reserve_exact(course_reserve_len(data_len, start, next));
    }
    entries.push(entry);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CourseTag {
    Course,
    CourseTranslit,
    Scripter,
    Description,
    Repeat,
    Banner,
    Background,
    Lives,
    Meter,
    Song,
    SongSelect,
    Unknown,
}

#[inline(always)]
fn course_tag(name: &[u8]) -> CourseTag {
    match name.len() {
        4 if name.eq_ignore_ascii_case(b"SONG") => CourseTag::Song,
        5 if name.eq_ignore_ascii_case(b"LIVES") => CourseTag::Lives,
        5 if name.eq_ignore_ascii_case(b"METER") => CourseTag::Meter,
        6 if name.eq_ignore_ascii_case(b"COURSE") => CourseTag::Course,
        6 if name.eq_ignore_ascii_case(b"REPEAT") => CourseTag::Repeat,
        6 if name.eq_ignore_ascii_case(b"BANNER") => CourseTag::Banner,
        8 if name.eq_ignore_ascii_case(b"SCRIPTER") => CourseTag::Scripter,
        10 if name.eq_ignore_ascii_case(b"BACKGROUND") => CourseTag::Background,
        10 if name.eq_ignore_ascii_case(b"SONGSELECT") => CourseTag::SongSelect,
        11 if name.eq_ignore_ascii_case(b"DESCRIPTION") => CourseTag::Description,
        14 if name.eq_ignore_ascii_case(b"COURSETRANSLIT") => CourseTag::CourseTranslit,
        _ => CourseTag::Unknown,
    }
}

#[cfg(any(test, feature = "profile"))]
fn course_tag_sequential(name: &[u8]) -> CourseTag {
    if name.eq_ignore_ascii_case(b"COURSE") {
        CourseTag::Course
    } else if name.eq_ignore_ascii_case(b"COURSETRANSLIT") {
        CourseTag::CourseTranslit
    } else if name.eq_ignore_ascii_case(b"SCRIPTER") {
        CourseTag::Scripter
    } else if name.eq_ignore_ascii_case(b"DESCRIPTION") {
        CourseTag::Description
    } else if name.eq_ignore_ascii_case(b"REPEAT") {
        CourseTag::Repeat
    } else if name.eq_ignore_ascii_case(b"BANNER") {
        CourseTag::Banner
    } else if name.eq_ignore_ascii_case(b"BACKGROUND") {
        CourseTag::Background
    } else if name.eq_ignore_ascii_case(b"LIVES") {
        CourseTag::Lives
    } else if name.eq_ignore_ascii_case(b"METER") {
        CourseTag::Meter
    } else if name.eq_ignore_ascii_case(b"SONG") {
        CourseTag::Song
    } else if name.eq_ignore_ascii_case(b"SONGSELECT") {
        CourseTag::SongSelect
    } else {
        CourseTag::Unknown
    }
}

fn parse_crs_with<
    const LEGACY: bool,
    const RESERVE_ENTRIES: bool,
    const INDEXED_TAGS: bool,
    const TIGHT_SELECT_CAPACITY: bool,
>(
    data: &[u8],
) -> Result<CourseFile, String> {
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
        let tag_start = i;
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

        let tag = if INDEXED_TAGS {
            course_tag(name_bytes)
        } else {
            #[cfg(any(test, feature = "profile"))]
            {
                course_tag_sequential(name_bytes)
            }
            #[cfg(not(any(test, feature = "profile")))]
            {
                unreachable!("sequential tag dispatch requires profile feature")
            }
        };

        match tag {
            CourseTag::Course => name = decode_trim(value),
            CourseTag::CourseTranslit => name_translit = decode_trim(value),
            CourseTag::Scripter => scripter = decode_trim(value),
            CourseTag::Description => description = decode_trim(value),
            CourseTag::Repeat => {
                repeat = if LEGACY {
                    #[cfg(feature = "profile")]
                    {
                        parse_repeat_legacy(value)
                    }
                    #[cfg(not(feature = "profile"))]
                    {
                        unreachable!("legacy parser requires profile feature")
                    }
                } else {
                    parse_repeat(value)
                };
            }
            CourseTag::Banner => banner = decode_trim(value),
            CourseTag::Background => background = decode_trim(value),
            CourseTag::Lives => lives = decode_trim(value).parse::<i32>().unwrap_or(0).max(0),
            CourseTag::Meter => {
                if LEGACY {
                    #[cfg(feature = "profile")]
                    parse_course_meter_tag_legacy(value, &mut meters);
                    #[cfg(not(feature = "profile"))]
                    unreachable!("legacy parser requires profile feature");
                } else {
                    parse_course_meter_tag(value, &mut meters);
                }
            }
            CourseTag::Song => push_course_entry::<RESERVE_ENTRIES>(
                &mut entries,
                parse_song_entry(value),
                data.len(),
                tag_start,
                i,
            ),
            CourseTag::SongSelect => {
                if let Some(entry) = parse_song_select::<TIGHT_SELECT_CAPACITY>(value) {
                    push_course_entry::<RESERVE_ENTRIES>(
                        &mut entries,
                        entry,
                        data.len(),
                        tag_start,
                        i,
                    );
                }
            }
            CourseTag::Unknown => {}
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

pub fn parse_crs(data: &[u8]) -> Result<CourseFile, String> {
    parse_crs_with::<false, true, true, true>(data)
}

#[cfg(feature = "profile")]
#[doc(hidden)]
pub fn profile_parse_crs(data: &[u8], legacy: bool) -> Result<CourseFile, String> {
    if legacy {
        parse_crs_with::<true, true, true, true>(data)
    } else {
        parse_crs_with::<false, true, true, true>(data)
    }
}

#[cfg(feature = "profile")]
#[doc(hidden)]
pub fn profile_parse_crs_reserve(data: &[u8], legacy_growth: bool) -> Result<CourseFile, String> {
    if legacy_growth {
        parse_crs_with::<false, false, true, true>(data)
    } else {
        parse_crs_with::<false, true, true, true>(data)
    }
}

#[cfg(feature = "profile")]
#[doc(hidden)]
pub fn profile_parse_crs_select_lists(
    data: &[u8],
    growing_lists: bool,
) -> Result<CourseFile, String> {
    if growing_lists {
        parse_crs_with::<false, true, true, false>(data)
    } else {
        parse_crs_with::<false, true, true, true>(data)
    }
}

#[cfg(feature = "profile")]
#[doc(hidden)]
pub fn profile_parse_crs_dispatch(data: &[u8], sequential: bool) -> Result<CourseFile, String> {
    if sequential {
        parse_crs_with::<false, true, false, true>(data)
    } else {
        parse_crs_with::<false, true, true, true>(data)
    }
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

fn course_title(title: &str, subtitle: &str) -> String {
    if subtitle.is_empty() {
        title.to_owned()
    } else {
        let mut out = String::with_capacity(title.len() + 1 + subtitle.len());
        out.push_str(title);
        out.push(' ');
        out.push_str(subtitle);
        out
    }
}

#[cfg(feature = "profile")]
#[doc(hidden)]
#[must_use]
pub fn profile_course_title(title: &str, subtitle: &str, prealloc: bool) -> String {
    if prealloc {
        course_title(title, subtitle)
    } else if subtitle.is_empty() {
        title.to_owned()
    } else {
        format!("{title} {subtitle}")
    }
}

#[cfg(feature = "profile")]
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

fn title_parts_eq(title: &str, subtitle: &str, expected: &str) -> bool {
    if subtitle.is_empty() {
        return title.eq_ignore_ascii_case(expected);
    }
    let expected = expected.as_bytes();
    expected
        .get(..title.len())
        .is_some_and(|value| value.eq_ignore_ascii_case(title.as_bytes()))
        && expected.get(title.len()) == Some(&b' ')
        && expected
            .get(title.len() + 1..)
            .is_some_and(|value| value.eq_ignore_ascii_case(subtitle.as_bytes()))
}

fn simfile_translit_title_eq(data: &[u8], ext: &str, expected: &str) -> Option<bool> {
    let parsed = extract_sections(data, ext).ok()?;
    let title_bytes = parsed.title_translit.or(parsed.title).unwrap_or_default();
    let title_decoded = decode_bytes(title_bytes);
    let title_unescaped = unescape_tag(title_decoded.as_ref());
    let title_cleaned = clean_tag(title_unescaped.as_ref());

    let subtitle_bytes = parsed
        .subtitle_translit
        .or(parsed.subtitle)
        .unwrap_or_default();
    let subtitle_decoded = decode_bytes(subtitle_bytes);
    let subtitle_unescaped = unescape_tag(subtitle_decoded.as_ref());

    Some(title_parts_eq(
        title_cleaned.trim(),
        subtitle_unescaped.trim(),
        expected,
    ))
}

#[cfg(feature = "profile")]
#[doc(hidden)]
#[must_use]
pub fn profile_simfile_title_eq(
    data: &[u8],
    ext: &str,
    expected: &str,
    legacy: bool,
) -> Option<bool> {
    if legacy {
        simfile_translit_full_title(data, ext).map(|title| title.eq_ignore_ascii_case(expected))
    } else {
        simfile_translit_title_eq(data, ext, expected)
    }
}

fn song_dir_name(dir: &Path) -> String {
    dir.file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default()
}

fn sorted_dir_names(dir: &Path) -> Option<Vec<OsString>> {
    let entries = std::fs::read_dir(dir).ok()?;
    let mut names = Vec::new();
    for entry in entries.flatten() {
        if assets::entry_is_dir(&entry) {
            names.push(entry.file_name());
        }
    }
    names.sort_by(|left, right| assets::cmp_os_ci(left, right));
    Some(names)
}

fn find_named_dir(parent: &Path, name: &str) -> Option<PathBuf> {
    assets::is_dir_ci(parent, name).or_else(|| {
        let path = parent.join(name);
        path.is_dir().then_some(path)
    })
}

fn resolve_group_song(group_dir: &Path, song: &str) -> Option<PathBuf> {
    let direct = find_named_dir(group_dir, song);
    if direct.is_some() {
        return direct;
    }

    for name in sorted_dir_names(group_dir)? {
        let dir = group_dir.join(name);
        let scan = pack::scan_song_dir(&dir, pack::ScanOpt::default()).ok()??;
        let sim = simfile::open(&scan.simfile).ok()?;
        if simfile_translit_title_eq(&sim.data, sim.extension, song)? {
            return Some(dir);
        }
    }
    None
}

fn resolve_song_dir(songs_dir: &Path, group: Option<&str>, song: &str) -> Option<PathBuf> {
    let song = song.trim();
    if song.is_empty() {
        return None;
    }

    if let Some(group) = group.map(str::trim).filter(|g| !g.is_empty()) {
        let group_dir = find_named_dir(songs_dir, group)?;
        return resolve_group_song(&group_dir, song);
    }

    for name in sorted_dir_names(songs_dir)? {
        let group_dir = songs_dir.join(name);
        if let Some(dir) = find_named_dir(&group_dir, song) {
            return Some(dir);
        }
    }
    None
}

#[cfg(feature = "profile")]
fn resolve_song_dir_legacy(songs_dir: &Path, group: Option<&str>, song: &str) -> Option<PathBuf> {
    let song = song.trim();
    if song.is_empty() {
        return None;
    }

    if let Some(group) = group.map(str::trim).filter(|g| !g.is_empty()) {
        let group_dir = assets::is_dir_ci(songs_dir, group).or_else(|| {
            let path = songs_dir.join(group);
            path.is_dir().then_some(path)
        })?;
        let direct = assets::is_dir_ci(&group_dir, song).or_else(|| {
            let path = group_dir.join(song);
            path.is_dir().then_some(path)
        });
        if direct.is_some() {
            return direct;
        }

        let entries = std::fs::read_dir(&group_dir).ok()?;
        let mut subdirs: Vec<PathBuf> = entries
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| path.is_dir())
            .collect();
        subdirs.sort_by_cached_key(|path| {
            path.file_name()
                .map(|name| name.to_string_lossy().to_ascii_lowercase())
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

    let entries = std::fs::read_dir(songs_dir).ok()?;
    let mut groups: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect();
    groups.sort_by_cached_key(|path| {
        path.file_name()
            .map(|name| name.to_string_lossy().to_ascii_lowercase())
    });
    for group_dir in groups {
        if let Some(dir) = assets::is_dir_ci(&group_dir, song).or_else(|| {
            let path = group_dir.join(song);
            path.is_dir().then_some(path)
        }) {
            return Some(dir);
        }
    }
    None
}

#[cfg(feature = "profile")]
#[doc(hidden)]
pub fn profile_resolve_song_dir(
    songs_dir: &Path,
    group: Option<&str>,
    song: &str,
    legacy: bool,
) -> Option<PathBuf> {
    if legacy {
        resolve_song_dir_legacy(songs_dir, group, song)
    } else {
        resolve_song_dir(songs_dir, group, song)
    }
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

// These keys are locally computed fixed-width chart digests, so the fast,
// per-table-seeded hasher is appropriate for this internal dedup set.
type CourseHashSet = HashSet<CourseHashKey, foldhash::fast::RandomState>;

// Small courses defer hash-table creation until a ninth distinct digest. Larger
// courses retain the prior in-loop hash path to bound the extra traversal.
const LINEAR_HASH_LIMIT: usize = 8;
const ADAPTIVE_HASH_MAX: usize = 64;

const fn course_hash_capacity(max_len: usize) -> usize {
    if max_len > 64 {
        0
    } else if max_len < 8 {
        max_len
    } else {
        8
    }
}

impl CourseHashKey {
    fn from_str(value: &str) -> Self {
        <[u8; 16]>::try_from(value.as_bytes())
            .map_or_else(|_| Self::Other(value.to_string()), Self::Short)
    }
}

fn dedup_push<S: BuildHasher>(
    vec: &mut Vec<String>,
    seen: &mut HashSet<CourseHashKey, S>,
    value: &str,
) {
    if value.is_empty() {
        return;
    }
    if seen.insert(CourseHashKey::from_str(value)) {
        vec.push(value.to_string());
    }
}

#[cold]
fn seed_hashes(values: &[String]) -> CourseHashSet {
    let mut hashed = CourseHashSet::with_capacity_and_hasher(
        LINEAR_HASH_LIMIT * 2,
        foldhash::fast::RandomState::default(),
    );
    hashed.extend(values.iter().map(|value| CourseHashKey::from_str(value)));
    hashed
}

fn collect_small_course_hashes<T>(
    values: &[T],
    get: impl for<'a> Fn(&'a T) -> &'a str,
) -> Vec<String> {
    debug_assert!(values.len() <= ADAPTIVE_HASH_MAX);
    let mut out = Vec::with_capacity(course_hash_capacity(values.len()));
    for (index, item) in values.iter().enumerate() {
        let value = get(item);
        if value.is_empty() || out.iter().any(|existing| existing == value) {
            continue;
        }
        if out.len() < LINEAR_HASH_LIMIT {
            out.push(value.to_string());
            continue;
        }

        let mut seen = seed_hashes(&out);
        dedup_push(&mut out, &mut seen, value);
        for item in &values[index + 1..] {
            dedup_push(&mut out, &mut seen, get(item));
        }
        break;
    }
    out
}

#[cfg(feature = "profile")]
#[doc(hidden)]
#[must_use]
pub fn profile_dedup_hashes(values: &[String], std_hash: bool) -> Vec<String> {
    fn collect<S: BuildHasher + Default>(values: &[String]) -> Vec<String> {
        let mut out = Vec::new();
        let mut seen = HashSet::with_hasher(S::default());
        for value in values {
            dedup_push(&mut out, &mut seen, value);
        }
        out
    }

    if std_hash {
        collect::<std::collections::hash_map::RandomState>(values)
    } else {
        collect::<foldhash::fast::RandomState>(values)
    }
}

#[cfg(feature = "profile")]
#[doc(hidden)]
#[must_use]
pub fn profile_dedup_hashes_reserved(values: &[String]) -> Vec<String> {
    let mut out = Vec::with_capacity(course_hash_capacity(values.len()));
    let mut seen = CourseHashSet::default();
    for value in values {
        dedup_push(&mut out, &mut seen, value);
    }
    out
}

#[cfg(feature = "profile")]
#[doc(hidden)]
#[must_use]
pub fn profile_dedup_hashes_adaptive(values: &[String]) -> Vec<String> {
    if values.len() > ADAPTIVE_HASH_MAX {
        profile_dedup_hashes_reserved(values)
    } else {
        collect_small_course_hashes(values, String::as_str)
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
    analyze_crs_path_impl::<true, true, true, true, true, true>(
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
    analyze_crs_path_impl::<true, true, true, true, true, true>(
        course_path,
        songs_dir,
        target_step_type,
        course_difficulty,
        options,
        true,
    )
}

#[cfg(feature = "profile")]
#[doc(hidden)]
pub fn profile_analyze_crs(
    course_path: &Path,
    songs_dir: Option<&Path>,
    target_step_type: &str,
    course_difficulty: &str,
    options: AnalysisOptions,
    song_key_cache: bool,
) -> Result<CourseSummary, String> {
    if song_key_cache {
        analyze_crs_path_impl::<true, false, false, false, true, true>(
            course_path,
            songs_dir,
            target_step_type,
            course_difficulty,
            options,
            false,
        )
    } else {
        analyze_crs_path_impl::<false, false, false, false, true, true>(
            course_path,
            songs_dir,
            target_step_type,
            course_difficulty,
            options,
            false,
        )
    }
}

#[cfg(feature = "profile")]
#[doc(hidden)]
pub fn profile_analyze_groups(
    course_path: &Path,
    songs_dir: Option<&Path>,
    target_step_type: &str,
    course_difficulty: &str,
    options: AnalysisOptions,
    group_cache: bool,
) -> Result<CourseSummary, String> {
    if group_cache {
        analyze_crs_path_impl::<true, true, false, false, true, true>(
            course_path,
            songs_dir,
            target_step_type,
            course_difficulty,
            options,
            false,
        )
    } else {
        analyze_crs_path_impl::<true, false, false, false, true, true>(
            course_path,
            songs_dir,
            target_step_type,
            course_difficulty,
            options,
            false,
        )
    }
}

#[cfg(feature = "profile")]
#[doc(hidden)]
pub fn profile_analyze_catalog(
    course_path: &Path,
    songs_dir: Option<&Path>,
    target_step_type: &str,
    course_difficulty: &str,
    options: AnalysisOptions,
    group_catalog: bool,
) -> Result<CourseSummary, String> {
    if group_catalog {
        analyze_crs_path_impl::<true, true, true, false, true, true>(
            course_path,
            songs_dir,
            target_step_type,
            course_difficulty,
            options,
            false,
        )
    } else {
        analyze_crs_path_impl::<true, true, false, false, true, true>(
            course_path,
            songs_dir,
            target_step_type,
            course_difficulty,
            options,
            false,
        )
    }
}

#[cfg(feature = "profile")]
#[doc(hidden)]
pub fn profile_catalog_dirs(
    course_path: &Path,
    songs_dir: Option<&Path>,
    target_step_type: &str,
    course_difficulty: &str,
    options: AnalysisOptions,
    trust_catalog: bool,
) -> Result<CourseSummary, String> {
    if trust_catalog {
        analyze_crs_path_impl::<true, true, true, true, true, true>(
            course_path,
            songs_dir,
            target_step_type,
            course_difficulty,
            options,
            false,
        )
    } else {
        analyze_crs_path_impl::<true, true, true, false, true, true>(
            course_path,
            songs_dir,
            target_step_type,
            course_difficulty,
            options,
            false,
        )
    }
}

#[cfg(feature = "profile")]
#[doc(hidden)]
pub fn profile_course_nps(
    course_path: &Path,
    songs_dir: Option<&Path>,
    target_step_type: &str,
    course_difficulty: &str,
    options: AnalysisOptions,
    prealloc_nps: bool,
) -> Result<CourseSummary, String> {
    if prealloc_nps {
        analyze_crs_path_impl::<true, true, true, true, true, true>(
            course_path,
            songs_dir,
            target_step_type,
            course_difficulty,
            options,
            false,
        )
    } else {
        analyze_crs_path_impl::<true, true, true, true, false, true>(
            course_path,
            songs_dir,
            target_step_type,
            course_difficulty,
            options,
            false,
        )
    }
}

#[cfg(feature = "profile")]
#[doc(hidden)]
pub fn profile_course_titles(
    course_path: &Path,
    songs_dir: Option<&Path>,
    target_step_type: &str,
    course_difficulty: &str,
    options: AnalysisOptions,
    prealloc_title: bool,
) -> Result<CourseSummary, String> {
    if prealloc_title {
        analyze_crs_path_impl::<true, true, true, true, true, true>(
            course_path,
            songs_dir,
            target_step_type,
            course_difficulty,
            options,
            false,
        )
    } else {
        analyze_crs_path_impl::<true, true, true, true, true, false>(
            course_path,
            songs_dir,
            target_step_type,
            course_difficulty,
            options,
            false,
        )
    }
}

fn resolve_course_song<'a, const GROUP_CACHE: bool>(
    songs_dir: &Path,
    group: Option<&'a str>,
    song: &str,
    last_group: &mut Option<(&'a str, PathBuf)>,
) -> Option<PathBuf> {
    if !GROUP_CACHE {
        return resolve_song_dir(songs_dir, group, song);
    }

    let song = song.trim();
    if song.is_empty() {
        return None;
    }
    let Some(group) = group.map(str::trim).filter(|group| !group.is_empty()) else {
        return resolve_song_dir(songs_dir, None, song);
    };
    if !last_group
        .as_ref()
        .is_some_and(|(cached, _)| *cached == group)
    {
        *last_group = Some((group, find_named_dir(songs_dir, group)?));
    }
    resolve_group_song(&last_group.as_ref()?.1, song)
}

struct GroupCatalog<'a> {
    key: &'a str,
    dir: PathBuf,
    songs: Vec<OsString>,
}

fn catalog_group(dir: &Path) -> Option<Vec<OsString>> {
    let entries = std::fs::read_dir(dir).ok()?;
    let mut songs = Vec::with_capacity(64);
    for entry in entries.flatten() {
        if assets::entry_is_dir(&entry) {
            songs.push(entry.file_name());
        }
    }
    Some(songs)
}

fn resolve_catalog_song<'a, const TRUST_CATALOG: bool>(
    songs_dir: &Path,
    group: Option<&'a str>,
    song: &str,
    catalog: &mut Option<GroupCatalog<'a>>,
) -> Option<PathBuf> {
    let song = song.trim();
    if song.is_empty() {
        return None;
    }
    let Some(group) = group.map(str::trim).filter(|group| !group.is_empty()) else {
        return resolve_song_dir(songs_dir, None, song);
    };
    if !catalog.as_ref().is_some_and(|cached| cached.key == group) {
        let dir = find_named_dir(songs_dir, group)?;
        let Some(songs) = catalog_group(&dir) else {
            return resolve_group_song(&dir, song);
        };
        *catalog = Some(GroupCatalog {
            key: group,
            dir,
            songs,
        });
    }

    let cached = catalog.as_ref()?;
    for name in &cached.songs {
        if !name.to_string_lossy().starts_with("._") && assets::name_eq_ci(name, song) {
            let path = cached.dir.join(name);
            if TRUST_CATALOG || path.is_dir() {
                return Some(path);
            }
        }
    }
    resolve_group_song(&cached.dir, song)
}

fn resolve_course_simfile<
    'a,
    const GROUP_CACHE: bool,
    const GROUP_CATALOG: bool,
    const TRUST_CATALOG: bool,
>(
    songs_dir: &Path,
    group: Option<&'a str>,
    song: &str,
    last_group: &mut Option<(&'a str, PathBuf)>,
    group_catalog: &mut Option<GroupCatalog<'a>>,
) -> Result<(PathBuf, PathBuf), String> {
    let song_dir = if GROUP_CATALOG {
        resolve_catalog_song::<TRUST_CATALOG>(songs_dir, group, song, group_catalog)
    } else {
        resolve_course_song::<GROUP_CACHE>(songs_dir, group, song, last_group)
    }
    .ok_or_else(|| format!("Song not found: {song}"))?;
    let scan = pack::scan_song_dir(&song_dir, pack::ScanOpt::default())
        .map_err(|e| format!("Failed scanning {}: {e:?}", song_dir.display()))?;
    let simfile = scan
        .map(|scan| scan.simfile)
        .ok_or_else(|| format!("No simfile in {}", song_dir.display()))?;
    Ok((song_dir, simfile))
}

fn analyze_crs_path_impl<
    const SONG_KEY_CACHE: bool,
    const GROUP_CACHE: bool,
    const GROUP_CATALOG: bool,
    const TRUST_CATALOG: bool,
    const PREALLOC_NPS: bool,
    const PREALLOC_TITLE: bool,
>(
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
    // - Keys: parsed group/song slices; hits bypass path resolution and directory scans.
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
    let mut path_cache: HashMap<PathBuf, SimfileSummary> =
        HashMap::with_capacity(if SONG_KEY_CACHE { 0 } else { cache_capacity });
    let mut song_cache: HashMap<(Option<&str>, &str), (String, SimfileSummary)> =
        HashMap::with_capacity(if SONG_KEY_CACHE { cache_capacity } else { 0 });
    // Worker-local, single-course, one-entry group path cache. The first named
    // group warms it; a change replaces it in O(1), and hits skip the Songs-root
    // scan. It drops after load-time analysis, so no miss or destruction reaches
    // gameplay; allocation benchmarks instrument its peak and worst-case cost.
    let mut last_group = None;
    // - Owner/thread safety/lifetime: worker-local, unshared, one course analysis.
    // - Capacity/warmup: one group's confirmed child directories, loaded on first use.
    // - Miss/overflow: use the exact resolver; directory size bounds stored names.
    // - Eviction/destruction: a group change replaces it; return drops it off-gameplay.
    // - Instrumentation: allocation/cycle benches track peak, churn, and lookup work.
    // - Worst-frame cost: none; load-time hits scan names without metadata rechecks.
    let mut group_catalog = None;
    let mut entries = Vec::with_capacity(entry_count);
    let mut hash_list = Vec::new();
    let mut hash_seen = CourseHashSet::default();
    let mut bpm_neutral_hash_list = Vec::new();
    let mut bpm_neutral_hash_seen = CourseHashSet::default();
    let hash_in_loop = entry_count > ADAPTIVE_HASH_MAX;
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

        let song_key = (group.as_deref(), song.as_str());
        let cache_song = cache_all || song_uses.get(&song_key).is_some_and(|&uses| uses > 1);
        let cache_len = if SONG_KEY_CACHE {
            song_cache.len()
        } else {
            path_cache.len()
        };
        let cache_has_room = cache_all || cache_len < MAX_CACHED_SIMS;
        let uncached_sim;
        let (sim, song_dir): (&SimfileSummary, String) = if cache_song && SONG_KEY_CACHE {
            match song_cache.entry(song_key) {
                Entry::Occupied(entry) => {
                    let cached = entry.into_mut();
                    let dir_name = cached.0.clone();
                    (&cached.1, dir_name)
                }
                Entry::Vacant(entry) if cache_has_room => {
                    let (dir, path) =
                        resolve_course_simfile::<GROUP_CACHE, GROUP_CATALOG, TRUST_CATALOG>(
                            &base_songs_dir,
                            song_key.0,
                            song_key.1,
                            &mut last_group,
                            &mut group_catalog,
                        )?;
                    let dir_name = song_dir_name(&dir);
                    let summary = analyze_course_song(&path, &prepared, &mut analysis_scratch)?;
                    let cached = entry.insert((dir_name.clone(), summary));
                    (&cached.1, dir_name)
                }
                Entry::Vacant(entry) => {
                    drop(entry);
                    let (dir, path) =
                        resolve_course_simfile::<GROUP_CACHE, GROUP_CATALOG, TRUST_CATALOG>(
                            &base_songs_dir,
                            song_key.0,
                            song_key.1,
                            &mut last_group,
                            &mut group_catalog,
                        )?;
                    uncached_sim = analyze_course_song(&path, &prepared, &mut analysis_scratch)?;
                    (&uncached_sim, song_dir_name(&dir))
                }
            }
        } else if cache_song {
            let (dir, path) = resolve_course_simfile::<GROUP_CACHE, GROUP_CATALOG, TRUST_CATALOG>(
                &base_songs_dir,
                song_key.0,
                song_key.1,
                &mut last_group,
                &mut group_catalog,
            )?;
            let dir_name = song_dir_name(&dir);
            match path_cache.entry(path) {
                Entry::Occupied(entry) => (entry.into_mut(), dir_name),
                Entry::Vacant(entry) if cache_has_room => {
                    let summary =
                        analyze_course_song(entry.key(), &prepared, &mut analysis_scratch)?;
                    (entry.insert(summary), dir_name)
                }
                Entry::Vacant(entry) => {
                    let path = entry.into_key();
                    uncached_sim = analyze_course_song(&path, &prepared, &mut analysis_scratch)?;
                    (&uncached_sim, dir_name)
                }
            }
        } else {
            let (dir, path) = resolve_course_simfile::<GROUP_CACHE, GROUP_CATALOG, TRUST_CATALOG>(
                &base_songs_dir,
                song_key.0,
                song_key.1,
                &mut last_group,
                &mut group_catalog,
            )?;
            uncached_sim = analyze_course_song(&path, &prepared, &mut analysis_scratch)?;
            (&uncached_sim, song_dir_name(&dir))
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

        if hash_in_loop {
            dedup_push(&mut hash_list, &mut hash_seen, &chart.short_hash);
            dedup_push(
                &mut bpm_neutral_hash_list,
                &mut bpm_neutral_hash_seen,
                &chart.bpm_neutral_hash,
            );
        }

        meters.push(parse_meter(&chart.rating_str));
        if PREALLOC_NPS && measure_nps_all.capacity() == 0 {
            // Worker-local and single-course: estimate the final measure count from
            // the first chart, then release this buffer before returning to gameplay.
            measure_nps_all.reserve_exact(chart.measure_nps_vec.len().saturating_mul(entry_count));
        }
        measure_nps_all.extend_from_slice(&chart.measure_nps_vec);

        entries.push(CourseEntrySummary {
            song: if PREALLOC_TITLE {
                course_title(&sim.title_str, &sim.subtitle_str)
            } else if sim.subtitle_str.is_empty() {
                sim.title_str.clone()
            } else {
                format!("{} {}", sim.title_str, sim.subtitle_str)
            },
            song_dir,
            step_type: chart.step_type_str.clone(),
            difficulty: chart.difficulty_str.clone(),
            rating: chart.rating_str.clone(),
            sha1: chart.short_hash.clone(),
            bpm_neutral_sha1: chart.bpm_neutral_hash.clone(),
        });
        add_course_chart(&mut total, chart);
    }

    if !hash_in_loop {
        hash_list = collect_small_course_hashes(&entries, |entry| entry.sha1.as_str());
        bpm_neutral_hash_list =
            collect_small_course_hashes(&entries, |entry| entry.bpm_neutral_sha1.as_str());
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

    let (max_nps_raw, median_nps_raw) = get_nps_stats_in_place(&mut measure_nps_all);
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
        step_type: step_type.into_owned(),
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
        CourseHashSet, CourseSong, CourseTag, Difficulty, SongSort, analyze_crs_path,
        analyze_crs_path_impl, collect_small_course_hashes, course_tag, course_tag_sequential,
        course_title, dedup_push, merge_custom_patterns, normalize_stepstype, parse_crs,
        parse_crs_with, parse_song_select, stepstype_eq,
    };
    use std::collections::HashSet;
    use std::path::{Path, PathBuf};

    #[test]
    fn course_title_preserves_parts() {
        for (title, subtitle, expected) in [
            ("Song", "", "Song"),
            ("Song", "Mix", "Song Mix"),
            ("", "Mix", " Mix"),
            ("Café 二", "夜", "Café 二 夜"),
        ] {
            assert_eq!(course_title(title, subtitle), expected);
        }
    }

    #[test]
    fn indexed_course_tags_match_sequential_dispatch() {
        for tag in [
            "COURSE",
            "coursetranslit",
            "Scripter",
            "DESCRIPTION",
            "repeat",
            "Banner",
            "BACKGROUND",
            "lives",
            "Meter",
            "SONG",
            "songselect",
            "UNKNOWN",
            "",
        ] {
            assert_eq!(
                course_tag(tag.as_bytes()),
                course_tag_sequential(tag.as_bytes()),
                "course tag dispatch changed for {tag:?}"
            );
        }
        assert_eq!(course_tag(b"SONG"), CourseTag::Song);
    }

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
        let mut actual_seen = CourseHashSet::default();

        for value in values {
            dedup_push_materialized(&mut expected, &mut expected_seen, value);
            dedup_push(&mut actual, &mut actual_seen, value);
        }

        assert_eq!(actual, expected);

        let values: Vec<_> = (0..64)
            .map(|index| format!("{:016x}", index % 48))
            .collect();
        let mut expected = Vec::new();
        let mut expected_seen = HashSet::new();
        for value in &values {
            dedup_push_materialized(&mut expected, &mut expected_seen, value);
        }
        assert_eq!(
            collect_small_course_hashes(&values, String::as_str),
            expected
        );
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
    fn songselect_entry_reserve_matches_growing_vector() {
        let mut data = b"#COURSE:Selection Reserve;\n".to_vec();
        for index in 0..64 {
            data.extend_from_slice(
                format!("#SONGSELECT:TITLE=Song {index}:GROUP=Group A,Group B;\n").as_bytes(),
            );
        }
        let growing = parse_crs_with::<false, false, true, true>(&data)
            .expect("growing selection course should parse");
        let reserved = parse_crs_with::<false, true, true, true>(&data)
            .expect("reserved selection course should parse");

        assert_eq!(reserved.entries, growing.entries);
        assert_eq!(reserved.entries.len(), 64);
    }

    #[test]
    fn songselect_tight_list_capacity_preserves_values() {
        let raw = concat!(
            "TITLE=First,Second\\, Mix:TITLE=Third:",
            "GROUP=Group A,Group B:ARTIST=Artist:GENRE=Pop,Rock:",
            "DIFFICULTY=Easy,invalid,Challenge"
        )
        .as_bytes();
        let growing = parse_song_select::<false>(raw).expect("growing selection should parse");
        let tight = parse_song_select::<true>(raw).expect("tight selection should parse");

        assert_eq!(tight, growing);
        let CourseSong::Select(select) = tight.song else {
            panic!("selection parser should produce selection criteria");
        };
        assert_eq!(select.titles, ["First", "Second, Mix", "Third"]);
        assert_eq!(select.groups, ["Group A", "Group B"]);
        assert_eq!(select.artists, ["Artist"]);
        assert_eq!(select.genres, ["Pop", "Rock"]);
        assert_eq!(
            select.difficulties,
            [Difficulty::Easy, Difficulty::Challenge]
        );
    }

    #[test]
    fn course_analysis_caches_songs_and_deduplicates_hashes() {
        let root = TempRoot::new();
        let songs_dir = root.path().join("Songs");
        for (group, songs) in [
            ("Group", &["SongA", "SongB"][..]),
            ("Other", &["SongC"][..]),
        ] {
            let group_dir = songs_dir.join(group);
            std::fs::create_dir_all(&group_dir).expect("group directory should be creatable");
            for song in songs {
                let song_dir = group_dir.join(song);
                std::fs::create_dir(&song_dir).expect("song directory should be creatable");
                std::fs::write(song_dir.join(format!("{song}.ssc")), SIMFILE)
                    .expect("simfile should be writable");
            }
        }
        let course_path = root.path().join("test.crs");
        std::fs::write(
            &course_path,
            concat!(
                "#COURSE:Optimization Test;\n",
                "#SONG:Group/SongA:Challenge:;\n",
                "#SONG:Group/SongB:Challenge:;\n",
                "#SONG:Other/RSSP Hash Perf Fixture Benchmark:Challenge:;\n",
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
        let path_cached = analyze_crs_path_impl::<false, false, false, false, true, true>(
            &course_path,
            Some(&songs_dir),
            "dance-single",
            "Medium",
            options.clone(),
            false,
        )
        .expect("path-key cache should analyze");
        let group_uncached = analyze_crs_path_impl::<true, false, false, false, true, true>(
            &course_path,
            Some(&songs_dir),
            "dance-single",
            "Medium",
            options.clone(),
            false,
        )
        .expect("uncached group lookup should analyze");
        let catalog_checked = analyze_crs_path_impl::<true, true, true, false, true, true>(
            &course_path,
            Some(&songs_dir),
            "dance-single",
            "Medium",
            options.clone(),
            false,
        )
        .expect("rechecked group catalog should analyze");
        let nps_growing = analyze_crs_path_impl::<true, true, true, true, false, true>(
            &course_path,
            Some(&songs_dir),
            "dance-single",
            "Medium",
            options.clone(),
            false,
        )
        .expect("growing NPS buffer should analyze");
        let title_formatted = analyze_crs_path_impl::<true, true, true, true, true, false>(
            &course_path,
            Some(&songs_dir),
            "dance-single",
            "Medium",
            options.clone(),
            false,
        )
        .expect("formatted course titles should analyze");
        let summary = analyze_crs_path(
            &course_path,
            Some(&songs_dir),
            "dance-single",
            "Medium",
            options,
        )
        .expect("course should analyze");

        assert_eq!(summary.course, "Optimization Test");
        assert_eq!(summary.entries.len(), 4);
        assert_eq!(summary.entries[0].song_dir, "SongA");
        assert_eq!(summary.entries[1].song_dir, "SongB");
        assert_eq!(summary.entries[2].song_dir, "SongC");
        assert_eq!(summary.entries[3].song_dir, "SongA");
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
        let mut uncached_json = Vec::new();
        let mut checked_json = Vec::new();
        let mut growing_json = Vec::new();
        let mut formatted_json = Vec::new();
        let mut actual_json = Vec::new();
        crate::report::write_course_reports(
            &path_cached,
            crate::report::OutputMode::JSON,
            &mut expected_json,
        )
        .expect("path-key cache summary should serialize");
        crate::report::write_course_reports(
            &group_uncached,
            crate::report::OutputMode::JSON,
            &mut uncached_json,
        )
        .expect("uncached group summary should serialize");
        crate::report::write_course_reports(
            &catalog_checked,
            crate::report::OutputMode::JSON,
            &mut checked_json,
        )
        .expect("rechecked catalog summary should serialize");
        crate::report::write_course_reports(
            &nps_growing,
            crate::report::OutputMode::JSON,
            &mut growing_json,
        )
        .expect("growing NPS summary should serialize");
        crate::report::write_course_reports(
            &title_formatted,
            crate::report::OutputMode::JSON,
            &mut formatted_json,
        )
        .expect("formatted course title summary should serialize");
        crate::report::write_course_reports(
            &summary,
            crate::report::OutputMode::JSON,
            &mut actual_json,
        )
        .expect("repeated-only cache summary should serialize");
        assert_eq!(actual_json, checked_json);
        assert_eq!(actual_json, growing_json);
        assert_eq!(actual_json, formatted_json);
        assert_eq!(actual_json, uncached_json);
        assert_eq!(actual_json, expected_json);
    }
}

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::analysis::{AnalysisOptions, normalize_difficulty_label};
use crate::assets;
use crate::math::{round_dp, round_sig_figs_6};
use crate::nps::get_nps_stats;
use crate::pack;
use crate::patterns::PATTERN_COUNT;
use crate::parse::{clean_tag, decode_bytes, extract_sections, unescape_tag};
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

impl CourseFile {
    #[inline(always)]
    pub const fn meter_for(&self, difficulty: Difficulty) -> Option<i32> {
        self.meters[difficulty as usize]
    }
}

const COURSE_BANNER_EXTS: [&str; 5] = ["png", "jpg", "jpeg", "bmp", "gif"];

#[derive(Debug, Clone)]
pub struct CourseEntry {
    pub song: CourseSong,
    pub steps: StepsSpec,
    pub modifiers: String,
    pub secret: bool,
    pub no_difficult: bool,
    pub gain_lives: i32,
}

#[derive(Debug, Clone)]
pub enum CourseSong {
    Fixed { group: Option<String>, song: String },
    RandomAny,
    RandomWithinGroup { group: String },
    SortPick { sort: SongSort, index: i32 },
    Unknown { raw: String },
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

#[derive(Debug, Clone)]
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
    decode_bytes(trim_ascii(bytes)).trim().to_string()
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

    let normalized = raw.replace('\\', "/");
    if let Some(group) = normalized.strip_suffix("/*").map(str::trim)
        && !group.is_empty() {
            return (CourseSong::RandomWithinGroup { group: group.to_string() }, true);
        }

    let mut parts = normalized.split('/').map(str::trim).filter(|s| !s.is_empty());
    let first = parts.next().unwrap_or_default();
    let second = parts.next();
    if parts.next().is_some() {
        return (CourseSong::Unknown { raw: raw.to_string() }, false);
    }
    let song = second.map_or_else(|| first.to_string(), str::to_string);
    let group = second.map(|_| first.to_string());
    (CourseSong::Fixed { group, song }, false)
}

fn parse_difficulty_label(label: &str) -> Option<Difficulty> {
    match label.trim().to_ascii_lowercase().as_str() {
        "beginner" => Some(Difficulty::Beginner),
        "easy" => Some(Difficulty::Easy),
        "medium" => Some(Difficulty::Medium),
        "hard" => Some(Difficulty::Hard),
        "challenge" => Some(Difficulty::Challenge),
        "edit" => Some(Difficulty::Edit),
        _ => None,
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
    let normalized = normalize_difficulty_label(raw);
    if let Some(diff) = parse_difficulty_label(&normalized) {
        return StepsSpec::Difficulty(diff);
    }
    if let Some((low, high)) = parse_meter_range(raw) {
        return StepsSpec::MeterRange { low, high };
    }
    StepsSpec::Unknown { raw: raw.to_string() }
}

fn apply_song_mods(mut secret: bool, mods_raw: &str) -> (bool, bool, i32, String) {
    let mut out_mods = Vec::new();
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
        out_mods.push(mod_str.to_string());
    }

    (secret, no_difficult, gain_lives, out_mods.join(","))
}

fn parse_song_entry(params: &[&[u8]]) -> CourseEntry {
    let song_raw = params.first().copied().unwrap_or_default();
    let diff_raw = params.get(1).copied().unwrap_or_default();
    let mods_raw = params.get(2).copied().unwrap_or_default();

    let (song, secret_default) = parse_song(&decode_trim(song_raw));
    let steps = parse_steps(&decode_trim(diff_raw));
    let mods_str = decode_trim(mods_raw);
    let (secret, no_difficult, gain_lives, modifiers) = apply_song_mods(secret_default, &mods_str);

    CourseEntry {
        song,
        steps,
        modifiers,
        secret,
        no_difficult,
        gain_lives,
    }
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
    let stem_lc = course_path.file_stem()?.to_string_lossy().to_ascii_lowercase();
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
    let mut out = CourseFile {
        name: String::new(),
        name_translit: String::new(),
        scripter: String::new(),
        description: String::new(),
        banner: String::new(),
        background: String::new(),
        repeat: false,
        lives: -1,
        meters: [None; 6],
        entries: Vec::new(),
    };

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
        let (value_end, adv) = scan_term(&s[value_start..]).unwrap_or((s.len() - value_start, s.len() - value_start));
        let value = &s[value_start..value_start + value_end];
        i += value_start + adv;

        if name_bytes.eq_ignore_ascii_case(b"COURSE") {
            out.name = decode_trim(value);
            continue;
        }
        if name_bytes.eq_ignore_ascii_case(b"COURSETRANSLIT") {
            out.name_translit = decode_trim(value);
            continue;
        }
        if name_bytes.eq_ignore_ascii_case(b"SCRIPTER") {
            out.scripter = decode_trim(value);
            continue;
        }
        if name_bytes.eq_ignore_ascii_case(b"DESCRIPTION") {
            out.description = decode_trim(value);
            continue;
        }
        if name_bytes.eq_ignore_ascii_case(b"REPEAT") {
            out.repeat = parse_repeat(&decode_trim(value));
            continue;
        }
        if name_bytes.eq_ignore_ascii_case(b"BANNER") {
            out.banner = decode_trim(value);
            continue;
        }
        if name_bytes.eq_ignore_ascii_case(b"BACKGROUND") {
            out.background = decode_trim(value);
            continue;
        }
        if name_bytes.eq_ignore_ascii_case(b"LIVES") {
            out.lives = decode_trim(value).parse::<i32>().unwrap_or(0).max(0);
            continue;
        }
        if name_bytes.eq_ignore_ascii_case(b"METER") {
            parse_course_meter_tag(value, &mut out.meters);
            continue;
        }
        if name_bytes.eq_ignore_ascii_case(b"SONG") {
            let params = split_unescaped(value, b':');
            out.entries.push(parse_song_entry(&params));
        }
    }

    if out.name.is_empty() {
        return Err("Missing #COURSE tag".to_string());
    }

    Ok(out)
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
        difficulty_str: difficulty_label(course_difficulty).to_string(),
        rating_str: meter.to_string(),
        matrix_rating: 0.0,
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
        custom_patterns: Vec::new(),
        short_hash: String::new(),
        bpm_neutral_hash: String::new(),
        elapsed: Duration::ZERO,
        measure_densities: Vec::new(),
        measure_nps_vec: Vec::new(),
        row_to_beat: Vec::new(),
        timing_segments: empty_timing_segments(),
        chart_offset_seconds: 0.0,
        chart_has_own_timing: false,
        minimized_note_data: Vec::new(),
        chart_stops: None,
        chart_speeds: None,
        chart_scrolls: None,
        chart_bpms: None,
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

    if !chart.custom_patterns.is_empty() {
        let mut merged: HashMap<&str, u32> =
            total.custom_patterns.iter().map(|c| (c.pattern.as_str(), c.count)).collect();
        for custom in &chart.custom_patterns {
            *merged.entry(&custom.pattern).or_insert(0) += custom.count;
        }
        total.custom_patterns = merged
            .into_iter()
            .map(|(pattern, count)| crate::patterns::CustomPatternSummary {
                pattern: pattern.to_string(),
                count,
            })
            .collect();
        total.custom_patterns.sort_by(|a, b| a.pattern.cmp(&b.pattern));
    }
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
        subdirs.sort_by_cached_key(|p| p.file_name().map(|s| s.to_string_lossy().to_ascii_lowercase()));

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
    groups.sort_by_cached_key(|p| p.file_name().map(|s| s.to_string_lossy().to_ascii_lowercase()));

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
        normalize_stepstype(&c.step_type_str) == step_type
            && c.difficulty_str.eq_ignore_ascii_case(difficulty_label(difficulty))
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

fn dedup_push(vec: &mut Vec<String>, seen: &mut HashSet<String>, value: &str) {
    if value.is_empty() {
        return;
    }
    if seen.insert(value.to_string()) {
        vec.push(value.to_string());
    }
}

pub fn analyze_crs_path(
    course_path: &Path,
    songs_dir: Option<&Path>,
    target_step_type: &str,
    course_difficulty: &str,
    options: AnalysisOptions,
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

    let mut sim_cache: HashMap<PathBuf, SimfileSummary> = HashMap::new();
    let mut entries = Vec::new();
    let mut hash_list = Vec::new();
    let mut hash_seen = HashSet::new();
    let mut bpm_neutral_hash_list = Vec::new();
    let mut bpm_neutral_hash_seen = HashSet::new();

    let mut meters = Vec::new();
    let mut measure_nps_all = Vec::new();

    let mut total = empty_course_chart(&step_type, course_diff, 0);

    for entry in &course.entries {
        let CourseSong::Fixed { group, song } = &entry.song else {
            return Err("Only fixed #SONG entries are supported (no RANDOM/BEST/WORST/SONGSELECT yet)".to_string());
        };
        let StepsSpec::Difficulty(base_diff) = entry.steps else {
            return Err("Only difficulty-based #SONG entries are supported (no meter ranges yet)".to_string());
        };

        let song_dir = resolve_song_dir(&base_songs_dir, group.as_deref(), song)
            .ok_or_else(|| format!("Song not found: {song}"))?;
        let scan = pack::scan_song_dir(&song_dir, pack::ScanOpt::default())
            .map_err(|e| format!("Failed scanning {}: {e:?}", song_dir.display()))?;
        let scan = scan.ok_or_else(|| format!("No simfile in {}", song_dir.display()))?;

        let sim = if let Some(cached) = sim_cache.get(&scan.simfile) {
            cached
        } else {
            let opened = simfile::open(&scan.simfile).map_err(|e| e.to_string())?;
            let summary =
                crate::analysis::analyze(&opened.data, opened.extension, &options.clone())?;
            sim_cache.insert(scan.simfile.clone(), summary);
            sim_cache
                .get(&scan.simfile)
                .ok_or_else(|| format!("Internal cache error for {}", scan.simfile.display()))?
        };

        let base_chart = select_chart(sim, &step_type, base_diff)
            .ok_or_else(|| format!("Chart not found for {} {} {}", song, step_type, difficulty_label(base_diff)))?;
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

    if let Some(meter) = course.meter_for(course_diff) {
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

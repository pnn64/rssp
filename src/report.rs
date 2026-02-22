use std::io::{self, Write};
use std::time::Duration;

use serde_json::{Map as JsonMap, Number as JsonNumber, Value as JsonValue};

use crate::bpm::{actual_bpm_range_raw, normalize_float_digits, resolve_display_bpm};
use crate::math::{round_dp, round_sig_figs_6, round_sig_figs_itg, roundtrip_bpm_itg};
use crate::patterns::{CustomPatternSummary, PatternVariant, PatternCounts};
use crate::stats::{
    ArrowStats, RADAR_CATEGORY_COUNT, StreamCounts, measure_equally_spaced, stream_sequences,
};
use crate::step_parity::TechCounts;
use crate::timing::{
    SpeedUnit, TimingFormat, TimingSegments, beat_to_note_row, format_bpm_segments_like_itg,
    normalize_scrolls_like_itg, normalize_speeds_like_itg, note_row_to_beat, steps_timing_allowed,
};

#[inline(always)]
fn compute_stream_percentages(
    total_streams: u32,
    total_breaks: u32,
    total_measures: usize,
) -> (f64, f64, f64) {
    let adj_stream_percent = if total_streams + total_breaks > 0 {
        (f64::from(total_streams) / f64::from(total_streams + total_breaks)) * 100.0
    } else {
        0.0
    };

    let stream_percent = if total_measures > 0 {
        (f64::from(total_streams) / total_measures as f64) * 100.0
    } else {
        0.0
    };

    let break_percent = 100.0 - adj_stream_percent;

    (
        round_dp(stream_percent, 2),
        round_dp(adj_stream_percent, 2),
        round_dp(break_percent, 2),
    )
}

#[derive(Clone, Copy)]
struct BoxParts {
    lr: u32,
    ud: u32,
    ld: u32,
    lu: u32,
    rd: u32,
    ru: u32,
}

#[inline(always)]
const fn compute_box_parts(patterns: &PatternCounts) -> BoxParts {
    BoxParts {
        lr: patterns[PatternVariant::BoxLR as usize],
        ud: patterns[PatternVariant::BoxUD as usize],
        ld: patterns[PatternVariant::BoxCornerLD as usize],
        lu: patterns[PatternVariant::BoxCornerLU as usize],
        rd: patterns[PatternVariant::BoxCornerRD as usize],
        ru: patterns[PatternVariant::BoxCornerRU as usize],
    }
}

#[derive(Clone, Copy)]
struct StairParts {
    left: u32,
    right: u32,
    left_inv: u32,
    right_inv: u32,
}

#[inline(always)]
const fn compute_stair_parts(
    patterns: &PatternCounts,
    left: PatternVariant,
    right: PatternVariant,
    left_inv: PatternVariant,
    right_inv: PatternVariant,
) -> StairParts {
    StairParts {
        left: patterns[left as usize],
        right: patterns[right as usize],
        left_inv: patterns[left_inv as usize],
        right_inv: patterns[right_inv as usize],
    }
}

#[derive(Clone, Copy)]
struct SweepParts {
    left: u32,
    right: u32,
    left_inv: u32,
    right_inv: u32,
}

#[inline(always)]
const fn compute_sweep_parts(
    patterns: &PatternCounts,
    left: PatternVariant,
    right: PatternVariant,
    left_inv: PatternVariant,
    right_inv: PatternVariant,
) -> SweepParts {
    SweepParts {
        left: patterns[left as usize],
        right: patterns[right as usize],
        left_inv: patterns[left_inv as usize],
        right_inv: patterns[right_inv as usize],
    }
}

#[derive(Clone, Copy)]
struct TowerParts {
    lr: u32,
    ud: u32,
    ld: u32,
    lu: u32,
    rd: u32,
    ru: u32,
}

#[inline(always)]
const fn compute_tower_parts(patterns: &PatternCounts) -> TowerParts {
    TowerParts {
        lr: patterns[PatternVariant::TowerLR as usize],
        ud: patterns[PatternVariant::TowerUD as usize],
        ld: patterns[PatternVariant::TowerCornerLD as usize],
        lu: patterns[PatternVariant::TowerCornerLU as usize],
        rd: patterns[PatternVariant::TowerCornerRD as usize],
        ru: patterns[PatternVariant::TowerCornerRU as usize],
    }
}

#[derive(Clone, Copy)]
struct TriangleParts {
    ldl: u32,
    lul: u32,
    rdr: u32,
    rur: u32,
}

#[inline(always)]
const fn compute_triangle_parts(patterns: &PatternCounts) -> TriangleParts {
    TriangleParts {
        ldl: patterns[PatternVariant::TriangleLDL as usize],
        lul: patterns[PatternVariant::TriangleLUL as usize],
        rdr: patterns[PatternVariant::TriangleRDR as usize],
        rur: patterns[PatternVariant::TriangleRUR as usize],
    }
}

#[derive(Clone, Copy)]
struct SimpleQuadParts {
    a: u32,
    b: u32,
    c: u32,
    d: u32,
}

#[inline(always)]
const fn compute_simple_quad_parts(
    patterns: &PatternCounts,
    a: PatternVariant,
    b: PatternVariant,
    c: PatternVariant,
    d: PatternVariant,
) -> SimpleQuadParts {
    SimpleQuadParts {
        a: patterns[a as usize],
        b: patterns[b as usize],
        c: patterns[c as usize],
        d: patterns[d as usize],
    }
}

// Make the struct and its fields public
#[derive(Debug)]
pub struct ChartSummary {
    pub step_type_str: String,
    pub step_artist_str: String,
    pub description_str: String,
    pub difficulty_str: String,
    pub rating_str: String,
    pub matrix_rating: f64,
    pub tech_notation_str: String,
    pub tier_bpm: f64,
    pub stats: ArrowStats,
    pub stream_counts: StreamCounts,
    pub total_measures: usize,
    pub total_streams: u32,
    /// Mines that are actually judgable (not inside warps or #FAKES).
    pub mines_nonfake: u32,
    pub sn_detailed_breakdown: String,
    pub sn_partial_breakdown: String,
    pub sn_simple_breakdown: String,
    pub detailed_breakdown: String,
    pub partial_breakdown: String,
    pub simple_breakdown: String,
    pub max_nps: f64,
    pub median_nps: f64,
    pub duration_seconds: f64,
    pub detected_patterns: PatternCounts,
    pub anchor_left: u32,
    pub anchor_down: u32,
    pub anchor_up: u32,
    pub anchor_right: u32,
    pub facing_left: u32,
    pub facing_right: u32,
    pub mono_total: u32,
    pub mono_percent: f64,
    pub candle_total: u32,
    pub candle_percent: f64,
    pub tech_counts: TechCounts,
    pub custom_patterns: Vec<CustomPatternSummary>,
    pub short_hash: String,
    pub bpm_neutral_hash: String,
    pub elapsed: Duration,
    pub measure_densities: Vec<usize>,
    pub measure_nps_vec: Vec<f64>,
    pub row_to_beat: Vec<f32>,
    pub timing_segments: TimingSegments,
    pub chart_offset_seconds: f64,
    pub chart_has_own_timing: bool,
    pub minimized_note_data: Vec<u8>,
    pub chart_stops: Option<String>,
    pub chart_speeds: Option<String>,
    pub chart_scrolls: Option<String>,
    pub chart_bpms: Option<String>,
    pub chart_delays: Option<String>,
    pub chart_warps: Option<String>,
    pub chart_fakes: Option<String>,
    pub chart_display_bpm: Option<String>,
    pub chart_time_signatures: Option<String>,
    pub chart_labels: Option<String>,
    pub chart_tickcounts: Option<String>,
    pub chart_combos: Option<String>,
    pub cached_radar_values: Option<[f32; RADAR_CATEGORY_COUNT]>,
}

// Make the struct and its fields public
#[derive(Debug)] // Add Debug for easier use in the engine
pub struct SimfileSummary {
    pub title_str: String,
    pub subtitle_str: String,
    pub artist_str: String,
    pub titletranslit_str: String,
    pub subtitletranslit_str: String,
    pub artisttranslit_str: String,
    pub offset: f64,
    pub normalized_bpms: String,
    pub normalized_stops: String,
    pub normalized_delays: String,
    pub normalized_speeds: String,
    pub normalized_scrolls: String,
    pub normalized_fakes: String,
    pub normalized_time_signatures: String,
    pub normalized_labels: String,
    pub normalized_tickcounts: String,
    pub normalized_combos: String,
    pub ssc_version: f32,
    pub timing_format: TimingFormat,
    pub banner_path: String,
    pub background_path: String,
    pub cdtitle_path: String,
    pub jacket_path: String,
    pub music_path: String,
    pub display_bpm_str: String,
    pub sample_start: f64,
    pub sample_length: f64,
    pub min_bpm: f64,
    pub max_bpm: f64,
    pub normalized_warps: String,
    pub median_bpm: f64,
    pub average_bpm: f64,
    pub total_length: i32,
    pub pattern_counts_enabled: bool,
    pub tech_counts_enabled: bool,
    pub charts: Vec<ChartSummary>,
    pub total_elapsed: Duration,
}

#[derive(Debug)]
pub struct CourseEntrySummary {
    pub song: String,
    pub song_dir: String,
    pub step_type: String,
    pub difficulty: String,
    pub rating: String,
    pub sha1: String,
    pub bpm_neutral_sha1: String,
}

#[derive(Debug)]
pub struct CourseSummary {
    pub course: String,
    pub course_difficulty: String,
    pub step_type: String,
    pub total_length: i32,
    pub entries: Vec<CourseEntrySummary>,
    pub chart: ChartSummary,
    pub sha1_hashes: Vec<String>,
    pub bpm_neutral_sha1_hashes: Vec<String>,
    pub pattern_counts_enabled: bool,
    pub tech_counts_enabled: bool,
    pub total_elapsed: Duration,
}

#[derive(Debug, Clone)]
pub struct TimingSnapshot {
    pub beat0_offset_seconds: f64,
    pub beat0_group_offset_seconds: f64,
    pub bpms: Vec<(f64, f64)>,
    pub bpms_formatted: String,
    pub bpm_min_raw: f64,
    pub bpm_max_raw: f64,
    pub stops: Vec<(f64, f64)>,
    pub delays: Vec<(f64, f64)>,
    pub time_signatures: Vec<(f64, i32, i32)>,
    pub warps: Vec<(f64, f64)>,
    pub labels: Vec<(f64, String)>,
    pub tickcounts: Vec<(f64, i32)>,
    pub combos: Vec<(f64, i32, i32)>,
    pub speeds: Vec<(f64, f64, f64, i32)>,
    pub scrolls: Vec<(f64, f64)>,
    pub fakes: Vec<(f64, f64)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutputMode {
    Full,
    Pretty,
    JSON,
    CSV,
}

pub fn write_reports<W: Write>(
    simfile: &SimfileSummary,
    mode: OutputMode,
    writer: &mut W,
) -> io::Result<()> {
    match mode {
        OutputMode::Full => write_full_all(writer, simfile),
        OutputMode::Pretty => write_pretty_all(writer, simfile),
        OutputMode::JSON => write_json_all(simfile, writer),
        OutputMode::CSV => write_csv_all(writer, simfile),
    }
}

pub fn write_course_reports<W: Write>(
    course: &CourseSummary,
    mode: OutputMode,
    writer: &mut W,
) -> io::Result<()> {
    match mode {
        OutputMode::Full => write_full_course(writer, course),
        OutputMode::Pretty => write_pretty_course(writer, course),
        OutputMode::JSON => write_json_course(course, writer),
        OutputMode::CSV => write_csv_course(writer, course),
    }
}

#[inline(always)]
#[must_use] 
pub fn format_json_float(value: f64) -> String {
    format!("{value:.2}")
}

fn format_duration(seconds: i32) -> String {
    let minutes = seconds / 60;
    let seconds = seconds % 60;
    format!("{minutes}m {seconds:02}s")
}

fn dummy_simfile_for_course(course: &CourseSummary) -> SimfileSummary {
    SimfileSummary {
        title_str: course.course.clone(),
        subtitle_str: String::new(),
        artist_str: String::new(),
        titletranslit_str: String::new(),
        subtitletranslit_str: String::new(),
        artisttranslit_str: String::new(),
        offset: 0.0,
        normalized_bpms: String::new(),
        normalized_stops: String::new(),
        normalized_delays: String::new(),
        normalized_speeds: String::new(),
        normalized_scrolls: String::new(),
        normalized_fakes: String::new(),
        normalized_time_signatures: String::new(),
        normalized_labels: String::new(),
        normalized_tickcounts: String::new(),
        normalized_combos: String::new(),
        ssc_version: f32::NAN,
        timing_format: TimingFormat::Sm,
        banner_path: String::new(),
        background_path: String::new(),
        cdtitle_path: String::new(),
        jacket_path: String::new(),
        music_path: String::new(),
        display_bpm_str: String::new(),
        sample_start: 0.0,
        sample_length: 0.0,
        min_bpm: 0.0,
        max_bpm: 0.0,
        normalized_warps: String::new(),
        median_bpm: 0.0,
        average_bpm: 0.0,
        total_length: course.total_length,
        pattern_counts_enabled: course.pattern_counts_enabled,
        tech_counts_enabled: course.tech_counts_enabled,
        charts: Vec::new(),
        total_elapsed: course.total_elapsed,
    }
}

fn write_pretty_course<W: Write>(writer: &mut W, course: &CourseSummary) -> io::Result<()> {
    writeln!(writer, "--- Course Details ---")?;
    writeln!(writer, "Course: {}", course.course)?;
    writeln!(writer, "Difficulty: {}", course.course_difficulty)?;
    writeln!(writer, "StepsType: {}", course.step_type)?;
    writeln!(writer, "Length: {}", format_duration(course.total_length))?;
    writeln!(writer, "Entries: {}", course.entries.len())?;

    if !course.entries.is_empty() {
        writeln!(writer, "\n--- Entries ---")?;
        for (i, entry) in course.entries.iter().enumerate() {
            writeln!(
                writer,
                "{:2}. {} ({}) {} {}",
                i + 1,
                entry.song,
                entry.song_dir,
                entry.difficulty,
                entry.rating
            )?;
        }
    }

    let dummy = dummy_simfile_for_course(course);
    write_pretty_chart(writer, &course.chart, &dummy)?;
    Ok(())
}

fn write_full_course<W: Write>(writer: &mut W, course: &CourseSummary) -> io::Result<()> {
    writeln!(writer, "--- Course Details ---")?;
    writeln!(writer, "Course: {}", course.course)?;
    writeln!(writer, "Difficulty: {}", course.course_difficulty)?;
    writeln!(writer, "StepsType: {}", course.step_type)?;
    writeln!(writer, "Length: {}", format_duration(course.total_length))?;
    writeln!(writer, "Entries: {}", course.entries.len())?;

    if !course.entries.is_empty() {
        writeln!(writer, "\n--- Entries ---")?;
        for (i, entry) in course.entries.iter().enumerate() {
            writeln!(
                writer,
                "{:2}. {} ({}) {} {}",
                i + 1,
                entry.song,
                entry.song_dir,
                entry.difficulty,
                entry.rating
            )?;
            writeln!(writer, "    sha1: {}", entry.sha1)?;
            writeln!(writer, "    bpm_neutral_sha1: {}", entry.bpm_neutral_sha1)?;
        }
    }

    let dummy = dummy_simfile_for_course(course);
    write_full_chart(writer, &course.chart, &dummy)?;
    writeln!(writer, "\nElapsed Time: {:?}", course.total_elapsed)?;
    Ok(())
}

fn write_json_course<W: Write>(course: &CourseSummary, writer: &mut W) -> io::Result<()> {
    let mut root_obj = JsonMap::new();
    root_obj.insert("course".to_string(), JsonValue::from(course.course.clone()));
    root_obj.insert(
        "course_difficulty".to_string(),
        JsonValue::from(course.course_difficulty.clone()),
    );
    root_obj.insert(
        "step_type".to_string(),
        JsonValue::from(course.step_type.clone()),
    );
    root_obj.insert(
        "length".to_string(),
        JsonValue::from(course.total_length.to_string()),
    );
    root_obj.insert(
        "sha1_hashes".to_string(),
        JsonValue::from(course.sha1_hashes.clone()),
    );
    root_obj.insert(
        "bpm_neutral_sha1_hashes".to_string(),
        JsonValue::from(course.bpm_neutral_sha1_hashes.clone()),
    );

    let entries: Vec<JsonValue> = course
        .entries
        .iter()
        .map(|entry| {
            let mut obj = JsonMap::new();
            obj.insert("song".to_string(), JsonValue::from(entry.song.clone()));
            obj.insert(
                "song_dir".to_string(),
                JsonValue::from(entry.song_dir.clone()),
            );
            obj.insert(
                "step_type".to_string(),
                JsonValue::from(entry.step_type.clone()),
            );
            obj.insert(
                "difficulty".to_string(),
                JsonValue::from(entry.difficulty.clone()),
            );
            obj.insert("rating".to_string(), JsonValue::from(entry.rating.clone()));
            obj.insert("sha1".to_string(), JsonValue::from(entry.sha1.clone()));
            obj.insert(
                "bpm_neutral_sha1".to_string(),
                JsonValue::from(entry.bpm_neutral_sha1.clone()),
            );
            JsonValue::Object(obj)
        })
        .collect();
    root_obj.insert("entries".to_string(), JsonValue::from(entries));

    let dummy = dummy_simfile_for_course(course);
    let mut chart_obj = JsonMap::new();
    chart_obj.insert("chart_info".to_string(), json_chart_info(&course.chart));
    chart_obj.insert("arrow_stats".to_string(), json_arrow_stats(&course.chart));
    chart_obj.insert("gimmicks".to_string(), json_gimmicks(&course.chart, &dummy));
    chart_obj.insert("timing".to_string(), json_timing(&course.chart, &dummy));
    chart_obj.insert("stream_info".to_string(), json_stream_info(&course.chart));
    chart_obj.insert("nps".to_string(), json_nps(&course.chart));
    chart_obj.insert("breakdown".to_string(), json_sn_breakdown(&course.chart));
    chart_obj.insert(
        "stream_breakdown".to_string(),
        json_stream_breakdown(&course.chart),
    );
    if course.pattern_counts_enabled {
        chart_obj.insert(
            "mono_candle_stats".to_string(),
            json_mono_candle_stats(&course.chart),
        );
        chart_obj.insert(
            "pattern_counts".to_string(),
            json_pattern_counts(&course.chart),
        );
    }
    if course.tech_counts_enabled {
        chart_obj.insert("tech_counts".to_string(), json_tech_counts(&course.chart));
    }
    root_obj.insert("chart".to_string(), JsonValue::Object(chart_obj));

    let root = JsonValue::Object(root_obj);

    write_json_value_with_key(writer, None, &root, 0)?;
    writeln!(writer)?;
    Ok(())
}

fn write_csv_course<W: Write>(writer: &mut W, course: &CourseSummary) -> io::Result<()> {
    let header = [
        "Course",
        "Difficulty",
        "StepsType",
        "Length",
        "Entries",
        "sha1_hashes",
        "bpm_neutral_sha1_hashes",
        "total_arrows",
        "total_steps",
        "jumps",
        "hands",
        "holds",
        "rolls",
        "mines",
        "lifts",
        "fakes",
        "total_streams",
        "total_breaks",
        "max_nps",
        "median_nps",
    ];
    writeln!(writer, "{}", header.join(","))?;

    let chart = &course.chart;
    let hashes = course.sha1_hashes.join("|");
    let bpm_hashes = course.bpm_neutral_sha1_hashes.join("|");
    writeln!(
        writer,
        "{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{:.6},{:.2}",
        course.course,
        course.course_difficulty,
        course.step_type,
        format_duration(course.total_length),
        course.entries.len(),
        hashes,
        bpm_hashes,
        chart.stats.total_arrows,
        chart.stats.total_steps,
        chart.stats.jumps,
        chart.stats.hands,
        chart.stats.holds,
        chart.stats.rolls,
        chart.stats.mines,
        chart.stats.lifts,
        chart.stats.fakes,
        chart.total_streams,
        chart.stream_counts.total_breaks,
        chart.max_nps,
        chart.median_nps
    )?;
    Ok(())
}

const fn count(counts: &PatternCounts, variant: PatternVariant) -> u32 {
    counts[variant as usize]
}

fn chart_or_global<'a>(
    allow_chart: bool,
    chart_has_own_timing: bool,
    chart_value: &'a Option<String>,
    global_value: &'a str,
) -> Option<&'a str> {
    if allow_chart && chart_has_own_timing {
        return chart_value.as_deref().filter(|s| !s.is_empty());
    }

    if allow_chart
        && let Some(s) = chart_value
            && !s.is_empty() {
                return Some(s.as_str());
            }

    if global_value.is_empty() {
        None
    } else {
        Some(global_value)
    }
}

#[inline(always)]
fn segment_index_at_row<T>(segments: &[(f64, T)], row: i32) -> usize {
    let pos = segments.partition_point(|(beat, _)| beat_to_note_row(*beat) <= row);
    if pos == 0 { 0 } else { pos - 1 }
}

fn add_indefinite_segment<T: PartialEq>(segments: &mut Vec<(f64, T)>, beat: f64, value: T) {
    let row = beat_to_note_row(beat);
    let beat = note_row_to_beat(row);
    if segments.is_empty() {
        segments.push((beat, value));
        return;
    }

    let idx = segment_index_at_row(segments, row);
    let b_on_same_row = beat_to_note_row(segments[idx].0) == row;
    let prev_idx = if b_on_same_row && idx > 0 {
        idx - 1
    } else {
        idx
    };

    if idx + 1 < segments.len() {
        let next_idx = idx + 1;
        if segments[next_idx].1 == value {
            if segments[prev_idx].1 == value {
                segments.remove(next_idx);
                if prev_idx != idx {
                    segments.remove(idx);
                }
                return;
            }
            segments[next_idx].0 = beat;
            if prev_idx != idx {
                segments.remove(idx);
            }
            return;
        }
        if segments[prev_idx].1 == value {
            if prev_idx != idx {
                segments.remove(idx);
            }
            return;
        }
    } else if segments[prev_idx].1 == value {
        if prev_idx != idx {
            segments.remove(idx);
        }
        return;
    }

    if b_on_same_row && segments[idx].1 == value {
        return;
    }

    if b_on_same_row {
        segments[idx] = (beat, value);
    } else {
        let insert_pos = segments.partition_point(|(b, _)| beat_to_note_row(*b) <= row);
        segments.insert(insert_pos, (beat, value));
    }
}

fn tidy_indefinite_segments<T: PartialEq>(segments: Vec<(f64, T)>) -> Vec<(f64, T)> {
    let mut out = Vec::with_capacity(segments.len());
    for (beat, value) in segments {
        add_indefinite_segment(&mut out, beat, value);
    }
    out
}

fn parse_time_signatures(opt: Option<&str>) -> Vec<(f64, i32, i32)> {
    let Some(s) = opt else {
        return vec![(0.0, 4, 4)];
    };

    let mut raw = Vec::new();
    for segment in s.split(',') {
        let segment = segment.trim();
        if segment.is_empty() {
            continue;
        }
        let mut parts = segment.split('=');
        let Some(beat_str) = parts.next() else {
            continue;
        };
        let Some(num_str) = parts.next() else {
            continue;
        };
        let Some(den_str) = parts.next() else {
            continue;
        };
        let Ok(beat) = beat_str.trim().parse::<f64>() else {
            continue;
        };
        let Ok(num) = num_str.trim().parse::<i32>() else {
            continue;
        };
        let Ok(den) = den_str.trim().parse::<i32>() else {
            continue;
        };
        raw.push((beat, (num, den)));
    }

    if raw.is_empty() {
        return vec![(0.0, 4, 4)];
    }

    let needs_default = raw
        .first()
        .is_some_and(|(beat, _)| beat_to_note_row(*beat) > 0);
    if needs_default {
        raw.insert(0, (0.0, (4, 4)));
    }

    tidy_indefinite_segments(raw)
        .into_iter()
        .map(|(beat, (num, den))| (beat, num, den))
        .collect()
}

fn parse_tickcounts(opt: Option<&str>) -> Vec<(f64, i32)> {
    let Some(s) = opt else {
        return vec![(0.0, 4)];
    };

    let mut raw = Vec::new();
    for segment in s.split(',') {
        let segment = segment.trim();
        if segment.is_empty() {
            continue;
        }
        let mut parts = segment.split('=');
        let Some(beat_str) = parts.next() else {
            continue;
        };
        let Some(count_str) = parts.next() else {
            continue;
        };
        let Ok(beat) = beat_str.trim().parse::<f64>() else {
            continue;
        };
        let Ok(count) = count_str.trim().parse::<i32>() else {
            continue;
        };
        raw.push((beat, count));
    }

    if raw.is_empty() {
        return vec![(0.0, 4)];
    }

    tidy_indefinite_segments(raw)
}

fn parse_combos(opt: Option<&str>) -> Vec<(f64, i32, i32)> {
    let Some(s) = opt else {
        return vec![(0.0, 1, 1)];
    };

    let mut raw = Vec::new();
    for segment in s.split(',') {
        let segment = segment.trim();
        if segment.is_empty() {
            continue;
        }
        let mut parts = segment.split('=');
        let Some(beat_str) = parts.next() else {
            continue;
        };
        let Some(combo_str) = parts.next() else {
            continue;
        };
        let Some(miss_str) = parts.next() else {
            continue;
        };
        let Ok(beat) = beat_str.trim().parse::<f64>() else {
            continue;
        };
        let Ok(combo) = combo_str.trim().parse::<i32>() else {
            continue;
        };
        let Ok(miss) = miss_str.trim().parse::<i32>() else {
            continue;
        };
        raw.push((beat, (combo, miss)));
    }

    if raw.is_empty() {
        return vec![(0.0, 1, 1)];
    }

    tidy_indefinite_segments(raw)
        .into_iter()
        .map(|(beat, (combo, miss))| (beat, combo, miss))
        .collect()
}

#[must_use] 
pub fn build_timing_snapshot(chart: &ChartSummary, simfile: &SimfileSummary) -> TimingSnapshot {
    let allow_steps_timing = steps_timing_allowed(simfile.ssc_version, simfile.timing_format);
    let timing = &chart.timing_segments;
    let finalize = |value: f64| round_sig_figs_6(round_sig_figs_itg(value));
    let bpms_raw: Vec<(f64, f64)> = timing
        .bpms
        .iter()
        .map(|(beat, bpm)| (f64::from(*beat), roundtrip_bpm_itg(f64::from(*bpm))))
        .collect();
    let bpms_formatted = format_bpm_segments_like_itg(&bpms_raw);
    let (bpm_min_raw, bpm_max_raw) = actual_bpm_range_raw(&bpms_raw);
    // Match itgmania-reference-harness default float precision (6 significant digits).
    let bpms: Vec<(f64, f64)> = bpms_raw
        .iter()
        .map(|(beat, bpm)| (finalize(*beat), finalize(*bpm)))
        .collect();
    let stops = timing
        .stops
        .iter()
        .map(|(beat, duration)| (finalize(f64::from(*beat)), finalize(f64::from(*duration))))
        .collect();
    let delays = timing
        .delays
        .iter()
        .map(|(beat, duration)| (finalize(f64::from(*beat)), finalize(f64::from(*duration))))
        .collect();
    let warps = timing
        .warps
        .iter()
        .map(|(beat, length)| (finalize(f64::from(*beat)), finalize(f64::from(*length))))
        .collect();
    let speeds = timing
        .speeds
        .iter()
        .map(|(beat, ratio, delay, unit)| {
            let unit = i32::from(*unit == SpeedUnit::Seconds);
            (f64::from(*beat), f64::from(*ratio), f64::from(*delay), unit)
        })
        .collect();
    let speeds = normalize_speeds_like_itg(speeds);
    let speeds: Vec<(f64, f64, f64, i32)> = speeds
        .into_iter()
        .map(|(beat, ratio, delay, unit)| (finalize(beat), finalize(ratio), finalize(delay), unit))
        .collect();
    let scrolls = timing
        .scrolls
        .iter()
        .map(|(beat, ratio)| (f64::from(*beat), f64::from(*ratio)))
        .collect();
    let scrolls = normalize_scrolls_like_itg(scrolls);
    let scrolls: Vec<(f64, f64)> = scrolls
        .into_iter()
        .map(|(beat, ratio)| (finalize(beat), finalize(ratio)))
        .collect();
    let fakes = timing
        .fakes
        .iter()
        .map(|(beat, length)| (finalize(f64::from(*beat)), finalize(f64::from(*length))))
        .collect();

    let time_signatures: Vec<(f64, i32, i32)> = parse_time_signatures(chart_or_global(
        allow_steps_timing,
        chart.chart_has_own_timing,
        &chart.chart_time_signatures,
        &simfile.normalized_time_signatures,
    ))
    .into_iter()
    .map(|(beat, numerator, denominator)| (finalize(beat), numerator, denominator))
    .collect();
    let labels: Vec<(f64, String)> = parse_labels(chart_or_global(
        allow_steps_timing,
        chart.chart_has_own_timing,
        &chart.chart_labels,
        &simfile.normalized_labels,
    ))
    .into_iter()
    .map(|(beat, label)| (finalize(beat), label))
    .collect();
    let tickcounts: Vec<(f64, i32)> = parse_tickcounts(chart_or_global(
        allow_steps_timing,
        chart.chart_has_own_timing,
        &chart.chart_tickcounts,
        &simfile.normalized_tickcounts,
    ))
    .into_iter()
    .map(|(beat, ticks)| (finalize(beat), ticks))
    .collect();
    let combos: Vec<(f64, i32, i32)> = parse_combos(chart_or_global(
        allow_steps_timing,
        chart.chart_has_own_timing,
        &chart.chart_combos,
        &simfile.normalized_combos,
    ))
    .into_iter()
    .map(|(beat, combo, miss)| (finalize(beat), combo, miss))
    .collect();

    TimingSnapshot {
        beat0_offset_seconds: finalize(
            chart.chart_offset_seconds + f64::from(timing.beat0_offset_adjust),
        ),
        beat0_group_offset_seconds: 0.0,
        bpms,
        bpms_formatted,
        bpm_min_raw,
        bpm_max_raw,
        stops,
        delays,
        time_signatures,
        warps,
        labels,
        tickcounts,
        combos,
        speeds,
        scrolls,
        fakes,
    }
}

fn parse_labels(opt: Option<&str>) -> Vec<(f64, String)> {
    let Some(s) = opt else {
        return vec![(0.0, "Song Start".to_string())];
    };

    let mut raw = Vec::new();
    for segment in s.split(',') {
        let segment = segment.trim();
        if segment.is_empty() {
            continue;
        }
        let Some((beat_str, label_raw)) = segment.split_once('=') else {
            continue;
        };
        let Ok(beat) = beat_str.trim().parse::<f64>() else {
            continue;
        };
        let label = label_raw.trim().to_string();
        if label.is_empty() {
            continue;
        }
        raw.push((beat, label));
    }

    if raw.is_empty() {
        return vec![(0.0, "Song Start".to_string())];
    }

    tidy_indefinite_segments(raw)
}

fn count_timing_segments_from_str(s: &str) -> u32 {
    s.split(',').filter(|part| !part.trim().is_empty()).count() as u32
}

fn count_timing_segments(opt: Option<&str>) -> u32 {
    opt.map_or(0, count_timing_segments_from_str)
}

fn count_gimmick_speed_segments(opt: Option<&str>) -> u32 {
    let Some(s) = opt else {
        return 0;
    };

    s.split(',')
        .filter_map(|segment| {
            let segment = segment.trim();
            if segment.is_empty() {
                return None;
            }

            let mut parts = segment.split('=');
            let _beat = parts.next();
            let factor_str = parts.next()?;
            let factor = factor_str.trim().parse::<f64>().ok()?;

            if (factor - 1.0).abs() > 1e-6 {
                Some(())
            } else {
                None
            }
        })
        .count() as u32
}

fn count_gimmick_scroll_segments(opt: Option<&str>) -> u32 {
    let Some(s) = opt else {
        return 0;
    };

    s.split(',')
        .filter_map(|segment| {
            let segment = segment.trim();
            if segment.is_empty() {
                return None;
            }

            let mut parts = segment.split('=');
            let _beat = parts.next();
            let value_str = parts.next()?;
            let value = value_str.trim().parse::<f64>().ok()?;

            if (value - 1.0).abs() > 1e-6 {
                Some(())
            } else {
                None
            }
        })
        .count() as u32
}

#[inline]
const fn chart_mine_fake_counts(chart: &ChartSummary) -> (u32, u32) {
    (chart.stats.mines, chart.stats.fakes)
}

fn write_gimmicks<W: Write>(
    writer: &mut W,
    chart: &ChartSummary,
    simfile: &SimfileSummary,
) -> io::Result<()> {
    let has_lifts = chart.stats.lifts > 0;
    let (_, fakes) = chart_mine_fake_counts(chart);
    let has_fakes = fakes > 0;
    let allow_steps_timing = steps_timing_allowed(simfile.ssc_version, simfile.timing_format);
    let stops = chart_or_global(
        allow_steps_timing,
        chart.chart_has_own_timing,
        &chart.chart_stops,
        &simfile.normalized_stops,
    );
    let delays = chart_or_global(
        allow_steps_timing,
        chart.chart_has_own_timing,
        &chart.chart_delays,
        &simfile.normalized_delays,
    );
    let warps = chart_or_global(
        allow_steps_timing,
        chart.chart_has_own_timing,
        &chart.chart_warps,
        &simfile.normalized_warps,
    );
    let speeds = chart_or_global(
        allow_steps_timing,
        chart.chart_has_own_timing,
        &chart.chart_speeds,
        &simfile.normalized_speeds,
    );
    let scrolls = chart_or_global(
        allow_steps_timing,
        chart.chart_has_own_timing,
        &chart.chart_scrolls,
        &simfile.normalized_scrolls,
    );

    let stop_count = count_timing_segments(stops);
    let delay_count = count_timing_segments(delays);
    let warp_count = count_timing_segments(warps);
    let speed_count = count_gimmick_speed_segments(speeds);
    let scroll_count = count_gimmick_scroll_segments(scrolls);

    if !has_lifts
        && !has_fakes
        && stop_count == 0
        && delay_count == 0
        && warp_count == 0
        && speed_count == 0
        && scroll_count == 0
    {
        return Ok(());
    }

    writeln!(writer, "\n--- Gimmicks ---")?;
    if has_lifts {
        writeln!(writer, "Lifts: {}", chart.stats.lifts)?;
    }
    if has_fakes {
        writeln!(writer, "Fakes: {fakes}")?;
    }
    if stop_count > 0 {
        writeln!(writer, "Stops/Freezes: {stop_count}")?;
    }
    if speed_count > 0 {
        writeln!(writer, "Speeds: {speed_count}")?;
    }
    if scroll_count > 0 {
        writeln!(writer, "Scrolls: {scroll_count}")?;
    }
    if delay_count > 0 {
        writeln!(writer, "Delays: {delay_count}")?;
    }
    if warp_count > 0 {
        writeln!(writer, "Warps: {warp_count}")?;
    }

    Ok(())
}

fn write_pretty_all<W: Write>(writer: &mut W, simfile: &SimfileSummary) -> io::Result<()> {
    writeln!(writer, "--- Song Details ---")?;
    writeln!(
        writer,
        "Title: {}{} by {}",
        simfile.title_str,
        if simfile.subtitle_str.is_empty() {
            String::new()
        } else {
            format!(" {}", simfile.subtitle_str)
        },
        simfile.artist_str
    )?;
    writeln!(writer, "Length: {}", format_duration(simfile.total_length))?;
    if (simfile.min_bpm - simfile.max_bpm).abs() < f64::EPSILON {
        writeln!(writer, "BPM: {:.0}", simfile.min_bpm)?;
    } else {
        writeln!(writer, "BPM: {:.0}-{:.0}", simfile.min_bpm, simfile.max_bpm)?;
        writeln!(writer, "Median BPM: {:.0}", simfile.median_bpm)?;
        writeln!(writer, "Average BPM: {:.0}", simfile.average_bpm)?;
    }

    for chart in &simfile.charts {
        write_pretty_chart(writer, chart, simfile)?;
    }

    Ok(())
}

fn write_pretty_chart<W: Write>(
    writer: &mut W,
    chart: &ChartSummary,
    simfile: &SimfileSummary,
) -> io::Result<()> {
    let header = format!(
        "{} {} : {}",
        chart.difficulty_str, chart.rating_str, chart.step_artist_str
    );
    writeln!(writer, "\n{header}")?;
    writeln!(writer, "{}", "-".repeat(header.len()))?;

    if (chart.median_nps - chart.max_nps).abs() < f64::EPSILON {
        writeln!(writer, "NPS: {:.2} Median/Peak", chart.median_nps)?;
    } else {
        writeln!(
            writer,
            "NPS: {:.2} Median, {:.2} Peak",
            chart.median_nps, chart.max_nps
        )?;
    }

    let total_stream = chart.total_streams;
    let total_break = chart.stream_counts.total_breaks;
    let total_measures = chart.total_measures;
    let (stream_percent, adjusted_stream_percent, break_percent) =
        compute_stream_percentages(total_stream, total_break, total_measures);

    writeln!(
        writer,
        "Total Stream: {total_stream} ({stream_percent:.2}%/{adjusted_stream_percent:.2}% Adj.)"
    )?;
    writeln!(
        writer,
        "Total Break: {total_break} ({break_percent:.2}%)"
    )?;

    writeln!(writer, "\n--- Chart Info ---")?;
    writeln!(
        writer,
        "Steps: {} ({} arrows)",
        chart.stats.total_steps, chart.stats.total_arrows
    )?;
    writeln!(writer, "Jumps: {}", chart.stats.jumps)?;
    writeln!(writer, "Hands: {}", chart.stats.hands)?;
    writeln!(writer, "Holds: {}", chart.stats.holds)?;
    writeln!(writer, "Rolls: {}", chart.stats.rolls)?;
    let (mines_judgable, _) = chart_mine_fake_counts(chart);
    writeln!(writer, "Mines: {mines_judgable}")?;

    write_gimmicks(writer, chart, simfile)?;
    if simfile.pattern_counts_enabled {
        writeln!(writer, "\n--- Pattern Analysis ---")?;
        let candle_left = chart.detected_patterns[PatternVariant::CandleLeft as usize];
        let candle_right = chart.detected_patterns[PatternVariant::CandleRight as usize];
        writeln!(
            writer,
            "Candles: {} ({} left, {} right)",
            candle_left + candle_right,
            candle_left,
            candle_right
        )?;
        writeln!(writer, "Candle%: {:.2}%", chart.candle_percent)?;
        writeln!(
            writer,
            "Mono: {} ({} left-facing, {} right-facing)",
            chart.mono_total, chart.facing_left, chart.facing_right
        )?;
        writeln!(writer, "Mono%: {:.2}%", chart.mono_percent)?;

        let box_parts = compute_box_parts(&chart.detected_patterns);
        let box_corners = box_parts.ld + box_parts.lu + box_parts.rd + box_parts.ru;
        writeln!(
            writer,
            "Boxes: {} ({} LRLR, {} UDUD, {} corner)",
            box_parts.lr + box_parts.ud + box_corners,
            box_parts.lr,
            box_parts.ud,
            box_corners
        )?;

        let anchor_total =
            chart.anchor_left + chart.anchor_down + chart.anchor_up + chart.anchor_right;
        writeln!(
            writer,
            "Anchors: {} ({} left, {} down, {} up, {} right)",
            anchor_total, chart.anchor_left, chart.anchor_down, chart.anchor_up, chart.anchor_right
        )?;
    }

    if simfile.tech_counts_enabled {
        writeln!(writer, "\n--- Step Parity Analysis ---")?;
        writeln!(writer, "Crossovers: {}", chart.tech_counts.crossovers)?;
        writeln!(
            writer,
            "Footswitches: {} ({} up, {} down)",
            chart.tech_counts.footswitches,
            chart.tech_counts.up_footswitches,
            chart.tech_counts.down_footswitches
        )?;
        writeln!(writer, "Sideswitches: {}", chart.tech_counts.sideswitches)?;
        writeln!(writer, "Jacks: {}", chart.tech_counts.jacks)?;
        writeln!(writer, "Brackets: {}", chart.tech_counts.brackets)?;
        writeln!(writer, "Doublesteps: {}", chart.tech_counts.doublesteps)?;
    }

    if simfile.pattern_counts_enabled && !chart.custom_patterns.is_empty() {
        writeln!(writer, "\n--- Custom Patterns ---")?;
        for cp in &chart.custom_patterns {
            writeln!(writer, "{}: {}", cp.pattern, cp.count)?;
        }
    }

    if !chart.detailed_breakdown.is_empty() {
        writeln!(writer, "\n--- Detailed Breakdown ---")?;
        writeln!(writer, "{}", chart.detailed_breakdown)?;
        writeln!(writer, "--- Partial Breakdown ---")?;
        writeln!(writer, "{}", chart.partial_breakdown)?;
        writeln!(writer, "--- Simple Breakdown ---")?;
        writeln!(writer, "{}", chart.simple_breakdown)?;
    }

    if !chart.sn_detailed_breakdown.is_empty() {
        writeln!(writer, "\n--- SN Detailed Breakdown ---")?;
        writeln!(writer, "{}", chart.sn_detailed_breakdown)?;
        writeln!(writer, "--- SN Partially Simplified ---")?;
        writeln!(writer, "{}", chart.sn_partial_breakdown)?;
        writeln!(writer, "--- SN Simplified Breakdown ---")?;
        writeln!(writer, "{}", chart.sn_simple_breakdown)?;
    }

    Ok(())
}

fn write_full_all<W: Write>(writer: &mut W, simfile: &SimfileSummary) -> io::Result<()> {
    writeln!(writer, "--- Song Details ---")?;
    writeln!(writer, "Title: {}", simfile.title_str)?;
    if !simfile.subtitle_str.is_empty() {
        writeln!(writer, "Subtitle: {}", simfile.subtitle_str)?;
    }
    writeln!(writer, "Artist: {}", simfile.artist_str)?;
    if !simfile.titletranslit_str.is_empty() {
        writeln!(writer, "Title trans: {}", simfile.titletranslit_str)?;
    }
    if !simfile.subtitletranslit_str.is_empty() {
        writeln!(writer, "Subtitle trans: {}", simfile.subtitletranslit_str)?;
    }
    if !simfile.artisttranslit_str.is_empty() {
        writeln!(writer, "Artist trans: {}", simfile.artisttranslit_str)?;
    }

    writeln!(writer, "Length: {}", format_duration(simfile.total_length))?;
    if (simfile.min_bpm - simfile.max_bpm).abs() < f64::EPSILON {
        writeln!(writer, "BPM: {:.0}", simfile.min_bpm)?;
    } else {
        writeln!(writer, "BPM: {:.0}-{:.0}", simfile.min_bpm, simfile.max_bpm)?;
    }
    writeln!(writer, "Average BPM: {:.2}", simfile.average_bpm)?;
    writeln!(writer, "Median BPM: {:.2}", simfile.median_bpm)?;
    writeln!(writer, "BPM Data: {}", simfile.normalized_bpms)?;
    writeln!(writer, "Offset: {:.3}", simfile.offset)?;

    for chart in &simfile.charts {
        write_full_chart(writer, chart, simfile)?;
    }
    writeln!(writer, "\nElapsed Time: {:?}", simfile.total_elapsed)?;

    Ok(())
}

fn write_full_chart<W: Write>(
    writer: &mut W,
    chart: &ChartSummary,
    simfile: &SimfileSummary,
) -> io::Result<()> {
    let header = format!(
        "{} {} : {}",
        chart.difficulty_str, chart.rating_str, chart.step_artist_str
    );
    writeln!(writer, "\n{header}")?;
    writeln!(writer, "{}", "-".repeat(header.len()))?;

    writeln!(writer, "Step Type: {}", chart.step_type_str)?;
    writeln!(writer, "Matrix Rating: {:.4}", chart.matrix_rating)?;
    writeln!(writer, "Tier BPM: {}", chart.tier_bpm)?;
    if !chart.tech_notation_str.is_empty() {
        writeln!(writer, "Tech Notations: {}", chart.tech_notation_str)?;
    }
    writeln!(writer, "SHA1 Hash: {}", chart.short_hash)?;
    writeln!(
        writer,
        "BPM Neutral SHA1 Hash: {}\n",
        chart.bpm_neutral_hash
    )?;

    if (chart.median_nps - chart.max_nps).abs() < f64::EPSILON {
        writeln!(writer, "NPS: {:.2} Median/Peak", chart.median_nps)?;
    } else {
        writeln!(
            writer,
            "NPS: {:.2} Median, {:.2} Peak",
            chart.median_nps, chart.max_nps
        )?;
    }
    let total_stream = chart.total_streams;
    let total_break = chart.stream_counts.total_breaks;
    let total_measures = chart.total_measures;
    let (stream_percent, adjusted_stream_percent, break_percent) =
        compute_stream_percentages(total_stream, total_break, total_measures);

    writeln!(
        writer,
        "Total Stream: {total_stream} ({stream_percent:.2}%/{adjusted_stream_percent:.2}% Adj.)"
    )?;
    writeln!(
        writer,
        "    16th_streams: {}",
        chart.stream_counts.run16_streams
    )?;
    writeln!(
        writer,
        "    20th_streams: {}",
        chart.stream_counts.run20_streams
    )?;
    writeln!(
        writer,
        "    24th_streams: {}",
        chart.stream_counts.run24_streams
    )?;
    writeln!(
        writer,
        "    32nd_streams: {}",
        chart.stream_counts.run32_streams
    )?;
    writeln!(
        writer,
        "Total Break: {total_break} ({break_percent:.2}%)"
    )?;

    writeln!(writer, "\n--- Chart Info ---")?;
    writeln!(
        writer,
        "Steps: {} ({} arrows) [{} left, {} down, {} up, {} right]",
        chart.stats.total_steps,
        chart.stats.total_arrows,
        chart.stats.left,
        chart.stats.down,
        chart.stats.up,
        chart.stats.right
    )?;
    writeln!(writer, "Jumps: {}", chart.stats.jumps)?;
    writeln!(writer, "Hands: {}", chart.stats.hands)?;
    writeln!(writer, "Holds: {}", chart.stats.holds)?;
    writeln!(writer, "Rolls: {}", chart.stats.rolls)?;
    let (mines_judgable, _) = chart_mine_fake_counts(chart);
    writeln!(writer, "Mines: {mines_judgable}")?;

    write_gimmicks(writer, chart, simfile)?;

    if simfile.pattern_counts_enabled {
        writeln!(writer, "\n--- Pattern Analysis ---")?;
        let candle_left = chart.detected_patterns[PatternVariant::CandleLeft as usize];
        let candle_right = chart.detected_patterns[PatternVariant::CandleRight as usize];
        writeln!(
            writer,
            "Candles: {} ({} left, {} right)",
            candle_left + candle_right,
            candle_left,
            candle_right
        )?;
        writeln!(writer, "Candle%: {:.2}%", chart.candle_percent)?;
        writeln!(
            writer,
            "Mono: {} ({} left-facing, {} right-facing)",
            chart.mono_total, chart.facing_left, chart.facing_right
        )?;
        writeln!(writer, "Mono%: {:.2}%", chart.mono_percent)?;

        let box_parts = compute_box_parts(&chart.detected_patterns);
        let box_corners =
            box_parts.lr + box_parts.ud + box_parts.ld + box_parts.lu + box_parts.rd + box_parts.ru;
        writeln!(
            writer,
            "Boxes: {} ({} LRLR, {} UDUD, {} LDLD, {} LULU, {} RDRD, {} RURU)",
            box_parts.lr + box_parts.ud + box_corners,
            box_parts.lr,
            box_parts.ud,
            box_parts.ld,
            box_parts.lu,
            box_parts.rd,
            box_parts.ru
        )?;

        let anchor_total =
            chart.anchor_left + chart.anchor_down + chart.anchor_up + chart.anchor_right;
        writeln!(
            writer,
            "Anchors: {} ({} left, {} down, {} up, {} right)",
            anchor_total, chart.anchor_left, chart.anchor_down, chart.anchor_up, chart.anchor_right
        )?;
    }

    if simfile.tech_counts_enabled {
        writeln!(writer, "\n--- Step Parity Analysis ---")?;
        writeln!(writer, "Crossovers: {}", chart.tech_counts.crossovers)?;
        writeln!(
            writer,
            "Footswitches: {} ({} up, {} down)",
            chart.tech_counts.footswitches,
            chart.tech_counts.up_footswitches,
            chart.tech_counts.down_footswitches
        )?;
        writeln!(writer, "Sideswitches: {}", chart.tech_counts.sideswitches)?;
        writeln!(writer, "Jacks: {}", chart.tech_counts.jacks)?;
        writeln!(writer, "Brackets: {}", chart.tech_counts.brackets)?;
        writeln!(writer, "Doublesteps: {}", chart.tech_counts.doublesteps)?;
    }

    if !chart.detailed_breakdown.is_empty() {
        writeln!(writer, "\n--- Detailed Breakdown ---")?;
        writeln!(writer, "{}", chart.detailed_breakdown)?;
        writeln!(writer, "--- Partial Breakdown ---")?;
        writeln!(writer, "{}", chart.partial_breakdown)?;
        writeln!(writer, "--- Simple Breakdown ---")?;
        writeln!(writer, "{}", chart.simple_breakdown)?;
    }

    if !chart.sn_detailed_breakdown.is_empty() {
        writeln!(writer, "\n--- SN Detailed Breakdown ---")?;
        writeln!(writer, "{}", chart.sn_detailed_breakdown)?;
        writeln!(writer, "--- SN Partially Simplified ---")?;
        writeln!(writer, "{}", chart.sn_partial_breakdown)?;
        writeln!(writer, "--- SN Simplified Breakdown ---")?;
        writeln!(writer, "{}", chart.sn_simple_breakdown)?;
    }

    if simfile.pattern_counts_enabled {
        write_other_patterns(writer, chart)?;
    }

    Ok(())
}

fn write_other_patterns<W: Write>(writer: &mut W, chart: &ChartSummary) -> io::Result<()> {
    writeln!(writer, "\n--- Other Patterns ---")?;
    let tower_parts = compute_tower_parts(&chart.detected_patterns);
    let corner_towers = tower_parts.ld + tower_parts.lu + tower_parts.rd + tower_parts.ru;
    let total_towers = tower_parts.lr + tower_parts.ud + corner_towers;
    writeln!(
        writer,
        "Total Towers: {} ({} LR, {} UD, {} LD, {} LU, {} RD, {} RU)",
        total_towers,
        tower_parts.lr,
        tower_parts.ud,
        tower_parts.ld,
        tower_parts.lu,
        tower_parts.rd,
        tower_parts.ru
    )?;

    // Triangles
    let triangle_parts = compute_triangle_parts(&chart.detected_patterns);
    let total_triangles =
        triangle_parts.ldl + triangle_parts.lul + triangle_parts.rdr + triangle_parts.rur;
    writeln!(
        writer,
        "Total Triangles: {} ({} LDL, {} LUL, {} RDR, {} RUR)",
        total_triangles,
        triangle_parts.ldl,
        triangle_parts.lul,
        triangle_parts.rdr,
        triangle_parts.rur
    )?;

    // Staircases
    let stairs = compute_stair_parts(
        &chart.detected_patterns,
        PatternVariant::StaircaseLeft,
        PatternVariant::StaircaseRight,
        PatternVariant::StaircaseInvLeft,
        PatternVariant::StaircaseInvRight,
    );
    let total_staircases = stairs.left + stairs.right + stairs.left_inv + stairs.right_inv;
    writeln!(
        writer,
        "Staircases: {} ({} Left, {} Right, {} Left Inv, {} Right Inv)",
        total_staircases, stairs.left, stairs.right, stairs.left_inv, stairs.right_inv
    )?;

    // Alternate Staircases
    let alt_stairs = compute_stair_parts(
        &chart.detected_patterns,
        PatternVariant::AltStaircasesLeft,
        PatternVariant::AltStaircasesRight,
        PatternVariant::AltStaircasesInvLeft,
        PatternVariant::AltStaircasesInvRight,
    );
    let total_alt = alt_stairs.left + alt_stairs.right + alt_stairs.left_inv + alt_stairs.right_inv;
    writeln!(
        writer,
        "Alt Staircases: {} ({} Left, {} Right, {} Left Inv, {} Right Inv)",
        total_alt, alt_stairs.left, alt_stairs.right, alt_stairs.left_inv, alt_stairs.right_inv
    )?;

    // Double Staircases
    let double_stairs = compute_stair_parts(
        &chart.detected_patterns,
        PatternVariant::DStaircaseLeft,
        PatternVariant::DStaircaseRight,
        PatternVariant::DStaircaseInvLeft,
        PatternVariant::DStaircaseInvRight,
    );
    let total_double =
        double_stairs.left + double_stairs.right + double_stairs.left_inv + double_stairs.right_inv;
    writeln!(
        writer,
        "Double Staircases: {} ({} Left, {} Right, {} Left Inv, {} Right Inv)",
        total_double,
        double_stairs.left,
        double_stairs.right,
        double_stairs.left_inv,
        double_stairs.right_inv
    )?;

    // Sweeps
    let sweeps = compute_sweep_parts(
        &chart.detected_patterns,
        PatternVariant::SweepLeft,
        PatternVariant::SweepRight,
        PatternVariant::SweepInvLeft,
        PatternVariant::SweepInvRight,
    );
    let total_sweeps = sweeps.left + sweeps.right + sweeps.left_inv + sweeps.right_inv;
    writeln!(
        writer,
        "Sweeps: {} ({} Left, {} Right, {} Left Inv, {} Right Inv)",
        total_sweeps, sweeps.left, sweeps.right, sweeps.left_inv, sweeps.right_inv
    )?;

    // Candle Sweeps
    let candle_sweeps = compute_sweep_parts(
        &chart.detected_patterns,
        PatternVariant::SweepCandleLeft,
        PatternVariant::SweepCandleRight,
        PatternVariant::SweepCandleInvLeft,
        PatternVariant::SweepCandleInvRight,
    );
    let total_candle_sweeps =
        candle_sweeps.left + candle_sweeps.right + candle_sweeps.left_inv + candle_sweeps.right_inv;
    writeln!(
        writer,
        "Candle Sweeps: {} ({} Left, {} Right, {} Left Inv, {} Right Inv)",
        total_candle_sweeps,
        candle_sweeps.left,
        candle_sweeps.right,
        candle_sweeps.left_inv,
        candle_sweeps.right_inv
    )?;

    // Copters
    let copters = compute_simple_quad_parts(
        &chart.detected_patterns,
        PatternVariant::CopterLeft,
        PatternVariant::CopterRight,
        PatternVariant::CopterInvLeft,
        PatternVariant::CopterInvRight,
    );
    let total_copters = copters.a + copters.b + copters.c + copters.d;
    writeln!(
        writer,
        "Copters: {} ({} Left, {} Right, {} Left Inv, {} Right Inv)",
        total_copters, copters.a, copters.b, copters.c, copters.d
    )?;

    // Spirals
    let spirals = compute_simple_quad_parts(
        &chart.detected_patterns,
        PatternVariant::SpiralLeft,
        PatternVariant::SpiralRight,
        PatternVariant::SpiralInvLeft,
        PatternVariant::SpiralInvRight,
    );
    let total_spirals = spirals.a + spirals.b + spirals.c + spirals.d;
    writeln!(
        writer,
        "Spirals: {} ({} Left, {} Right, {} Left Inv, {} Right Inv)",
        total_spirals, spirals.a, spirals.b, spirals.c, spirals.d
    )?;

    // Turbo Candles
    let turbo_candles = compute_simple_quad_parts(
        &chart.detected_patterns,
        PatternVariant::TurboCandleLeft,
        PatternVariant::TurboCandleRight,
        PatternVariant::TurboCandleInvLeft,
        PatternVariant::TurboCandleInvRight,
    );
    let total_turbo_candles = turbo_candles.a + turbo_candles.b + turbo_candles.c + turbo_candles.d;
    writeln!(
        writer,
        "Turbo Candles: {} ({} Left, {} Right, {} Left Inv, {} Right Inv)",
        total_turbo_candles, turbo_candles.a, turbo_candles.b, turbo_candles.c, turbo_candles.d
    )?;

    // Hip Breakers
    let hip_breakers = compute_simple_quad_parts(
        &chart.detected_patterns,
        PatternVariant::HipBreakerLeft,
        PatternVariant::HipBreakerRight,
        PatternVariant::HipBreakerInvLeft,
        PatternVariant::HipBreakerInvRight,
    );
    let total_hip_breakers = hip_breakers.a + hip_breakers.b + hip_breakers.c + hip_breakers.d;
    writeln!(
        writer,
        "Hip Breakers: {} ({} Left, {} Right, {} Left Inv, {} Right Inv)",
        total_hip_breakers, hip_breakers.a, hip_breakers.b, hip_breakers.c, hip_breakers.d
    )?;

    // Doritos
    let doritos = compute_simple_quad_parts(
        &chart.detected_patterns,
        PatternVariant::DoritoLeft,
        PatternVariant::DoritoRight,
        PatternVariant::DoritoInvLeft,
        PatternVariant::DoritoInvRight,
    );
    let total_doritos = doritos.a + doritos.b + doritos.c + doritos.d;
    writeln!(
        writer,
        "Doritos: {} ({} Left, {} Right, {} Left Inv, {} Right Inv)",
        total_doritos, doritos.a, doritos.b, doritos.c, doritos.d
    )?;

    // Luchis
    let luchis = compute_simple_quad_parts(
        &chart.detected_patterns,
        PatternVariant::LuchiLeftDU,
        PatternVariant::LuchiLeftUD,
        PatternVariant::LuchiRightDU,
        PatternVariant::LuchiRightUD,
    );
    let total_luchis = luchis.a + luchis.b + luchis.c + luchis.d;
    writeln!(
        writer,
        "Luchis: {} ({} Left DU, {} Left UD, {} Right DU, {} Right UD)",
        total_luchis, luchis.a, luchis.b, luchis.c, luchis.d
    )?;

    if !chart.custom_patterns.is_empty() {
        writeln!(writer, "\n--- Custom Patterns ---")?;
        for cp in &chart.custom_patterns {
            writeln!(writer, "{}: {}", cp.pattern, cp.count)?;
        }
    }

    Ok(())
}

fn json_chart_info(chart: &ChartSummary) -> JsonValue {
    serde_json::json!({
        "step_type": chart.step_type_str,
        "difficulty": chart.difficulty_str,
        "tier_bpm": chart.tier_bpm,
        "rating": chart.rating_str,
        "matrix_rating": chart.matrix_rating,
        "step_artists": chart.step_artist_str,
        "tech_notation": chart.tech_notation_str,
        "sha1": chart.short_hash,
        "bpm_neutral_sha1": chart.bpm_neutral_hash,
    })
}

fn json_arrow_stats(chart: &ChartSummary) -> JsonValue {
    let (mines_judgable, _) = chart_mine_fake_counts(chart);
    serde_json::json!({
        "total_arrows": chart.stats.total_arrows,
        "left_arrows": chart.stats.left,
        "down_arrows": chart.stats.down,
        "up_arrows": chart.stats.up,
        "right_arrows": chart.stats.right,
        "total_steps": chart.stats.total_steps,
        "jumps": chart.stats.jumps,
        "hands": chart.stats.hands,
        "holds": chart.stats.holds,
        "rolls": chart.stats.rolls,
        "mines": mines_judgable,
    })
}

fn json_stream_info(chart: &ChartSummary) -> JsonValue {
    let total_stream = chart.total_streams;
    let total_break = chart.stream_counts.total_breaks;
    let total_measures = chart.total_measures;

    let (stream_percent, adj_stream_percent, break_percent) =
        compute_stream_percentages(total_stream, total_break, total_measures);

    let segments = stream_sequences(&chart.measure_densities);
    let mut stream_sequences = Vec::with_capacity(segments.len());
    for segment in segments {
        stream_sequences.push(serde_json::json!({
            "stream_start": segment.start as u32,
            "stream_end": segment.end as u32,
            "is_break": segment.is_break,
        }));
    }

    serde_json::json!({
        "total_streams": total_stream,
        "16th_streams": chart.stream_counts.run16_streams,
        "20th_streams": chart.stream_counts.run20_streams,
        "24th_streams": chart.stream_counts.run24_streams,
        "32nd_streams": chart.stream_counts.run32_streams,
        "total_breaks": total_break,
        "sn_breaks": chart.stream_counts.sn_breaks,
        "stream_percent": stream_percent,
        "adj_stream_percent": adj_stream_percent,
        "break_percent": break_percent,
        "stream_sequences": stream_sequences,
    })
}

fn json_nps(chart: &ChartSummary) -> JsonValue {
    let mut notes_per_measure = Vec::with_capacity(chart.measure_densities.len());
    for &count in &chart.measure_densities {
        notes_per_measure.push(JsonValue::from(count as u32));
    }

    let mut nps_per_measure = Vec::with_capacity(chart.measure_nps_vec.len());
    for &value in &chart.measure_nps_vec {
        nps_per_measure.push(JsonValue::from(value));
    }

    let lanes = crate::step_type_lanes(&chart.step_type_str);
    let spaced = measure_equally_spaced(&chart.minimized_note_data, lanes);
    let mut equally_spaced_per_measure = Vec::with_capacity(spaced.len());
    for value in spaced {
        equally_spaced_per_measure.push(JsonValue::from(value));
    }

    serde_json::json!({
        "max_nps": chart.max_nps,
        "median_nps": chart.median_nps,
        "notes_per_measure": notes_per_measure,
        "nps_per_measure": nps_per_measure,
        "equally_spaced_per_measure": equally_spaced_per_measure,
    })
}

fn json_sn_breakdown(chart: &ChartSummary) -> JsonValue {
    serde_json::json!({
        "sn_detailed_breakdown": chart.sn_detailed_breakdown,
        "sn_partial_breakdown": chart.sn_partial_breakdown,
        "sn_simple_breakdown": chart.sn_simple_breakdown,
    })
}

fn json_stream_breakdown(chart: &ChartSummary) -> JsonValue {
    serde_json::json!({
        "detailed_breakdown": chart.detailed_breakdown,
        "partial_breakdown": chart.partial_breakdown,
        "simple_breakdown": chart.simple_breakdown,
    })
}

fn json_mono_candle_stats(chart: &ChartSummary) -> JsonValue {
    let left_foot_candles = count(&chart.detected_patterns, PatternVariant::CandleLeft);
    let right_foot_candles = count(&chart.detected_patterns, PatternVariant::CandleRight);
    let total_candles = left_foot_candles + right_foot_candles;

    serde_json::json!({
        "total_candles": total_candles,
        "left_foot_candles": left_foot_candles,
        "right_foot_candles": right_foot_candles,
        "candles_percent": chart.candle_percent,
        "total_mono": chart.mono_total,
        "left_face_mono": chart.facing_left,
        "right_face_mono": chart.facing_right,
        "mono_percent": chart.mono_percent,
    })
}

fn json_gimmicks(chart: &ChartSummary, simfile: &SimfileSummary) -> JsonValue {
    let lifts = chart.stats.lifts;
    let (_, fakes) = chart_mine_fake_counts(chart);
    let allow_steps_timing = steps_timing_allowed(simfile.ssc_version, simfile.timing_format);
    let stops = chart_or_global(
        allow_steps_timing,
        chart.chart_has_own_timing,
        &chart.chart_stops,
        &simfile.normalized_stops,
    );
    let delays = chart_or_global(
        allow_steps_timing,
        chart.chart_has_own_timing,
        &chart.chart_delays,
        &simfile.normalized_delays,
    );
    let warps = chart_or_global(
        allow_steps_timing,
        chart.chart_has_own_timing,
        &chart.chart_warps,
        &simfile.normalized_warps,
    );
    let speeds = chart_or_global(
        allow_steps_timing,
        chart.chart_has_own_timing,
        &chart.chart_speeds,
        &simfile.normalized_speeds,
    );
    let scrolls = chart_or_global(
        allow_steps_timing,
        chart.chart_has_own_timing,
        &chart.chart_scrolls,
        &simfile.normalized_scrolls,
    );

    let stop_count = count_timing_segments(stops);
    let delay_count = count_timing_segments(delays);
    let warp_count = count_timing_segments(warps);
    let speed_count = count_gimmick_speed_segments(speeds);
    let scroll_count = count_gimmick_scroll_segments(scrolls);

    let mut obj = JsonMap::new();

    obj.insert("lifts".to_string(), JsonValue::from(lifts));
    obj.insert("fakes".to_string(), JsonValue::from(fakes));
    obj.insert("stops_freezes".to_string(), JsonValue::from(stop_count));
    obj.insert("speeds".to_string(), JsonValue::from(speed_count));
    obj.insert("scrolls".to_string(), JsonValue::from(scroll_count));
    obj.insert("delays".to_string(), JsonValue::from(delay_count));
    obj.insert("warps".to_string(), JsonValue::from(warp_count));

    JsonValue::Object(obj)
}

fn json_timing(chart: &ChartSummary, simfile: &SimfileSummary) -> JsonValue {
    let TimingSnapshot {
        beat0_offset_seconds,
        beat0_group_offset_seconds,
        bpms,
        bpms_formatted,
        bpm_min_raw,
        bpm_max_raw,
        stops,
        delays,
        time_signatures,
        warps,
        labels,
        tickcounts,
        combos,
        speeds,
        scrolls,
        fakes,
    } = build_timing_snapshot(chart, simfile);

    let bpm_min = round_sig_figs_6(round_sig_figs_itg(bpm_min_raw));
    let bpm_max = round_sig_figs_6(round_sig_figs_itg(bpm_max_raw));

    let chart_display_bpm = chart
        .chart_display_bpm
        .as_deref()
        .filter(|s| !s.trim().is_empty());
    let display_tag = chart_display_bpm;
    let (display_bpm_min_raw, display_bpm_max_raw, display_bpm) =
        resolve_display_bpm(display_tag, bpm_min_raw, bpm_max_raw, 1.0);
    let display_bpm_min = round_sig_figs_6(round_sig_figs_itg(display_bpm_min_raw));
    let display_bpm_max = round_sig_figs_6(round_sig_figs_itg(display_bpm_max_raw));
    let bpms: Vec<JsonValue> = bpms
        .into_iter()
        .map(|(beat, bpm)| serde_json::json!([beat, bpm]))
        .collect();
    let stops: Vec<JsonValue> = stops
        .into_iter()
        .map(|(beat, duration)| serde_json::json!([beat, duration]))
        .collect();
    let delays: Vec<JsonValue> = delays
        .into_iter()
        .map(|(beat, duration)| serde_json::json!([beat, duration]))
        .collect();
    let warps: Vec<JsonValue> = warps
        .into_iter()
        .map(|(beat, length)| serde_json::json!([beat, length]))
        .collect();
    let speeds: Vec<JsonValue> = speeds
        .into_iter()
        .map(|(beat, ratio, delay, unit)| serde_json::json!([beat, ratio, delay, unit]))
        .collect();
    let scrolls: Vec<JsonValue> = scrolls
        .into_iter()
        .map(|(beat, ratio)| serde_json::json!([beat, ratio]))
        .collect();
    let fakes: Vec<JsonValue> = fakes
        .into_iter()
        .map(|(beat, length)| serde_json::json!([beat, length]))
        .collect();
    // SL-ChartParser uses chart BPMS for hashing when present, regardless of split timing.
    let hash_bpms = chart
        .chart_bpms
        .as_deref()
        .map(normalize_float_digits)
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| simfile.normalized_bpms.clone());

    serde_json::json!({
        "beat0_offset_seconds": beat0_offset_seconds,
        "beat0_group_offset_seconds": beat0_group_offset_seconds,
        "hash_bpms": hash_bpms,
        "bpms_formatted": bpms_formatted,
        "bpm_min": bpm_min,
        "bpm_max": bpm_max,
        "display_bpm": display_bpm,
        "display_bpm_min": display_bpm_min,
        "display_bpm_max": display_bpm_max,
        "bpms": bpms,
        "stops": stops,
        "delays": delays,
        "time_signatures": time_signatures
            .into_iter()
            .map(|(beat, num, den)| serde_json::json!([beat, num, den]))
            .collect::<Vec<_>>(),
        "warps": warps,
        "labels": labels
            .into_iter()
            .map(|(beat, label)| serde_json::json!([beat, label]))
            .collect::<Vec<_>>(),
        "tickcounts": tickcounts
            .into_iter()
            .map(|(beat, count)| serde_json::json!([beat, count]))
            .collect::<Vec<_>>(),
        "combos": combos
            .into_iter()
            .map(|(beat, combo, miss)| serde_json::json!([beat, combo, miss]))
            .collect::<Vec<_>>(),
        "speeds": speeds,
        "scrolls": scrolls,
        "fakes": fakes,
        "duration_seconds": chart.duration_seconds,
    })
}

fn json_pattern_counts(chart: &ChartSummary) -> JsonValue {
    let mut obj = JsonMap::new();

    // Boxes
    let box_parts = compute_box_parts(&chart.detected_patterns);
    let corner_boxes = box_parts.ld + box_parts.lu + box_parts.rd + box_parts.ru;
    let total_boxes = box_parts.lr + box_parts.ud + corner_boxes;
    obj.insert(
        "boxes".to_string(),
        serde_json::json!({
            "total_boxes": total_boxes,
            "lr_boxes": box_parts.lr,
            "ud_boxes": box_parts.ud,
            "corner_boxes": corner_boxes,
            "ld_boxes": box_parts.ld,
            "lu_boxes": box_parts.lu,
            "rd_boxes": box_parts.rd,
            "ru_boxes": box_parts.ru,
        }),
    );

    // Anchors
    let total_anchors =
        chart.anchor_left + chart.anchor_down + chart.anchor_up + chart.anchor_right;
    obj.insert(
        "anchors".to_string(),
        serde_json::json!({
            "total_anchors": total_anchors,
            "left_anchors": chart.anchor_left,
            "down_anchors": chart.anchor_down,
            "up_anchors": chart.anchor_up,
            "right_anchors": chart.anchor_right,
        }),
    );

    // Towers
    let tower_parts = compute_tower_parts(&chart.detected_patterns);
    let corner_towers = tower_parts.ld + tower_parts.lu + tower_parts.rd + tower_parts.ru;
    let total_towers = tower_parts.lr + tower_parts.ud + corner_towers;
    obj.insert(
        "towers".to_string(),
        serde_json::json!({
            "total_towers": total_towers,
            "lr_towers": tower_parts.lr,
            "ud_towers": tower_parts.ud,
            "corner_towers": corner_towers,
            "ld_towers": tower_parts.ld,
            "lu_towers": tower_parts.lu,
            "rd_towers": tower_parts.rd,
            "ru_towers": tower_parts.ru,
        }),
    );

    // Triangles
    let triangle_parts = compute_triangle_parts(&chart.detected_patterns);
    let total_triangles =
        triangle_parts.ldl + triangle_parts.lul + triangle_parts.rdr + triangle_parts.rur;
    obj.insert(
        "triangles".to_string(),
        serde_json::json!({
            "total_triangles": total_triangles,
            "ldl_triangles": triangle_parts.ldl,
            "lul_triangles": triangle_parts.lul,
            "rdr_triangles": triangle_parts.rdr,
            "rur_triangles": triangle_parts.rur,
        }),
    );

    // Staircases
    let stairs = compute_stair_parts(
        &chart.detected_patterns,
        PatternVariant::StaircaseLeft,
        PatternVariant::StaircaseRight,
        PatternVariant::StaircaseInvLeft,
        PatternVariant::StaircaseInvRight,
    );
    let total_staircases = stairs.left + stairs.right + stairs.left_inv + stairs.right_inv;
    let alt_stairs = compute_stair_parts(
        &chart.detected_patterns,
        PatternVariant::AltStaircasesLeft,
        PatternVariant::AltStaircasesRight,
        PatternVariant::AltStaircasesInvLeft,
        PatternVariant::AltStaircasesInvRight,
    );
    let total_alt = alt_stairs.left + alt_stairs.right + alt_stairs.left_inv + alt_stairs.right_inv;
    let double_stairs = compute_stair_parts(
        &chart.detected_patterns,
        PatternVariant::DStaircaseLeft,
        PatternVariant::DStaircaseRight,
        PatternVariant::DStaircaseInvLeft,
        PatternVariant::DStaircaseInvRight,
    );
    let total_double =
        double_stairs.left + double_stairs.right + double_stairs.left_inv + double_stairs.right_inv;
    obj.insert(
        "staircases".to_string(),
        serde_json::json!({
            "total_staircases": total_staircases,
            "left_staircases": stairs.left,
            "right_staircases": stairs.right,
            "left_inv_staircases": stairs.left_inv,
            "right_inv_staircases": stairs.right_inv,
            "total_alt_staircases": total_alt,
            "left_alt_staircases": alt_stairs.left,
            "right_alt_staircases": alt_stairs.right,
            "left_inv_alt_staircases": alt_stairs.left_inv,
            "right_inv_alt_staircases": alt_stairs.right_inv,
            "total_double_staircases": total_double,
            "left_double_staircases": double_stairs.left,
            "right_double_staircases": double_stairs.right,
            "left_inv_double_staircases": double_stairs.left_inv,
            "right_inv_double_staircases": double_stairs.right_inv,
        }),
    );

    // Sweeps
    let sweeps = compute_sweep_parts(
        &chart.detected_patterns,
        PatternVariant::SweepLeft,
        PatternVariant::SweepRight,
        PatternVariant::SweepInvLeft,
        PatternVariant::SweepInvRight,
    );
    let total_sweeps = sweeps.left + sweeps.right + sweeps.left_inv + sweeps.right_inv;
    obj.insert(
        "sweeps".to_string(),
        serde_json::json!({
            "total_sweeps": total_sweeps,
            "left_sweeps": sweeps.left,
            "right_sweeps": sweeps.right,
            "left_inv_sweeps": sweeps.left_inv,
            "right_inv_sweeps": sweeps.right_inv,
        }),
    );

    // Candle Sweeps
    let candle_sweeps = compute_sweep_parts(
        &chart.detected_patterns,
        PatternVariant::SweepCandleLeft,
        PatternVariant::SweepCandleRight,
        PatternVariant::SweepCandleInvLeft,
        PatternVariant::SweepCandleInvRight,
    );
    let total_candle_sweeps =
        candle_sweeps.left + candle_sweeps.right + candle_sweeps.left_inv + candle_sweeps.right_inv;
    obj.insert(
        "candle_sweeps".to_string(),
        serde_json::json!({
            "total_candle_sweeps": total_candle_sweeps,
            "left_candle_sweeps": candle_sweeps.left,
            "right_candle_sweeps": candle_sweeps.right,
            "left_inv_candle_sweeps": candle_sweeps.left_inv,
            "right_inv_candle_sweeps": candle_sweeps.right_inv,
        }),
    );

    // Copters
    let copters = compute_simple_quad_parts(
        &chart.detected_patterns,
        PatternVariant::CopterLeft,
        PatternVariant::CopterRight,
        PatternVariant::CopterInvLeft,
        PatternVariant::CopterInvRight,
    );
    let total_copters = copters.a + copters.b + copters.c + copters.d;
    obj.insert(
        "copters".to_string(),
        serde_json::json!({
            "total_copters": total_copters,
            "left_copters": copters.a,
            "right_copters": copters.b,
            "left_inv_copters": copters.c,
            "right_inv_copters": copters.d,
        }),
    );

    // Spirals
    let spirals = compute_simple_quad_parts(
        &chart.detected_patterns,
        PatternVariant::SpiralLeft,
        PatternVariant::SpiralRight,
        PatternVariant::SpiralInvLeft,
        PatternVariant::SpiralInvRight,
    );
    let total_spirals = spirals.a + spirals.b + spirals.c + spirals.d;
    obj.insert(
        "spirals".to_string(),
        serde_json::json!({
            "total_spirals": total_spirals,
            "left_spirals": spirals.a,
            "right_spirals": spirals.b,
            "left_inv_spirals": spirals.c,
            "right_inv_spirals": spirals.d,
        }),
    );

    // Turbo Candles
    let turbo_candles = compute_simple_quad_parts(
        &chart.detected_patterns,
        PatternVariant::TurboCandleLeft,
        PatternVariant::TurboCandleRight,
        PatternVariant::TurboCandleInvLeft,
        PatternVariant::TurboCandleInvRight,
    );
    let total_turbo_candles = turbo_candles.a + turbo_candles.b + turbo_candles.c + turbo_candles.d;
    obj.insert(
        "turbo_candles".to_string(),
        serde_json::json!({
            "total_turbo_candles": total_turbo_candles,
            "left_turbo_candles": turbo_candles.a,
            "right_turbo_candles": turbo_candles.b,
            "left_inv_turbo_candles": turbo_candles.c,
            "right_inv_turbo_candles": turbo_candles.d,
        }),
    );

    // Hip Breakers
    let hip_breakers = compute_simple_quad_parts(
        &chart.detected_patterns,
        PatternVariant::HipBreakerLeft,
        PatternVariant::HipBreakerRight,
        PatternVariant::HipBreakerInvLeft,
        PatternVariant::HipBreakerInvRight,
    );
    let total_hip_breakers = hip_breakers.a + hip_breakers.b + hip_breakers.c + hip_breakers.d;
    obj.insert(
        "hip_breakers".to_string(),
        serde_json::json!({
            "total_hip_breakers": total_hip_breakers,
            "left_hip_breakers": hip_breakers.a,
            "right_hip_breakers": hip_breakers.b,
            "left_inv_hip_breakers": hip_breakers.c,
            "right_inv_hip_breakers": hip_breakers.d,
        }),
    );

    // Doritos
    let doritos = compute_simple_quad_parts(
        &chart.detected_patterns,
        PatternVariant::DoritoLeft,
        PatternVariant::DoritoRight,
        PatternVariant::DoritoInvLeft,
        PatternVariant::DoritoInvRight,
    );
    let total_doritos = doritos.a + doritos.b + doritos.c + doritos.d;
    obj.insert(
        "doritos".to_string(),
        serde_json::json!({
            "total_doritos": total_doritos,
            "left_doritos": doritos.a,
            "right_doritos": doritos.b,
            "left_inv_doritos": doritos.c,
            "right_inv_doritos": doritos.d,
        }),
    );

    // Luchis
    let luchis = compute_simple_quad_parts(
        &chart.detected_patterns,
        PatternVariant::LuchiLeftDU,
        PatternVariant::LuchiLeftUD,
        PatternVariant::LuchiRightDU,
        PatternVariant::LuchiRightUD,
    );
    let total_luchis = luchis.a + luchis.b + luchis.c + luchis.d;
    obj.insert(
        "luchis".to_string(),
        serde_json::json!({
            "total_luchis": total_luchis,
            "left_du_luchis": luchis.a,
            "left_ud_luchis": luchis.b,
            "right_du_luchis": luchis.c,
            "right_ud_luchis": luchis.d,
        }),
    );

    // Custom patterns
    if !chart.custom_patterns.is_empty() {
        let mut custom = JsonMap::new();
        for cp in &chart.custom_patterns {
            custom.insert(cp.pattern.clone(), JsonValue::from(cp.count));
        }
        obj.insert("custom_patterns".to_string(), JsonValue::Object(custom));
    }

    JsonValue::Object(obj)
}

fn json_tech_counts(chart: &ChartSummary) -> JsonValue {
    serde_json::json!({
        "crossovers": chart.tech_counts.crossovers,
        "footswitches": chart.tech_counts.footswitches,
        "up_footswitches": chart.tech_counts.up_footswitches,
        "down_footswitches": chart.tech_counts.down_footswitches,
        "sideswitches": chart.tech_counts.sideswitches,
        "jacks": chart.tech_counts.jacks,
        "brackets": chart.tech_counts.brackets,
        "doublesteps": chart.tech_counts.doublesteps,
    })
}

fn write_indent<W: Write>(writer: &mut W, indent: usize) -> io::Result<()> {
    for _ in 0..indent {
        writer.write_all(b" ")?;
    }
    Ok(())
}

fn write_json_string<W: Write>(writer: &mut W, s: &str) -> io::Result<()> {
    let bytes = s.as_bytes();
    let mut needs_escape = false;
    for &b in bytes {
        if b < 0x20 || b == b'"' || b == b'\\' {
            needs_escape = true;
            break;
        }
    }

    writer.write_all(b"\"")?;
    if !needs_escape {
        writer.write_all(bytes)?;
        writer.write_all(b"\"")?;
        return Ok(());
    }

    let hex = |value: u8| -> u8 {
        if value < 10 {
            b'0' + value
        } else {
            b'a' + (value - 10)
        }
    };

    let mut start = 0usize;
    for (i, &b) in bytes.iter().enumerate() {
        let escape = match b {
            b'"' => Some(b"\\\"".as_slice()),
            b'\\' => Some(b"\\\\".as_slice()),
            b'\n' => Some(b"\\n".as_slice()),
            b'\r' => Some(b"\\r".as_slice()),
            b'\t' => Some(b"\\t".as_slice()),
            b'\x08' => Some(b"\\b".as_slice()),
            b'\x0c' => Some(b"\\f".as_slice()),
            0x00..=0x1F => None,
            _ => continue,
        };

        if start < i {
            writer.write_all(&bytes[start..i])?;
        }

        if let Some(escape) = escape {
            writer.write_all(escape)?;
        } else {
            let mut buf = [b'\\', b'u', b'0', b'0', b'0', b'0'];
            buf[4] = hex((b >> 4) & 0x0f);
            buf[5] = hex(b & 0x0f);
            writer.write_all(&buf)?;
        }

        start = i + 1;
    }

    if start < bytes.len() {
        writer.write_all(&bytes[start..])?;
    }
    writer.write_all(b"\"")
}

fn write_json_number_for_key<W: Write>(
    writer: &mut W,
    key: Option<&str>,
    number: &JsonNumber,
) -> io::Result<()> {
    if let Some(i) = number.as_i64() {
        write!(writer, "{i}")
    } else if let Some(u) = number.as_u64() {
        write!(writer, "{u}")
    } else if let Some(f) = number.as_f64() {
        match key {
            None => write!(writer, "{f}"),
            Some("offset") => write!(writer, "{f:.3}"),
            Some("beat0_offset_seconds" | "beat0_group_offset_seconds" |
"duration_seconds" | "max_nps" | "bpm_min" | "bpm_max" | "display_bpm_min" |
"display_bpm_max") => write!(writer, "{f}"),
            Some("bpm") => write!(writer, "{f}"),
            _ => write!(writer, "{f:.2}"),
        }
    } else {
        write!(writer, "0")
    }
}

fn write_json_value_with_key<W: Write>(
    writer: &mut W,
    key: Option<&str>,
    value: &JsonValue,
    indent: usize,
) -> io::Result<()> {
    match value {
        JsonValue::Null => writer.write_all(b"null"),
        JsonValue::Bool(b) => {
            if *b {
                writer.write_all(b"true")
            } else {
                writer.write_all(b"false")
            }
        }
        JsonValue::Number(n) => write_json_number_for_key(writer, key, n),
        JsonValue::String(s) => write_json_string(writer, s),
        JsonValue::Array(arr) => write_json_array(writer, arr, indent),
        JsonValue::Object(obj) => write_json_object(writer, obj, indent),
    }
}

fn write_json_scalar_array<W: Write>(
    writer: &mut W,
    arr: &[JsonValue],
    indent: usize,
) -> io::Result<()> {
    writer.write_all(b"[")?;
    for (i, value) in arr.iter().enumerate() {
        if i != 0 {
            writer.write_all(b", ")?;
        }
        write_json_value_with_key(writer, None, value, indent)?;
    }
    writer.write_all(b"]")
}

fn write_json_array_multiline<W: Write>(
    writer: &mut W,
    arr: &[JsonValue],
    indent: usize,
) -> io::Result<()> {
    writer.write_all(b"[\n")?;
    let mut first = true;
    for value in arr {
        if !first {
            writer.write_all(b",\n")?;
        }
        first = false;
        write_indent(writer, indent + 2)?;
        write_json_value_with_key(writer, None, value, indent + 2)?;
    }
    writer.write_all(b"\n")?;
    write_indent(writer, indent)?;
    writer.write_all(b"]")
}

fn write_json_array<W: Write>(writer: &mut W, arr: &[JsonValue], indent: usize) -> io::Result<()> {
    if arr.is_empty() {
        return writer.write_all(b"[]");
    }

    if arr.iter().all(|v| {
        matches!(
            v,
            JsonValue::Null | JsonValue::Bool(_) | JsonValue::Number(_) | JsonValue::String(_)
        )
    }) {
        return write_json_scalar_array(writer, arr, indent);
    }

    write_json_array_multiline(writer, arr, indent)
}

fn write_json_object<W: Write>(
    writer: &mut W,
    obj: &JsonMap<String, JsonValue>,
    indent: usize,
) -> io::Result<()> {
    writer.write_all(b"{\n")?;
    let mut first = true;
    for (key, value) in obj {
        if !first {
            writer.write_all(b",\n")?;
        }
        first = false;
        write_indent(writer, indent + 2)?;
        write_json_string(writer, key)?;
        writer.write_all(b": ")?;
        write_json_value_with_key(writer, Some(key.as_str()), value, indent + 2)?;
    }
    if !obj.is_empty() {
        writer.write_all(b"\n")?;
    }
    write_indent(writer, indent)?;
    writer.write_all(b"}")
}

pub fn write_json_all<W: Write>(simfile: &SimfileSummary, writer: &mut W) -> io::Result<()> {
    let bpm_value = if (simfile.min_bpm - simfile.max_bpm).abs() < f64::EPSILON {
        JsonValue::from(simfile.min_bpm)
    } else {
        JsonValue::from(format!("{:.0}-{:.0}", simfile.min_bpm, simfile.max_bpm))
    };

    let charts: Vec<JsonValue> = simfile
        .charts
        .iter()
        .map(|chart| {
            let mut chart_obj = JsonMap::new();

            chart_obj.insert("chart_info".to_string(), json_chart_info(chart));
            chart_obj.insert("arrow_stats".to_string(), json_arrow_stats(chart));
            chart_obj.insert("gimmicks".to_string(), json_gimmicks(chart, simfile));
            chart_obj.insert("timing".to_string(), json_timing(chart, simfile));
            chart_obj.insert("stream_info".to_string(), json_stream_info(chart));
            chart_obj.insert("nps".to_string(), json_nps(chart));
            chart_obj.insert("breakdown".to_string(), json_sn_breakdown(chart));
            chart_obj.insert("stream_breakdown".to_string(), json_stream_breakdown(chart));
            if simfile.pattern_counts_enabled {
                chart_obj.insert(
                    "mono_candle_stats".to_string(),
                    json_mono_candle_stats(chart),
                );
                chart_obj.insert("pattern_counts".to_string(), json_pattern_counts(chart));
            }
            if simfile.tech_counts_enabled {
                chart_obj.insert("tech_counts".to_string(), json_tech_counts(chart));
            }

            JsonValue::Object(chart_obj)
        })
        .collect();

    let mut root_obj = JsonMap::new();
    root_obj.insert(
        "title".to_string(),
        JsonValue::from(simfile.title_str.clone()),
    );
    root_obj.insert(
        "subtitle".to_string(),
        JsonValue::from(simfile.subtitle_str.clone()),
    );
    root_obj.insert(
        "artist".to_string(),
        JsonValue::from(simfile.artist_str.clone()),
    );
    root_obj.insert(
        "title_trans".to_string(),
        JsonValue::from(simfile.titletranslit_str.clone()),
    );
    root_obj.insert(
        "subtitle_trans".to_string(),
        JsonValue::from(simfile.subtitletranslit_str.clone()),
    );
    root_obj.insert(
        "artist_trans".to_string(),
        JsonValue::from(simfile.artisttranslit_str.clone()),
    );
    root_obj.insert(
        "length".to_string(),
        JsonValue::from(simfile.total_length.to_string()),
    );
    root_obj.insert("bpm".to_string(), bpm_value);
    root_obj.insert("min_bpm".to_string(), JsonValue::from(simfile.min_bpm));
    root_obj.insert("max_bpm".to_string(), JsonValue::from(simfile.max_bpm));
    root_obj.insert(
        "average_bpm".to_string(),
        JsonValue::from(simfile.average_bpm),
    );
    root_obj.insert(
        "median_bpm".to_string(),
        JsonValue::from(simfile.median_bpm),
    );
    root_obj.insert(
        "bpm_data".to_string(),
        JsonValue::from(simfile.normalized_bpms.clone()),
    );
    root_obj.insert("offset".to_string(), JsonValue::from(simfile.offset));
    root_obj.insert("charts".to_string(), JsonValue::from(charts));

    let root = JsonValue::Object(root_obj);

    write_json_value_with_key(writer, None, &root, 0)?;
    writeln!(writer)?;
    Ok(())
}

const CSV_HEADER_BASE: &str = "Title,Subtitle,Artist,Title trans,Subtitle trans,Artist trans,Length,BPM,BPM Tier,min_bpm,max_bpm,average_bpm,median bpm,BPM-data,offset,file_md5_hash,step_type,difficulty,rating,step_artist,tech_notation,sha1_hash,bpm_neutral_hash,total_arrows,left_arrows,down_arrows,up_arrows,right_arrows,total_steps,jumps,hands,holds,rolls,mines,lifts,fakes,stops_freezes,delays,warps,speeds,scrolls,total_streams,16th_streams,20th_streams,24th_streams,32nd_streams,total_breaks,sn_breaks,stream_percent,adj_stream_percent,max_nps,median_nps,matrix_rating";
const CSV_HEADER_PATTERN_1: &str = "mono_total,total_candles,left_foot_candles,right_foot_candles,candles_percent,total_mono,left_face_mono,right_face_mono,mono_percent,total_boxes,lr_boxes,ud_boxes,corner_boxes,ld_boxes,lu_boxes,rd_boxes,ru_boxes,total_anchors,left_anchors,down_anchors,up_anchors,right_anchors";
const CSV_HEADER_BREAKDOWNS: &str = "sn_detailed_breakdown,sn_partial_breakdown,sn_simple_breakdown,detailed_breakdown,partial_breakdown,simple_breakdown";
const CSV_HEADER_PATTERN_2: &str = "total_towers,lr_towers,ud_towers,corner_towers,ld_towers,lu_towers,rd_towers,ru_towers,total_triangles,ldl_triangles,lul_triangles,rdr_triangles,rur_triangles";
const CSV_HEADER_TECH: &str = "crossovers,half_crossovers,full_crossovers,footswitches,up_footswitches,down_footswitches,sideswitches,jacks,brackets,doublesteps";
const CSV_HEADER_PATTERN_3: &str = "total staircases,left_staircases,right_staircases,left_inv_staircases,right_inv_staircases,total_alt_staircases,left_alt_staircases,right_alt_staircases,left_inv_alt_staircases,right_inv_alt_staircases,total_double_staircases,left_double_staircases,right_double_staircases,left_inv_double_staircases,right_inv_double_staircases,total_sweeps,left_sweeps,right_sweeps,left_inv_sweeps,right_inv_sweeps,total_candle_sweeps,left_candle_sweeps,right_candle_sweeps,left_inv_candle_sweeps,right_inv_candle_sweeps,total copters,left_copters,right_copters,left_inv_copters,right_inv_copters,total_spirals,left_spirals,right_spirals,left_inv_spirals,right_inv_spirals,total_turbo_candles,left_turbo_candles,right_turbo_candles,left_inv_turbo_candles,right_inv_turbo_candles,total_hip_breakers,left_hip_breakers,right_hip_breakers,left_inv_hip_breakers,right_inv_hip_breakers,total_doritos,left_doritos,right_doritos,left_inv_doritos,right_inv_doritos,total_luchis,left_du_luchis,left_ud_luchis,right_du_luchis,right_ud_luchis";

fn write_csv_all<W: Write>(writer: &mut W, simfile: &SimfileSummary) -> io::Result<()> {
    let mut header: Vec<String> = CSV_HEADER_BASE.split(',').map(str::to_string).collect();
    if simfile.pattern_counts_enabled {
        header.extend(CSV_HEADER_PATTERN_1.split(',').map(str::to_string));
    }
    header.extend(CSV_HEADER_BREAKDOWNS.split(',').map(str::to_string));
    if simfile.pattern_counts_enabled {
        header.extend(CSV_HEADER_PATTERN_2.split(',').map(str::to_string));
    }
    if simfile.tech_counts_enabled {
        header.extend(CSV_HEADER_TECH.split(',').map(str::to_string));
    }
    if simfile.pattern_counts_enabled {
        header.extend(CSV_HEADER_PATTERN_3.split(',').map(str::to_string));
        if let Some(first_chart) = simfile.charts.first() {
            for cp in &first_chart.custom_patterns {
                header.push(format!("custom_pattern_{}", cp.pattern));
            }
        }
    }

    writeln!(writer, "{}", header.join(","))?;

    for chart in &simfile.charts {
        write_csv_row(writer, simfile, chart)?;
    }

    Ok(())
}

fn write_csv_row<W: Write>(
    writer: &mut W,
    simfile: &SimfileSummary,
    chart: &ChartSummary,
) -> io::Result<()> {
    fn esc_csv(s: &str) -> String {
        if s.contains('"') || s.contains(',') {
            format!("\"{}\"", s.replace('"', "\"\""))
        } else {
            s.to_string()
        }
    }

    fn push_str(out: &mut Vec<String>, value: &str) {
        out.push(esc_csv(value));
    }

    fn push_num<T: ToString>(out: &mut Vec<String>, value: T) {
        out.push(value.to_string());
    }

    let mut row = Vec::new();

    push_str(&mut row, &simfile.title_str);
    push_str(&mut row, &simfile.subtitle_str);
    push_str(&mut row, &simfile.artist_str);
    push_str(&mut row, &simfile.titletranslit_str);
    push_str(&mut row, &simfile.subtitletranslit_str);
    push_str(&mut row, &simfile.artisttranslit_str);
    push_str(&mut row, &format_duration(simfile.total_length));

    if (simfile.min_bpm - simfile.max_bpm).abs() < f64::EPSILON {
        push_num(&mut row, simfile.min_bpm);
    } else {
        push_str(
            &mut row,
            &format!("{}-{}", simfile.min_bpm, simfile.max_bpm),
        );
    }

    push_num(&mut row, simfile.min_bpm);
    push_num(&mut row, simfile.max_bpm);
    push_num(&mut row, simfile.average_bpm);
    push_num(&mut row, simfile.median_bpm);
    push_str(&mut row, &simfile.normalized_bpms);
    push_num(&mut row, simfile.offset);
    row.push(String::new());

    push_str(&mut row, &chart.step_type_str);
    push_str(&mut row, &chart.difficulty_str);
    push_str(&mut row, &chart.rating_str);
    push_str(&mut row, &chart.step_artist_str);
    push_str(&mut row, &chart.tech_notation_str);
    push_str(&mut row, &chart.short_hash);
    push_str(&mut row, &chart.bpm_neutral_hash);

    push_num(&mut row, chart.stats.total_arrows);
    push_num(&mut row, chart.stats.left);
    push_num(&mut row, chart.stats.down);
    push_num(&mut row, chart.stats.up);
    push_num(&mut row, chart.stats.right);

    let (mines_judgable, fakes) = chart_mine_fake_counts(chart);

    push_num(&mut row, chart.stats.total_steps);
    push_num(&mut row, chart.stats.jumps);
    push_num(&mut row, chart.stats.hands);
    push_num(&mut row, chart.stats.holds);
    push_num(&mut row, chart.stats.rolls);
    push_num(&mut row, mines_judgable);
    push_num(&mut row, chart.stats.lifts);
    push_num(&mut row, fakes);

    let allow_steps_timing = steps_timing_allowed(simfile.ssc_version, simfile.timing_format);
    let stops = chart_or_global(
        allow_steps_timing,
        chart.chart_has_own_timing,
        &chart.chart_stops,
        &simfile.normalized_stops,
    );
    let delays = chart_or_global(
        allow_steps_timing,
        chart.chart_has_own_timing,
        &chart.chart_delays,
        &simfile.normalized_delays,
    );
    let warps = chart_or_global(
        allow_steps_timing,
        chart.chart_has_own_timing,
        &chart.chart_warps,
        &simfile.normalized_warps,
    );
    let speeds = chart_or_global(
        allow_steps_timing,
        chart.chart_has_own_timing,
        &chart.chart_speeds,
        &simfile.normalized_speeds,
    );
    let scrolls = chart_or_global(
        allow_steps_timing,
        chart.chart_has_own_timing,
        &chart.chart_scrolls,
        &simfile.normalized_scrolls,
    );

    let stop_count = count_timing_segments(stops);
    let delay_count = count_timing_segments(delays);
    let warp_count = count_timing_segments(warps);
    let speed_count = count_gimmick_speed_segments(speeds);
    let scroll_count = count_gimmick_scroll_segments(scrolls);

    push_num(&mut row, stop_count);
    push_num(&mut row, delay_count);
    push_num(&mut row, warp_count);
    push_num(&mut row, speed_count);
    push_num(&mut row, scroll_count);

    let total_streams = chart.total_streams;
    let total_breaks = chart.stream_counts.total_breaks;
    let (_stream_percent, adj_stream_percent, _break_percent) =
        compute_stream_percentages(total_streams, total_breaks, chart.total_measures);

    push_num(&mut row, total_streams);
    push_num(&mut row, chart.stream_counts.run16_streams);
    push_num(&mut row, chart.stream_counts.run20_streams);
    push_num(&mut row, chart.stream_counts.run24_streams);
    push_num(&mut row, chart.stream_counts.run32_streams);
    push_num(&mut row, total_breaks);
    push_num(&mut row, chart.stream_counts.sn_breaks);
    push_num(&mut row, adj_stream_percent);
    row.push(String::new());

    push_num(&mut row, chart.max_nps);
    push_num(&mut row, chart.median_nps);
    push_num(&mut row, chart.matrix_rating);

    if simfile.pattern_counts_enabled {
        push_num(&mut row, chart.mono_total);

        let left_foot_candles = count(&chart.detected_patterns, PatternVariant::CandleLeft);
        let right_foot_candles = count(&chart.detected_patterns, PatternVariant::CandleRight);
        let total_candles = left_foot_candles + right_foot_candles;
        push_num(&mut row, total_candles);
        push_num(&mut row, left_foot_candles);
        push_num(&mut row, right_foot_candles);
        push_num(&mut row, chart.candle_percent);

        push_num(&mut row, chart.mono_total);
        push_num(&mut row, chart.facing_left);
        push_num(&mut row, chart.facing_right);
        push_num(&mut row, chart.mono_percent);

        let box_parts = compute_box_parts(&chart.detected_patterns);
        let corner_boxes = box_parts.ld + box_parts.lu + box_parts.rd + box_parts.ru;
        let total_boxes = box_parts.lr + box_parts.ud + corner_boxes;
        push_num(&mut row, total_boxes);
        push_num(&mut row, box_parts.lr);
        push_num(&mut row, box_parts.ud);
        push_num(&mut row, corner_boxes);
        push_num(&mut row, box_parts.ld);
        push_num(&mut row, box_parts.lu);
        push_num(&mut row, box_parts.rd);
        push_num(&mut row, box_parts.ru);

        let total_anchors =
            chart.anchor_left + chart.anchor_down + chart.anchor_up + chart.anchor_right;
        push_num(&mut row, total_anchors);
        push_num(&mut row, chart.anchor_left);
        push_num(&mut row, chart.anchor_down);
        push_num(&mut row, chart.anchor_up);
        push_num(&mut row, chart.anchor_right);
    }

    push_str(&mut row, &chart.sn_detailed_breakdown);
    push_str(&mut row, &chart.sn_partial_breakdown);
    push_str(&mut row, &chart.sn_simple_breakdown);
    push_str(&mut row, &chart.detailed_breakdown);
    push_str(&mut row, &chart.partial_breakdown);
    push_str(&mut row, &chart.simple_breakdown);

    if simfile.pattern_counts_enabled {
        let tower_parts = compute_tower_parts(&chart.detected_patterns);
        let corner_towers = tower_parts.ld + tower_parts.lu + tower_parts.rd + tower_parts.ru;
        let total_towers = tower_parts.lr + tower_parts.ud + corner_towers;
        push_num(&mut row, total_towers);
        push_num(&mut row, tower_parts.lr);
        push_num(&mut row, tower_parts.ud);
        push_num(&mut row, corner_towers);
        push_num(&mut row, tower_parts.ld);
        push_num(&mut row, tower_parts.lu);
        push_num(&mut row, tower_parts.rd);
        push_num(&mut row, tower_parts.ru);

        let triangle_parts = compute_triangle_parts(&chart.detected_patterns);
        let total_triangles =
            triangle_parts.ldl + triangle_parts.lul + triangle_parts.rdr + triangle_parts.rur;
        push_num(&mut row, total_triangles);
        push_num(&mut row, triangle_parts.ldl);
        push_num(&mut row, triangle_parts.lul);
        push_num(&mut row, triangle_parts.rdr);
        push_num(&mut row, triangle_parts.rur);
    }

    if simfile.tech_counts_enabled {
        push_num(&mut row, chart.tech_counts.crossovers);
        push_num(&mut row, chart.tech_counts.footswitches);
        push_num(&mut row, chart.tech_counts.up_footswitches);
        push_num(&mut row, chart.tech_counts.down_footswitches);
        push_num(&mut row, chart.tech_counts.sideswitches);
        push_num(&mut row, chart.tech_counts.jacks);
        push_num(&mut row, chart.tech_counts.brackets);
        push_num(&mut row, chart.tech_counts.doublesteps);
    }

    if simfile.pattern_counts_enabled {
        let stairs = compute_stair_parts(
            &chart.detected_patterns,
            PatternVariant::StaircaseLeft,
            PatternVariant::StaircaseRight,
            PatternVariant::StaircaseInvLeft,
            PatternVariant::StaircaseInvRight,
        );
        let total_staircases = stairs.left + stairs.right + stairs.left_inv + stairs.right_inv;
        push_num(&mut row, total_staircases);
        push_num(&mut row, stairs.left);
        push_num(&mut row, stairs.right);
        push_num(&mut row, stairs.left_inv);
        push_num(&mut row, stairs.right_inv);

        let alt_stairs = compute_stair_parts(
            &chart.detected_patterns,
            PatternVariant::AltStaircasesLeft,
            PatternVariant::AltStaircasesRight,
            PatternVariant::AltStaircasesInvLeft,
            PatternVariant::AltStaircasesInvRight,
        );
        let total_alt =
            alt_stairs.left + alt_stairs.right + alt_stairs.left_inv + alt_stairs.right_inv;

        let double_stairs = compute_stair_parts(
            &chart.detected_patterns,
            PatternVariant::DStaircaseLeft,
            PatternVariant::DStaircaseRight,
            PatternVariant::DStaircaseInvLeft,
            PatternVariant::DStaircaseInvRight,
        );
        let total_double = double_stairs.left
            + double_stairs.right
            + double_stairs.left_inv
            + double_stairs.right_inv;

        push_num(&mut row, total_alt);
        push_num(&mut row, alt_stairs.left);
        push_num(&mut row, alt_stairs.right);
        push_num(&mut row, alt_stairs.left_inv);
        push_num(&mut row, alt_stairs.right_inv);
        push_num(&mut row, total_double);
        push_num(&mut row, double_stairs.left);
        push_num(&mut row, double_stairs.right);
        push_num(&mut row, double_stairs.left_inv);
        push_num(&mut row, double_stairs.right_inv);

        let sweeps = compute_sweep_parts(
            &chart.detected_patterns,
            PatternVariant::SweepLeft,
            PatternVariant::SweepRight,
            PatternVariant::SweepInvLeft,
            PatternVariant::SweepInvRight,
        );
        let total_sweeps = sweeps.left + sweeps.right + sweeps.left_inv + sweeps.right_inv;
        push_num(&mut row, total_sweeps);
        push_num(&mut row, sweeps.left);
        push_num(&mut row, sweeps.right);
        push_num(&mut row, sweeps.left_inv);
        push_num(&mut row, sweeps.right_inv);

        let candle_sweeps = compute_sweep_parts(
            &chart.detected_patterns,
            PatternVariant::SweepCandleLeft,
            PatternVariant::SweepCandleRight,
            PatternVariant::SweepCandleInvLeft,
            PatternVariant::SweepCandleInvRight,
        );
        let total_candle_sweeps = candle_sweeps.left
            + candle_sweeps.right
            + candle_sweeps.left_inv
            + candle_sweeps.right_inv;
        push_num(&mut row, total_candle_sweeps);
        push_num(&mut row, candle_sweeps.left);
        push_num(&mut row, candle_sweeps.right);
        push_num(&mut row, candle_sweeps.left_inv);
        push_num(&mut row, candle_sweeps.right_inv);

        let copters = compute_simple_quad_parts(
            &chart.detected_patterns,
            PatternVariant::CopterLeft,
            PatternVariant::CopterRight,
            PatternVariant::CopterInvLeft,
            PatternVariant::CopterInvRight,
        );
        let total_copters = copters.a + copters.b + copters.c + copters.d;
        push_num(&mut row, total_copters);
        push_num(&mut row, copters.a);
        push_num(&mut row, copters.b);
        push_num(&mut row, copters.c);
        push_num(&mut row, copters.d);

        let spirals = compute_simple_quad_parts(
            &chart.detected_patterns,
            PatternVariant::SpiralLeft,
            PatternVariant::SpiralRight,
            PatternVariant::SpiralInvLeft,
            PatternVariant::SpiralInvRight,
        );
        let total_spirals = spirals.a + spirals.b + spirals.c + spirals.d;
        push_num(&mut row, total_spirals);
        push_num(&mut row, spirals.a);
        push_num(&mut row, spirals.b);
        push_num(&mut row, spirals.c);
        push_num(&mut row, spirals.d);

        let turbo_candles = compute_simple_quad_parts(
            &chart.detected_patterns,
            PatternVariant::TurboCandleLeft,
            PatternVariant::TurboCandleRight,
            PatternVariant::TurboCandleInvLeft,
            PatternVariant::TurboCandleInvRight,
        );
        let total_turbo_candles =
            turbo_candles.a + turbo_candles.b + turbo_candles.c + turbo_candles.d;
        push_num(&mut row, total_turbo_candles);
        push_num(&mut row, turbo_candles.a);
        push_num(&mut row, turbo_candles.b);
        push_num(&mut row, turbo_candles.c);
        push_num(&mut row, turbo_candles.d);

        let hip_breakers = compute_simple_quad_parts(
            &chart.detected_patterns,
            PatternVariant::HipBreakerLeft,
            PatternVariant::HipBreakerRight,
            PatternVariant::HipBreakerInvLeft,
            PatternVariant::HipBreakerInvRight,
        );
        let total_hip_breakers = hip_breakers.a + hip_breakers.b + hip_breakers.c + hip_breakers.d;
        push_num(&mut row, total_hip_breakers);
        push_num(&mut row, hip_breakers.a);
        push_num(&mut row, hip_breakers.b);
        push_num(&mut row, hip_breakers.c);
        push_num(&mut row, hip_breakers.d);

        let doritos = compute_simple_quad_parts(
            &chart.detected_patterns,
            PatternVariant::DoritoLeft,
            PatternVariant::DoritoRight,
            PatternVariant::DoritoInvLeft,
            PatternVariant::DoritoInvRight,
        );
        let total_doritos = doritos.a + doritos.b + doritos.c + doritos.d;
        push_num(&mut row, total_doritos);
        push_num(&mut row, doritos.a);
        push_num(&mut row, doritos.b);
        push_num(&mut row, doritos.c);
        push_num(&mut row, doritos.d);

        let luchis = compute_simple_quad_parts(
            &chart.detected_patterns,
            PatternVariant::LuchiLeftDU,
            PatternVariant::LuchiLeftUD,
            PatternVariant::LuchiRightDU,
            PatternVariant::LuchiRightUD,
        );
        let total_luchis = luchis.a + luchis.b + luchis.c + luchis.d;
        push_num(&mut row, total_luchis);
        push_num(&mut row, luchis.a);
        push_num(&mut row, luchis.b);
        push_num(&mut row, luchis.c);
        push_num(&mut row, luchis.d);

        for cp in &chart.custom_patterns {
            push_num(&mut row, cp.count);
        }
    }

    writeln!(writer, "{}", row.join(","))?;
    Ok(())
}

use std::fmt::{self, Write as _};
use std::io::{self, Write};
use std::sync::Arc;
#[cfg(not(target_arch = "wasm32"))]
use std::time::Duration;
#[cfg(target_arch = "wasm32")]
use web_time::Duration;

use serde_json::Number as JsonNumber;

use crate::bpm::{actual_bpm_range_raw_f32, normalize_float_digits, resolve_display_bpm};
use crate::math::{round_dp, round_sig_figs_6, round_sig_figs_itg, roundtrip_bpm_itg};
use crate::patterns::{CustomPatternSummary, PatternCounts, PatternVariant};
use crate::stats::{
    ArrowStats, RADAR_CATEGORY_COUNT, StreamCounts, visit_measure_spacing, visit_stream_sequences,
};
use crate::step_parity::{RowAnnotation, TechCounts};
use crate::timing::{
    SpeedUnit, TimingFormat, TimingSegments, beat_to_note_row, format_bpm_segments_f32_like_itg,
    native_bpms_display, normalize_scrolls_like_itg, normalize_speeds_like_itg, note_row_to_beat,
    steps_timing_allowed,
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

#[inline(always)]
#[allow(clippy::cast_possible_truncation)] // ITG serializes timing values through f32.
fn timing_fixed_6(value: f64) -> f64 {
    let value = f64::from(value as f32);
    if !value.is_finite() || value.abs() >= 8_388_608.0 {
        return value;
    }
    (value * 1_000_000.0).round_ties_even() / 1_000_000.0
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
#[derive(Debug, Clone)]
pub struct ChartSummary {
    pub step_type_str: String,
    pub step_artist_str: String,
    pub description_str: String,
    pub chart_name_str: String,
    pub chart_style_str: String,
    pub difficulty_str: String,
    pub rating_str: String,
    pub matrix_rating: f64,
    pub matrix_profile: crate::matrix::MatrixProfile,
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
    pub note_annotations: Option<Vec<RowAnnotation>>,
    pub custom_patterns: Vec<CustomPatternSummary>,
    pub short_hash: String,
    pub bpm_neutral_hash: String,
    pub elapsed: Duration,
    pub measure_densities: Vec<usize>,
    pub measure_nps_vec: Vec<f64>,
    pub row_to_beat: Vec<f32>,
    pub timing_segments: Arc<TimingSegments>,
    pub chart_offset_seconds: f64,
    pub chart_has_own_timing: bool,
    pub minimized_note_data: Vec<u8>,
    pub music_path: String,
    pub chart_attacks: Option<String>,
    // TODO: remove this property, don't populate chart_attacks from global attacks,
    // and update deadsync_gameplay to combine global & chart attacks like SM5 / ITGm
    pub chart_has_own_attacks: bool,
    pub chart_stops: Option<String>,
    pub chart_speeds: Option<String>,
    pub chart_scrolls: Option<String>,
    pub chart_bpms: Option<String>,
    pub chart_bpms_norm: Option<String>,
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
#[derive(Debug, Clone)] // Add Debug for easier use in the engine
pub struct SimfileSummary {
    pub title_str: String,
    pub subtitle_str: String,
    pub artist_str: String,
    pub genre_str: String,
    pub titletranslit_str: String,
    pub subtitletranslit_str: String,
    pub artisttranslit_str: String,
    pub origin_str: String,
    pub credit_str: String,
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
    pub normalized_bgchanges: String,
    pub normalized_fgchanges: String,
    pub normalized_keysounds: String,
    pub normalized_attacks: String,
    pub last_second_hint: Option<f64>,
    pub ssc_version: f32,
    pub timing_format: TimingFormat,
    pub banner_path: String,
    pub background_path: String,
    pub cdtitle_path: String,
    pub jacket_path: String,
    pub music_path: String,
    pub previewvid_path: String,
    pub cdimage_path: String,
    pub discimage_path: String,
    pub lyrics_path: String,
    pub selectable: bool,
    pub display_bpm_str: String,
    pub sample_start: f64,
    pub sample_length: f64,
    pub min_bpm: f64,
    pub max_bpm: f64,
    pub normalized_warps: String,
    pub median_bpm: f64,
    pub average_bpm: f64,
    pub total_length: i32,
    pub global_timing_segments: Arc<TimingSegments>,
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

/// Writes a simfile report in `mode`.
///
/// # Errors
///
/// Returns the first I/O error produced by `writer`.
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

/// Writes a course report in `mode`.
///
/// # Errors
///
/// Returns the first I/O error produced by `writer`.
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

struct DurationDisplay(i32);

impl fmt::Display for DurationDisplay {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let minutes = self.0 / 60;
        let seconds = self.0 % 60;
        write!(formatter, "{minutes}m {seconds:02}s")
    }
}

const fn format_duration(seconds: i32) -> DurationDisplay {
    DurationDisplay(seconds)
}

fn dummy_simfile_for_course(course: &CourseSummary) -> SimfileSummary {
    SimfileSummary {
        title_str: course.course.clone(),
        subtitle_str: String::new(),
        artist_str: String::new(),
        genre_str: String::new(),
        titletranslit_str: String::new(),
        subtitletranslit_str: String::new(),
        artisttranslit_str: String::new(),
        origin_str: String::new(),
        credit_str: String::new(),
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
        normalized_bgchanges: String::new(),
        normalized_fgchanges: String::new(),
        normalized_keysounds: String::new(),
        normalized_attacks: String::new(),
        last_second_hint: None,
        ssc_version: f32::NAN,
        timing_format: TimingFormat::Sm,
        banner_path: String::new(),
        background_path: String::new(),
        cdtitle_path: String::new(),
        jacket_path: String::new(),
        music_path: String::new(),
        previewvid_path: String::new(),
        cdimage_path: String::new(),
        discimage_path: String::new(),
        lyrics_path: String::new(),
        selectable: true,
        display_bpm_str: String::new(),
        sample_start: 0.0,
        sample_length: 0.0,
        min_bpm: 0.0,
        max_bpm: 0.0,
        normalized_warps: String::new(),
        median_bpm: 0.0,
        average_bpm: 0.0,
        total_length: course.total_length,
        global_timing_segments: Arc::default(),
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
fn write_json_course_hashes<W: Write>(writer: &mut W, hashes: &[String]) -> io::Result<()> {
    write_json_scalar_iter(writer, hashes, |writer, hash| {
        write_json_string(writer, hash)
    })
}

fn write_json_course_entry<W: Write>(
    writer: &mut W,
    entry: &CourseEntrySummary,
    indent: usize,
) -> io::Result<()> {
    let mut object = JsonObjectWriter::new(writer, indent)?;
    object.field_string("song", &entry.song)?;
    object.field_string("song_dir", &entry.song_dir)?;
    object.field_string("step_type", &entry.step_type)?;
    object.field_string("difficulty", &entry.difficulty)?;
    object.field_string("rating", &entry.rating)?;
    object.field_string("sha1", &entry.sha1)?;
    object.field_string("bpm_neutral_sha1", &entry.bpm_neutral_sha1)?;
    object.finish()
}

fn write_json_course<W: Write>(course: &CourseSummary, writer: &mut W) -> io::Result<()> {
    let mut root = JsonObjectWriter::new(writer, 0)?;
    root.field_string("course", &course.course)?;
    root.field_string("course_difficulty", &course.course_difficulty)?;
    root.field_string("step_type", &course.step_type)?;
    root.field_display_string("length", course.total_length)?;
    root.field_with("sha1_hashes", |writer, _| {
        write_json_course_hashes(writer, &course.sha1_hashes)
    })?;
    root.field_with("bpm_neutral_sha1_hashes", |writer, _| {
        write_json_course_hashes(writer, &course.bpm_neutral_sha1_hashes)
    })?;
    root.field_with("entries", |writer, indent| {
        write_json_multiline_array(
            writer,
            course.entries.len(),
            indent,
            |writer, index, item_indent| {
                write_json_course_entry(writer, &course.entries[index], item_indent)
            },
        )
    })?;

    let dummy = dummy_simfile_for_course(course);
    root.field_with("chart", |writer, indent| {
        write_json_chart(writer, &course.chart, &dummy, indent)
    })?;
    root.finish()?;
    writeln!(writer)
}
const COURSE_CSV_HEADER: &[u8] = concat!(
    "Course,Difficulty,StepsType,Length,Entries,sha1_hashes,",
    "bpm_neutral_sha1_hashes,total_arrows,total_steps,jumps,hands,",
    "holds,rolls,mines,lifts,fakes,total_streams,total_breaks,max_nps,",
    "median_nps"
)
.as_bytes();

fn csv_hashes_len(hashes: &[String]) -> usize {
    hashes
        .iter()
        .map(String::len)
        .sum::<usize>()
        .saturating_add(hashes.len().saturating_sub(1))
}

fn buffer_csv_hashes(buffer: &mut String, hashes: &[String]) {
    for (index, hash) in hashes.iter().enumerate() {
        if index != 0 {
            buffer.push('|');
        }
        buffer.push_str(hash);
    }
}

fn write_csv_course<W: Write>(writer: &mut W, course: &CourseSummary) -> io::Result<()> {
    writer.write_all(COURSE_CSV_HEADER)?;
    writer.write_all(b"\n")?;
    write!(
        writer,
        "{},{},{},",
        course.course, course.course_difficulty, course.step_type
    )?;
    write_duration(writer, course.total_length)?;
    write!(writer, ",{},", course.entries.len())?;
    let hash_capacity =
        csv_hashes_len(&course.sha1_hashes).max(csv_hashes_len(&course.bpm_neutral_sha1_hashes));
    let mut hash_buffer = String::with_capacity(hash_capacity);
    buffer_csv_hashes(&mut hash_buffer, &course.sha1_hashes);
    writer.write_all(hash_buffer.as_bytes())?;
    writer.write_all(b",")?;
    hash_buffer.clear();
    buffer_csv_hashes(&mut hash_buffer, &course.bpm_neutral_sha1_hashes);
    writer.write_all(hash_buffer.as_bytes())?;

    let chart = &course.chart;
    writeln!(
        writer,
        ",{},{},{},{},{},{},{},{},{},{},{},{:.6},{:.2}",
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
    )
}

const fn count(counts: &PatternCounts, variant: PatternVariant) -> u32 {
    counts[variant as usize]
}

fn chart_or_global<'a>(
    allow_chart: bool,
    chart_has_own_timing: bool,
    chart_value: Option<&'a str>,
    global_value: &'a str,
) -> Option<&'a str> {
    if allow_chart && chart_has_own_timing {
        return chart_value.filter(|s| !s.is_empty());
    }

    if allow_chart
        && let Some(s) = chart_value
        && !s.is_empty()
    {
        return Some(s);
    }

    if global_value.is_empty() {
        None
    } else {
        Some(global_value)
    }
}

fn add_segment_by<T>(
    segments: &mut Vec<T>,
    mut segment: T,
    get_beat: impl Fn(&T) -> f64,
    same_value: impl Fn(&T, &T) -> bool,
    set_beat: impl Fn(&mut T, f64),
) {
    let row = beat_to_note_row(get_beat(&segment));
    let beat = note_row_to_beat(row);
    set_beat(&mut segment, beat);
    if segments.is_empty() {
        segments.push(segment);
        return;
    }

    let pos = segments.partition_point(|item| beat_to_note_row(get_beat(item)) <= row);
    let idx = if pos == 0 { 0 } else { pos - 1 };
    let b_on_same_row = beat_to_note_row(get_beat(&segments[idx])) == row;
    let prev_idx = if b_on_same_row && idx > 0 {
        idx - 1
    } else {
        idx
    };

    if idx + 1 < segments.len() {
        let next_idx = idx + 1;
        if same_value(&segments[next_idx], &segment) {
            if same_value(&segments[prev_idx], &segment) {
                segments.remove(next_idx);
                if prev_idx != idx {
                    segments.remove(idx);
                }
                return;
            }
            set_beat(&mut segments[next_idx], beat);
            if prev_idx != idx {
                segments.remove(idx);
            }
            return;
        }
        if same_value(&segments[prev_idx], &segment) {
            if prev_idx != idx {
                segments.remove(idx);
            }
            return;
        }
    } else if same_value(&segments[prev_idx], &segment) {
        if prev_idx != idx {
            segments.remove(idx);
        }
        return;
    }

    if b_on_same_row && same_value(&segments[idx], &segment) {
        return;
    }

    if b_on_same_row {
        segments[idx] = segment;
    } else {
        let insert_pos = segments.partition_point(|item| beat_to_note_row(get_beat(item)) <= row);
        segments.insert(insert_pos, segment);
    }
}

fn add_indefinite_segment<T: PartialEq>(segments: &mut Vec<(f64, T)>, beat: f64, value: T) {
    add_segment_by(
        segments,
        (beat, value),
        |segment| segment.0,
        |left, right| left.1 == right.1,
        |segment, beat| segment.0 = beat,
    );
}

fn tidy_in_place<T>(mut segments: Vec<T>, mut add: impl FnMut(&mut Vec<T>, T)) -> Vec<T> {
    let input_len = segments.len();
    let input = segments.as_mut_ptr();
    // SAFETY: Each consumed entry adds at most one output, so before reading
    // index `i`, output occupies only slots below `i`. Reading `i` first lets
    // the normalizer reuse that slot without touching unread entries, and the
    // original capacity prevents reallocation from invalidating `input`.
    unsafe { segments.set_len(0) };
    for index in 0..input_len {
        debug_assert!(segments.len() <= index);
        // SAFETY: `index` addresses an initialized, unread element from the
        // original length. It is read before output may overwrite this slot.
        let segment = unsafe { input.add(index).read() };
        add(&mut segments, segment);
    }
    segments
}

fn tidy_indefinite_segments<T: PartialEq>(segments: Vec<(f64, T)>) -> Vec<(f64, T)> {
    tidy_in_place(segments, |out, (beat, value)| {
        add_indefinite_segment(out, beat, value);
    })
}

fn tidy_timing_triples(segments: Vec<(f64, i32, i32)>) -> Vec<(f64, i32, i32)> {
    tidy_in_place(segments, |out, segment| {
        add_segment_by(
            out,
            segment,
            |segment| segment.0,
            |left, right| left.1 == right.1 && left.2 == right.2,
            |segment, beat| segment.0 = beat,
        );
    })
}

fn parse_tickcounts_with(
    opt: Option<&str>,
    capacity: usize,
    tidy: impl FnOnce(Vec<(f64, i32)>) -> Vec<(f64, i32)>,
) -> Vec<(f64, i32)> {
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
        if raw.is_empty() {
            raw.reserve(capacity);
        }
        raw.push((beat, count));
    }

    if raw.is_empty() {
        return vec![(0.0, 4)];
    }

    tidy(raw)
}

fn estimate_text_segments(text: &str, bytes_per_segment: usize) -> usize {
    // Avoid a delimiter-count pass while bounding corrupt-input over-reservation.
    text.len().div_ceil(bytes_per_segment).clamp(4, 4_096)
}

fn parse_timing_triples(
    opt: Option<&str>,
    capacity: usize,
    default: (f64, i32, i32),
    prefix_default: bool,
) -> Vec<(f64, i32, i32)> {
    let Some(text) = opt else {
        return vec![default];
    };

    let mut raw = Vec::new();
    for segment in text
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
    {
        let mut parts = segment.split('=');
        let (Some(beat), Some(first), Some(second)) = (parts.next(), parts.next(), parts.next())
        else {
            continue;
        };
        let (Ok(beat), Ok(first), Ok(second)) = (
            beat.trim().parse::<f64>(),
            first.trim().parse::<i32>(),
            second.trim().parse::<i32>(),
        ) else {
            continue;
        };
        if raw.is_empty() {
            raw.reserve(capacity);
        }
        raw.push((beat, first, second));
    }
    if raw.is_empty() {
        return vec![default];
    }
    if prefix_default && beat_to_note_row(raw[0].0) > 0 {
        raw.insert(0, default);
    }
    tidy_timing_triples(raw)
}

fn parse_time_signatures(opt: Option<&str>) -> Vec<(f64, i32, i32)> {
    let capacity = opt.map_or(0, |text| estimate_text_segments(text, 7) + 1);
    parse_timing_triples(opt, capacity, (0.0, 4, 4), true)
}

fn parse_tickcounts(opt: Option<&str>) -> Vec<(f64, i32)> {
    let capacity = opt.map_or(0, |text| estimate_text_segments(text, 6));
    parse_tickcounts_with(opt, capacity, tidy_indefinite_segments)
}

fn parse_combos(opt: Option<&str>) -> Vec<(f64, i32, i32)> {
    let capacity = opt.map_or(0, |text| estimate_text_segments(text, 8));
    parse_timing_triples(opt, capacity, (0.0, 1, 1), false)
}

struct NormalizedTimingTables {
    time_signatures: Vec<(f64, i32, i32)>,
    labels: Vec<(f64, String)>,
    tickcounts: Vec<(f64, i32)>,
    combos: Vec<(f64, i32, i32)>,
    speeds: Vec<(f64, f64, f64, i32)>,
    scrolls: Vec<(f64, f64)>,
}

struct TimingTextTables {
    time_signatures: Vec<(f64, i32, i32)>,
    labels: Vec<(f64, String)>,
    tickcounts: Vec<(f64, i32)>,
    combos: Vec<(f64, i32, i32)>,
}

fn build_timing_text_tables(chart: &ChartSummary, simfile: &SimfileSummary) -> TimingTextTables {
    let allow_steps_timing = steps_timing_allowed(simfile.ssc_version, simfile.timing_format);
    let finalize = |value: f64| timing_fixed_6(value);

    let mut time_signatures = parse_time_signatures(chart_or_global(
        allow_steps_timing,
        chart.chart_has_own_timing,
        chart.chart_time_signatures.as_deref(),
        &simfile.normalized_time_signatures,
    ));
    for (beat, _, _) in &mut time_signatures {
        *beat = finalize(*beat);
    }
    let mut labels = parse_labels(chart_or_global(
        allow_steps_timing,
        chart.chart_has_own_timing,
        chart.chart_labels.as_deref(),
        &simfile.normalized_labels,
    ));
    for (beat, _) in &mut labels {
        *beat = finalize(*beat);
    }
    let mut tickcounts = parse_tickcounts(chart_or_global(
        allow_steps_timing,
        chart.chart_has_own_timing,
        chart.chart_tickcounts.as_deref(),
        &simfile.normalized_tickcounts,
    ));
    for (beat, _) in &mut tickcounts {
        *beat = finalize(*beat);
    }
    let mut combos = parse_combos(chart_or_global(
        allow_steps_timing,
        chart.chart_has_own_timing,
        chart.chart_combos.as_deref(),
        &simfile.normalized_combos,
    ));
    for (beat, _, _) in &mut combos {
        *beat = finalize(*beat);
    }

    TimingTextTables {
        time_signatures,
        labels,
        tickcounts,
        combos,
    }
}

fn build_normalized_timing_tables(
    chart: &ChartSummary,
    simfile: &SimfileSummary,
) -> NormalizedTimingTables {
    let timing = &chart.timing_segments;
    let finalize = |value: f64| timing_fixed_6(value);
    let mut speeds = normalize_speeds_like_itg(
        timing
            .speeds
            .iter()
            .map(|(beat, ratio, delay, unit)| {
                let unit = i32::from(*unit == SpeedUnit::Seconds);
                (f64::from(*beat), f64::from(*ratio), f64::from(*delay), unit)
            })
            .collect(),
    );
    for (beat, ratio, delay, _) in &mut speeds {
        *beat = finalize(*beat);
        *ratio = finalize(*ratio);
        *delay = finalize(*delay);
    }
    let mut scrolls = normalize_scrolls_like_itg(
        timing
            .scrolls
            .iter()
            .map(|(beat, ratio)| (f64::from(*beat), f64::from(*ratio)))
            .collect(),
    );
    for (beat, ratio) in &mut scrolls {
        *beat = finalize(*beat);
        *ratio = finalize(*ratio);
    }
    let TimingTextTables {
        time_signatures,
        labels,
        tickcounts,
        combos,
    } = build_timing_text_tables(chart, simfile);

    NormalizedTimingTables {
        time_signatures,
        labels,
        tickcounts,
        combos,
        speeds,
        scrolls,
    }
}

#[must_use]
pub fn build_timing_snapshot(chart: &ChartSummary, simfile: &SimfileSummary) -> TimingSnapshot {
    let timing = &chart.timing_segments;
    // The local harness serializes timing tables as ITG float values with fixed
    // six decimal places, not six significant digits.
    let finalize = |value: f64| timing_fixed_6(value);
    let bpms_formatted = format_bpm_segments_f32_like_itg(&timing.bpms);
    let (bpm_min_raw, bpm_max_raw) = actual_bpm_range_raw_f32(&timing.bpms);
    let bpms = timing
        .bpms
        .iter()
        .map(|(beat, bpm)| {
            (
                finalize(f64::from(*beat)),
                finalize(roundtrip_bpm_itg(f64::from(*bpm))),
            )
        })
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
    let fakes = timing
        .fakes
        .iter()
        .map(|(beat, length)| (finalize(f64::from(*beat)), finalize(f64::from(*length))))
        .collect();

    let tables = build_normalized_timing_tables(chart, simfile);

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
        time_signatures: tables.time_signatures,
        warps,
        labels: tables.labels,
        tickcounts: tables.tickcounts,
        combos: tables.combos,
        speeds: tables.speeds,
        scrolls: tables.scrolls,
        fakes,
    }
}

fn parse_labels_with(
    opt: Option<&str>,
    capacity: usize,
    tidy: impl FnOnce(Vec<(f64, String)>) -> Vec<(f64, String)>,
) -> Vec<(f64, String)> {
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
        if raw.is_empty() {
            raw.reserve(capacity);
        }
        raw.push((beat, label));
    }

    if raw.is_empty() {
        return vec![(0.0, "Song Start".to_string())];
    }

    tidy(raw)
}

fn parse_labels(opt: Option<&str>) -> Vec<(f64, String)> {
    let capacity = opt.map_or(0, |text| estimate_text_segments(text, 16));
    parse_labels_with(opt, capacity, tidy_indefinite_segments)
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
        chart.chart_stops.as_deref(),
        &simfile.normalized_stops,
    );
    let delays = chart_or_global(
        allow_steps_timing,
        chart.chart_has_own_timing,
        chart.chart_delays.as_deref(),
        &simfile.normalized_delays,
    );
    let warps = chart_or_global(
        allow_steps_timing,
        chart.chart_has_own_timing,
        chart.chart_warps.as_deref(),
        &simfile.normalized_warps,
    );
    let speeds = chart_or_global(
        allow_steps_timing,
        chart.chart_has_own_timing,
        chart.chart_speeds.as_deref(),
        &simfile.normalized_speeds,
    );
    let scrolls = chart_or_global(
        allow_steps_timing,
        chart.chart_has_own_timing,
        chart.chart_scrolls.as_deref(),
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

fn write_chart_header<W: Write>(writer: &mut W, chart: &ChartSummary) -> io::Result<()> {
    const DASHES: &[u8; 64] = b"----------------------------------------------------------------";

    writeln!(
        writer,
        "\n{} {} : {}",
        chart.difficulty_str, chart.rating_str, chart.step_artist_str
    )?;
    let mut remaining =
        chart.difficulty_str.len() + chart.rating_str.len() + chart.step_artist_str.len() + 4;
    while remaining >= DASHES.len() {
        writer.write_all(DASHES)?;
        remaining -= DASHES.len();
    }
    writer.write_all(&DASHES[..remaining])?;
    writer.write_all(b"\n")
}

fn write_pretty_all<W: Write>(writer: &mut W, simfile: &SimfileSummary) -> io::Result<()> {
    writeln!(writer, "--- Song Details ---")?;
    write!(writer, "Title: {}", simfile.title_str)?;
    if !simfile.subtitle_str.is_empty() {
        write!(writer, " {}", simfile.subtitle_str)?;
    }
    writeln!(writer, " by {}", simfile.artist_str)?;
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

// Report writers stay linear so output order remains obvious and allocation-free.
#[allow(clippy::too_many_lines)]
fn write_pretty_chart<W: Write>(
    writer: &mut W,
    chart: &ChartSummary,
    simfile: &SimfileSummary,
) -> io::Result<()> {
    write_chart_header(writer, chart)?;

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
    writeln!(writer, "Total Break: {total_break} ({break_percent:.2}%)")?;

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

// Report writers stay linear so output order remains obvious and allocation-free.
#[allow(clippy::too_many_lines)]
fn write_full_chart<W: Write>(
    writer: &mut W,
    chart: &ChartSummary,
    simfile: &SimfileSummary,
) -> io::Result<()> {
    write_chart_header(writer, chart)?;

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
    writeln!(writer, "Total Break: {total_break} ({break_percent:.2}%)")?;

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

// Report writers stay linear so output order remains obvious and allocation-free.
#[allow(clippy::too_many_lines)]
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
            let mut buf = *b"\\u0000";
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

struct StackText<const N: usize> {
    bytes: [u8; N],
    len: usize,
}

impl<const N: usize> StackText<N> {
    const fn new() -> Self {
        Self {
            bytes: [0; N],
            len: 0,
        }
    }

    fn as_bytes(&self) -> &[u8] {
        &self.bytes[..self.len]
    }
}

impl<const N: usize> std::fmt::Write for StackText<N> {
    fn write_str(&mut self, value: &str) -> std::fmt::Result {
        if value.len() > N - self.len {
            return Err(std::fmt::Error);
        }
        let end = self.len + value.len();
        self.bytes[self.len..end].copy_from_slice(value.as_bytes());
        self.len = end;
        Ok(())
    }
}

fn write_json_native_bpms<W: Write>(writer: &mut W, bpms: &[(f32, f32)]) -> io::Result<()> {
    let mut buffer = StackText::<16_384>::new();
    if write!(&mut buffer, "{}", native_bpms_display(bpms)).is_err() {
        writer.write_all(b"\"")?;
        write!(writer, "{}", native_bpms_display(bpms))?;
        return writer.write_all(b"\"");
    }
    writer.write_all(b"\"")?;
    writer.write_all(buffer.as_bytes())?;
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
            Some("offset") => write!(writer, "{f:.3}"),
            None
            | Some(
                "beat0_offset_seconds"
                | "beat0_group_offset_seconds"
                | "duration_seconds"
                | "max_nps"
                | "bpm_min"
                | "bpm_max"
                | "display_bpm_min"
                | "display_bpm_max"
                | "bpm",
            ) => write!(writer, "{f}"),
            _ => write!(writer, "{f:.2}"),
        }
    } else {
        write!(writer, "0")
    }
}
struct JsonObjectWriter<'a, W> {
    writer: &'a mut W,
    indent: usize,
    first: bool,
}

impl<'a, W: Write> JsonObjectWriter<'a, W> {
    fn new(writer: &'a mut W, indent: usize) -> io::Result<Self> {
        writer.write_all(b"{\n")?;
        Ok(Self {
            writer,
            indent,
            first: true,
        })
    }

    fn field_with(
        &mut self,
        key: &str,
        write_value: impl FnOnce(&mut W, usize) -> io::Result<()>,
    ) -> io::Result<()> {
        if !self.first {
            self.writer.write_all(b",\n")?;
        }
        self.first = false;
        let value_indent = self.indent + 2;
        write_indent(self.writer, value_indent)?;
        write_json_string(self.writer, key)?;
        self.writer.write_all(b": ")?;
        write_value(self.writer, value_indent)
    }
    fn field_string(&mut self, key: &str, value: &str) -> io::Result<()> {
        self.field_with(key, |writer, _| write_json_string(writer, value))
    }

    fn field_display_string(&mut self, key: &str, value: impl std::fmt::Display) -> io::Result<()> {
        self.field_with(key, |writer, _| {
            writer.write_all(b"\"")?;
            write!(writer, "{value}")?;
            writer.write_all(b"\"")
        })
    }

    fn field_f64(&mut self, key: &str, value: f64) -> io::Result<()> {
        self.field_with(key, |writer, _| {
            let Some(number) = JsonNumber::from_f64(value) else {
                return writer.write_all(b"null");
            };
            write_json_number_for_key(writer, Some(key), &number)
        })
    }

    fn field_u32(&mut self, key: &str, value: u32) -> io::Result<()> {
        self.field_with(key, |writer, _| write_json_raw_u32(writer, value))
    }

    fn field_bool(&mut self, key: &str, value: bool) -> io::Result<()> {
        self.field_with(key, |writer, _| {
            writer.write_all(if value { b"true" } else { b"false" })
        })
    }

    fn finish(self) -> io::Result<()> {
        if !self.first {
            self.writer.write_all(b"\n")?;
        }
        write_indent(self.writer, self.indent)?;
        self.writer.write_all(b"}")
    }
}

fn write_json_u32_object<W: Write>(
    writer: &mut W,
    indent: usize,
    fields: &[(&str, u32)],
) -> io::Result<()> {
    let mut object = JsonObjectWriter::new(writer, indent)?;
    for &(key, value) in fields {
        object.field_u32(key, value)?;
    }
    object.finish()
}

fn write_json_raw_u32<W: Write>(writer: &mut W, mut value: u32) -> io::Result<()> {
    let mut buffer = [0u8; 10];
    let mut start = buffer.len();
    loop {
        start -= 1;
        buffer[start] = b'0' + (value % 10) as u8;
        value /= 10;
        if value == 0 {
            return writer.write_all(&buffer[start..]);
        }
    }
}

fn write_json_chart_info<W: Write>(
    writer: &mut W,
    chart: &ChartSummary,
    indent: usize,
) -> io::Result<()> {
    let mut object = JsonObjectWriter::new(writer, indent)?;
    object.field_string("step_type", &chart.step_type_str)?;
    object.field_string("difficulty", &chart.difficulty_str)?;
    object.field_f64("tier_bpm", chart.tier_bpm)?;
    object.field_string("rating", &chart.rating_str)?;
    object.field_f64("matrix_rating", chart.matrix_rating)?;
    object.field_string("step_artists", &chart.step_artist_str)?;
    object.field_string("tech_notation", &chart.tech_notation_str)?;
    object.field_string("sha1", &chart.short_hash)?;
    object.field_string("bpm_neutral_sha1", &chart.bpm_neutral_hash)?;
    object.finish()
}

fn write_json_arrow_stats<W: Write>(
    writer: &mut W,
    chart: &ChartSummary,
    indent: usize,
) -> io::Result<()> {
    let (mines_judgable, _) = chart_mine_fake_counts(chart);
    write_json_u32_object(
        writer,
        indent,
        &[
            ("total_arrows", chart.stats.total_arrows),
            ("left_arrows", chart.stats.left),
            ("down_arrows", chart.stats.down),
            ("up_arrows", chart.stats.up),
            ("right_arrows", chart.stats.right),
            ("total_steps", chart.stats.total_steps),
            ("jumps", chart.stats.jumps),
            ("hands", chart.stats.hands),
            ("holds", chart.stats.holds),
            ("rolls", chart.stats.rolls),
            ("mines", mines_judgable),
        ],
    )
}

fn write_json_gimmicks<W: Write>(
    writer: &mut W,
    chart: &ChartSummary,
    simfile: &SimfileSummary,
    indent: usize,
) -> io::Result<()> {
    let (_, fakes) = chart_mine_fake_counts(chart);
    let allow_steps_timing = steps_timing_allowed(simfile.ssc_version, simfile.timing_format);
    let stops = chart_or_global(
        allow_steps_timing,
        chart.chart_has_own_timing,
        chart.chart_stops.as_deref(),
        &simfile.normalized_stops,
    );
    let delays = chart_or_global(
        allow_steps_timing,
        chart.chart_has_own_timing,
        chart.chart_delays.as_deref(),
        &simfile.normalized_delays,
    );
    let warps = chart_or_global(
        allow_steps_timing,
        chart.chart_has_own_timing,
        chart.chart_warps.as_deref(),
        &simfile.normalized_warps,
    );
    let speeds = chart_or_global(
        allow_steps_timing,
        chart.chart_has_own_timing,
        chart.chart_speeds.as_deref(),
        &simfile.normalized_speeds,
    );
    let scrolls = chart_or_global(
        allow_steps_timing,
        chart.chart_has_own_timing,
        chart.chart_scrolls.as_deref(),
        &simfile.normalized_scrolls,
    );
    write_json_u32_object(
        writer,
        indent,
        &[
            ("lifts", chart.stats.lifts),
            ("fakes", fakes),
            ("stops_freezes", count_timing_segments(stops)),
            ("speeds", count_gimmick_speed_segments(speeds)),
            ("scrolls", count_gimmick_scroll_segments(scrolls)),
            ("delays", count_timing_segments(delays)),
            ("warps", count_timing_segments(warps)),
        ],
    )
}

fn write_json_stream_segment<W: Write>(
    writer: &mut W,
    indent: usize,
    start: usize,
    end: usize,
    is_break: bool,
) -> io::Result<()> {
    let mut object = JsonObjectWriter::new(writer, indent)?;
    object.field_u32("stream_start", start as u32)?;
    object.field_u32("stream_end", end as u32)?;
    object.field_bool("is_break", is_break)?;
    object.finish()
}

fn write_json_stream_sequences<W: Write>(
    writer: &mut W,
    measures: &[usize],
    indent: usize,
) -> io::Result<()> {
    writer.write_all(b"[")?;
    let item_indent = indent + 2;
    let mut first = true;
    visit_stream_sequences(measures, |segment| {
        if first {
            writer.write_all(b"\n")?;
            first = false;
        } else {
            writer.write_all(b",\n")?;
        }
        write_indent(writer, item_indent)?;
        write_json_stream_segment(
            writer,
            item_indent,
            segment.start,
            segment.end,
            segment.is_break,
        )
    })?;
    if first {
        return writer.write_all(b"]");
    }
    writer.write_all(b"\n")?;
    write_indent(writer, indent)?;
    writer.write_all(b"]")
}

fn write_json_stream_info<W: Write>(
    writer: &mut W,
    chart: &ChartSummary,
    indent: usize,
) -> io::Result<()> {
    let total_stream = chart.total_streams;
    let total_break = chart.stream_counts.total_breaks;
    let (stream_percent, adj_stream_percent, break_percent) =
        compute_stream_percentages(total_stream, total_break, chart.total_measures);

    let mut object = JsonObjectWriter::new(writer, indent)?;
    object.field_u32("total_streams", total_stream)?;
    object.field_u32("16th_streams", chart.stream_counts.run16_streams)?;
    object.field_u32("20th_streams", chart.stream_counts.run20_streams)?;
    object.field_u32("24th_streams", chart.stream_counts.run24_streams)?;
    object.field_u32("32nd_streams", chart.stream_counts.run32_streams)?;
    object.field_u32("total_breaks", total_break)?;
    object.field_u32("sn_breaks", chart.stream_counts.sn_breaks)?;
    object.field_f64("stream_percent", stream_percent)?;
    object.field_f64("adj_stream_percent", adj_stream_percent)?;
    object.field_f64("break_percent", break_percent)?;
    object.field_with("stream_sequences", |writer, indent| {
        write_json_stream_sequences(writer, &chart.measure_densities, indent)
    })?;
    object.finish()
}

fn write_json_sn_breakdown<W: Write>(
    writer: &mut W,
    chart: &ChartSummary,
    indent: usize,
) -> io::Result<()> {
    let mut object = JsonObjectWriter::new(writer, indent)?;
    object.field_string("sn_detailed_breakdown", &chart.sn_detailed_breakdown)?;
    object.field_string("sn_partial_breakdown", &chart.sn_partial_breakdown)?;
    object.field_string("sn_simple_breakdown", &chart.sn_simple_breakdown)?;
    object.finish()
}

fn write_json_stream_breakdown<W: Write>(
    writer: &mut W,
    chart: &ChartSummary,
    indent: usize,
) -> io::Result<()> {
    let mut object = JsonObjectWriter::new(writer, indent)?;
    object.field_string("detailed_breakdown", &chart.detailed_breakdown)?;
    object.field_string("partial_breakdown", &chart.partial_breakdown)?;
    object.field_string("simple_breakdown", &chart.simple_breakdown)?;
    object.finish()
}

fn write_json_mono_candle_stats<W: Write>(
    writer: &mut W,
    chart: &ChartSummary,
    indent: usize,
) -> io::Result<()> {
    let left = count(&chart.detected_patterns, PatternVariant::CandleLeft);
    let right = count(&chart.detected_patterns, PatternVariant::CandleRight);
    let mut object = JsonObjectWriter::new(writer, indent)?;
    object.field_u32("total_candles", left + right)?;
    object.field_u32("left_foot_candles", left)?;
    object.field_u32("right_foot_candles", right)?;
    object.field_f64("candles_percent", chart.candle_percent)?;
    object.field_u32("total_mono", chart.mono_total)?;
    object.field_u32("left_face_mono", chart.facing_left)?;
    object.field_u32("right_face_mono", chart.facing_right)?;
    object.field_f64("mono_percent", chart.mono_percent)?;
    object.finish()
}

fn write_json_tech_counts<W: Write>(
    writer: &mut W,
    chart: &ChartSummary,
    indent: usize,
) -> io::Result<()> {
    write_json_u32_object(
        writer,
        indent,
        &[
            ("crossovers", chart.tech_counts.crossovers),
            ("footswitches", chart.tech_counts.footswitches),
            ("up_footswitches", chart.tech_counts.up_footswitches),
            ("down_footswitches", chart.tech_counts.down_footswitches),
            ("sideswitches", chart.tech_counts.sideswitches),
            ("jacks", chart.tech_counts.jacks),
            ("brackets", chart.tech_counts.brackets),
            ("doublesteps", chart.tech_counts.doublesteps),
        ],
    )
}

fn write_json_raw_f64<W: Write>(writer: &mut W, value: f64) -> io::Result<()> {
    let Some(number) = JsonNumber::from_f64(value) else {
        return writer.write_all(b"null");
    };
    write_json_number_for_key(writer, None, &number)
}

fn write_json_scalar_iter<W: Write, T>(
    writer: &mut W,
    values: impl IntoIterator<Item = T>,
    mut write_value: impl FnMut(&mut W, T) -> io::Result<()>,
) -> io::Result<()> {
    writer.write_all(b"[")?;
    for (idx, value) in values.into_iter().enumerate() {
        if idx != 0 {
            writer.write_all(b", ")?;
        }
        write_value(writer, value)?;
    }
    writer.write_all(b"]")
}

fn write_json_multiline_array<W: Write>(
    writer: &mut W,
    len: usize,
    indent: usize,
    mut write_value: impl FnMut(&mut W, usize, usize) -> io::Result<()>,
) -> io::Result<()> {
    if len == 0 {
        return writer.write_all(b"[]");
    }
    writer.write_all(b"[\n")?;
    let item_indent = indent + 2;
    for idx in 0..len {
        if idx != 0 {
            writer.write_all(b",\n")?;
        }
        write_indent(writer, item_indent)?;
        write_value(writer, idx, item_indent)?;
    }
    writer.write_all(b"\n")?;
    write_indent(writer, indent)?;
    writer.write_all(b"]")
}

fn write_json_pair_iter<W: Write>(
    writer: &mut W,
    values: impl IntoIterator<Item = (f64, f64)>,
    indent: usize,
) -> io::Result<()> {
    let mut values = values.into_iter().peekable();
    if values.peek().is_none() {
        return writer.write_all(b"[]");
    }

    writer.write_all(b"[\n")?;
    let item_indent = indent + 2;
    for (index, (a, b)) in values.enumerate() {
        if index != 0 {
            writer.write_all(b",\n")?;
        }
        write_indent(writer, item_indent)?;
        writer.write_all(b"[")?;
        write_json_raw_f64(writer, a)?;
        writer.write_all(b", ")?;
        write_json_raw_f64(writer, b)?;
        writer.write_all(b"]")?;
    }
    writer.write_all(b"\n")?;
    write_indent(writer, indent)?;
    writer.write_all(b"]")
}

fn write_json_speed_iter<W: Write>(
    writer: &mut W,
    values: impl IntoIterator<Item = (f64, f64, f64, i32)>,
    indent: usize,
) -> io::Result<()> {
    let mut values = values.into_iter().peekable();
    if values.peek().is_none() {
        return writer.write_all(b"[]");
    }

    writer.write_all(b"[\n")?;
    let item_indent = indent + 2;
    for (index, (beat, ratio, delay, unit)) in values.enumerate() {
        if index != 0 {
            writer.write_all(b",\n")?;
        }
        write_indent(writer, item_indent)?;
        writer.write_all(b"[")?;
        write_json_raw_f64(writer, beat)?;
        writer.write_all(b", ")?;
        write_json_raw_f64(writer, ratio)?;
        writer.write_all(b", ")?;
        write_json_raw_f64(writer, delay)?;
        write!(writer, ", {unit}]")?;
    }
    writer.write_all(b"\n")?;
    write_indent(writer, indent)?;
    writer.write_all(b"]")
}

fn write_json_native_speeds<W: Write>(
    writer: &mut W,
    speeds: &[(f32, f32, f32, SpeedUnit)],
    indent: usize,
) -> io::Result<()> {
    if speeds.is_empty() {
        return write_json_speed_iter(writer, [(0.0, 1.0, 0.0, 0)], indent);
    }

    writer.write_all(b"[\n")?;
    let item_indent = indent + 2;
    for (index, &(beat, ratio, delay, unit)) in speeds.iter().enumerate() {
        if index != 0 {
            writer.write_all(b",\n")?;
        }
        write_indent(writer, item_indent)?;
        writer.write_all(b"[")?;
        write_json_raw_f64(writer, timing_fixed_6(f64::from(beat)))?;
        writer.write_all(b", ")?;
        write_json_raw_f64(writer, timing_fixed_6(f64::from(ratio)))?;
        writer.write_all(b", ")?;
        write_json_raw_f64(writer, timing_fixed_6(f64::from(delay)))?;
        write!(writer, ", {}]", i32::from(unit == SpeedUnit::Seconds))?;
    }
    writer.write_all(b"\n")?;
    write_indent(writer, indent)?;
    writer.write_all(b"]")
}

fn write_json_native_scrolls<W: Write>(
    writer: &mut W,
    scrolls: &[(f32, f32)],
    indent: usize,
) -> io::Result<()> {
    if scrolls.is_empty() {
        return write_json_pair_iter(writer, [(0.0, 1.0)], indent);
    }

    writer.write_all(b"[\n")?;
    let item_indent = indent + 2;
    for (index, &(beat, ratio)) in scrolls.iter().enumerate() {
        if index != 0 {
            writer.write_all(b",\n")?;
        }
        write_indent(writer, item_indent)?;
        writer.write_all(b"[")?;
        write_json_raw_f64(writer, timing_fixed_6(f64::from(beat)))?;
        writer.write_all(b", ")?;
        write_json_raw_f64(writer, timing_fixed_6(f64::from(ratio)))?;
        writer.write_all(b"]")?;
    }
    writer.write_all(b"\n")?;
    write_indent(writer, indent)?;
    writer.write_all(b"]")
}

// JSON field order is part of the report contract and is kept visible here.
#[allow(clippy::too_many_lines)]
fn write_json_timing<W: Write>(
    writer: &mut W,
    chart: &ChartSummary,
    simfile: &SimfileSummary,
    indent: usize,
) -> io::Result<()> {
    let timing = &chart.timing_segments;
    let beat0_offset_seconds =
        timing_fixed_6(chart.chart_offset_seconds + f64::from(timing.beat0_offset_adjust));
    let beat0_group_offset_seconds = 0.0;
    let (bpm_min_raw, bpm_max_raw) = actual_bpm_range_raw_f32(&timing.bpms);
    let tables = build_timing_text_tables(chart, simfile);
    let time_signatures = tables.time_signatures.as_slice();
    let labels = tables.labels.as_slice();
    let tickcounts = tables.tickcounts.as_slice();
    let combos = tables.combos.as_slice();

    let bpm_min = round_sig_figs_6(round_sig_figs_itg(bpm_min_raw));
    let bpm_max = round_sig_figs_6(round_sig_figs_itg(bpm_max_raw));
    let display_tag = chart
        .chart_display_bpm
        .as_deref()
        .filter(|s| !s.trim().is_empty());
    let (display_bpm_min_raw, display_bpm_max_raw, display_bpm) =
        resolve_display_bpm(display_tag, bpm_min_raw, bpm_max_raw, 1.0);
    let display_bpm_min = round_sig_figs_6(round_sig_figs_itg(display_bpm_min_raw));
    let display_bpm_max = round_sig_figs_6(round_sig_figs_itg(display_bpm_max_raw));
    let hash_bpms_cached = chart
        .chart_bpms_norm
        .as_deref()
        .filter(|value| !value.is_empty());
    let hash_bpms_owned = hash_bpms_cached
        .is_none()
        .then(|| chart.chart_bpms.as_deref().map(normalize_float_digits))
        .flatten()
        .filter(|value| !value.is_empty());
    let hash_bpms = hash_bpms_cached
        .or(hash_bpms_owned.as_deref())
        .unwrap_or(&simfile.normalized_bpms);

    let mut object = JsonObjectWriter::new(writer, indent)?;
    object.field_f64("beat0_offset_seconds", beat0_offset_seconds)?;
    object.field_f64("beat0_group_offset_seconds", beat0_group_offset_seconds)?;
    object.field_string("hash_bpms", hash_bpms)?;
    object.field_with("bpms_formatted", |writer, _| {
        write_json_native_bpms(writer, &timing.bpms)
    })?;
    object.field_f64("bpm_min", bpm_min)?;
    object.field_f64("bpm_max", bpm_max)?;
    object.field_string("display_bpm", &display_bpm)?;
    object.field_f64("display_bpm_min", display_bpm_min)?;
    object.field_f64("display_bpm_max", display_bpm_max)?;
    object.field_with("bpms", |writer, indent| {
        write_json_pair_iter(
            writer,
            timing.bpms.iter().map(|(beat, bpm)| {
                (
                    timing_fixed_6(f64::from(*beat)),
                    timing_fixed_6(roundtrip_bpm_itg(f64::from(*bpm))),
                )
            }),
            indent,
        )
    })?;
    object.field_with("stops", |writer, indent| {
        write_json_pair_iter(
            writer,
            timing.stops.iter().map(|(beat, duration)| {
                (
                    timing_fixed_6(f64::from(*beat)),
                    timing_fixed_6(f64::from(*duration)),
                )
            }),
            indent,
        )
    })?;
    object.field_with("delays", |writer, indent| {
        write_json_pair_iter(
            writer,
            timing.delays.iter().map(|(beat, duration)| {
                (
                    timing_fixed_6(f64::from(*beat)),
                    timing_fixed_6(f64::from(*duration)),
                )
            }),
            indent,
        )
    })?;
    object.field_with("time_signatures", |writer, indent| {
        write_json_multiline_array(writer, time_signatures.len(), indent, |writer, idx, _| {
            let (beat, numerator, denominator) = time_signatures[idx];
            writer.write_all(b"[")?;
            write_json_raw_f64(writer, beat)?;
            write!(writer, ", {numerator}, {denominator}]")
        })
    })?;
    object.field_with("warps", |writer, indent| {
        write_json_pair_iter(
            writer,
            timing.warps.iter().map(|(beat, length)| {
                (
                    timing_fixed_6(f64::from(*beat)),
                    timing_fixed_6(f64::from(*length)),
                )
            }),
            indent,
        )
    })?;
    object.field_with("labels", |writer, indent| {
        write_json_multiline_array(writer, labels.len(), indent, |writer, idx, _| {
            let (beat, label) = &labels[idx];
            writer.write_all(b"[")?;
            write_json_raw_f64(writer, *beat)?;
            writer.write_all(b", ")?;
            write_json_string(writer, label)?;
            writer.write_all(b"]")
        })
    })?;
    object.field_with("tickcounts", |writer, indent| {
        write_json_multiline_array(writer, tickcounts.len(), indent, |writer, idx, _| {
            let (beat, count) = tickcounts[idx];
            writer.write_all(b"[")?;
            write_json_raw_f64(writer, beat)?;
            write!(writer, ", {count}]")
        })
    })?;
    object.field_with("combos", |writer, indent| {
        write_json_multiline_array(writer, combos.len(), indent, |writer, idx, _| {
            let (beat, combo, miss) = combos[idx];
            writer.write_all(b"[")?;
            write_json_raw_f64(writer, beat)?;
            write!(writer, ", {combo}, {miss}]")
        })
    })?;
    object.field_with("speeds", |writer, indent| {
        write_json_native_speeds(writer, &timing.speeds, indent)
    })?;
    object.field_with("scrolls", |writer, indent| {
        write_json_native_scrolls(writer, &timing.scrolls, indent)
    })?;
    object.field_with("fakes", |writer, indent| {
        write_json_pair_iter(
            writer,
            timing.fakes.iter().map(|(beat, length)| {
                (
                    timing_fixed_6(f64::from(*beat)),
                    timing_fixed_6(f64::from(*length)),
                )
            }),
            indent,
        )
    })?;
    object.field_f64("duration_seconds", chart.duration_seconds)?;
    object.finish()
}

fn write_json_spacing<W: Write>(writer: &mut W, chart: &ChartSummary) -> io::Result<()> {
    let lanes = crate::step_type_lanes(&chart.step_type_str);
    writer.write_all(b"[")?;
    let mut first = true;
    visit_measure_spacing(&chart.minimized_note_data, lanes, |value| {
        if !first {
            writer.write_all(b", ")?;
        }
        first = false;
        writer.write_all(if value { b"true" } else { b"false" })
    })?;
    writer.write_all(b"]")
}

fn write_json_nps<W: Write>(writer: &mut W, chart: &ChartSummary, indent: usize) -> io::Result<()> {
    let mut object = JsonObjectWriter::new(writer, indent)?;
    object.field_f64("max_nps", chart.max_nps)?;
    object.field_f64("median_nps", chart.median_nps)?;
    object.field_with("notes_per_measure", |writer, _| {
        write_json_scalar_iter(
            writer,
            chart.measure_densities.iter().copied(),
            |writer, value| write_json_raw_u32(writer, value as u32),
        )
    })?;
    object.field_with("nps_per_measure", |writer, _| {
        write_json_scalar_iter(
            writer,
            chart.measure_nps_vec.iter().copied(),
            |writer, value| write_json_raw_f64(writer, value),
        )
    })?;
    object.field_with("equally_spaced_per_measure", |writer, _| {
        write_json_spacing(writer, chart)
    })?;
    object.finish()
}

// JSON field order is part of the report contract and is kept visible here.
#[allow(clippy::too_many_lines)]
fn write_json_pattern_counts<W: Write>(
    writer: &mut W,
    chart: &ChartSummary,
    indent: usize,
) -> io::Result<()> {
    let mut object = JsonObjectWriter::new(writer, indent)?;

    let boxes = compute_box_parts(&chart.detected_patterns);
    let corner_boxes = boxes.ld + boxes.lu + boxes.rd + boxes.ru;
    object.field_with("boxes", |writer, indent| {
        write_json_u32_object(
            writer,
            indent,
            &[
                ("total_boxes", boxes.lr + boxes.ud + corner_boxes),
                ("lr_boxes", boxes.lr),
                ("ud_boxes", boxes.ud),
                ("corner_boxes", corner_boxes),
                ("ld_boxes", boxes.ld),
                ("lu_boxes", boxes.lu),
                ("rd_boxes", boxes.rd),
                ("ru_boxes", boxes.ru),
            ],
        )
    })?;

    object.field_with("anchors", |writer, indent| {
        write_json_u32_object(
            writer,
            indent,
            &[
                (
                    "total_anchors",
                    chart.anchor_left + chart.anchor_down + chart.anchor_up + chart.anchor_right,
                ),
                ("left_anchors", chart.anchor_left),
                ("down_anchors", chart.anchor_down),
                ("up_anchors", chart.anchor_up),
                ("right_anchors", chart.anchor_right),
            ],
        )
    })?;

    let towers = compute_tower_parts(&chart.detected_patterns);
    let corner_towers = towers.ld + towers.lu + towers.rd + towers.ru;
    object.field_with("towers", |writer, indent| {
        write_json_u32_object(
            writer,
            indent,
            &[
                ("total_towers", towers.lr + towers.ud + corner_towers),
                ("lr_towers", towers.lr),
                ("ud_towers", towers.ud),
                ("corner_towers", corner_towers),
                ("ld_towers", towers.ld),
                ("lu_towers", towers.lu),
                ("rd_towers", towers.rd),
                ("ru_towers", towers.ru),
            ],
        )
    })?;

    let triangles = compute_triangle_parts(&chart.detected_patterns);
    object.field_with("triangles", |writer, indent| {
        write_json_u32_object(
            writer,
            indent,
            &[
                (
                    "total_triangles",
                    triangles.ldl + triangles.lul + triangles.rdr + triangles.rur,
                ),
                ("ldl_triangles", triangles.ldl),
                ("lul_triangles", triangles.lul),
                ("rdr_triangles", triangles.rdr),
                ("rur_triangles", triangles.rur),
            ],
        )
    })?;

    let stairs = compute_stair_parts(
        &chart.detected_patterns,
        PatternVariant::StaircaseLeft,
        PatternVariant::StaircaseRight,
        PatternVariant::StaircaseInvLeft,
        PatternVariant::StaircaseInvRight,
    );
    let alt_stairs = compute_stair_parts(
        &chart.detected_patterns,
        PatternVariant::AltStaircasesLeft,
        PatternVariant::AltStaircasesRight,
        PatternVariant::AltStaircasesInvLeft,
        PatternVariant::AltStaircasesInvRight,
    );
    let double_stairs = compute_stair_parts(
        &chart.detected_patterns,
        PatternVariant::DStaircaseLeft,
        PatternVariant::DStaircaseRight,
        PatternVariant::DStaircaseInvLeft,
        PatternVariant::DStaircaseInvRight,
    );
    object.field_with("staircases", |writer, indent| {
        write_json_u32_object(
            writer,
            indent,
            &[
                (
                    "total_staircases",
                    stairs.left + stairs.right + stairs.left_inv + stairs.right_inv,
                ),
                ("left_staircases", stairs.left),
                ("right_staircases", stairs.right),
                ("left_inv_staircases", stairs.left_inv),
                ("right_inv_staircases", stairs.right_inv),
                (
                    "total_alt_staircases",
                    alt_stairs.left + alt_stairs.right + alt_stairs.left_inv + alt_stairs.right_inv,
                ),
                ("left_alt_staircases", alt_stairs.left),
                ("right_alt_staircases", alt_stairs.right),
                ("left_inv_alt_staircases", alt_stairs.left_inv),
                ("right_inv_alt_staircases", alt_stairs.right_inv),
                (
                    "total_double_staircases",
                    double_stairs.left
                        + double_stairs.right
                        + double_stairs.left_inv
                        + double_stairs.right_inv,
                ),
                ("left_double_staircases", double_stairs.left),
                ("right_double_staircases", double_stairs.right),
                ("left_inv_double_staircases", double_stairs.left_inv),
                ("right_inv_double_staircases", double_stairs.right_inv),
            ],
        )
    })?;

    let sweeps = compute_sweep_parts(
        &chart.detected_patterns,
        PatternVariant::SweepLeft,
        PatternVariant::SweepRight,
        PatternVariant::SweepInvLeft,
        PatternVariant::SweepInvRight,
    );
    object.field_with("sweeps", |writer, indent| {
        write_json_u32_object(
            writer,
            indent,
            &[
                (
                    "total_sweeps",
                    sweeps.left + sweeps.right + sweeps.left_inv + sweeps.right_inv,
                ),
                ("left_sweeps", sweeps.left),
                ("right_sweeps", sweeps.right),
                ("left_inv_sweeps", sweeps.left_inv),
                ("right_inv_sweeps", sweeps.right_inv),
            ],
        )
    })?;

    let candle_sweeps = compute_sweep_parts(
        &chart.detected_patterns,
        PatternVariant::SweepCandleLeft,
        PatternVariant::SweepCandleRight,
        PatternVariant::SweepCandleInvLeft,
        PatternVariant::SweepCandleInvRight,
    );
    object.field_with("candle_sweeps", |writer, indent| {
        write_json_u32_object(
            writer,
            indent,
            &[
                (
                    "total_candle_sweeps",
                    candle_sweeps.left
                        + candle_sweeps.right
                        + candle_sweeps.left_inv
                        + candle_sweeps.right_inv,
                ),
                ("left_candle_sweeps", candle_sweeps.left),
                ("right_candle_sweeps", candle_sweeps.right),
                ("left_inv_candle_sweeps", candle_sweeps.left_inv),
                ("right_inv_candle_sweeps", candle_sweeps.right_inv),
            ],
        )
    })?;

    {
        let mut write_quad = |key: &str,
                              total_key: &str,
                              left_key: &str,
                              right_key: &str,
                              left_inv_key: &str,
                              right_inv_key: &str,
                              values: SimpleQuadParts|
         -> io::Result<()> {
            object.field_with(key, |writer, indent| {
                write_json_u32_object(
                    writer,
                    indent,
                    &[
                        (total_key, values.a + values.b + values.c + values.d),
                        (left_key, values.a),
                        (right_key, values.b),
                        (left_inv_key, values.c),
                        (right_inv_key, values.d),
                    ],
                )
            })
        };

        write_quad(
            "copters",
            "total_copters",
            "left_copters",
            "right_copters",
            "left_inv_copters",
            "right_inv_copters",
            compute_simple_quad_parts(
                &chart.detected_patterns,
                PatternVariant::CopterLeft,
                PatternVariant::CopterRight,
                PatternVariant::CopterInvLeft,
                PatternVariant::CopterInvRight,
            ),
        )?;
        write_quad(
            "spirals",
            "total_spirals",
            "left_spirals",
            "right_spirals",
            "left_inv_spirals",
            "right_inv_spirals",
            compute_simple_quad_parts(
                &chart.detected_patterns,
                PatternVariant::SpiralLeft,
                PatternVariant::SpiralRight,
                PatternVariant::SpiralInvLeft,
                PatternVariant::SpiralInvRight,
            ),
        )?;
        write_quad(
            "turbo_candles",
            "total_turbo_candles",
            "left_turbo_candles",
            "right_turbo_candles",
            "left_inv_turbo_candles",
            "right_inv_turbo_candles",
            compute_simple_quad_parts(
                &chart.detected_patterns,
                PatternVariant::TurboCandleLeft,
                PatternVariant::TurboCandleRight,
                PatternVariant::TurboCandleInvLeft,
                PatternVariant::TurboCandleInvRight,
            ),
        )?;
        write_quad(
            "hip_breakers",
            "total_hip_breakers",
            "left_hip_breakers",
            "right_hip_breakers",
            "left_inv_hip_breakers",
            "right_inv_hip_breakers",
            compute_simple_quad_parts(
                &chart.detected_patterns,
                PatternVariant::HipBreakerLeft,
                PatternVariant::HipBreakerRight,
                PatternVariant::HipBreakerInvLeft,
                PatternVariant::HipBreakerInvRight,
            ),
        )?;
        write_quad(
            "doritos",
            "total_doritos",
            "left_doritos",
            "right_doritos",
            "left_inv_doritos",
            "right_inv_doritos",
            compute_simple_quad_parts(
                &chart.detected_patterns,
                PatternVariant::DoritoLeft,
                PatternVariant::DoritoRight,
                PatternVariant::DoritoInvLeft,
                PatternVariant::DoritoInvRight,
            ),
        )?;
        write_quad(
            "luchis",
            "total_luchis",
            "left_du_luchis",
            "left_ud_luchis",
            "right_du_luchis",
            "right_ud_luchis",
            compute_simple_quad_parts(
                &chart.detected_patterns,
                PatternVariant::LuchiLeftDU,
                PatternVariant::LuchiLeftUD,
                PatternVariant::LuchiRightDU,
                PatternVariant::LuchiRightUD,
            ),
        )?;
    }

    if !chart.custom_patterns.is_empty() {
        object.field_with("custom_patterns", |writer, indent| {
            let mut custom = JsonObjectWriter::new(writer, indent)?;
            // Analysis deduplicates names and retains their stable input order,
            // matching serde_json's preserve_order map without rebuilding it.
            for pattern in &chart.custom_patterns {
                custom.field_u32(&pattern.pattern, pattern.count)?;
            }
            custom.finish()
        })?;
    }
    object.finish()
}

fn write_json_chart<W: Write>(
    writer: &mut W,
    chart: &ChartSummary,
    simfile: &SimfileSummary,
    indent: usize,
) -> io::Result<()> {
    let mut object = JsonObjectWriter::new(writer, indent)?;
    object.field_with("chart_info", |writer, indent| {
        write_json_chart_info(writer, chart, indent)
    })?;
    object.field_with("arrow_stats", |writer, indent| {
        write_json_arrow_stats(writer, chart, indent)
    })?;
    object.field_with("gimmicks", |writer, indent| {
        write_json_gimmicks(writer, chart, simfile, indent)
    })?;
    object.field_with("timing", |writer, indent| {
        write_json_timing(writer, chart, simfile, indent)
    })?;
    object.field_with("stream_info", |writer, indent| {
        write_json_stream_info(writer, chart, indent)
    })?;
    object.field_with("nps", |writer, indent| {
        write_json_nps(writer, chart, indent)
    })?;
    object.field_with("breakdown", |writer, indent| {
        write_json_sn_breakdown(writer, chart, indent)
    })?;
    object.field_with("stream_breakdown", |writer, indent| {
        write_json_stream_breakdown(writer, chart, indent)
    })?;
    if simfile.pattern_counts_enabled {
        object.field_with("mono_candle_stats", |writer, indent| {
            write_json_mono_candle_stats(writer, chart, indent)
        })?;
        object.field_with("pattern_counts", |writer, indent| {
            write_json_pattern_counts(writer, chart, indent)
        })?;
    }
    if simfile.tech_counts_enabled {
        object.field_with("tech_counts", |writer, indent| {
            write_json_tech_counts(writer, chart, indent)
        })?;
    }
    object.finish()
}
/// Writes the complete JSON representation of a simfile analysis.
///
/// # Errors
///
/// Returns the first I/O error produced by `writer`.
pub fn write_json_all<W: Write>(simfile: &SimfileSummary, writer: &mut W) -> io::Result<()> {
    let mut root = JsonObjectWriter::new(writer, 0)?;
    root.field_string("title", &simfile.title_str)?;
    root.field_string("subtitle", &simfile.subtitle_str)?;
    root.field_string("artist", &simfile.artist_str)?;
    root.field_string("title_trans", &simfile.titletranslit_str)?;
    root.field_string("subtitle_trans", &simfile.subtitletranslit_str)?;
    root.field_string("artist_trans", &simfile.artisttranslit_str)?;
    root.field_display_string("length", simfile.total_length)?;
    if (simfile.min_bpm - simfile.max_bpm).abs() < f64::EPSILON {
        root.field_f64("bpm", simfile.min_bpm)?;
    } else {
        root.field_with("bpm", |writer, _| {
            writer.write_all(b"\"")?;
            write!(writer, "{:.0}-{:.0}", simfile.min_bpm, simfile.max_bpm)?;
            writer.write_all(b"\"")
        })?;
    }
    root.field_f64("min_bpm", simfile.min_bpm)?;
    root.field_f64("max_bpm", simfile.max_bpm)?;
    root.field_f64("average_bpm", simfile.average_bpm)?;
    root.field_f64("median_bpm", simfile.median_bpm)?;
    root.field_string("bpm_data", &simfile.normalized_bpms)?;
    root.field_f64("offset", simfile.offset)?;
    root.field_with("charts", |writer, indent| {
        write_json_multiline_array(
            writer,
            simfile.charts.len(),
            indent,
            |writer, idx, item_indent| {
                write_json_chart(writer, &simfile.charts[idx], simfile, item_indent)
            },
        )
    })?;
    root.finish()?;
    writeln!(writer)
}

const CSV_HEADER_BASE: &str = "Title,Subtitle,Artist,Title trans,Subtitle trans,Artist trans,Length,BPM,BPM Tier,min_bpm,max_bpm,average_bpm,median bpm,BPM-data,offset,file_md5_hash,step_type,difficulty,rating,step_artist,tech_notation,sha1_hash,bpm_neutral_hash,total_arrows,left_arrows,down_arrows,up_arrows,right_arrows,total_steps,jumps,hands,holds,rolls,mines,lifts,fakes,stops_freezes,delays,warps,speeds,scrolls,total_streams,16th_streams,20th_streams,24th_streams,32nd_streams,total_breaks,sn_breaks,stream_percent,adj_stream_percent,max_nps,median_nps,matrix_rating";
const CSV_HEADER_PATTERN_1: &str = "mono_total,total_candles,left_foot_candles,right_foot_candles,candles_percent,total_mono,left_face_mono,right_face_mono,mono_percent,total_boxes,lr_boxes,ud_boxes,corner_boxes,ld_boxes,lu_boxes,rd_boxes,ru_boxes,total_anchors,left_anchors,down_anchors,up_anchors,right_anchors";
const CSV_HEADER_BREAKDOWNS: &str = "sn_detailed_breakdown,sn_partial_breakdown,sn_simple_breakdown,detailed_breakdown,partial_breakdown,simple_breakdown";
const CSV_HEADER_PATTERN_2: &str = "total_towers,lr_towers,ud_towers,corner_towers,ld_towers,lu_towers,rd_towers,ru_towers,total_triangles,ldl_triangles,lul_triangles,rdr_triangles,rur_triangles";
const CSV_HEADER_TECH: &str = "crossovers,half_crossovers,full_crossovers,footswitches,up_footswitches,down_footswitches,sideswitches,jacks,brackets,doublesteps";
const CSV_HEADER_PATTERN_3: &str = "total staircases,left_staircases,right_staircases,left_inv_staircases,right_inv_staircases,total_alt_staircases,left_alt_staircases,right_alt_staircases,left_inv_alt_staircases,right_inv_alt_staircases,total_double_staircases,left_double_staircases,right_double_staircases,left_inv_double_staircases,right_inv_double_staircases,total_sweeps,left_sweeps,right_sweeps,left_inv_sweeps,right_inv_sweeps,total_candle_sweeps,left_candle_sweeps,right_candle_sweeps,left_inv_candle_sweeps,right_inv_candle_sweeps,total copters,left_copters,right_copters,left_inv_copters,right_inv_copters,total_spirals,left_spirals,right_spirals,left_inv_spirals,right_inv_spirals,total_turbo_candles,left_turbo_candles,right_turbo_candles,left_inv_turbo_candles,right_inv_turbo_candles,total_hip_breakers,left_hip_breakers,right_hip_breakers,left_inv_hip_breakers,right_inv_hip_breakers,total_doritos,left_doritos,right_doritos,left_inv_doritos,right_inv_doritos,total_luchis,left_du_luchis,left_ud_luchis,right_du_luchis,right_ud_luchis";

fn write_csv_all<W: Write>(writer: &mut W, simfile: &SimfileSummary) -> io::Result<()> {
    writer.write_all(CSV_HEADER_BASE.as_bytes())?;
    if simfile.pattern_counts_enabled {
        write!(writer, ",{CSV_HEADER_PATTERN_1}")?;
    }
    write!(writer, ",{CSV_HEADER_BREAKDOWNS}")?;
    if simfile.pattern_counts_enabled {
        write!(writer, ",{CSV_HEADER_PATTERN_2}")?;
    }
    if simfile.tech_counts_enabled {
        write!(writer, ",{CSV_HEADER_TECH}")?;
    }
    if simfile.pattern_counts_enabled {
        write!(writer, ",{CSV_HEADER_PATTERN_3}")?;
        if let Some(first_chart) = simfile.charts.first() {
            for cp in &first_chart.custom_patterns {
                write!(writer, ",custom_pattern_{}", cp.pattern)?;
            }
        }
    }

    writeln!(writer)?;

    for chart in &simfile.charts {
        write_csv_row(writer, simfile, chart)?;
    }

    Ok(())
}

struct CsvRow<'a, W> {
    writer: &'a mut W,
    first: bool,
    error: Option<io::Error>,
}

impl<'a, W: Write> CsvRow<'a, W> {
    fn new(writer: &'a mut W) -> Self {
        Self {
            writer,
            first: true,
            error: None,
        }
    }

    fn write_field(&mut self, write_value: impl FnOnce(&mut W) -> io::Result<()>) {
        if self.error.is_some() {
            return;
        }
        let result = (|| {
            if self.first {
                self.first = false;
            } else {
                self.writer.write_all(b",")?;
            }
            write_value(self.writer)
        })();
        if let Err(error) = result {
            self.error = Some(error);
        }
    }

    fn finish(self) -> io::Result<()> {
        if let Some(error) = self.error {
            return Err(error);
        }
        self.writer.write_all(b"\n")
    }
}

fn push_str<W: Write>(out: &mut CsvRow<'_, W>, value: &str) {
    out.write_field(|writer| {
        if !value.contains(['"', ',']) {
            return writer.write_all(value.as_bytes());
        }

        writer.write_all(b"\"")?;
        let mut rest = value;
        while let Some(quote) = rest.find('"') {
            writer.write_all(&rest.as_bytes()[..quote])?;
            writer.write_all(b"\"\"")?;
            rest = &rest[quote + 1..];
        }
        writer.write_all(rest.as_bytes())?;
        writer.write_all(b"\"")
    });
}

fn push_num<W: Write, T: std::fmt::Display>(out: &mut CsvRow<'_, W>, value: T) {
    out.write_field(|writer| write!(writer, "{value}"));
}

fn write_duration<W: Write>(writer: &mut W, seconds: i32) -> io::Result<()> {
    let minutes = seconds / 60;
    let seconds = seconds % 60;
    write!(writer, "{minutes}m {seconds:02}s")
}

fn push_duration<W: Write>(out: &mut CsvRow<'_, W>, seconds: i32) {
    out.write_field(|writer| write_duration(writer, seconds));
}

fn push_bpm_range<W: Write>(out: &mut CsvRow<'_, W>, min_bpm: f64, max_bpm: f64) {
    out.write_field(|writer| write!(writer, "{min_bpm}-{max_bpm}"));
}

// CSV column order is part of the report contract and is kept visible here.
#[allow(clippy::too_many_lines)]
fn write_csv_row<W: Write>(
    writer: &mut W,
    simfile: &SimfileSummary,
    chart: &ChartSummary,
) -> io::Result<()> {
    let mut row = CsvRow::new(writer);

    push_str(&mut row, &simfile.title_str);
    push_str(&mut row, &simfile.subtitle_str);
    push_str(&mut row, &simfile.artist_str);
    push_str(&mut row, &simfile.titletranslit_str);
    push_str(&mut row, &simfile.subtitletranslit_str);
    push_str(&mut row, &simfile.artisttranslit_str);
    push_duration(&mut row, simfile.total_length);

    if (simfile.min_bpm - simfile.max_bpm).abs() < f64::EPSILON {
        push_num(&mut row, simfile.min_bpm);
    } else {
        push_bpm_range(&mut row, simfile.min_bpm, simfile.max_bpm);
    }

    push_num(&mut row, simfile.min_bpm);
    push_num(&mut row, simfile.max_bpm);
    push_num(&mut row, simfile.average_bpm);
    push_num(&mut row, simfile.median_bpm);
    push_str(&mut row, &simfile.normalized_bpms);
    push_num(&mut row, simfile.offset);
    push_str(&mut row, "");

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
        chart.chart_stops.as_deref(),
        &simfile.normalized_stops,
    );
    let delays = chart_or_global(
        allow_steps_timing,
        chart.chart_has_own_timing,
        chart.chart_delays.as_deref(),
        &simfile.normalized_delays,
    );
    let warps = chart_or_global(
        allow_steps_timing,
        chart.chart_has_own_timing,
        chart.chart_warps.as_deref(),
        &simfile.normalized_warps,
    );
    let speeds = chart_or_global(
        allow_steps_timing,
        chart.chart_has_own_timing,
        chart.chart_speeds.as_deref(),
        &simfile.normalized_speeds,
    );
    let scrolls = chart_or_global(
        allow_steps_timing,
        chart.chart_has_own_timing,
        chart.chart_scrolls.as_deref(),
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
    push_str(&mut row, "");

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

    row.finish()
}

#[cfg(test)]
mod tests {
    use super::{timing_fixed_6, write_json_native_bpms};
    #[test]
    fn timing_fixed_6_matches_harness_style_values() {
        assert_eq!(timing_fixed_6(0.009), 0.009);
        assert_eq!(timing_fixed_6(4231.5625), 4231.5625);
        assert_eq!(timing_fixed_6(171.395_004_272_460_94), 171.395_004);
        assert_eq!(timing_fixed_6(159.789_993_286_132_8), 159.789_993);
    }
    #[test]
    fn streamed_bpm_text_preserves_stack_overflow_output() {
        let bpms: Vec<_> = (0..1_024)
            .map(|index| (index as f32 * 4.0, 90.0 + (index % 211) as f32))
            .collect();
        let formatted = crate::timing::format_bpm_segments_f32_like_itg(&bpms);
        assert!(formatted.len() > 16_384);

        let mut actual = Vec::new();
        write_json_native_bpms(&mut actual, &bpms).expect("BPM text should stream");
        assert_eq!(actual.first(), Some(&b'"'));
        assert_eq!(actual.last(), Some(&b'"'));
        assert_eq!(&actual[1..actual.len() - 1], formatted.as_bytes());
    }
}

use crate::bpm::{clean_timing_map_cow, parse_beat_or_row, parse_bpm_map};
use crate::math::{lrint_f32, lrint_f64, push_dec6_itg, roundtrip_bpm_itg};
use crate::parse::parse_offset_seconds;
use std::borrow::Cow;
use std::cmp::Ordering;
use std::fmt;

// --- Constants ---
pub const STEPFILE_VERSION_NUMBER: f32 = 0.83;
pub const VERSION_SPLIT_TIMING: f32 = 0.7;
pub const ROWS_PER_BEAT: i32 = 48;

const DEFAULT_BPM: f64 = 60.0;
const FAST_BPM_WARP_F32: f32 = 9_999_999.0;
const SEGMENT_EPSILON: f64 = 1e-6;

// --- Types ---
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimingFormat {
    Sm,
    Ssc,
}

#[inline(always)]
#[must_use]
pub const fn timing_format_from_ext(ext: &str) -> TimingFormat {
    if ext.eq_ignore_ascii_case("sm") {
        TimingFormat::Sm
    } else {
        TimingFormat::Ssc
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpeedUnit {
    Beats,
    Seconds,
}

#[derive(Debug, Clone, Copy)]
pub struct Segment {
    pub beat: f64,
    pub value: f64,
}

#[derive(Debug, Clone, Copy)]
pub struct SpeedSegment {
    pub beat: f64,
    pub ratio: f64,
    pub delay: f64,
    pub unit: SpeedUnit,
}

pub type StopSegment = Segment;
pub type DelaySegment = Segment;
pub type WarpSegment = Segment;
pub type FakeSegment = Segment;
pub type ScrollSegment = Segment;

type BpmStopResult = (Vec<(f64, f64)>, Vec<Segment>, Vec<Segment>, f64);

// --- Core math ---
#[inline(always)]
#[must_use]
pub fn note_row_to_beat(row: i32) -> f64 {
    f64::from(row) / f64::from(ROWS_PER_BEAT)
}

#[inline(always)]
fn note_row_to_beat_f32(row: i32) -> f32 {
    row as f32 / ROWS_PER_BEAT as f32
}

#[inline(always)]
#[must_use]
pub fn beat_to_note_row(beat: f64) -> i32 {
    lrint_f64(beat * f64::from(ROWS_PER_BEAT)) as i32
}

#[inline(always)]
pub(crate) fn beat_to_note_row_f32(beat: f32) -> i32 {
    lrint_f32(beat * ROWS_PER_BEAT as f32)
}

#[inline(always)]
fn quantize_beat(beat: f64) -> f64 {
    f64::from(note_row_to_beat_f32(beat_to_note_row_f32(beat as f32)))
}

#[inline(always)]
fn quantize_beat_f32(beat: f32) -> f32 {
    note_row_to_beat_f32(beat_to_note_row_f32(beat))
}

#[inline(always)]
#[must_use]
pub fn steps_timing_allowed(version: f32, format: TimingFormat) -> bool {
    matches!(format, TimingFormat::Sm) || version >= VERSION_SPLIT_TIMING
}

#[derive(Clone, Copy)]
pub struct ChartTiming<'a> {
    pub chart_offset_seconds: f64,
    pub chart_has_own_timing: bool,
    pub global_bpms: &'a str,
    pub global_stops: &'a str,
    pub global_delays: &'a str,
    pub global_warps: &'a str,
    pub global_speeds: &'a str,
    pub global_scrolls: &'a str,
    pub global_fakes: &'a str,
}

#[inline(always)]
#[must_use]
pub fn resolve_chart_timing<'a>(
    allow_steps_timing: bool,
    song_offset_seconds: f64,
    chart_offset: Option<&[u8]>,
    chart_bpms: Option<&[u8]>,
    chart_stops: Option<&[u8]>,
    chart_delays: Option<&[u8]>,
    chart_warps: Option<&[u8]>,
    chart_speeds: Option<&[u8]>,
    chart_scrolls: Option<&[u8]>,
    chart_fakes: Option<&[u8]>,
    chart_time_signatures: Option<&[u8]>,
    chart_labels: Option<&[u8]>,
    chart_tickcounts: Option<&[u8]>,
    chart_combos: Option<&[u8]>,
    global_bpms: &'a str,
    global_stops: &'a str,
    global_delays: &'a str,
    global_warps: &'a str,
    global_speeds: &'a str,
    global_scrolls: &'a str,
    global_fakes: &'a str,
) -> ChartTiming<'a> {
    let chart_offset_seconds = if allow_steps_timing && chart_offset.is_some() {
        parse_offset_seconds(chart_offset)
    } else {
        song_offset_seconds
    };

    let chart_has_own_timing = allow_steps_timing
        && (chart_bpms.is_some()
            || chart_stops.is_some()
            || chart_delays.is_some()
            || chart_warps.is_some()
            || chart_speeds.is_some()
            || chart_scrolls.is_some()
            || chart_fakes.is_some()
            || chart_time_signatures.is_some()
            || chart_labels.is_some()
            || chart_tickcounts.is_some()
            || chart_combos.is_some()
            || chart_offset.is_some());

    let (
        global_bpms,
        global_stops,
        global_delays,
        global_warps,
        global_speeds,
        global_scrolls,
        global_fakes,
    ) = if chart_has_own_timing {
        ("", "", "", "", "", "", "")
    } else {
        (
            global_bpms,
            global_stops,
            global_delays,
            global_warps,
            global_speeds,
            global_scrolls,
            global_fakes,
        )
    };

    ChartTiming {
        chart_offset_seconds,
        chart_has_own_timing,
        global_bpms,
        global_stops,
        global_delays,
        global_warps,
        global_speeds,
        global_scrolls,
        global_fakes,
    }
}

#[inline(always)]
fn float_eq(a: f64, b: f64) -> bool {
    (a - b).abs() < SEGMENT_EPSILON
}

#[inline(always)]
fn parse_f64_fast(s: &str) -> Option<f64> {
    s.trim().parse().ok()
}

// --- Unified parsing ---
fn parse_segments(s: &str) -> Vec<Segment> {
    const ESTIMATED_COMPONENT_BYTES: usize = 9;
    const LARGE_MAP_BYTES: usize = 32 * 1_024;
    const MAX_INITIAL_COMPONENTS: usize = 4_096;

    if s.is_empty() {
        return Vec::new();
    }
    let capacity = if s.len() >= LARGE_MAP_BYTES {
        s.len()
            .div_ceil(ESTIMATED_COMPONENT_BYTES)
            .min(MAX_INITIAL_COMPONENTS)
    } else {
        crate::stats::count_byte(s.as_bytes(), b',') + 1
    };
    let mut segments = Vec::with_capacity(capacity);
    for part in s.trim().split(',') {
        let Some((beat_str, val_str)) = part.trim().split_once('=') else {
            continue;
        };
        let Some(beat) = parse_beat_or_row(beat_str) else {
            continue;
        };
        let Some(value) = parse_f64_fast(val_str) else {
            continue;
        };
        if beat.is_finite() && value.is_finite() {
            segments.push(Segment {
                beat,
                value: f64::from(value as f32),
            });
        }
    }
    segments
}

fn parse_segments_positive(s: &str) -> Vec<Segment> {
    let mut segments = parse_segments(s);
    segments.retain(|segment| segment.value > 0.0);
    segments
}

fn parse_speeds(s: &str) -> Vec<SpeedSegment> {
    const ESTIMATED_COMPONENT_BYTES: usize = 10;

    if s.is_empty() {
        return Vec::new();
    }
    let mut speeds = Vec::with_capacity(s.len().div_ceil(ESTIMATED_COMPONENT_BYTES));
    speeds.extend(s.split(',').filter_map(|chunk| {
        let mut parts = chunk.split('=').map(str::trim);
        let beat = parse_beat_or_row(parts.next()?)?;
        let ratio = f64::from(parts.next()?.parse::<f64>().ok()? as f32);
        let delay = f64::from(parts.next()?.parse::<f64>().ok()? as f32);
        let unit = if parts.next() == Some("1") {
            SpeedUnit::Seconds
        } else {
            SpeedUnit::Beats
        };
        Some(SpeedSegment {
            beat,
            ratio,
            delay,
            unit,
        })
    }));
    speeds
}

// --- Row builders ---
fn append_segment_rows(rows: &mut Vec<i32>, segments: &[Segment], require_positive: bool) {
    let start = rows.len();
    let mut ordered = true;
    let mut previous_row = i32::MIN;
    for segment in segments {
        if require_positive && (!segment.value.is_finite() || segment.value <= 0.0) {
            continue;
        }
        let row = beat_to_note_row_f32(segment.beat as f32);
        ordered &= row >= previous_row;
        previous_row = row;
        rows.push(row);
    }
    finish_segment_rows(rows, start, ordered, require_positive);
}

fn finish_segment_rows(rows: &mut Vec<i32>, start: usize, ordered: bool, deduplicate: bool) {
    if !ordered {
        rows[start..].sort_unstable();
    }
    if deduplicate {
        let mut write = start;
        for read in start..rows.len() {
            if write == start || rows[read] != rows[write - 1] {
                rows[write] = rows[read];
                write += 1;
            }
        }
        rows.truncate(write);
    }
}

fn append_packed_segments(
    segments: &mut Vec<Segment>,
    rows: &mut Vec<i32>,
    source: &[(f32, f32)],
    require_positive: bool,
) {
    let row_start = rows.len();
    let mut ordered = true;
    let mut previous_row = i32::MIN;
    for &(beat, value) in source {
        segments.push(Segment {
            beat: f64::from(beat),
            value: f64::from(value),
        });
        if require_positive && (!value.is_finite() || value <= 0.0) {
            continue;
        }
        let row = beat_to_note_row_f32(beat);
        ordered &= row >= previous_row;
        previous_row = row;
        rows.push(row);
    }
    finish_segment_rows(rows, row_start, ordered, require_positive);
}

fn build_segment_row_storage(
    stops: &[Segment],
    delays: &[Segment],
    warps: &[Segment],
    fakes: &[Segment],
) -> (Vec<i32>, [usize; 5]) {
    let capacity = stops
        .len()
        .saturating_add(delays.len())
        .saturating_add(warps.len())
        .saturating_add(fakes.len());
    let mut rows = Vec::with_capacity(capacity);
    let mut offsets = [0usize; 5];
    append_segment_rows(&mut rows, stops, true);
    offsets[1] = rows.len();
    append_segment_rows(&mut rows, delays, true);
    offsets[2] = rows.len();
    append_segment_rows(&mut rows, warps, false);
    offsets[3] = rows.len();
    append_segment_rows(&mut rows, fakes, false);
    offsets[4] = rows.len();
    (rows, offsets)
}

#[inline]
fn segment_index_at_row(rows: &[i32], row: i32) -> Option<usize> {
    let idx = rows.partition_point(|r| *r <= row);
    if idx == 0 { None } else { Some(idx - 1) }
}

#[inline]
fn has_row(rows: &[i32], row: i32) -> bool {
    rows.binary_search(&row).is_ok()
}

// --- Segment tidying ---
fn compact_row_segments(mut segments: Vec<Segment>) -> Vec<Segment> {
    let mut write = 0;
    for read in 0..segments.len() {
        let row = segment_row(&segments[read]);
        if write != 0 && segment_row(&segments[write - 1]) == row {
            segments[write - 1] = segments[read];
        } else {
            if write != read {
                segments[write] = segments[read];
            }
            write += 1;
        }
    }
    segments.truncate(write);
    segments
}

fn tidy_key_indices(segments: &[Segment]) -> Vec<Segment> {
    let mut keys: Vec<_> = segments
        .iter()
        .enumerate()
        .map(|(index, segment)| segment_sort_key(segment_row(segment), index as u32))
        .collect();
    keys.sort_unstable();

    let mut out = Vec::with_capacity(keys.len());
    let mut index = 0;
    while index < keys.len() {
        let row = keys[index] >> 32;
        let mut last = keys[index] as u32 as usize;
        index += 1;
        while index < keys.len() && keys[index] >> 32 == row {
            last = keys[index] as u32 as usize;
            index += 1;
        }
        out.push(segments[last]);
    }
    out
}

fn tidy_wide_records(segments: Vec<Segment>) -> Vec<Segment> {
    let mut keyed: Vec<_> = segments
        .into_iter()
        .enumerate()
        .map(|(idx, seg)| (segment_row(&seg), idx, seg))
        .collect();
    keyed.sort_unstable_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));

    let mut out = Vec::with_capacity(keyed.len());
    let mut i = 0;
    while i < keyed.len() {
        let row = keyed[i].0;
        let mut last = keyed[i].2;
        i += 1;
        while i < keyed.len() && keyed[i].0 == row {
            last = keyed[i].2;
            i += 1;
        }
        out.push(last);
    }
    out
}

fn tidy_row_segments(mut segments: Vec<Segment>) -> Vec<Segment> {
    let mut ordered = true;
    let mut previous_row = i32::MIN;
    for segment in &mut segments {
        let row = beat_to_note_row(segment.beat);
        segment.beat = note_row_to_beat(row);
        ordered &= row >= previous_row;
        previous_row = row;
    }

    if ordered {
        compact_row_segments(segments)
    } else if segments.len() > u32::MAX as usize {
        tidy_wide_records(segments)
    } else {
        tidy_key_indices(&segments)
    }
}

#[inline]
fn segment_row(seg: &Segment) -> i32 {
    beat_to_note_row(seg.beat)
}

#[inline(always)]
const fn segment_sort_key(row: i32, index: u32) -> u64 {
    (((row as u32 ^ (1_u32 << 31)) as u64) << 32) | index as u64
}

#[inline]
fn eq_segment(a: &Segment, b: &Segment) -> bool {
    float_eq(a.value, b.value)
}

fn add_scroll_segment_slow(out: &mut Vec<Segment>, seg: Segment, row: i32) {
    let idx = {
        let pos = out.partition_point(|s| segment_row(s) <= row);
        if pos == 0 { 0 } else { pos - 1 }
    };
    let on_same_row = segment_row(&out[idx]) == row;
    let prev_idx = if on_same_row && idx > 0 { idx - 1 } else { idx };

    if idx + 1 < out.len() {
        let next_idx = idx + 1;
        if eq_segment(&seg, &out[next_idx]) {
            if eq_segment(&seg, &out[prev_idx]) {
                out.remove(next_idx);
                if prev_idx != idx {
                    out.remove(idx);
                }
                return;
            }
            out[next_idx].beat = seg.beat;
            if prev_idx != idx {
                out.remove(idx);
            }
            return;
        }
        if eq_segment(&seg, &out[prev_idx]) {
            if prev_idx != idx {
                out.remove(idx);
            }
            return;
        }
    } else if eq_segment(&seg, &out[prev_idx]) {
        if prev_idx != idx {
            out.remove(idx);
        }
        return;
    }

    if on_same_row {
        if !eq_segment(&seg, &out[idx]) {
            out[idx] = seg;
        }
    } else {
        let insert_pos = out.partition_point(|s| segment_row(s) <= row);
        out.insert(insert_pos, seg);
    }
}

fn tidy_scroll_segments(mut segments: Vec<Segment>) -> Vec<Segment> {
    let mut ordered = true;
    let mut previous_row = i32::MIN;
    for segment in &mut segments {
        let row = beat_to_note_row(segment.beat);
        segment.beat = note_row_to_beat(row);
        ordered &= row >= previous_row;
        previous_row = row;
    }

    if !ordered {
        let mut out = Vec::with_capacity(segments.len());
        for segment in segments {
            if out.is_empty() {
                out.push(segment);
            } else {
                let row = segment_row(&segment);
                add_scroll_segment_slow(&mut out, segment, row);
            }
        }
        return out;
    }

    let mut write = 0;
    for read in 0..segments.len() {
        let segment = segments[read];
        if write == 0 {
            segments[write] = segment;
            write += 1;
            continue;
        }

        let last = write - 1;
        if segment_row(&segment) > segment_row(&segments[last]) {
            if !eq_segment(&segment, &segments[last]) {
                segments[write] = segment;
                write += 1;
            }
        } else if write > 1 && eq_segment(&segment, &segments[write - 2]) {
            write -= 1;
        } else if !eq_segment(&segment, &segments[last]) {
            segments[last] = segment;
        }
    }
    segments.truncate(write);
    segments
}

#[inline]
fn speed_row(seg: &SpeedSegment) -> i32 {
    beat_to_note_row(seg.beat)
}

#[inline]
fn eq_speed(a: &SpeedSegment, b: &SpeedSegment) -> bool {
    float_eq(a.ratio, b.ratio) && float_eq(a.delay, b.delay) && a.unit == b.unit
}

fn add_speed_segment_slow(out: &mut Vec<SpeedSegment>, seg: SpeedSegment, row: i32) {
    let idx = {
        let pos = out.partition_point(|s| speed_row(s) <= row);
        if pos == 0 { 0 } else { pos - 1 }
    };
    let on_same_row = speed_row(&out[idx]) == row;
    let prev_idx = if on_same_row && idx > 0 { idx - 1 } else { idx };

    if idx + 1 < out.len() {
        let next_idx = idx + 1;
        if eq_speed(&seg, &out[next_idx]) {
            if eq_speed(&seg, &out[prev_idx]) {
                out.remove(next_idx);
                if prev_idx != idx {
                    out.remove(idx);
                }
                return;
            }
            out[next_idx].beat = seg.beat;
            if prev_idx != idx {
                out.remove(idx);
            }
            return;
        }
        if eq_speed(&seg, &out[prev_idx]) {
            if prev_idx != idx {
                out.remove(idx);
            }
            return;
        }
    } else if eq_speed(&seg, &out[prev_idx]) {
        if prev_idx != idx {
            out.remove(idx);
        }
        return;
    }

    if on_same_row {
        if !eq_speed(&seg, &out[idx]) {
            out[idx] = seg;
        }
    } else {
        let insert_pos = out.partition_point(|s| speed_row(s) <= row);
        out.insert(insert_pos, seg);
    }
}

fn tidy_speed_segments(mut segments: Vec<SpeedSegment>) -> Vec<SpeedSegment> {
    let mut ordered = true;
    let mut previous_row = i32::MIN;
    for segment in &mut segments {
        let row = beat_to_note_row(segment.beat);
        segment.beat = note_row_to_beat(row);
        ordered &= row >= previous_row;
        previous_row = row;
    }

    if !ordered {
        let mut out = Vec::with_capacity(segments.len());
        for segment in segments {
            if out.is_empty() {
                out.push(segment);
            } else {
                let row = speed_row(&segment);
                add_speed_segment_slow(&mut out, segment, row);
            }
        }
        return out;
    }

    let mut write = 0;
    for read in 0..segments.len() {
        let segment = segments[read];
        if write == 0 {
            segments[write] = segment;
            write += 1;
            continue;
        }

        let last = write - 1;
        if speed_row(&segment) > speed_row(&segments[last]) {
            if !eq_speed(&segment, &segments[last]) {
                segments[write] = segment;
                write += 1;
            }
        } else if write > 1 && eq_speed(&segment, &segments[write - 2]) {
            write -= 1;
        } else if !eq_speed(&segment, &segments[last]) {
            segments[last] = segment;
        }
    }
    segments.truncate(write);
    segments
}

// --- Optional timing parsing helper ---
fn parse_optional_timing<T, F>(
    chart_val: Option<&str>,
    global_val: &str,
    parser: F,
    cleaned: bool,
) -> Vec<T>
where
    F: Fn(&str) -> Vec<T>,
{
    let s = chart_val.filter(|s| !s.is_empty()).unwrap_or(global_val);
    if cleaned {
        parser(s)
    } else {
        parser(clean_timing_map_cow(s).as_ref())
    }
}

// --- TimingSegments output ---
#[derive(Debug, Clone, Default)]
pub struct TimingSegments {
    pub beat0_offset_adjust: f32,
    pub bpms: Vec<(f32, f32)>,
    pub stops: Vec<(f32, f32)>,
    pub delays: Vec<(f32, f32)>,
    pub warps: Vec<(f32, f32)>,
    pub speeds: Vec<(f32, f32, f32, SpeedUnit)>,
    pub scrolls: Vec<(f32, f32)>,
    pub fakes: Vec<(f32, f32)>,
}

#[allow(clippy::too_many_arguments)]
pub fn compute_timing_segments(
    chart_bpms: Option<&str>,
    global_bpms: &str,
    chart_stops: Option<&str>,
    global_stops: &str,
    chart_delays: Option<&str>,
    global_delays: &str,
    chart_warps: Option<&str>,
    global_warps: &str,
    chart_speeds: Option<&str>,
    global_speeds: &str,
    chart_scrolls: Option<&str>,
    global_scrolls: &str,
    chart_fakes: Option<&str>,
    global_fakes: &str,
    format: TimingFormat,
    cleaned: bool,
) -> TimingSegments {
    let bpms_str = chart_bpms.filter(|s| !s.is_empty()).unwrap_or(global_bpms);
    let mut parsed_bpms: Vec<(f64, f64)> = if cleaned {
        parse_bpm_map(bpms_str)
    } else {
        parse_bpm_map(clean_timing_map_cow(bpms_str).as_ref())
    };
    if parsed_bpms.is_empty() {
        parsed_bpms.push((0.0, DEFAULT_BPM));
    }

    let raw_stops = parse_optional_timing(chart_stops, global_stops, parse_segments, cleaned);
    let (mut parsed_bpms, stops, extra_warps, beat0_offset_adjust) =
        process_bpms_and_stops(format, parsed_bpms, raw_stops);
    let stops = tidy_row_segments(stops);
    if parsed_bpms.is_empty() {
        parsed_bpms.push((0.0, DEFAULT_BPM));
    }

    let quantize_seg = |seg: Segment| Segment {
        beat: quantize_beat(seg.beat),
        value: seg.value,
    };

    let delays: Vec<_> =
        parse_optional_timing(chart_delays, global_delays, parse_segments, cleaned)
            .into_iter()
            .map(quantize_seg)
            .collect();
    let delays = tidy_row_segments(delays);

    let warps = parse_optional_timing(chart_warps, global_warps, parse_segments, cleaned);
    let warps = merge_extra_warps(warps, extra_warps);
    let warps: Vec<_> = warps
        .into_iter()
        .map(|s| Segment {
            beat: quantize_beat(s.beat),
            value: quantize_beat(s.value),
        })
        .collect();
    let warps = tidy_row_segments(warps);

    let speeds: Vec<_> = parse_optional_timing(chart_speeds, global_speeds, parse_speeds, cleaned)
        .into_iter()
        .map(|s| SpeedSegment {
            beat: quantize_beat(s.beat),
            ..s
        })
        .collect();
    let speeds = tidy_speed_segments(speeds);

    let scrolls: Vec<_> =
        parse_optional_timing(chart_scrolls, global_scrolls, parse_segments, cleaned)
            .into_iter()
            .map(quantize_seg)
            .collect();
    let scrolls = tidy_scroll_segments(scrolls);

    let fakes: Vec<_> =
        parse_optional_timing(chart_fakes, global_fakes, parse_segments_positive, cleaned)
            .into_iter()
            .map(|s| Segment {
                beat: quantize_beat(s.beat),
                value: quantize_beat(s.value),
            })
            .collect();
    let fakes = tidy_row_segments(fakes);

    let to_f32_pair = |s: &Segment| (s.beat as f32, s.value as f32);

    TimingSegments {
        beat0_offset_adjust: beat0_offset_adjust as f32,
        bpms: parsed_bpms
            .iter()
            .map(|(b, v)| (*b as f32, *v as f32))
            .collect(),
        stops: stops.iter().map(to_f32_pair).collect(),
        delays: delays.iter().map(to_f32_pair).collect(),
        warps: warps.iter().map(to_f32_pair).collect(),
        speeds: speeds
            .iter()
            .map(|s| (s.beat as f32, s.ratio as f32, s.delay as f32, s.unit))
            .collect(),
        scrolls: scrolls.iter().map(to_f32_pair).collect(),
        fakes: fakes.iter().map(to_f32_pair).collect(),
    }
}

#[must_use]
pub fn normalize_speeds_like_itg(
    mut speeds: Vec<(f64, f64, f64, i32)>,
) -> Vec<(f64, f64, f64, i32)> {
    if speeds.is_empty() {
        speeds.push((0.0, 1.0, 0.0, 0));
    }
    speeds
}

#[must_use]
pub fn normalize_scrolls_like_itg(mut scrolls: Vec<(f64, f64)>) -> Vec<(f64, f64)> {
    if scrolls.is_empty() {
        scrolls.push((0.0, 1.0));
    }
    scrolls
}

#[must_use]
pub fn format_bpm_segments_like_itg(bpms: &[(f64, f64)]) -> String {
    format_bpm_segments_iter(bpms.iter().copied(), bpms.len())
}

/// Formats native timing BPM segments using ITG-compatible fixed decimal notation.
#[must_use]
pub fn format_bpm_segments_f32_like_itg(bpms: &[(f32, f32)]) -> String {
    format_bpm_segments_iter(
        bpms.iter()
            .map(|&(beat, bpm)| (f64::from(beat), f64::from(bpm))),
        bpms.len(),
    )
}

/// Returns an allocation-free formatter for native BPM segments.
#[must_use]
pub fn native_bpms_display(bpms: &[(f32, f32)]) -> impl fmt::Display + '_ {
    struct DisplayBpms<'a>(&'a [(f32, f32)]);

    impl fmt::Display for DisplayBpms<'_> {
        fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            for (index, &(beat, bpm)) in self.0.iter().enumerate() {
                if index != 0 {
                    formatter.write_str(",")?;
                }
                let beat = note_row_to_beat_f32(beat_to_note_row_f32(beat));
                write!(
                    formatter,
                    "{beat:.6}={:.6}",
                    roundtrip_bpm_itg(f64::from(bpm)) as f32
                )?;
            }
            Ok(())
        }
    }

    DisplayBpms(bpms)
}

fn format_bpm_segments_iter(
    bpms: impl Iterator<Item = (f64, f64)>,
    segment_count: usize,
) -> String {
    let mut out = String::with_capacity(segment_count.saturating_mul(24));
    for (idx, (beat, bpm)) in bpms.enumerate() {
        if idx > 0 {
            out.push(',');
        }
        let beat = f64::from(note_row_to_beat_f32(beat_to_note_row_f32(beat as f32)));
        push_dec6_itg(&mut out, beat);
        out.push('=');
        push_dec6_itg(&mut out, roundtrip_bpm_itg(bpm));
    }
    out
}

#[must_use]
pub fn compute_row_to_beat(minimized_note_data: &[u8]) -> Vec<f32> {
    compute_row_to_beat_impl(minimized_note_data)
}

fn first_row_capacity(data: &[u8]) -> usize {
    // Minimized rows have one fixed lane width plus a newline. Inspecting only
    // the first row keeps the estimate O(row width) instead of adding a prepass.
    let end = data
        .iter()
        .position(|&byte| matches!(byte, b'\n' | b','))
        .unwrap_or(data.len());
    let mut row = &data[..end];
    while row.first().is_some_and(u8::is_ascii_whitespace) {
        row = &row[1..];
    }
    while row.last().is_some_and(u8::is_ascii_whitespace) {
        row = &row[..row.len() - 1];
    }
    if row.is_empty() {
        0
    } else {
        data.len().div_ceil(row.len() + 1)
    }
}

fn compute_row_to_beat_impl(minimized_note_data: &[u8]) -> Vec<f32> {
    let capacity = first_row_capacity(minimized_note_data);
    let mut row_to_beat = Vec::with_capacity(capacity);
    for (measure_index, measure_bytes) in minimized_note_data.split(|&b| b == b',').enumerate() {
        let num_rows = count_measure_rows(measure_bytes);
        if num_rows == 0 {
            continue;
        }
        row_to_beat.reserve(num_rows);
        let measure_start = measure_index as f32 * 4.0;
        let row_step = 4.0 / num_rows as f32;
        for row in 0..num_rows {
            row_to_beat.push((row as f32).mul_add(row_step, measure_start));
        }
    }
    row_to_beat
}

#[inline(always)]
fn count_measure_rows(measure: &[u8]) -> usize {
    let mut count = 0;
    let mut has_non_ws = false;
    for &b in measure {
        if b == b'\n' {
            if has_non_ws {
                count += 1;
                has_non_ws = false;
            }
        } else if !b.is_ascii_whitespace() {
            has_non_ws = true;
        }
    }
    if has_non_ws {
        count += 1;
    }
    count
}

// --- BPM/Stop processing ---
fn process_bpms_and_stops(
    format: TimingFormat,
    bpms: Vec<(f64, f64)>,
    stops: Vec<Segment>,
) -> BpmStopResult {
    match format {
        TimingFormat::Sm => process_bpms_and_stops_sm(&bpms, &stops),
        TimingFormat::Ssc => process_bpms_and_stops_ssc(bpms, stops),
    }
}

fn tidy_bpms(mut bpms: Vec<(f64, f64)>) -> Vec<(f64, f64)> {
    if bpms.is_empty() {
        return vec![(0.0, DEFAULT_BPM)];
    }

    let mut ordered = true;
    let mut previous_beat = f64::NEG_INFINITY;
    for &(beat, _) in &bpms {
        ordered &= beat >= previous_beat;
        previous_beat = beat;
    }
    if !ordered {
        bpms.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(Ordering::Equal));
    }

    let mut write = 0;
    for read in 0..bpms.len() {
        if write != 0 && bpms[read].0 == bpms[write - 1].0 {
            bpms[write - 1] = bpms[read];
        } else {
            if write != read {
                bpms[write] = bpms[read];
            }
            write += 1;
        }
    }
    bpms.truncate(write);
    bpms[0].0 = 0.0;

    write = 0;
    for read in 0..bpms.len() {
        if write == 0 || bpms[read].1 != bpms[write - 1].1 {
            if write != read {
                bpms[write] = bpms[read];
            }
            write += 1;
        }
    }
    bpms.truncate(write);
    bpms
}

fn process_bpms_and_stops_ssc(mut bpms: Vec<(f64, f64)>, mut stops: Vec<Segment>) -> BpmStopResult {
    bpms.retain(|(beat, bpm)| beat.is_finite() && bpm.is_finite() && *beat >= 0.0 && *bpm > 0.0);
    for (beat, _) in &mut bpms {
        *beat = quantize_beat(*beat);
    }

    stops.retain(|segment| {
        segment.beat.is_finite()
            && segment.value.is_finite()
            && segment.beat >= 0.0
            && segment.value > 0.0
    });
    for segment in &mut stops {
        segment.beat = quantize_beat(segment.beat);
    }

    (tidy_bpms(bpms), stops, Vec::new(), 0.0)
}

fn sort_changes_by_beat(changes: &mut [(f32, f32)]) {
    let mut ordered = true;
    let mut previous_beat = f32::NEG_INFINITY;
    for &(beat, _) in changes.iter() {
        ordered &= beat >= previous_beat;
        previous_beat = beat;
    }
    if !ordered {
        changes.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(Ordering::Equal));
    }
}

fn sort_segments_by_beat(segments: &mut [Segment]) {
    let mut ordered = true;
    let mut previous_beat = f64::NEG_INFINITY;
    for segment in segments.iter() {
        ordered &= segment.beat >= previous_beat;
        previous_beat = segment.beat;
    }
    if !ordered {
        segments.sort_by(|a, b| a.beat.partial_cmp(&b.beat).unwrap_or(Ordering::Less));
    }
}

fn push_sm_bpm(final_bpms: &mut Vec<(f64, f64)>, beat: f32, bpm: f32) {
    let beat = quantize_beat_f32(beat);
    final_bpms.push((f64::from(beat), f64::from(bpm)));
}

#[cold]
fn reserve_sm_warps(
    warps: &mut Vec<Segment>,
    bpm_changes: &[(f32, f32)],
    stop_changes: &[(f32, f32)],
) {
    let capacity = bpm_changes
        .iter()
        .filter(|(beat, value)| *beat > 0.0 && !(0.0..=FAST_BPM_WARP_F32).contains(value))
        .count()
        + stop_changes
            .iter()
            .filter(|(beat, value)| *beat >= 0.0 && *value < 0.0)
            .count();
    warps.reserve(capacity.max(1));
}

#[inline(always)]
fn push_sm_warp(
    warps: &mut Vec<Segment>,
    bpm_changes: &[(f32, f32)],
    stop_changes: &[(f32, f32)],
    warp: Segment,
) {
    if warps.capacity() == 0 {
        reserve_sm_warps(warps, bpm_changes, stop_changes);
    }
    warps.push(warp);
}

// SM timing conversion is a state machine whose ordering defines compatibility.
#[allow(clippy::too_many_lines)]
fn process_bpms_and_stops_sm(bpms: &[(f64, f64)], stops: &[Segment]) -> BpmStopResult {
    let mut bpm_changes = Vec::with_capacity(bpms.len());
    for &(beat, value) in bpms {
        if beat.is_finite() && value.is_finite() && value != 0.0 {
            bpm_changes.push((beat as f32, value as f32));
        }
    }
    sort_changes_by_beat(&mut bpm_changes);

    let mut stop_changes = Vec::with_capacity(stops.len());
    for segment in stops {
        if segment.beat.is_finite() && segment.value.is_finite() && segment.value != 0.0 {
            stop_changes.push((segment.beat as f32, segment.value as f32));
        }
    }
    sort_changes_by_beat(&mut stop_changes);

    let mut beat0_offset = 0.0_f32;
    let mut stop_idx = 0;
    while stop_idx < stop_changes.len() && stop_changes[stop_idx].0 < 0.0 {
        beat0_offset -= stop_changes[stop_idx].1;
        stop_idx += 1;
    }

    let mut bpm_idx = 0;
    let mut bpm = 0.0_f32;
    while bpm_idx < bpm_changes.len() && bpm_changes[bpm_idx].0 <= 0.0 {
        bpm = bpm_changes[bpm_idx].1;
        bpm_idx += 1;
    }
    if bpm == 0.0 {
        bpm = if bpm_idx < bpm_changes.len() {
            let v = bpm_changes[bpm_idx].1;
            bpm_idx += 1;
            v
        } else {
            DEFAULT_BPM as f32
        };
    }

    let bpm_capacity = bpm_changes.len().max(1);
    let mut out_bpms = Vec::with_capacity(bpm_capacity);
    let mut out_stops = Vec::with_capacity(stop_changes.len().saturating_sub(stop_idx));
    let mut out_warps: Vec<Segment> = Vec::new();

    if bpm > 0.0 && bpm <= FAST_BPM_WARP_F32 {
        push_sm_bpm(&mut out_bpms, 0.0, bpm);
    }

    let mut prev_beat = 0.0_f32;
    let mut warp_start: Option<f32> = None;
    let mut prewarp_bpm = 0.0_f32;
    let mut time_offset = 0.0_f32;

    while bpm_idx < bpm_changes.len() || stop_idx < stop_changes.len() {
        let is_bpm = stop_idx == stop_changes.len()
            || (bpm_idx < bpm_changes.len() && bpm_changes[bpm_idx].0 <= stop_changes[stop_idx].0);
        let (change_beat, change_val) = if is_bpm {
            bpm_changes[bpm_idx]
        } else {
            stop_changes[stop_idx]
        };

        if bpm <= FAST_BPM_WARP_F32 {
            time_offset += (change_beat - prev_beat) * 60.0 / bpm;
            if let Some(start) = warp_start
                && bpm > 0.0
                && time_offset > 0.0
            {
                let warp_end = change_beat - (time_offset * bpm / 60.0);
                if warp_end > start {
                    push_sm_warp(
                        &mut out_warps,
                        &bpm_changes,
                        &stop_changes,
                        Segment {
                            beat: f64::from(quantize_beat_f32(start)),
                            value: f64::from(quantize_beat_f32(warp_end - start)),
                        },
                    );
                }
                if bpm != prewarp_bpm {
                    push_sm_bpm(&mut out_bpms, start, bpm);
                }
                warp_start = None;
            }
        }
        prev_beat = change_beat;

        if is_bpm {
            if warp_start.is_none() && !(0.0..=FAST_BPM_WARP_F32).contains(&change_val) {
                warp_start = Some(change_beat);
                prewarp_bpm = bpm;
                time_offset = 0.0;
            } else if warp_start.is_none() {
                push_sm_bpm(&mut out_bpms, change_beat, change_val);
            }
            bpm = change_val;
            bpm_idx += 1;
        } else {
            if warp_start.is_none() && change_val < 0.0 {
                warp_start = Some(change_beat);
                prewarp_bpm = bpm;
                time_offset = change_val;
            } else if warp_start.is_none() {
                out_stops.push(Segment {
                    beat: f64::from(quantize_beat_f32(change_beat)),
                    value: f64::from(change_val),
                });
            } else {
                time_offset += change_val;
                if change_val > 0.0
                    && time_offset > 0.0
                    && let Some(start) = warp_start
                {
                    if change_beat > start {
                        push_sm_warp(
                            &mut out_warps,
                            &bpm_changes,
                            &stop_changes,
                            Segment {
                                beat: f64::from(quantize_beat_f32(start)),
                                value: f64::from(quantize_beat_f32(change_beat - start)),
                            },
                        );
                    }
                    out_stops.push(Segment {
                        beat: f64::from(quantize_beat_f32(change_beat)),
                        value: f64::from(time_offset),
                    });
                    if (0.0..=FAST_BPM_WARP_F32).contains(&bpm) {
                        if bpm != prewarp_bpm {
                            push_sm_bpm(&mut out_bpms, start, bpm);
                        }
                        warp_start = None;
                    } else {
                        warp_start = Some(change_beat);
                        time_offset = 0.0;
                    }
                }
            }
            stop_idx += 1;
        }
    }

    if let Some(start) = warp_start {
        let warp_end = if (0.0..=FAST_BPM_WARP_F32).contains(&bpm) {
            prev_beat - (time_offset * bpm / 60.0)
        } else {
            99_999_999.0_f32
        };
        if warp_end > start {
            push_sm_warp(
                &mut out_warps,
                &bpm_changes,
                &stop_changes,
                Segment {
                    beat: f64::from(quantize_beat_f32(start)),
                    value: f64::from(quantize_beat_f32(warp_end - start)),
                },
            );
        }
        if bpm != prewarp_bpm {
            push_sm_bpm(&mut out_bpms, start, bpm);
        }
    }

    let out_bpms = tidy_bpms(out_bpms);
    sort_segments_by_beat(&mut out_stops);
    sort_segments_by_beat(&mut out_warps);

    (out_bpms, out_stops, out_warps, f64::from(beat0_offset))
}

fn merge_extra_warps(mut warps: Vec<Segment>, extra_warps: Vec<Segment>) -> Vec<Segment> {
    if warps.is_empty() {
        return extra_warps;
    }
    warps.extend(extra_warps);
    warps
}

#[must_use]
pub fn convert_warps_and_delays_to_sm_stops<'a>(
    bpms: &[(f32, f32)],
    stops: &'a [(f32, f32)],
    delays: &[(f32, f32)],
    warps: &[(f32, f32)],
) -> Cow<'a, [(f32, f32)]> {
    if delays.is_empty() && warps.is_empty() {
        return Cow::Borrowed(stops);
    }

    // Perform a 3-way merge on the SM stop sources.
    // If two sources apply the same beat, sum up their values into a single pair.
    let mut sm_stops = Vec::with_capacity(stops.len() + delays.len() + warps.len());
    let mut bpm_index = 0;
    let mut stops_index = 0;
    let mut delays_index = 0;
    let mut warp_index = 0;

    while stops_index < stops.len() || delays_index < delays.len() || warp_index < warps.len() {
        // Find the smallest remaining key.
        let mut key = None;

        if stops_index < stops.len() {
            key = Some(stops[stops_index].0);
        }
        if delays_index < delays.len() && key.is_none_or(|k| delays[delays_index].0 < k) {
            key = Some(delays[delays_index].0);
        }
        if warp_index < warps.len() && key.is_none_or(|k| warps[warp_index].0 < k) {
            key = Some(warps[warp_index].0);
        }

        let Some(key) = key else { break };
        let mut value = 0.0;

        if stops_index < stops.len() && stops[stops_index].0 == key {
            value += stops[stops_index].1;
            stops_index += 1;
        }
        if delays_index < delays.len() && delays[delays_index].0 == key {
            value += delays[delays_index].1;
            delays_index += 1;
        }
        if warp_index < warps.len() && warps[warp_index].0 == key {
            // Convert at the merge head instead of materializing every warp first.
            while bpm_index + 1 < bpms.len() && bpms[bpm_index + 1].0 <= key {
                bpm_index += 1;
            }
            value -= 60.0 / bpms[bpm_index].1 * warps[warp_index].1;
            warp_index += 1;
        }

        if value != 0.0 {
            sm_stops.push((key, value));
        }
    }

    Cow::Owned(sm_stops)
}

// --- TimingData ---
#[derive(Debug, Clone, Copy)]
struct BeatTimePoint {
    beat: f64,
    bpm: f64,
}

#[derive(Debug, Clone, Copy)]
struct SpeedRuntime {
    start_time: f64,
    end_time: f64,
    prev_ratio: f64,
}

#[derive(Debug, Clone, Copy)]
struct ScrollPrefix {
    beat: f64,
    cum_displayed: f64,
    ratio: f64,
}

#[derive(Debug, Clone, Copy, Default)]
struct GetBeatState {
    bpm_idx: usize,
    stop_idx: usize,
    delay_idx: usize,
    warp_idx: usize,
    last_row: i32,
    last_time: f64,
    warp_destination: f64,
    is_warping: bool,
}

#[derive(Debug, Clone, Copy, Default)]
struct GetBeatStateF32 {
    bpm_idx: usize,
    stop_idx: usize,
    delay_idx: usize,
    warp_idx: usize,
    last_row: i32,
    last_time: f32,
    warp_destination: f32,
    is_warping: bool,
}

#[derive(PartialEq, Eq)]
enum TimingEvent {
    Bpm,
    Stop,
    Delay,
    Warp,
    WarpDest,
    Marker,
    NotFound,
}

#[derive(Debug, Clone, Default)]
pub struct TimingData {
    beat_to_time: Vec<BeatTimePoint>,
    segments: Vec<Segment>,
    segment_offsets: [usize; 5],
    speeds: Vec<SpeedSegment>,
    scrolls: Vec<Segment>,
    segment_rows: Vec<i32>,
    segment_row_offsets: [usize; 5],
    speed_runtime: Vec<SpeedRuntime>,
    scroll_prefix: Vec<ScrollPrefix>,
    beat0_offset_sec: f64,
    global_offset_sec: f64,
    max_bpm: f64,
}

#[repr(usize)]
#[derive(Clone, Copy)]
enum SegmentRowSet {
    Stops,
    Delays,
    Warps,
    Fakes,
}

impl TimingData {
    #[inline(always)]
    fn segments(&self, set: SegmentRowSet) -> &[Segment] {
        let index = set as usize;
        &self.segments[self.segment_offsets[index]..self.segment_offsets[index + 1]]
    }

    #[inline(always)]
    fn segment_rows(&self, set: SegmentRowSet) -> &[i32] {
        let index = set as usize;
        &self.segment_rows[self.segment_row_offsets[index]..self.segment_row_offsets[index + 1]]
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct BeatInfo {
    pub beat: f64,
    pub is_in_freeze: bool,
    pub is_in_delay: bool,
}

#[must_use]
pub fn timing_data_from_segments(
    song_offset: f64,
    global_offset: f64,
    segments: &TimingSegments,
) -> TimingData {
    let mut beat_to_time = Vec::with_capacity(segments.bpms.len().max(1));
    if segments.bpms.is_empty() {
        beat_to_time.push(BeatTimePoint {
            beat: 0.0,
            bpm: DEFAULT_BPM,
        });
    } else {
        beat_to_time.extend(segments.bpms.iter().map(|&(beat, bpm)| BeatTimePoint {
            beat: f64::from(beat),
            bpm: f64::from(bpm),
        }));
    }

    let segment_count =
        segments.stops.len() + segments.delays.len() + segments.warps.len() + segments.fakes.len();
    let mut packed_segments = Vec::with_capacity(segment_count);
    let mut segment_rows = Vec::with_capacity(segment_count);
    let mut segment_offsets = [0; 5];
    let mut segment_row_offsets = [0; 5];
    for (index, (source, require_positive)) in [
        (segments.stops.as_slice(), true),
        (segments.delays.as_slice(), true),
        (segments.warps.as_slice(), false),
        (segments.fakes.as_slice(), false),
    ]
    .into_iter()
    .enumerate()
    {
        append_packed_segments(
            &mut packed_segments,
            &mut segment_rows,
            source,
            require_positive,
        );
        segment_offsets[index + 1] = packed_segments.len();
        segment_row_offsets[index + 1] = segment_rows.len();
    }
    let scrolls: Vec<_> = segments
        .scrolls
        .iter()
        .map(|&(beat, value)| Segment {
            beat: f64::from(beat),
            value: f64::from(value),
        })
        .collect();
    let speeds: Vec<_> = segments
        .speeds
        .iter()
        .map(|(b, r, d, u)| SpeedSegment {
            beat: f64::from(*b),
            ratio: f64::from(*r),
            delay: f64::from(*d),
            unit: *u,
        })
        .collect();

    timing_data_build(
        song_offset + f64::from(segments.beat0_offset_adjust),
        global_offset,
        beat_to_time,
        packed_segments,
        segment_offsets,
        segment_rows,
        segment_row_offsets,
        speeds,
        scrolls,
    )
}

#[allow(clippy::too_many_arguments)]
pub fn timing_data_from_chart_data(
    song_offset: f64,
    global_offset: f64,
    chart_bpms: Option<&str>,
    global_bpms: &str,
    chart_stops: Option<&str>,
    global_stops: &str,
    chart_delays: Option<&str>,
    global_delays: &str,
    chart_warps: Option<&str>,
    global_warps: &str,
    chart_speeds: Option<&str>,
    global_speeds: &str,
    chart_scrolls: Option<&str>,
    global_scrolls: &str,
    chart_fakes: Option<&str>,
    global_fakes: &str,
    format: TimingFormat,
    cleaned: bool,
) -> TimingData {
    let bpms_str = chart_bpms.filter(|s| !s.is_empty()).unwrap_or(global_bpms);
    let mut bpms: Vec<(f64, f64)> = if cleaned {
        parse_bpm_map(bpms_str)
    } else {
        parse_bpm_map(clean_timing_map_cow(bpms_str).as_ref())
    };
    if bpms.is_empty() {
        bpms.push((0.0, DEFAULT_BPM));
    }

    let raw_stops = parse_optional_timing(chart_stops, global_stops, parse_segments, cleaned);
    let (mut bpms, stops, extra_warps, beat0_adj) = process_bpms_and_stops(format, bpms, raw_stops);
    let stops = tidy_row_segments(stops);
    if bpms.is_empty() {
        bpms.push((0.0, DEFAULT_BPM));
    }

    let q = |s: Segment| Segment {
        beat: quantize_beat(s.beat),
        value: s.value,
    };
    let qv = |s: Segment| Segment {
        beat: quantize_beat(s.beat),
        value: quantize_beat(s.value),
    };

    let delays = tidy_row_segments(
        parse_optional_timing(chart_delays, global_delays, parse_segments, cleaned)
            .into_iter()
            .map(q)
            .collect(),
    );
    let warps = parse_optional_timing(chart_warps, global_warps, parse_segments, cleaned);
    let warps = merge_extra_warps(warps, extra_warps);
    let warps = tidy_row_segments(warps.into_iter().map(qv).collect());
    let speeds = tidy_speed_segments(
        parse_optional_timing(chart_speeds, global_speeds, parse_speeds, cleaned)
            .into_iter()
            .map(|s| SpeedSegment {
                beat: quantize_beat(s.beat),
                ..s
            })
            .collect(),
    );
    let scrolls = tidy_scroll_segments(
        parse_optional_timing(chart_scrolls, global_scrolls, parse_segments, cleaned)
            .into_iter()
            .map(q)
            .collect(),
    );
    let fakes = tidy_row_segments(
        parse_optional_timing(chart_fakes, global_fakes, parse_segments_positive, cleaned)
            .into_iter()
            .map(qv)
            .collect(),
    );

    let beat_to_time = bpms
        .into_iter()
        .map(|(beat, bpm)| BeatTimePoint { beat, bpm })
        .collect();

    let (segments, segment_offsets) = pack_segments(stops, delays, warps, fakes);
    let (segment_rows, segment_row_offsets) = build_segment_row_storage(
        &segments[segment_offsets[0]..segment_offsets[1]],
        &segments[segment_offsets[1]..segment_offsets[2]],
        &segments[segment_offsets[2]..segment_offsets[3]],
        &segments[segment_offsets[3]..segment_offsets[4]],
    );

    timing_data_build(
        song_offset + beat0_adj,
        global_offset,
        beat_to_time,
        segments,
        segment_offsets,
        segment_rows,
        segment_row_offsets,
        speeds,
        scrolls,
    )
}

fn pack_segments(
    stops: Vec<Segment>,
    delays: Vec<Segment>,
    warps: Vec<Segment>,
    fakes: Vec<Segment>,
) -> (Vec<Segment>, [usize; 5]) {
    let segment_count = stops.len() + delays.len() + warps.len() + fakes.len();
    let mut segments = stops;
    segments.reserve_exact(segment_count - segments.len());
    let mut offsets = [0, segments.len(), 0, 0, 0];
    segments.extend(delays);
    offsets[2] = segments.len();
    segments.extend(warps);
    offsets[3] = segments.len();
    segments.extend(fakes);
    offsets[4] = segments.len();
    (segments, offsets)
}

fn timing_data_build(
    song_offset: f64,
    global_offset: f64,
    beat_to_time: Vec<BeatTimePoint>,
    segments: Vec<Segment>,
    segment_offsets: [usize; 5],
    segment_rows: Vec<i32>,
    segment_row_offsets: [usize; 5],
    speeds: Vec<SpeedSegment>,
    scrolls: Vec<Segment>,
) -> TimingData {
    let mut max_bpm = 0.0_f64;

    for point in &beat_to_time {
        if point.bpm.is_finite() && point.bpm > max_bpm {
            max_bpm = point.bpm;
        }
    }

    let mut timing = TimingData {
        beat_to_time,
        segments,
        segment_offsets,
        speeds,
        scrolls,
        segment_rows,
        segment_row_offsets,
        speed_runtime: Vec::new(),
        scroll_prefix: Vec::new(),
        beat0_offset_sec: song_offset,
        global_offset_sec: global_offset,
        max_bpm,
    };

    if !timing.speeds.is_empty() {
        let mut prev_ratio = 1.0;
        let mut cursor = BeatTimeCursor::new(&timing);
        timing.speed_runtime = timing
            .speeds
            .iter()
            .map(|seg| {
                let start = cursor.time_for_beat(seg.beat);
                let end = if seg.delay <= 0.0 {
                    start
                } else if seg.unit == SpeedUnit::Seconds {
                    start + seg.delay
                } else {
                    cursor.time_for_beat(seg.beat + seg.delay)
                };
                let rt = SpeedRuntime {
                    start_time: start,
                    end_time: end,
                    prev_ratio,
                };
                prev_ratio = seg.ratio;
                rt
            })
            .collect();
    }

    if !timing.scrolls.is_empty() {
        let mut cum = 0.0;
        let mut last_beat = 0.0;
        let mut last_ratio = 1.0;
        timing.scroll_prefix = timing
            .scrolls
            .iter()
            .map(|seg| {
                cum += (seg.beat - last_beat) * last_ratio;
                let p = ScrollPrefix {
                    beat: seg.beat,
                    cum_displayed: cum,
                    ratio: seg.value,
                };
                last_beat = seg.beat;
                last_ratio = seg.value;
                p
            })
            .collect();
    }

    timing
}

#[inline(always)]
#[must_use]
pub const fn beat0_offset_seconds(t: &TimingData) -> f64 {
    t.beat0_offset_sec
}
#[inline(always)]
#[must_use]
pub const fn beat0_group_offset_seconds(t: &TimingData) -> f64 {
    t.global_offset_sec
}
#[inline(always)]
#[must_use]
pub fn warps(t: &TimingData) -> &[Segment] {
    t.segments(SegmentRowSet::Warps)
}
#[inline(always)]
#[must_use]
pub fn stops(t: &TimingData) -> &[Segment] {
    t.segments(SegmentRowSet::Stops)
}
#[inline(always)]
#[must_use]
pub fn delays(t: &TimingData) -> &[Segment] {
    t.segments(SegmentRowSet::Delays)
}
#[inline(always)]
#[must_use]
pub fn speeds(t: &TimingData) -> &[SpeedSegment] {
    &t.speeds
}
#[inline(always)]
#[must_use]
pub fn scrolls(t: &TimingData) -> &[Segment] {
    &t.scrolls
}
#[inline(always)]
#[must_use]
pub fn fakes(t: &TimingData) -> &[Segment] {
    t.segments(SegmentRowSet::Fakes)
}

#[inline(always)]
#[must_use]
pub fn has_nonjudgable_rows(t: &TimingData) -> bool {
    !(warps(t).is_empty() && fakes(t).is_empty())
}

#[must_use]
pub fn bpm_segments(t: &TimingData) -> Vec<(f64, f64)> {
    t.beat_to_time.iter().map(|p| (p.beat, p.bpm)).collect()
}

#[inline(always)]
#[must_use]
pub fn is_fake_at_beat(t: &TimingData, beat: f64) -> bool {
    is_in_range_segment(fakes(t), t.segment_rows(SegmentRowSet::Fakes), beat)
}

#[inline(always)]
#[must_use]
pub fn is_fake_at_row(t: &TimingData, row: i32) -> bool {
    is_in_range_segment(
        fakes(t),
        t.segment_rows(SegmentRowSet::Fakes),
        note_row_to_beat(row),
    )
}

#[inline(always)]
#[must_use]
pub fn is_warp_at_beat(t: &TimingData, beat: f64) -> bool {
    is_warp_at_row(t, beat_to_note_row_f32(beat as f32))
}

#[inline(always)]
#[must_use]
pub fn is_warp_at_row(t: &TimingData, row: i32) -> bool {
    let Some(idx) = segment_index_at_row(t.segment_rows(SegmentRowSet::Warps), row) else {
        return false;
    };
    let seg = warps(t)[idx];
    if !(seg.value.is_finite() && seg.value > 0.0) {
        return false;
    }
    let beat_row = note_row_to_beat(row) as f32;
    let seg_beat = seg.beat as f32;
    if !(seg_beat <= beat_row && beat_row < seg_beat + seg.value as f32) {
        return false;
    }
    !(has_row(t.segment_rows(SegmentRowSet::Stops), row)
        || has_row(t.segment_rows(SegmentRowSet::Delays), row))
}

fn is_in_range_segment(segs: &[Segment], rows: &[i32], beat: f64) -> bool {
    let row = beat_to_note_row_f32(beat as f32);
    let Some(idx) = segment_index_at_row(rows, row) else {
        return false;
    };
    is_in_range_segment_at_row(segs, idx, row)
}

#[inline(always)]
fn is_in_range_segment_at_row(segs: &[Segment], idx: usize, row: i32) -> bool {
    let seg = segs[idx];
    if !seg.value.is_finite() {
        return false;
    }
    let beat_f = note_row_to_beat(row) as f32;
    beat_f >= seg.beat as f32 && beat_f < (seg.beat + seg.value) as f32
}

struct SegmentRowCursor<'a> {
    rows: &'a [i32],
    next: usize,
    last_row: i32,
}

impl<'a> SegmentRowCursor<'a> {
    const fn new(rows: &'a [i32]) -> Self {
        Self {
            rows,
            next: 0,
            last_row: i32::MIN,
        }
    }

    #[inline(always)]
    fn index_at(&mut self, row: i32) -> Option<usize> {
        if row < self.last_row {
            self.next = 0;
        }
        while self.next < self.rows.len() && self.rows[self.next] <= row {
            self.next += 1;
        }
        self.last_row = row;
        self.next.checked_sub(1)
    }
}

pub(crate) struct FakeRowCursor<'a> {
    timing: &'a TimingData,
    segments: SegmentRowCursor<'a>,
}

impl<'a> FakeRowCursor<'a> {
    #[inline]
    pub(crate) fn new(timing: &'a TimingData) -> Self {
        Self {
            timing,
            segments: SegmentRowCursor::new(timing.segment_rows(SegmentRowSet::Fakes)),
        }
    }

    #[inline(always)]
    pub(crate) fn is_fake(&mut self, row: i32) -> bool {
        let Some(idx) = self.segments.index_at(row) else {
            return false;
        };
        is_in_range_segment_at_row(fakes(self.timing), idx, row)
    }
}

pub(crate) struct JudgableRowCursor<'a> {
    timing: &'a TimingData,
    warps: SegmentRowCursor<'a>,
    fakes: SegmentRowCursor<'a>,
}

impl<'a> JudgableRowCursor<'a> {
    #[inline]
    pub(crate) fn new(timing: &'a TimingData) -> Self {
        Self {
            timing,
            warps: SegmentRowCursor::new(timing.segment_rows(SegmentRowSet::Warps)),
            fakes: SegmentRowCursor::new(timing.segment_rows(SegmentRowSet::Fakes)),
        }
    }

    #[inline(always)]
    pub(crate) fn is_judgable(&mut self, row: i32) -> bool {
        if let Some(idx) = self.warps.index_at(row) {
            let seg = warps(self.timing)[idx];
            if seg.value.is_finite() && seg.value > 0.0 {
                let beat_row = note_row_to_beat(row) as f32;
                let seg_beat = seg.beat as f32;
                if seg_beat <= beat_row
                    && beat_row < seg_beat + seg.value as f32
                    && !(has_row(self.timing.segment_rows(SegmentRowSet::Stops), row)
                        || has_row(self.timing.segment_rows(SegmentRowSet::Delays), row))
                {
                    return false;
                }
            }
        }

        let Some(idx) = self.fakes.index_at(row) else {
            return true;
        };
        !is_in_range_segment_at_row(fakes(self.timing), idx, row)
    }
}

#[inline(always)]
#[must_use]
pub fn is_judgable_at_row(t: &TimingData, row: i32) -> bool {
    !is_warp_at_row(t, row) && !is_fake_at_row(t, row)
}

#[inline(always)]
#[must_use]
pub fn is_judgable_at_beat(t: &TimingData, beat: f64) -> bool {
    is_judgable_at_row(t, beat_to_note_row_f32(beat as f32))
}

#[must_use]
pub fn get_beat_info_from_time(t: &TimingData, time: f64) -> BeatInfo {
    let elapsed = time + t.global_offset_sec;
    let start_time = -t.beat0_offset_sec - t.global_offset_sec;
    get_beat_internal(t, elapsed, start_time)
}

#[must_use]
pub fn get_beat_for_time(t: &TimingData, time: f64) -> f64 {
    get_beat_info_from_time(t, time).beat
}

#[must_use]
pub fn get_time_for_beat(t: &TimingData, beat: f64) -> f64 {
    get_time_internal(t, beat) - t.global_offset_sec
}

pub(crate) type FixedTimingParts = (f32, f32, f64);

struct BeatTimeCursor<'a> {
    timing: &'a TimingData,
    state: GetBeatState,
    last_target_row: i32,
}

impl<'a> BeatTimeCursor<'a> {
    fn new(timing: &'a TimingData) -> Self {
        Self {
            timing,
            state: GetBeatState {
                last_time: -timing.beat0_offset_sec - timing.global_offset_sec,
                ..Default::default()
            },
            last_target_row: i32::MIN,
        }
    }

    fn time_for_beat(&mut self, target_beat: f64) -> f64 {
        let target_row = beat_to_note_row(target_beat);
        if target_row < self.last_target_row {
            self.state = GetBeatState {
                last_time: -self.timing.beat0_offset_sec - self.timing.global_offset_sec,
                ..Default::default()
            };
        }
        let elapsed = get_elapsed_time(self.timing, &mut self.state, target_beat);
        self.last_target_row = target_row;
        elapsed - self.timing.global_offset_sec
    }
}

#[inline(always)]
pub(crate) fn fixed_timing_parts(t: &TimingData) -> Option<FixedTimingParts> {
    if t.beat_to_time.len() == 1
        && t.beat_to_time[0].beat == 0.0
        && stops(t).is_empty()
        && delays(t).is_empty()
        && warps(t).is_empty()
    {
        let start = (-t.beat0_offset_sec - t.global_offset_sec) as f32;
        let bps = t.beat_to_time[0].bpm as f32 / 60.0;
        Some((start, bps, t.global_offset_sec))
    } else {
        None
    }
}

#[inline(always)]
pub(crate) fn fixed_time_for_beat(parts: FixedTimingParts, target_beat: f64) -> f64 {
    let (start, bps, global_offset) = parts;
    let row = beat_to_note_row_f32(target_beat as f32);
    f64::from(start + note_row_to_beat_f32(row) / bps) - global_offset
}

pub(crate) struct BeatTimeCursorF32<'a> {
    timing: &'a TimingData,
    fixed: Option<FixedTimingParts>,
    state: GetBeatStateF32,
    last_target_row: i32,
}

impl<'a> BeatTimeCursorF32<'a> {
    #[inline]
    pub(crate) fn new(timing: &'a TimingData) -> Self {
        Self {
            timing,
            fixed: fixed_timing_parts(timing),
            state: GetBeatStateF32 {
                last_time: (-timing.beat0_offset_sec - timing.global_offset_sec) as f32,
                ..Default::default()
            },
            last_target_row: i32::MIN,
        }
    }

    #[inline]
    pub(crate) fn time_for_beat(&mut self, target_beat: f64) -> f64 {
        if let Some(parts) = self.fixed {
            return fixed_time_for_beat(parts, target_beat);
        }

        let target_row = beat_to_note_row_f32(target_beat as f32);
        if target_row < self.last_target_row {
            self.state = GetBeatStateF32 {
                last_time: (-self.timing.beat0_offset_sec - self.timing.global_offset_sec) as f32,
                ..Default::default()
            };
        }
        let elapsed = get_elapsed_time_f32(self.timing, &mut self.state, target_beat as f32);
        self.last_target_row = target_row;
        f64::from(elapsed) - self.timing.global_offset_sec
    }
}

pub(crate) fn get_time_for_beat_f32(t: &TimingData, target_beat: f64) -> f64 {
    if let Some(parts) = fixed_timing_parts(t) {
        return fixed_time_for_beat(parts, target_beat);
    }

    let mut state = GetBeatStateF32 {
        last_time: (-t.beat0_offset_sec - t.global_offset_sec) as f32,
        ..Default::default()
    };
    let elapsed = get_elapsed_time_f32(t, &mut state, target_beat as f32);
    f64::from(elapsed) - t.global_offset_sec
}

fn get_time_internal(t: &TimingData, target_beat: f64) -> f64 {
    let mut state = GetBeatState {
        last_time: -t.beat0_offset_sec - t.global_offset_sec,
        ..Default::default()
    };
    get_elapsed_time(t, &mut state, target_beat)
}

fn get_beat_internal(t: &TimingData, elapsed: f64, start_time: f64) -> BeatInfo {
    let mut state = GetBeatState {
        last_time: start_time,
        ..Default::default()
    };
    let mut bps = get_bpm_for_beat(t, 0.0) / 60.0;
    let stops = stops(t);
    let delays = delays(t);
    let warps = warps(t);

    loop {
        let (event_row, event_type) = find_next_event(t, &state, 0.0, false);
        if event_type == TimingEvent::NotFound {
            break;
        }

        let time_to_event = if state.is_warping {
            0.0
        } else {
            note_row_to_beat(event_row - state.last_row) / bps
        };
        let next_time = state.last_time + time_to_event;
        if elapsed < next_time {
            break;
        }
        state.last_time = next_time;

        match event_type {
            TimingEvent::WarpDest => state.is_warping = false,
            TimingEvent::Bpm => {
                bps = t.beat_to_time[state.bpm_idx].bpm / 60.0;
                state.bpm_idx += 1;
            }
            TimingEvent::Delay => {
                let d = delays[state.delay_idx].value;
                if elapsed < state.last_time + d {
                    return BeatInfo {
                        beat: delays[state.delay_idx].beat,
                        is_in_delay: true,
                        is_in_freeze: false,
                    };
                }
                state.last_time += d;
                state.delay_idx += 1;
            }
            TimingEvent::Stop => {
                let d = stops[state.stop_idx].value;
                if elapsed < state.last_time + d {
                    return BeatInfo {
                        beat: stops[state.stop_idx].beat,
                        is_in_freeze: true,
                        is_in_delay: false,
                    };
                }
                state.last_time += d;
                state.stop_idx += 1;
            }
            TimingEvent::Warp => {
                state.is_warping = true;
                let w = &warps[state.warp_idx];
                state.warp_destination = state.warp_destination.max(w.beat + w.value);
                state.warp_idx += 1;
            }
            TimingEvent::Marker | TimingEvent::NotFound => {}
        }
        state.last_row = event_row;
    }

    BeatInfo {
        beat: (elapsed - state.last_time).mul_add(bps, note_row_to_beat(state.last_row)),
        is_in_freeze: false,
        is_in_delay: false,
    }
}

fn get_elapsed_time(t: &TimingData, state: &mut GetBeatState, target_beat: f64) -> f64 {
    let find_marker = target_beat < f64::MAX;
    let mut bps = if state.bpm_idx == 0 {
        get_bpm_for_beat(t, note_row_to_beat(state.last_row))
    } else {
        t.beat_to_time[state.bpm_idx - 1].bpm
    } / 60.0;
    let stops = stops(t);
    let delays = delays(t);
    let warps = warps(t);

    loop {
        let (event_row, event_type) = find_next_event(t, state, target_beat, find_marker);
        if event_type == TimingEvent::NotFound {
            break;
        }

        let dt = if state.is_warping {
            0.0
        } else {
            note_row_to_beat(event_row - state.last_row) / bps
        };
        if event_type == TimingEvent::Marker {
            return state.last_time + dt;
        }
        state.last_time += dt;

        match event_type {
            TimingEvent::WarpDest => state.is_warping = false,
            TimingEvent::Bpm => {
                bps = t.beat_to_time[state.bpm_idx].bpm / 60.0;
                state.bpm_idx += 1;
            }
            TimingEvent::Stop => {
                state.last_time += stops[state.stop_idx].value;
                state.stop_idx += 1;
            }
            TimingEvent::Delay => {
                state.last_time += delays[state.delay_idx].value;
                state.delay_idx += 1;
            }
            TimingEvent::Marker => unreachable!("marker is returned before state mutation"),
            TimingEvent::Warp => {
                state.is_warping = true;
                let w = &warps[state.warp_idx];
                state.warp_destination = state.warp_destination.max(w.beat + w.value);
                state.warp_idx += 1;
            }
            TimingEvent::NotFound => {}
        }
        state.last_row = event_row;
    }
    state.last_time
}

fn get_elapsed_time_f32(t: &TimingData, state: &mut GetBeatStateF32, target_beat: f32) -> f32 {
    let find_marker = target_beat < f32::MAX;
    let mut bps = if state.bpm_idx == 0 {
        get_bpm_for_row_f32(t, state.last_row)
    } else {
        t.beat_to_time[state.bpm_idx - 1].bpm as f32
    } / 60.0;
    let mut curr_segment = state.bpm_idx + state.warp_idx + state.stop_idx + state.delay_idx;
    let stops = stops(t);
    let delays = delays(t);
    let warps = warps(t);

    while curr_segment < u32::MAX as usize {
        let (event_row, event_type) = find_next_event_f32(t, state, target_beat, find_marker);
        if event_type == TimingEvent::NotFound {
            break;
        }

        let dt = if state.is_warping {
            0.0
        } else {
            note_row_to_beat_f32(event_row - state.last_row) / bps
        };
        if event_type == TimingEvent::Marker {
            return state.last_time + dt;
        }
        state.last_time += dt;

        match event_type {
            TimingEvent::WarpDest => state.is_warping = false,
            TimingEvent::Bpm => {
                bps = t.beat_to_time[state.bpm_idx].bpm as f32 / 60.0;
                state.bpm_idx += 1;
                curr_segment += 1;
            }
            TimingEvent::Stop => {
                state.last_time += stops[state.stop_idx].value as f32;
                state.stop_idx += 1;
                curr_segment += 1;
            }
            TimingEvent::Delay => {
                state.last_time += delays[state.delay_idx].value as f32;
                state.delay_idx += 1;
                curr_segment += 1;
            }
            TimingEvent::Marker => unreachable!("marker is returned before state mutation"),
            TimingEvent::Warp => {
                state.is_warping = true;
                let w = &warps[state.warp_idx];
                let warp_sum = w.value as f32 + w.beat as f32;
                if warp_sum > state.warp_destination {
                    state.warp_destination = warp_sum;
                }
                state.warp_idx += 1;
                curr_segment += 1;
            }
            TimingEvent::NotFound => {}
        }
        state.last_row = event_row;
    }
    state.last_time
}

fn find_next_event(
    t: &TimingData,
    state: &GetBeatState,
    beat: f64,
    find_marker: bool,
) -> (i32, TimingEvent) {
    let mut row = i32::MAX;
    let mut event = TimingEvent::NotFound;
    let stops = stops(t);
    let delays = delays(t);
    let warps = warps(t);

    if state.is_warping {
        let r = beat_to_note_row(state.warp_destination);
        if r < row {
            row = r;
            event = TimingEvent::WarpDest;
        }
    }
    if state.bpm_idx < t.beat_to_time.len() {
        let r = beat_to_note_row(t.beat_to_time[state.bpm_idx].beat);
        if r < row {
            row = r;
            event = TimingEvent::Bpm;
        }
    }
    if state.delay_idx < delays.len() {
        let r = beat_to_note_row(delays[state.delay_idx].beat);
        if r < row {
            row = r;
            event = TimingEvent::Delay;
        }
    }
    if find_marker {
        let r = beat_to_note_row(beat);
        if r < row {
            row = r;
            event = TimingEvent::Marker;
        }
    }
    if state.stop_idx < stops.len() {
        let r = beat_to_note_row(stops[state.stop_idx].beat);
        if r < row {
            row = r;
            event = TimingEvent::Stop;
        }
    }
    if state.warp_idx < warps.len() {
        let r = beat_to_note_row(warps[state.warp_idx].beat);
        if r < row {
            row = r;
            event = TimingEvent::Warp;
        }
    }

    (row, event)
}

fn find_next_event_f32(
    t: &TimingData,
    state: &GetBeatStateF32,
    beat: f32,
    find_marker: bool,
) -> (i32, TimingEvent) {
    let mut row = i32::MAX;
    let mut event = TimingEvent::NotFound;
    let stops = stops(t);
    let delays = delays(t);
    let warps = warps(t);

    if state.is_warping {
        let r = beat_to_note_row_f32(state.warp_destination);
        if r < row {
            row = r;
            event = TimingEvent::WarpDest;
        }
    }
    if state.bpm_idx < t.beat_to_time.len() {
        let r = beat_to_note_row_f32(t.beat_to_time[state.bpm_idx].beat as f32);
        if r < row {
            row = r;
            event = TimingEvent::Bpm;
        }
    }
    if state.delay_idx < delays.len() {
        let r = beat_to_note_row_f32(delays[state.delay_idx].beat as f32);
        if r < row {
            row = r;
            event = TimingEvent::Delay;
        }
    }
    if find_marker {
        let r = beat_to_note_row_f32(beat);
        if r < row {
            row = r;
            event = TimingEvent::Marker;
        }
    }
    if state.stop_idx < stops.len() {
        let r = beat_to_note_row_f32(stops[state.stop_idx].beat as f32);
        if r < row {
            row = r;
            event = TimingEvent::Stop;
        }
    }
    if state.warp_idx < warps.len() {
        let r = beat_to_note_row_f32(warps[state.warp_idx].beat as f32);
        if r < row {
            row = r;
            event = TimingEvent::Warp;
        }
    }

    (row, event)
}

fn get_bpm_for_row_f32(t: &TimingData, row: i32) -> f32 {
    if t.beat_to_time.is_empty() {
        return DEFAULT_BPM as f32;
    }
    let pos = t
        .beat_to_time
        .partition_point(|p| beat_to_note_row_f32(p.beat as f32) <= row);
    if pos == 0 {
        t.beat_to_time[0].bpm as f32
    } else {
        t.beat_to_time[pos - 1].bpm as f32
    }
}

#[must_use]
pub fn get_bpm_for_beat(t: &TimingData, beat: f64) -> f64 {
    if t.beat_to_time.is_empty() {
        return DEFAULT_BPM;
    }
    let idx = t.beat_to_time.partition_point(|p| p.beat <= beat);
    t.beat_to_time[idx.saturating_sub(1)].bpm
}

pub fn get_capped_max_bpm(t: &TimingData, cap: Option<f64>) -> f64 {
    let mut max = t.max_bpm.max(
        t.beat_to_time
            .iter()
            .map(|p| p.bpm)
            .filter(|b| b.is_finite() && *b > 0.0)
            .fold(0.0, f64::max),
    );
    if let Some(c) = cap
        && c > 0.0
    {
        max = max.min(c);
    }
    if max > 0.0 { max } else { DEFAULT_BPM }
}

#[must_use]
pub fn get_displayed_beat(t: &TimingData, beat: f64) -> f64 {
    if t.scroll_prefix.is_empty() || beat < t.scroll_prefix[0].beat {
        return beat;
    }
    let idx = t
        .scroll_prefix
        .partition_point(|p| p.beat <= beat)
        .saturating_sub(1);
    let p = t.scroll_prefix[idx];
    (beat - p.beat).mul_add(p.ratio, p.cum_displayed)
}

#[must_use]
pub fn get_speed_multiplier(t: &TimingData, beat: f64, time: f64) -> f64 {
    if t.speeds.is_empty() {
        return 1.0;
    }
    let idx = t.speeds.partition_point(|s| s.beat <= beat);
    if idx == 0 {
        return 1.0;
    }
    let i = idx - 1;
    let seg = &t.speeds[i];
    let rt = &t.speed_runtime[i];

    if time >= rt.end_time || seg.delay <= 0.0 {
        return seg.ratio;
    }
    if time < rt.start_time {
        return rt.prev_ratio;
    }
    let progress = (time - rt.start_time) / (rt.end_time - rt.start_time);
    (seg.ratio - rt.prev_ratio).mul_add(progress, rt.prev_ratio)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_bpm_display_matches_itg_text() {
        assert_eq!(native_bpms_display(&[]).to_string(), "");
        assert_eq!(
            native_bpms_display(&[(0.0, 120.0), (4.0, 180.0)]).to_string(),
            "0.000000=120.000000,4.000000=180.000000"
        );
    }

    #[test]
    fn parse_segments_preserves_sparse_entries_and_row_beats() {
        assert!(parse_segments("").is_empty());

        let segments = parse_segments(" ,0=1,missing,4=2,=3,8=nope,96r=-0.5,12=NaN,16=inf, ");
        assert_eq!(segments.len(), 3);
        assert_eq!((segments[0].beat, segments[0].value), (0.0, 1.0));
        assert_eq!((segments[1].beat, segments[1].value), (4.0, 2.0));
        assert_eq!((segments[2].beat, segments[2].value), (2.0, -0.5));

        let positive =
            parse_segments_positive(" ,0=1,missing,4=2,=3,8=nope,96r=-0.5,12=NaN,16=inf, ");
        assert_eq!(positive.len(), 2);
        assert_eq!((positive[0].beat, positive[0].value), (0.0, 1.0));
        assert_eq!((positive[1].beat, positive[1].value), (4.0, 2.0));
    }

    #[test]
    fn parse_segments_large_map_preserves_all_entries() {
        use std::fmt::Write;

        let mut map = String::with_capacity(4_096 * 12);
        for idx in 0..4_096 {
            if idx != 0 {
                map.push(',');
            }
            write!(&mut map, "{}={}", idx * 4, 60 + idx % 300).unwrap();
        }
        assert!(map.len() >= 32 * 1_024);

        let segments = parse_segments(&map);
        assert_eq!(segments.len(), 4_096);
        assert_eq!((segments[0].beat, segments[0].value), (0.0, 60.0));
        assert_eq!(
            (segments[4_095].beat, segments[4_095].value),
            (16_380.0, 255.0)
        );
    }

    #[test]
    fn row_beats_follow_measure_spacing() {
        let actual = compute_row_to_beat(b"1000\n0100\n,\n0010\n0001\n");
        assert_eq!(actual, vec![0.0, 2.0, 4.0, 6.0]);
    }

    #[test]
    fn parse_speeds_preserves_sparse_entries_and_units() {
        assert!(parse_speeds("").is_empty());

        let speeds = parse_speeds(
            " ,0=1=0=0,missing,4=2=0.5=1,=3=0=1,8=nope=1=1,12=3=0=1=ignored,96r=4=1=0",
        );
        assert_eq!(speeds.len(), 4);
        assert_eq!(
            (speeds[0].beat, speeds[0].ratio, speeds[0].delay),
            (0.0, 1.0, 0.0)
        );
        assert_eq!(speeds[0].unit, SpeedUnit::Beats);
        assert_eq!(
            (speeds[1].beat, speeds[1].ratio, speeds[1].delay),
            (4.0, 2.0, 0.5)
        );
        assert_eq!(speeds[1].unit, SpeedUnit::Seconds);
        assert_eq!(speeds[2].beat, 12.0);
        assert_eq!(speeds[2].unit, SpeedUnit::Seconds);
        assert_eq!(speeds[3].beat, 2.0);
        assert_eq!(speeds[3].unit, SpeedUnit::Beats);
    }

    fn build_segment_rows_sorted(segments: &[Segment], require_positive: bool) -> Vec<i32> {
        let mut rows: Vec<_> = segments
            .iter()
            .filter(|segment| {
                !require_positive || (segment.value.is_finite() && segment.value > 0.0)
            })
            .map(|segment| beat_to_note_row_f32(segment.beat as f32))
            .collect();
        rows.sort_unstable();
        if require_positive {
            rows.dedup();
        }
        rows
    }
    fn tidy_scroll_segments_slow(segments: Vec<Segment>) -> Vec<Segment> {
        let mut out = Vec::with_capacity(segments.len());
        for mut seg in segments {
            let row = beat_to_note_row(seg.beat);
            seg.beat = note_row_to_beat(row);
            if out.is_empty() {
                out.push(seg);
            } else {
                add_scroll_segment_slow(&mut out, seg, row);
            }
        }
        out
    }

    fn tidy_speed_segments_slow(segments: Vec<SpeedSegment>) -> Vec<SpeedSegment> {
        let mut out = Vec::with_capacity(segments.len());
        for mut seg in segments {
            let row = beat_to_note_row(seg.beat);
            seg.beat = note_row_to_beat(row);
            if out.is_empty() {
                out.push(seg);
            } else {
                add_speed_segment_slow(&mut out, seg, row);
            }
        }
        out
    }
    fn assert_segment_bits_eq(actual: &[Segment], expected: &[Segment]) {
        assert_eq!(actual.len(), expected.len());
        for (actual, expected) in actual.iter().zip(expected) {
            assert_eq!(actual.beat.to_bits(), expected.beat.to_bits());
            assert_eq!(actual.value.to_bits(), expected.value.to_bits());
        }
    }

    #[test]
    fn generated_warp_merge_appends_in_order() {
        let extra = vec![Segment {
            beat: 4.0,
            value: 2.0,
        }];
        assert_segment_bits_eq(&merge_extra_warps(Vec::new(), extra.clone()), &extra);

        let expected = [
            Segment {
                beat: 0.0,
                value: 1.0,
            },
            extra[0],
        ];
        assert_segment_bits_eq(&merge_extra_warps(vec![expected[0]], extra), &expected);
    }

    #[test]
    fn generated_warps_follow_negative_stops() {
        let stops: Vec<_> = (0..64)
            .map(|index| Segment {
                beat: f64::from(index) * 4.0 + 2.0,
                value: -0.5,
            })
            .collect();
        let output = process_bpms_and_stops_sm(&[(0.0, 120.0)], &stops);
        assert_eq!(output.2.len(), stops.len());

        let no_warps = process_bpms_and_stops_sm(
            &[(0.0, 120.0), (4.0, 180.0)],
            &[Segment {
                beat: 2.0,
                value: 0.25,
            }],
        );
        assert!(no_warps.2.is_empty());
        assert_eq!(no_warps.2.capacity(), 0);
    }

    fn assert_bpm_bits_eq(actual: &[(f64, f64)], expected: &[(f64, f64)]) {
        assert_eq!(actual.len(), expected.len());
        for (actual, expected) in actual.iter().zip(expected) {
            assert_eq!(actual.0.to_bits(), expected.0.to_bits());
            assert_eq!(actual.1.to_bits(), expected.1.to_bits());
        }
    }

    fn assert_speed_bits_eq(actual: &[SpeedSegment], expected: &[SpeedSegment]) {
        assert_eq!(actual.len(), expected.len());
        for (actual, expected) in actual.iter().zip(expected) {
            assert_eq!(actual.beat.to_bits(), expected.beat.to_bits());
            assert_eq!(actual.ratio.to_bits(), expected.ratio.to_bits());
            assert_eq!(actual.delay.to_bits(), expected.delay.to_bits());
            assert_eq!(actual.unit, expected.unit);
        }
    }

    fn assert_change_bits_eq(actual: &[(f32, f32)], expected: &[(f32, f32)]) {
        assert_eq!(actual.len(), expected.len());
        for (actual, expected) in actual.iter().zip(expected) {
            assert_eq!(actual.0.to_bits(), expected.0.to_bits());
            assert_eq!(actual.1.to_bits(), expected.1.to_bits());
        }
    }

    fn variable_timing() -> TimingData {
        timing_data_from_chart_data(
            0.125,
            0.0,
            None,
            "0=120,4=180,12=90,24=240,48=150",
            None,
            "2=0.500,16=0.250,52=0.125",
            None,
            "8=0.125,32=0.375",
            None,
            "20=4,40=2",
            None,
            "",
            None,
            "",
            None,
            "",
            TimingFormat::Ssc,
            true,
        )
    }

    #[test]
    fn ordered_change_sort_matches_stable_sort_bit_for_bit() {
        let cases = [
            Vec::new(),
            vec![(0.0, 120.0)],
            vec![(-4.0, 90.0), (0.0, 120.0), (0.0, 180.0), (8.0, 60.0)],
            vec![(8.0, 60.0), (-0.0, 120.0), (4.0, 90.0), (0.0, 180.0)],
        ];

        for mut changes in cases {
            let mut expected = changes.clone();
            expected.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(Ordering::Equal));
            sort_changes_by_beat(&mut changes);
            assert_change_bits_eq(&changes, &expected);
        }
    }

    #[test]
    fn generated_change_sort_matches_stable_sort_bit_for_bit() {
        let mut state = 0x1f83_d9ab_fb41_bd6b_u64;
        for len in 0..128 {
            let mut changes: Vec<_> = (0..len)
                .map(|idx| {
                    state ^= state << 13;
                    state ^= state >> 7;
                    state ^= state << 17;
                    let beat = if len % 2 == 0 {
                        idx as f32 / 3.0
                    } else {
                        (state % 64) as f32 - 16.0
                    };
                    (beat, f32::from_bits(state as u32))
                })
                .collect();
            let mut expected = changes.clone();
            expected.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(Ordering::Equal));
            sort_changes_by_beat(&mut changes);
            assert_change_bits_eq(&changes, &expected);
        }
    }

    #[test]
    fn segment_sort_fast_path_matches_stable_sort_bit_for_bit() {
        let cases = [
            Vec::new(),
            vec![Segment {
                beat: 0.0,
                value: 1.0,
            }],
            vec![
                Segment {
                    beat: -4.0,
                    value: 1.0,
                },
                Segment {
                    beat: 0.0,
                    value: 2.0,
                },
                Segment {
                    beat: 0.0,
                    value: 3.0,
                },
                Segment {
                    beat: 8.0,
                    value: 4.0,
                },
            ],
            vec![
                Segment {
                    beat: 8.0,
                    value: 1.0,
                },
                Segment {
                    beat: -0.0,
                    value: 2.0,
                },
                Segment {
                    beat: 4.0,
                    value: 3.0,
                },
                Segment {
                    beat: 0.0,
                    value: 4.0,
                },
            ],
        ];

        for mut segments in cases {
            let mut expected = segments.clone();
            expected.sort_by(|a, b| a.beat.partial_cmp(&b.beat).unwrap_or(Ordering::Less));
            sort_segments_by_beat(&mut segments);
            assert_segment_bits_eq(&segments, &expected);
        }
    }

    #[test]
    fn timing_data_from_segments_preserves_bpms_and_supplies_default() {
        for bpms in [
            Vec::new(),
            vec![(0.0, 120.0)],
            vec![(-0.0, 90.0), (4.0, 180.0), (12.0, 60.0)],
        ] {
            let expected = if bpms.is_empty() {
                vec![(0.0, DEFAULT_BPM)]
            } else {
                bpms.iter()
                    .map(|&(beat, bpm)| (f64::from(beat), f64::from(bpm)))
                    .collect()
            };
            let segments = TimingSegments {
                beat0_offset_adjust: 0.0,
                bpms,
                stops: Vec::new(),
                delays: Vec::new(),
                warps: Vec::new(),
                speeds: Vec::new(),
                scrolls: Vec::new(),
                fakes: Vec::new(),
            };
            let timing = timing_data_from_segments(0.0, 0.0, &segments);
            assert_bpm_bits_eq(&bpm_segments(&timing), &expected);
        }
    }

    #[test]
    fn packed_segment_rows_match_independent_vectors() {
        let segments = TimingSegments {
            bpms: vec![(0.0, 120.0)],
            stops: vec![(8.0, 0.5), (4.0, -1.0), (8.0, 0.25)],
            delays: vec![(2.0, 0.125), (6.0, f32::NAN), (10.0, 0.25)],
            warps: vec![(12.0, 4.0), (1.0, 2.0)],
            fakes: vec![(16.0, 1.0), (3.0, 0.5)],
            ..TimingSegments::default()
        };
        let timing = timing_data_from_segments(0.0, 0.0, &segments);
        for (set, values, require_positive) in [
            (SegmentRowSet::Stops, stops(&timing), true),
            (SegmentRowSet::Delays, delays(&timing), true),
            (SegmentRowSet::Warps, warps(&timing), false),
            (SegmentRowSet::Fakes, fakes(&timing), false),
        ] {
            assert_eq!(
                timing.segment_rows(set),
                build_segment_rows_sorted(values, require_positive)
            );
        }
    }
    #[test]
    fn ordered_speed_and_scroll_cleanup_match_search_path_bit_for_bit() {
        let scrolls = vec![
            Segment {
                beat: 0.0,
                value: 1.0,
            },
            Segment {
                beat: 1.0,
                value: 1.0,
            },
            Segment {
                beat: 2.0,
                value: 2.0,
            },
            Segment {
                beat: 2.0,
                value: 0.5,
            },
            Segment {
                beat: 3.0,
                value: 0.5,
            },
            Segment {
                beat: 4.0,
                value: 1.5,
            },
        ];
        let expected = tidy_scroll_segments_slow(scrolls.clone());
        let actual = tidy_scroll_segments(scrolls);
        assert_segment_bits_eq(&actual, &expected);

        let speeds = vec![
            SpeedSegment {
                beat: 0.0,
                ratio: 1.0,
                delay: 0.0,
                unit: SpeedUnit::Beats,
            },
            SpeedSegment {
                beat: 1.0,
                ratio: 1.0,
                delay: 0.0,
                unit: SpeedUnit::Beats,
            },
            SpeedSegment {
                beat: 2.0,
                ratio: 2.0,
                delay: 1.0,
                unit: SpeedUnit::Beats,
            },
            SpeedSegment {
                beat: 2.0,
                ratio: 0.5,
                delay: 0.25,
                unit: SpeedUnit::Seconds,
            },
            SpeedSegment {
                beat: 4.0,
                ratio: 1.5,
                delay: 0.0,
                unit: SpeedUnit::Seconds,
            },
        ];
        let expected = tidy_speed_segments_slow(speeds.clone());
        let actual = tidy_speed_segments(speeds);
        assert_speed_bits_eq(&actual, &expected);
    }

    #[test]
    fn generated_speed_and_scroll_cleanup_match_search_paths() {
        let mut state = 0xa54f_f53a_5f1d_36f1_u64;
        for len in 0..96 {
            let mut scrolls = Vec::with_capacity(len);
            let mut speeds = Vec::with_capacity(len);
            for idx in 0..len {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                let row = if len % 2 == 0 {
                    idx as i32 / 2
                } else {
                    (state % 64) as i32 - 16
                };
                let beat = note_row_to_beat(row);
                scrolls.push(Segment {
                    beat,
                    value: 0.5 + (state % 5) as f64 * 0.5,
                });
                speeds.push(SpeedSegment {
                    beat,
                    ratio: 0.5 + (state % 7) as f64 * 0.25,
                    delay: (state % 3) as f64 * 0.5,
                    unit: if state & 1 == 0 {
                        SpeedUnit::Beats
                    } else {
                        SpeedUnit::Seconds
                    },
                });
            }

            let expected = tidy_scroll_segments_slow(scrolls.clone());
            let actual = tidy_scroll_segments(scrolls);
            assert_segment_bits_eq(&actual, &expected);

            let expected = tidy_speed_segments_slow(speeds.clone());
            let actual = tidy_speed_segments(speeds);
            assert_speed_bits_eq(&actual, &expected);
        }
    }
    #[test]
    fn sequential_beat_cursor_matches_independent_queries_bit_for_bit() {
        let timing = variable_timing();
        let mut cursor = BeatTimeCursorF32::new(&timing);
        let targets = [
            -1.0, 0.0, 1.0, 2.0, 4.0, 7.5, 8.0, 12.0, 16.0, 20.0, 21.0, 24.0, 32.0, 40.0, 41.0,
            48.0, 52.0, 64.0,
        ];

        for target in targets {
            assert_eq!(
                cursor.time_for_beat(target).to_bits(),
                get_time_for_beat_f32(&timing, target).to_bits(),
                "target beat {target}"
            );
        }
    }

    #[test]
    fn sequential_beat_cursor_resets_for_decreasing_queries() {
        let timing = variable_timing();
        let mut cursor = BeatTimeCursorF32::new(&timing);
        for target in [0.0, 32.0, 4.0, 52.0, -0.5, 64.0] {
            assert_eq!(
                cursor.time_for_beat(target).to_bits(),
                get_time_for_beat_f32(&timing, target).to_bits(),
                "target beat {target}"
            );
        }
    }

    #[test]
    fn row_membership_cursors_match_independent_queries() {
        let timing = timing_data_from_chart_data(
            0.0,
            0.0,
            None,
            "0=120",
            None,
            "10=0.25",
            None,
            "6=0.125",
            None,
            "4=4,12=2",
            None,
            "",
            None,
            "",
            None,
            "1=1.5,8=2,16=0.5",
            TimingFormat::Ssc,
            true,
        );
        let mut fake_cursor = FakeRowCursor::new(&timing);
        let mut judgable_cursor = JudgableRowCursor::new(&timing);
        let beats = [
            -1.0, 0.0, 1.0, 1.5, 2.5, 4.0, 6.0, 8.0, 9.5, 10.0, 12.0, 14.0, 16.0, 16.5, 20.0,
        ];

        for beat in beats {
            let row = beat_to_note_row(beat);
            assert_eq!(fake_cursor.is_fake(row), is_fake_at_row(&timing, row));
            assert_eq!(
                judgable_cursor.is_judgable(row),
                is_judgable_at_row(&timing, row)
            );
        }
    }

    #[test]
    fn row_membership_cursors_reset_for_decreasing_queries() {
        let timing = timing_data_from_chart_data(
            0.0,
            0.0,
            None,
            "0=120",
            None,
            "",
            None,
            "",
            None,
            "4=4",
            None,
            "",
            None,
            "",
            None,
            "2=1,10=2",
            TimingFormat::Ssc,
            true,
        );
        let mut fake_cursor = FakeRowCursor::new(&timing);
        let mut judgable_cursor = JudgableRowCursor::new(&timing);

        for beat in [0.0, 12.0, 2.5, 8.0, -1.0, 16.0] {
            let row = beat_to_note_row(beat);
            assert_eq!(fake_cursor.is_fake(row), is_fake_at_row(&timing, row));
            assert_eq!(
                judgable_cursor.is_judgable(row),
                is_judgable_at_row(&timing, row)
            );
        }
    }
}

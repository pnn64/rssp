use crate::bpm::{clean_timing_map_cow, parse_beat_or_row, parse_bpm_map};
use crate::math::{lrint_f32, lrint_f64, push_dec6_itg, roundtrip_bpm_itg};
use crate::parse::parse_offset_seconds;
use std::cmp::Ordering;

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

// --- Core math ---
#[inline(always)]
pub fn note_row_to_beat(row: i32) -> f64 {
    f64::from(row) / f64::from(ROWS_PER_BEAT)
}

#[inline(always)]
fn note_row_to_beat_f32(row: i32) -> f32 {
    row as f32 / ROWS_PER_BEAT as f32
}

#[inline(always)]
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
    let component_count = if s.is_empty() {
        0
    } else {
        s.bytes().filter(|&byte| byte == b',').count() + 1
    };
    let mut segments = Vec::with_capacity(component_count);
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
    const ESTIMATED_COMPONENT_BYTES: usize = 11;

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
fn build_segment_rows(segments: &[Segment], require_positive: bool) -> Vec<i32> {
    let mut rows = Vec::with_capacity(segments.len());
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
    if !ordered {
        rows.sort_unstable();
    }
    if require_positive {
        rows.dedup();
    }
    rows
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
fn tidy_row_segments(mut segments: Vec<Segment>) -> Vec<Segment> {
    let mut ordered = true;
    let mut previous_row = i32::MIN;

    for seg in &mut segments {
        let row = beat_to_note_row(seg.beat);
        seg.beat = note_row_to_beat(row);
        ordered &= row >= previous_row;
        previous_row = row;
    }

    if ordered {
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
        return segments;
    }

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

#[inline]
fn segment_row(seg: &Segment) -> i32 {
    beat_to_note_row(seg.beat)
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
#[derive(Debug, Clone)]
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
        process_bpms_and_stops(format, &parsed_bpms, &raw_stops);
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

    let mut warps = parse_optional_timing(chart_warps, global_warps, parse_segments, cleaned);
    warps.extend(extra_warps);
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
    let mut row_to_beat = Vec::new();
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
    bpms: &[(f64, f64)],
    stops: &[Segment],
) -> (Vec<(f64, f64)>, Vec<Segment>, Vec<Segment>, f64) {
    match format {
        TimingFormat::Sm => process_bpms_and_stops_sm(bpms, stops),
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

fn process_bpms_and_stops_ssc(
    bpms: &[(f64, f64)],
    stops: &[Segment],
) -> (Vec<(f64, f64)>, Vec<Segment>, Vec<Segment>, f64) {
    let bpm_changes: Vec<_> = bpms
        .iter()
        .filter(|(b, v)| b.is_finite() && v.is_finite() && *b >= 0.0 && *v > 0.0)
        .map(|(b, v)| (quantize_beat(*b), *v))
        .collect();

    let out_stops: Vec<_> = stops
        .iter()
        .filter(|s| s.beat.is_finite() && s.value.is_finite() && s.beat >= 0.0 && s.value > 0.0)
        .map(|s| Segment {
            beat: quantize_beat(s.beat),
            value: s.value,
        })
        .collect();

    (tidy_bpms(bpm_changes), out_stops, Vec::new(), 0.0)
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

fn process_bpms_and_stops_sm(
    bpms: &[(f64, f64)],
    stops: &[Segment],
) -> (Vec<(f64, f64)>, Vec<Segment>, Vec<Segment>, f64) {
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

    let mut out_bpms = Vec::with_capacity(bpm_changes.len().max(1));
    let mut out_stops = Vec::with_capacity(stop_changes.len().saturating_sub(stop_idx));
    let mut out_warps: Vec<Segment> = Vec::new();

    if bpm > 0.0 && bpm <= FAST_BPM_WARP_F32 {
        out_bpms.push((quantize_beat_f32(0.0), bpm));
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
                    out_warps.push(Segment {
                        beat: f64::from(quantize_beat_f32(start)),
                        value: f64::from(quantize_beat_f32(warp_end - start)),
                    });
                }
                if bpm != prewarp_bpm {
                    out_bpms.push((quantize_beat_f32(start), bpm));
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
                out_bpms.push((quantize_beat_f32(change_beat), change_val));
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
                        out_warps.push(Segment {
                            beat: f64::from(quantize_beat_f32(start)),
                            value: f64::from(quantize_beat_f32(change_beat - start)),
                        });
                    }
                    out_stops.push(Segment {
                        beat: f64::from(quantize_beat_f32(change_beat)),
                        value: f64::from(time_offset),
                    });
                    if (0.0..=FAST_BPM_WARP_F32).contains(&bpm) {
                        if bpm != prewarp_bpm {
                            out_bpms.push((quantize_beat_f32(start), bpm));
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
            out_warps.push(Segment {
                beat: f64::from(quantize_beat_f32(start)),
                value: f64::from(quantize_beat_f32(warp_end - start)),
            });
        }
        if bpm != prewarp_bpm {
            out_bpms.push((quantize_beat_f32(start), bpm));
        }
    }

    let out_bpms = tidy_bpms(
        out_bpms
            .into_iter()
            .map(|(b, v)| (f64::from(b), f64::from(v)))
            .collect(),
    );
    sort_segments_by_beat(&mut out_stops);
    sort_segments_by_beat(&mut out_warps);

    (out_bpms, out_stops, out_warps, f64::from(beat0_offset))
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
    stops: Vec<Segment>,
    stop_rows: Vec<i32>,
    delays: Vec<Segment>,
    delay_rows: Vec<i32>,
    warps: Vec<Segment>,
    warp_start_rows: Vec<i32>,
    speeds: Vec<SpeedSegment>,
    scrolls: Vec<Segment>,
    fakes: Vec<Segment>,
    fake_start_rows: Vec<i32>,
    speed_runtime: Vec<SpeedRuntime>,
    scroll_prefix: Vec<ScrollPrefix>,
    beat0_offset_sec: f64,
    global_offset_sec: f64,
    max_bpm: f64,
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
    let to_seg = |(b, v): &(f32, f32)| Segment {
        beat: f64::from(*b),
        value: f64::from(*v),
    };

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

    let stops: Vec<_> = segments.stops.iter().map(to_seg).collect();
    let delays: Vec<_> = segments.delays.iter().map(to_seg).collect();
    let warps: Vec<_> = segments.warps.iter().map(to_seg).collect();
    let scrolls: Vec<_> = segments.scrolls.iter().map(to_seg).collect();
    let fakes: Vec<_> = segments.fakes.iter().map(to_seg).collect();
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
        stops,
        delays,
        warps,
        speeds,
        scrolls,
        fakes,
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
    let (mut bpms, stops, extra_warps, beat0_adj) =
        process_bpms_and_stops(format, &bpms, &raw_stops);
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
    let mut warps = parse_optional_timing(chart_warps, global_warps, parse_segments, cleaned);
    warps.extend(extra_warps);
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

    timing_data_build(
        song_offset + beat0_adj,
        global_offset,
        beat_to_time,
        stops,
        delays,
        warps,
        speeds,
        scrolls,
        fakes,
    )
}

fn timing_data_build(
    song_offset: f64,
    global_offset: f64,
    beat_to_time: Vec<BeatTimePoint>,
    stops: Vec<Segment>,
    delays: Vec<Segment>,
    warps: Vec<Segment>,
    speeds: Vec<SpeedSegment>,
    scrolls: Vec<Segment>,
    fakes: Vec<Segment>,
) -> TimingData {
    let mut max_bpm = 0.0_f64;

    for point in &beat_to_time {
        if point.bpm.is_finite() && point.bpm > max_bpm {
            max_bpm = point.bpm;
        }
    }

    let stop_rows = build_segment_rows(&stops, true);
    let delay_rows = build_segment_rows(&delays, true);
    let warp_start_rows = build_segment_rows(&warps, false);
    let fake_start_rows = build_segment_rows(&fakes, false);

    let mut timing = TimingData {
        beat_to_time,
        stops,
        stop_rows,
        delays,
        delay_rows,
        warps,
        warp_start_rows,
        speeds,
        scrolls,
        fakes,
        fake_start_rows,
        speed_runtime: Vec::new(),
        scroll_prefix: Vec::new(),
        beat0_offset_sec: song_offset,
        global_offset_sec: global_offset,
        max_bpm,
    };

    if !timing.speeds.is_empty() {
        let mut prev_ratio = 1.0;
        timing.speed_runtime = timing
            .speeds
            .iter()
            .map(|seg| {
                let start = get_time_for_beat(&timing, seg.beat);
                let end = if seg.delay <= 0.0 {
                    start
                } else if seg.unit == SpeedUnit::Seconds {
                    start + seg.delay
                } else {
                    get_time_for_beat(&timing, seg.beat + seg.delay)
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
    &t.warps
}
#[inline(always)]
#[must_use]
pub fn stops(t: &TimingData) -> &[Segment] {
    &t.stops
}
#[inline(always)]
#[must_use]
pub fn delays(t: &TimingData) -> &[Segment] {
    &t.delays
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
    &t.fakes
}

#[inline(always)]
#[must_use]
pub fn has_nonjudgable_rows(t: &TimingData) -> bool {
    !(t.warps.is_empty() && t.fakes.is_empty())
}

#[must_use]
pub fn bpm_segments(t: &TimingData) -> Vec<(f64, f64)> {
    t.beat_to_time.iter().map(|p| (p.beat, p.bpm)).collect()
}

#[inline(always)]
#[must_use]
pub fn is_fake_at_beat(t: &TimingData, beat: f64) -> bool {
    is_in_range_segment(&t.fakes, &t.fake_start_rows, beat)
}

#[inline(always)]
#[must_use]
pub fn is_fake_at_row(t: &TimingData, row: i32) -> bool {
    is_in_range_segment(&t.fakes, &t.fake_start_rows, note_row_to_beat(row))
}

#[inline(always)]
#[must_use]
pub fn is_warp_at_beat(t: &TimingData, beat: f64) -> bool {
    is_warp_at_row(t, beat_to_note_row_f32(beat as f32))
}

#[inline(always)]
#[must_use]
pub fn is_warp_at_row(t: &TimingData, row: i32) -> bool {
    let Some(idx) = segment_index_at_row(&t.warp_start_rows, row) else {
        return false;
    };
    let seg = t.warps[idx];
    if !(seg.value.is_finite() && seg.value > 0.0) {
        return false;
    }
    let beat_row = note_row_to_beat(row) as f32;
    let seg_beat = seg.beat as f32;
    if !(seg_beat <= beat_row && beat_row < seg_beat + seg.value as f32) {
        return false;
    }
    !(has_row(&t.stop_rows, row) || has_row(&t.delay_rows, row))
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
            segments: SegmentRowCursor::new(&timing.fake_start_rows),
        }
    }

    #[inline(always)]
    pub(crate) fn is_fake(&mut self, row: i32) -> bool {
        let Some(idx) = self.segments.index_at(row) else {
            return false;
        };
        is_in_range_segment_at_row(&self.timing.fakes, idx, row)
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
            warps: SegmentRowCursor::new(&timing.warp_start_rows),
            fakes: SegmentRowCursor::new(&timing.fake_start_rows),
        }
    }

    #[inline(always)]
    pub(crate) fn is_judgable(&mut self, row: i32) -> bool {
        if let Some(idx) = self.warps.index_at(row) {
            let seg = self.timing.warps[idx];
            if seg.value.is_finite() && seg.value > 0.0 {
                let beat_row = note_row_to_beat(row) as f32;
                let seg_beat = seg.beat as f32;
                if seg_beat <= beat_row
                    && beat_row < seg_beat + seg.value as f32
                    && !(has_row(&self.timing.stop_rows, row)
                        || has_row(&self.timing.delay_rows, row))
                {
                    return false;
                }
            }
        }

        let Some(idx) = self.fakes.index_at(row) else {
            return true;
        };
        !is_in_range_segment_at_row(&self.timing.fakes, idx, row)
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

#[inline(always)]
pub(crate) fn fixed_timing_parts(t: &TimingData) -> Option<FixedTimingParts> {
    if t.beat_to_time.len() == 1
        && t.beat_to_time[0].beat == 0.0
        && t.stops.is_empty()
        && t.delays.is_empty()
        && t.warps.is_empty()
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
    get_elapsed_time(t, &mut state, target_beat);
    state.last_time
}

fn get_beat_internal(t: &TimingData, elapsed: f64, start_time: f64) -> BeatInfo {
    let mut state = GetBeatState {
        last_time: start_time,
        ..Default::default()
    };
    let mut bps = get_bpm_for_beat(t, 0.0) / 60.0;

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
                let d = t.delays[state.delay_idx].value;
                if elapsed < state.last_time + d {
                    return BeatInfo {
                        beat: t.delays[state.delay_idx].beat,
                        is_in_delay: true,
                        is_in_freeze: false,
                    };
                }
                state.last_time += d;
                state.delay_idx += 1;
            }
            TimingEvent::Stop => {
                let d = t.stops[state.stop_idx].value;
                if elapsed < state.last_time + d {
                    return BeatInfo {
                        beat: t.stops[state.stop_idx].beat,
                        is_in_freeze: true,
                        is_in_delay: false,
                    };
                }
                state.last_time += d;
                state.stop_idx += 1;
            }
            TimingEvent::Warp => {
                state.is_warping = true;
                let w = &t.warps[state.warp_idx];
                state.warp_destination = state.warp_destination.max(w.beat + w.value);
                state.warp_idx += 1;
            }
            _ => {}
        }
        state.last_row = event_row;
    }

    BeatInfo {
        beat: (elapsed - state.last_time).mul_add(bps, note_row_to_beat(state.last_row)),
        is_in_freeze: false,
        is_in_delay: false,
    }
}

fn get_elapsed_time(t: &TimingData, state: &mut GetBeatState, target_beat: f64) {
    let find_marker = target_beat < f64::MAX;
    let mut bps = get_bpm_for_beat(t, note_row_to_beat(state.last_row)) / 60.0;

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
        state.last_time += dt;

        match event_type {
            TimingEvent::WarpDest => state.is_warping = false,
            TimingEvent::Bpm => {
                bps = t.beat_to_time[state.bpm_idx].bpm / 60.0;
                state.bpm_idx += 1;
            }
            TimingEvent::Stop => {
                state.last_time += t.stops[state.stop_idx].value;
                state.stop_idx += 1;
            }
            TimingEvent::Delay => {
                state.last_time += t.delays[state.delay_idx].value;
                state.delay_idx += 1;
            }
            TimingEvent::Marker => return,
            TimingEvent::Warp => {
                state.is_warping = true;
                let w = &t.warps[state.warp_idx];
                state.warp_destination = state.warp_destination.max(w.beat + w.value);
                state.warp_idx += 1;
            }
            _ => {}
        }
        state.last_row = event_row;
    }
}

fn get_elapsed_time_f32(t: &TimingData, state: &mut GetBeatStateF32, target_beat: f32) -> f32 {
    let find_marker = target_beat < f32::MAX;
    let mut bps = get_bpm_for_row_f32(t, state.last_row) / 60.0;
    let mut curr_segment = state.bpm_idx + state.warp_idx + state.stop_idx + state.delay_idx;

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
                state.last_time += t.stops[state.stop_idx].value as f32;
                state.stop_idx += 1;
                curr_segment += 1;
            }
            TimingEvent::Delay => {
                state.last_time += t.delays[state.delay_idx].value as f32;
                state.delay_idx += 1;
                curr_segment += 1;
            }
            TimingEvent::Marker => unreachable!("marker is returned before state mutation"),
            TimingEvent::Warp => {
                state.is_warping = true;
                let w = &t.warps[state.warp_idx];
                let warp_sum = w.value as f32 + w.beat as f32;
                if warp_sum > state.warp_destination {
                    state.warp_destination = warp_sum;
                }
                state.warp_idx += 1;
                curr_segment += 1;
            }
            _ => {}
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
    if state.delay_idx < t.delays.len() {
        let r = beat_to_note_row(t.delays[state.delay_idx].beat);
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
    if state.stop_idx < t.stops.len() {
        let r = beat_to_note_row(t.stops[state.stop_idx].beat);
        if r < row {
            row = r;
            event = TimingEvent::Stop;
        }
    }
    if state.warp_idx < t.warps.len() {
        let r = beat_to_note_row(t.warps[state.warp_idx].beat);
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
    if state.delay_idx < t.delays.len() {
        let r = beat_to_note_row_f32(t.delays[state.delay_idx].beat as f32);
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
    if state.stop_idx < t.stops.len() {
        let r = beat_to_note_row_f32(t.stops[state.stop_idx].beat as f32);
        if r < row {
            row = r;
            event = TimingEvent::Stop;
        }
    }
    if state.warp_idx < t.warps.len() {
        let r = beat_to_note_row_f32(t.warps[state.warp_idx].beat as f32);
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
    t.beat_to_time[idx.saturating_sub(1).max(0)].bpm
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

    fn tidy_bpms_materialized(mut bpms: Vec<(f64, f64)>) -> Vec<(f64, f64)> {
        if bpms.is_empty() {
            return vec![(0.0, DEFAULT_BPM)];
        }
        bpms.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(Ordering::Equal));

        let mut last_per_beat: Vec<(f64, f64)> = Vec::with_capacity(bpms.len());
        for (beat, bpm) in bpms {
            if let Some(last) = last_per_beat.last_mut()
                && beat == last.0
            {
                *last = (beat, bpm);
                continue;
            }
            last_per_beat.push((beat, bpm));
        }
        last_per_beat[0].0 = 0.0;

        let mut tidied = Vec::with_capacity(last_per_beat.len());
        let mut last_value = None;
        for (beat, bpm) in last_per_beat {
            if last_value == Some(bpm) {
                continue;
            }
            last_value = Some(bpm);
            tidied.push((beat, bpm));
        }
        tidied
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

    fn tidy_row_segments_materialized(segments: Vec<Segment>) -> Vec<Segment> {
        let mut keyed: Vec<(i32, usize, Segment)> = Vec::with_capacity(segments.len());
        for (idx, mut seg) in segments.into_iter().enumerate() {
            let row = beat_to_note_row(seg.beat);
            seg.beat = note_row_to_beat(row);
            keyed.push((row, idx, seg));
        }
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

    fn assert_segment_bits_eq(actual: &[Segment], expected: &[Segment]) {
        assert_eq!(actual.len(), expected.len());
        for (actual, expected) in actual.iter().zip(expected) {
            assert_eq!(actual.beat.to_bits(), expected.beat.to_bits());
            assert_eq!(actual.value.to_bits(), expected.value.to_bits());
        }
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
    fn ordered_segment_rows_match_sorted_path() {
        let cases = [
            Vec::new(),
            vec![Segment {
                beat: 0.0,
                value: 1.0,
            }],
            vec![
                Segment {
                    beat: 0.0,
                    value: 1.0,
                },
                Segment {
                    beat: 0.0,
                    value: 2.0,
                },
                Segment {
                    beat: 4.0,
                    value: -1.0,
                },
                Segment {
                    beat: 8.0,
                    value: f64::NAN,
                },
                Segment {
                    beat: 12.0,
                    value: 0.5,
                },
            ],
            vec![
                Segment {
                    beat: 8.0,
                    value: 1.0,
                },
                Segment {
                    beat: -4.0,
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
        ];

        for segments in cases {
            for require_positive in [false, true] {
                assert_eq!(
                    build_segment_rows(&segments, require_positive),
                    build_segment_rows_sorted(&segments, require_positive),
                    "{segments:?}, require_positive={require_positive}"
                );
            }
        }
    }

    #[test]
    fn generated_segment_rows_match_sorted_path() {
        let mut state = 0x510e_527f_ade6_82d1_u64;
        for len in 0..128 {
            let segments: Vec<_> = (0..len)
                .map(|idx| {
                    state ^= state << 13;
                    state ^= state >> 7;
                    state ^= state << 17;
                    let row = if len % 2 == 0 {
                        idx as i32 / 3
                    } else {
                        (state % 128) as i32 - 32
                    };
                    Segment {
                        beat: note_row_to_beat(row),
                        value: match state % 5 {
                            0 => -1.0,
                            1 => 0.0,
                            _ => 0.125,
                        },
                    }
                })
                .collect();
            for require_positive in [false, true] {
                assert_eq!(
                    build_segment_rows(&segments, require_positive),
                    build_segment_rows_sorted(&segments, require_positive)
                );
            }
        }
    }

    #[test]
    fn in_place_bpm_cleanup_matches_materialized_path_bit_for_bit() {
        let cases = [
            Vec::new(),
            vec![(4.0, 120.0)],
            vec![
                (0.0, 120.0),
                (4.0, 120.0),
                (8.0, 150.0),
                (8.0, 180.0),
                (12.0, 180.0),
            ],
            vec![(8.0, 180.0), (-4.0, 90.0), (0.0, 120.0), (8.0, 150.0)],
            vec![(f64::NAN, 90.0), (4.0, f64::NAN), (0.0, 120.0)],
        ];

        for bpms in cases {
            let expected = tidy_bpms_materialized(bpms.clone());
            let actual = tidy_bpms(bpms);
            assert_bpm_bits_eq(&actual, &expected);
        }
    }

    #[test]
    fn generated_bpm_cleanup_matches_materialized_path_bit_for_bit() {
        let mut state = 0x3c6e_f372_fe94_f82b_u64;
        for len in 0..128 {
            let bpms: Vec<_> = (0..len)
                .map(|idx| {
                    state ^= state << 13;
                    state ^= state >> 7;
                    state ^= state << 17;
                    let beat = if len % 2 == 0 {
                        idx as f64 / 3.0
                    } else {
                        (state % 64) as f64 - 16.0
                    };
                    (beat, 60.0 + (state % 7) as f64 * 30.0)
                })
                .collect();
            let expected = tidy_bpms_materialized(bpms.clone());
            let actual = tidy_bpms(bpms);
            assert_bpm_bits_eq(&actual, &expected);
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
    fn ordered_row_segment_cleanup_matches_materialized_sort_path() {
        let cases = [
            Vec::new(),
            vec![Segment {
                beat: 0.0,
                value: 1.0,
            }],
            vec![
                Segment {
                    beat: 0.0,
                    value: 1.0,
                },
                Segment {
                    beat: 1.0 / 192.0,
                    value: 2.0,
                },
                Segment {
                    beat: 1.0,
                    value: 3.0,
                },
                Segment {
                    beat: 1.0 + 1.0 / 192.0,
                    value: 4.0,
                },
            ],
            vec![
                Segment {
                    beat: 8.0,
                    value: 1.0,
                },
                Segment {
                    beat: 0.0,
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

        for segments in cases {
            let expected = tidy_row_segments_materialized(segments.clone());
            let actual = tidy_row_segments(segments);
            assert_segment_bits_eq(&actual, &expected);
        }
    }

    #[test]
    fn generated_row_segment_cleanup_matches_materialized_sort_path() {
        let mut state = 0xbb67_ae85_84ca_a73b_u64;
        for len in 0..128 {
            let segments: Vec<_> = (0..len)
                .map(|idx| {
                    state ^= state << 13;
                    state ^= state >> 7;
                    state ^= state << 17;
                    let row = if len % 2 == 0 {
                        idx as i32 / 3
                    } else {
                        (state % 96) as i32 - 32
                    };
                    Segment {
                        beat: note_row_to_beat(row),
                        value: f64::from((state >> 32) as u32),
                    }
                })
                .collect();
            let expected = tidy_row_segments_materialized(segments.clone());
            let actual = tidy_row_segments(segments);
            assert_segment_bits_eq(&actual, &expected);
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

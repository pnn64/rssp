use crate::bpm::{clean_timing_map_cow, parse_beat_or_row, parse_bpm_map};
use std::cmp::Ordering;
use std::collections::HashMap;

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

impl TimingFormat {
    pub fn from_extension(ext: &str) -> Self {
        if ext.eq_ignore_ascii_case("sm") { Self::Sm } else { Self::Ssc }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
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
pub(crate) fn note_row_to_beat(row: i32) -> f64 {
    row as f64 / ROWS_PER_BEAT as f64
}

#[inline(always)]
fn note_row_to_beat_f32(row: i32) -> f32 {
    row as f32 / ROWS_PER_BEAT as f32
}

#[inline(always)]
pub(crate) fn beat_to_note_row(beat: f64) -> i32 {
    lrint_f64(beat * ROWS_PER_BEAT as f64) as i32
}

#[inline(always)]
pub(crate) fn beat_to_note_row_f32_exact(beat: f32) -> i32 {
    lrint_f32(beat * ROWS_PER_BEAT as f32)
}

#[inline(always)]
fn beat_to_note_row_f32(beat: f32) -> i32 {
    beat_to_note_row_f32_exact(beat)
}

#[inline(always)]
fn lrint_f64(v: f64) -> f64 {
    if !v.is_finite() { return 0.0; }
    if v.fract() == 0.0 { return v; }
    let floor = v.floor();
    let frac = v - floor;
    match frac.partial_cmp(&0.5) {
        Some(Ordering::Less) => floor,
        Some(Ordering::Greater) => floor + 1.0,
        _ => if ((floor as i64) & 1) == 0 { floor } else { floor + 1.0 },
    }
}

#[inline(always)]
fn lrint_f32(v: f32) -> i32 {
    if !v.is_finite() { return 0; }
    if v.fract() == 0.0 { return v as i32; }
    let floor = v.floor();
    let frac = v - floor;
    let fi = floor as i32;
    match frac.partial_cmp(&0.5) {
        Some(Ordering::Less) => fi,
        Some(Ordering::Greater) => fi + 1,
        _ => if (fi & 1) == 0 { fi } else { fi + 1 },
    }
}

#[inline(always)]
fn quantize_beat(beat: f64) -> f64 {
    note_row_to_beat_f32(beat_to_note_row_f32(beat as f32)) as f64
}

#[inline(always)]
fn quantize_beat_f32(beat: f32) -> f32 {
    note_row_to_beat_f32(beat_to_note_row_f32(beat))
}

#[inline(always)]
pub fn steps_timing_allowed(version: f32, format: TimingFormat) -> bool {
    matches!(format, TimingFormat::Sm) || version >= VERSION_SPLIT_TIMING
}

#[inline(always)]
pub(crate) fn roundtrip_bpm_itg(bpm: f64) -> f64 {
    let bpm_f = bpm as f32;
    if !bpm_f.is_finite() { 0.0 } else { (bpm_f / 60.0 * 60.0) as f64 }
}

#[inline(always)]
pub(crate) fn round_sig_figs_itg(value: f64) -> f64 {
    if !value.is_finite() || value == 0.0 { return value; }
    format!("{:.5e}", value as f32 as f64).parse().unwrap_or(value)
}

#[inline(always)]
pub fn round_millis(value: f64) -> f64 {
    round_sig_figs_itg(value)
}

#[inline(always)]
fn normalize_decimal_itg(value: f64) -> String {
    format!("{:.3}", (value as f32 * 1000.0).round() / 1000.0)
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
    s.trim()
        .split(',')
        .filter_map(|part| {
            let (beat_str, val_str) = part.trim().split_once('=')?;
            let beat = parse_beat_or_row(beat_str)?;
            let value = parse_f64_fast(val_str)?;
            (beat.is_finite() && value.is_finite())
                .then(|| Segment { beat, value: value as f32 as f64 })
        })
        .collect()
}

fn parse_segments_positive(s: &str) -> Vec<Segment> {
    parse_segments(s).into_iter().filter(|s| s.value > 0.0).collect()
}

fn parse_speeds(s: &str) -> Vec<SpeedSegment> {
    if s.is_empty() { return Vec::new(); }
    s.split(',')
        .filter_map(|chunk| {
            let parts: Vec<_> = chunk.split('=').map(str::trim).collect();
            if parts.len() < 3 { return None; }
            let beat = parse_beat_or_row(parts[0])?;
            let ratio = parts[1].parse::<f64>().ok()? as f32 as f64;
            let delay = parts[2].parse::<f64>().ok()? as f32 as f64;
            let unit = if parts.get(3) == Some(&"1") { SpeedUnit::Seconds } else { SpeedUnit::Beats };
            Some(SpeedSegment { beat, ratio, delay, unit })
        })
        .collect()
}

// --- Row builders ---
fn build_segment_rows(segments: &[Segment], require_positive: bool) -> Vec<i32> {
    let mut rows: Vec<i32> = segments
        .iter()
        .filter(|s| !require_positive || (s.value.is_finite() && s.value > 0.0))
        .map(|s| beat_to_note_row_f32(s.beat as f32))
        .collect();
    rows.sort_unstable();
    if require_positive { rows.dedup(); }
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
fn tidy_row_segments(segments: Vec<Segment>) -> Vec<Segment> {
    let mut out: Vec<(i32, Segment)> = Vec::with_capacity(segments.len());
    let mut rows: HashMap<i32, usize> = HashMap::new();

    for mut seg in segments {
        let row = beat_to_note_row(seg.beat);
        seg.beat = note_row_to_beat(row);
        if let Some(&idx) = rows.get(&row) {
            out[idx] = (row, seg);
        } else {
            rows.insert(row, out.len());
            out.push((row, seg));
        }
    }
    out.sort_by_key(|(row, _)| *row);
    out.into_iter().map(|(_, seg)| seg).collect()
}

trait RowSegment: Clone {
    fn beat(&self) -> f64;
    fn set_beat(&mut self, beat: f64);
    fn eq_value(&self, other: &Self) -> bool;
}

impl RowSegment for Segment {
    fn beat(&self) -> f64 { self.beat }
    fn set_beat(&mut self, beat: f64) { self.beat = beat; }
    fn eq_value(&self, other: &Self) -> bool { float_eq(self.value, other.value) }
}

impl RowSegment for SpeedSegment {
    fn beat(&self) -> f64 { self.beat }
    fn set_beat(&mut self, beat: f64) { self.beat = beat; }
    fn eq_value(&self, other: &Self) -> bool {
        float_eq(self.ratio, other.ratio) && float_eq(self.delay, other.delay) && self.unit == other.unit
    }
}

fn segment_row<T: RowSegment>(seg: &T) -> i32 {
    beat_to_note_row(seg.beat())
}

fn add_segment<T: RowSegment>(out: &mut Vec<T>, mut seg: T) {
    let row = beat_to_note_row(seg.beat());
    seg.set_beat(note_row_to_beat(row));

    if out.is_empty() {
        out.push(seg);
        return;
    }

    let idx = {
        let pos = out.partition_point(|s| segment_row(s) <= row);
        if pos == 0 { 0 } else { pos - 1 }
    };
    let on_same_row = segment_row(&out[idx]) == row;
    let prev_idx = if on_same_row && idx > 0 { idx - 1 } else { idx };

    if idx + 1 < out.len() {
        let next_idx = idx + 1;
        if seg.eq_value(&out[next_idx]) {
            if seg.eq_value(&out[prev_idx]) {
                out.remove(next_idx);
                if prev_idx != idx { out.remove(idx); }
                return;
            }
            out[next_idx].set_beat(seg.beat());
            if prev_idx != idx { out.remove(idx); }
            return;
        }
        if seg.eq_value(&out[prev_idx]) {
            if prev_idx != idx { out.remove(idx); }
            return;
        }
    } else if seg.eq_value(&out[prev_idx]) {
        if prev_idx != idx { out.remove(idx); }
        return;
    }

    if on_same_row {
        if !seg.eq_value(&out[idx]) { out[idx] = seg; }
    } else {
        let insert_pos = out.partition_point(|s| segment_row(s) <= row);
        out.insert(insert_pos, seg);
    }
}

fn tidy_segments<T: RowSegment>(segments: Vec<T>) -> Vec<T> {
    let mut out = Vec::with_capacity(segments.len());
    for seg in segments { add_segment(&mut out, seg); }
    out
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
    if parsed_bpms.is_empty() { parsed_bpms.push((0.0, DEFAULT_BPM)); }

    let raw_stops = parse_optional_timing(chart_stops, global_stops, parse_segments, cleaned);
    let (mut parsed_bpms, stops, extra_warps, beat0_offset_adjust) =
        process_bpms_and_stops(format, &parsed_bpms, &raw_stops);
    let stops = tidy_row_segments(stops);
    if parsed_bpms.is_empty() { parsed_bpms.push((0.0, DEFAULT_BPM)); }

    let quantize_seg = |seg: Segment| Segment {
        beat: quantize_beat(seg.beat),
        value: seg.value,
    };

    let delays: Vec<_> = parse_optional_timing(chart_delays, global_delays, parse_segments, cleaned)
        .into_iter().map(quantize_seg).collect();
    let delays = tidy_row_segments(delays);

    let mut warps = parse_optional_timing(chart_warps, global_warps, parse_segments, cleaned);
    warps.extend(extra_warps);
    let warps: Vec<_> = warps.into_iter()
        .map(|s| Segment { beat: quantize_beat(s.beat), value: quantize_beat(s.value) })
        .collect();
    let warps = tidy_row_segments(warps);

    let speeds: Vec<_> = parse_optional_timing(chart_speeds, global_speeds, parse_speeds, cleaned)
        .into_iter()
        .map(|s| SpeedSegment { beat: quantize_beat(s.beat), ..s })
        .collect();
    let speeds = tidy_segments(speeds);

    let scrolls: Vec<_> = parse_optional_timing(chart_scrolls, global_scrolls, parse_segments, cleaned)
        .into_iter().map(quantize_seg).collect();
    let scrolls = tidy_segments(scrolls);

    let fakes: Vec<_> = parse_optional_timing(chart_fakes, global_fakes, parse_segments_positive, cleaned)
        .into_iter()
        .map(|s| Segment { beat: quantize_beat(s.beat), value: quantize_beat(s.value) })
        .collect();
    let fakes = tidy_row_segments(fakes);

    let to_f32_pair = |s: &Segment| (s.beat as f32, s.value as f32);

    TimingSegments {
        beat0_offset_adjust: beat0_offset_adjust as f32,
        bpms: parsed_bpms.iter().map(|(b, v)| (*b as f32, *v as f32)).collect(),
        stops: stops.iter().map(to_f32_pair).collect(),
        delays: delays.iter().map(to_f32_pair).collect(),
        warps: warps.iter().map(to_f32_pair).collect(),
        speeds: speeds.iter().map(|s| (s.beat as f32, s.ratio as f32, s.delay as f32, s.unit)).collect(),
        scrolls: scrolls.iter().map(to_f32_pair).collect(),
        fakes: fakes.iter().map(to_f32_pair).collect(),
    }
}

/// Compatibility wrapper for pre-cleaned inputs
#[allow(clippy::too_many_arguments)]
pub fn compute_timing_segments_cleaned(
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
) -> TimingSegments {
    compute_timing_segments(
        chart_bpms, global_bpms, chart_stops, global_stops,
        chart_delays, global_delays, chart_warps, global_warps,
        chart_speeds, global_speeds, chart_scrolls, global_scrolls,
        chart_fakes, global_fakes, format, true,
    )
}

pub fn normalize_speeds_like_itg(mut speeds: Vec<(f64, f64, f64, i32)>) -> Vec<(f64, f64, f64, i32)> {
    if speeds.is_empty() { speeds.push((0.0, 1.0, 0.0, 0)); }
    speeds
}

pub fn normalize_scrolls_like_itg(mut scrolls: Vec<(f64, f64)>) -> Vec<(f64, f64)> {
    if scrolls.is_empty() { scrolls.push((0.0, 1.0)); }
    scrolls
}

pub fn format_bpm_segments_like_itg(bpms: &[(f64, f64)]) -> String {
    bpms.iter()
        .enumerate()
        .fold(String::new(), |mut out, (idx, (beat, bpm))| {
            if idx > 0 { out.push(','); }
            let beat = note_row_to_beat_f32(beat_to_note_row_f32(*beat as f32)) as f64;
            out.push_str(&normalize_decimal_itg(beat));
            out.push('=');
            out.push_str(&normalize_decimal_itg(roundtrip_bpm_itg(*bpm)));
            out
        })
}

pub fn compute_row_to_beat(minimized_note_data: &[u8]) -> Vec<f32> {
    let mut row_to_beat = Vec::new();
    for (measure_index, measure_bytes) in minimized_note_data.split(|&b| b == b',').enumerate() {
        let num_rows = count_measure_rows(measure_bytes);
        if num_rows == 0 { continue; }
        row_to_beat.reserve(num_rows);
        let measure_start = measure_index as f32 * 4.0;
        let row_step = 4.0 / num_rows as f32;
        for row in 0..num_rows {
            row_to_beat.push(measure_start + row as f32 * row_step);
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
            if has_non_ws { count += 1; has_non_ws = false; }
        } else if !b.is_ascii_whitespace() {
            has_non_ws = true;
        }
    }
    if has_non_ws { count += 1; }
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
    if bpms.is_empty() { return vec![(0.0, DEFAULT_BPM)]; }
    bpms.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(Ordering::Equal));

    let mut last_per_beat: Vec<(f64, f64)> = Vec::with_capacity(bpms.len());
    for (beat, bpm) in bpms {
        if let Some(last) = last_per_beat.last_mut() {
            if beat == last.0 { *last = (beat, bpm); continue; }
        }
        last_per_beat.push((beat, bpm));
    }
    if let Some(first) = last_per_beat.first_mut() { first.0 = 0.0; }

    let mut tidied = Vec::with_capacity(last_per_beat.len());
    let mut last_value: Option<f64> = None;
    for (beat, bpm) in last_per_beat {
        if last_value == Some(bpm) { continue; }
        last_value = Some(bpm);
        tidied.push((beat, bpm));
    }
    if tidied.is_empty() { tidied.push((0.0, DEFAULT_BPM)); }
    tidied
}

fn process_bpms_and_stops_ssc(
    bpms: &[(f64, f64)],
    stops: &[Segment],
) -> (Vec<(f64, f64)>, Vec<Segment>, Vec<Segment>, f64) {
    let bpm_changes: Vec<_> = bpms.iter()
        .filter(|(b, v)| b.is_finite() && v.is_finite() && *b >= 0.0 && *v > 0.0)
        .map(|(b, v)| (quantize_beat(*b), *v))
        .collect();

    let out_stops: Vec<_> = stops.iter()
        .filter(|s| s.beat.is_finite() && s.value.is_finite() && s.beat >= 0.0 && s.value > 0.0)
        .map(|s| Segment { beat: quantize_beat(s.beat), value: s.value })
        .collect();

    (tidy_bpms(bpm_changes), out_stops, Vec::new(), 0.0)
}

fn process_bpms_and_stops_sm(
    bpms: &[(f64, f64)],
    stops: &[Segment],
) -> (Vec<(f64, f64)>, Vec<Segment>, Vec<Segment>, f64) {
    let mut bpm_changes: Vec<(f32, f32)> = bpms.iter()
        .filter(|(b, v)| b.is_finite() && v.is_finite() && *v != 0.0)
        .map(|(b, v)| (*b as f32, *v as f32))
        .collect();
    bpm_changes.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(Ordering::Equal));

    let mut stop_changes: Vec<(f32, f32)> = stops.iter()
        .filter(|s| s.beat.is_finite() && s.value.is_finite() && s.value != 0.0)
        .map(|s| (s.beat as f32, s.value as f32))
        .collect();
    stop_changes.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(Ordering::Equal));

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
            let v = bpm_changes[bpm_idx].1; bpm_idx += 1; v
        } else { DEFAULT_BPM as f32 };
    }

    let mut out_bpms: Vec<(f32, f32)> = Vec::new();
    let mut out_stops: Vec<Segment> = Vec::new();
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
        let (change_beat, change_val) = if is_bpm { bpm_changes[bpm_idx] } else { stop_changes[stop_idx] };

        if bpm <= FAST_BPM_WARP_F32 {
            time_offset += (change_beat - prev_beat) * 60.0 / bpm;
            if let Some(start) = warp_start {
                if bpm > 0.0 && time_offset > 0.0 {
                    let warp_end = change_beat - (time_offset * bpm / 60.0);
                    if warp_end > start {
                        out_warps.push(Segment {
                            beat: quantize_beat_f32(start) as f64,
                            value: quantize_beat_f32(warp_end - start) as f64,
                        });
                    }
                    if bpm != prewarp_bpm { out_bpms.push((quantize_beat_f32(start), bpm)); }
                    warp_start = None;
                }
            }
        }
        prev_beat = change_beat;

        if is_bpm {
            if warp_start.is_none() && (change_val < 0.0 || change_val > FAST_BPM_WARP_F32) {
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
                    beat: quantize_beat_f32(change_beat) as f64,
                    value: change_val as f64,
                });
            } else {
                time_offset += change_val;
                if change_val > 0.0 && time_offset > 0.0 {
                    if let Some(start) = warp_start {
                        if change_beat > start {
                            out_warps.push(Segment {
                                beat: quantize_beat_f32(start) as f64,
                                value: quantize_beat_f32(change_beat - start) as f64,
                            });
                        }
                        out_stops.push(Segment {
                            beat: quantize_beat_f32(change_beat) as f64,
                            value: time_offset as f64,
                        });
                        if bpm < 0.0 || bpm > FAST_BPM_WARP_F32 {
                            warp_start = Some(change_beat);
                            time_offset = 0.0;
                        } else {
                            if bpm != prewarp_bpm { out_bpms.push((quantize_beat_f32(start), bpm)); }
                            warp_start = None;
                        }
                    }
                }
            }
            stop_idx += 1;
        }
    }

    if let Some(start) = warp_start {
        let warp_end = if bpm < 0.0 || bpm > FAST_BPM_WARP_F32 {
            99_999_999.0_f32
        } else {
            prev_beat - (time_offset * bpm / 60.0)
        };
        if warp_end > start {
            out_warps.push(Segment {
                beat: quantize_beat_f32(start) as f64,
                value: quantize_beat_f32(warp_end - start) as f64,
            });
        }
        if bpm != prewarp_bpm { out_bpms.push((quantize_beat_f32(start), bpm)); }
    }

    let out_bpms = tidy_bpms(out_bpms.into_iter().map(|(b, v)| (b as f64, v as f64)).collect());
    out_stops.sort_by(|a, b| a.beat.partial_cmp(&b.beat).unwrap_or(Ordering::Less));
    out_warps.sort_by(|a, b| a.beat.partial_cmp(&b.beat).unwrap_or(Ordering::Less));

    (out_bpms, out_stops, out_warps, beat0_offset as f64)
}

// --- TimingData ---
#[derive(Debug, Clone, Copy)]
struct BeatTimePoint { beat: f64, time_sec: f64, bpm: f64 }

#[derive(Debug, Clone, Copy)]
struct SpeedRuntime { start_time: f64, end_time: f64, prev_ratio: f64 }

#[derive(Debug, Clone, Copy)]
struct ScrollPrefix { beat: f64, cum_displayed: f64, ratio: f64 }

#[derive(Debug, Clone, Copy, Default)]
struct GetBeatState {
    bpm_idx: usize, stop_idx: usize, delay_idx: usize, warp_idx: usize,
    last_row: i32, last_time: f64, warp_destination: f64, is_warping: bool,
}

#[derive(Debug, Clone, Copy, Default)]
struct GetBeatStateF32 {
    bpm_idx: usize, stop_idx: usize, delay_idx: usize, warp_idx: usize,
    last_row: i32, last_time: f32, warp_destination: f32, is_warping: bool,
}

#[derive(PartialEq, Eq)]
enum TimingEvent { Bpm, Stop, Delay, StopDelay, Warp, WarpDest, Marker, NotFound }

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
pub struct BeatInfo { pub beat: f64, pub is_in_freeze: bool, pub is_in_delay: bool }

impl TimingData {
    pub fn from_segments(song_offset: f64, global_offset: f64, segments: &TimingSegments) -> Self {
        let to_seg = |(b, v): &(f32, f32)| Segment { beat: *b as f64, value: *v as f64 };
        
        let mut bpms: Vec<_> = segments.bpms.iter().map(|(b, v)| (*b as f64, *v as f64)).collect();
        if bpms.is_empty() { bpms.push((0.0, DEFAULT_BPM)); }

        let stops: Vec<_> = segments.stops.iter().map(to_seg).collect();
        let delays: Vec<_> = segments.delays.iter().map(to_seg).collect();
        let warps: Vec<_> = segments.warps.iter().map(to_seg).collect();
        let scrolls: Vec<_> = segments.scrolls.iter().map(to_seg).collect();
        let fakes: Vec<_> = segments.fakes.iter().map(to_seg).collect();
        let speeds: Vec<_> = segments.speeds.iter()
            .map(|(b, r, d, u)| SpeedSegment { beat: *b as f64, ratio: *r as f64, delay: *d as f64, unit: *u })
            .collect();

        Self::build(song_offset + segments.beat0_offset_adjust as f64, global_offset, bpms, stops, delays, warps, speeds, scrolls, fakes)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn from_chart_data(
        song_offset: f64, global_offset: f64,
        chart_bpms: Option<&str>, global_bpms: &str,
        chart_stops: Option<&str>, global_stops: &str,
        chart_delays: Option<&str>, global_delays: &str,
        chart_warps: Option<&str>, global_warps: &str,
        chart_speeds: Option<&str>, global_speeds: &str,
        chart_scrolls: Option<&str>, global_scrolls: &str,
        chart_fakes: Option<&str>, global_fakes: &str,
        format: TimingFormat, cleaned: bool,
    ) -> Self {
        let bpms_str = chart_bpms.filter(|s| !s.is_empty()).unwrap_or(global_bpms);
        let mut bpms: Vec<(f64, f64)> = if cleaned {
            parse_bpm_map(bpms_str)
        } else {
            parse_bpm_map(clean_timing_map_cow(bpms_str).as_ref())
        };
        if bpms.is_empty() { bpms.push((0.0, DEFAULT_BPM)); }

        let raw_stops = parse_optional_timing(chart_stops, global_stops, parse_segments, cleaned);
        let (mut bpms, stops, extra_warps, beat0_adj) = process_bpms_and_stops(format, &bpms, &raw_stops);
        let stops = tidy_row_segments(stops);
        if bpms.is_empty() { bpms.push((0.0, DEFAULT_BPM)); }

        let q = |s: Segment| Segment { beat: quantize_beat(s.beat), value: s.value };
        let qv = |s: Segment| Segment { beat: quantize_beat(s.beat), value: quantize_beat(s.value) };

        let delays = tidy_row_segments(parse_optional_timing(chart_delays, global_delays, parse_segments, cleaned).into_iter().map(q).collect());
        let mut warps = parse_optional_timing(chart_warps, global_warps, parse_segments, cleaned);
        warps.extend(extra_warps);
        let warps = tidy_row_segments(warps.into_iter().map(qv).collect());
        let speeds = tidy_segments(parse_optional_timing(chart_speeds, global_speeds, parse_speeds, cleaned)
            .into_iter().map(|s| SpeedSegment { beat: quantize_beat(s.beat), ..s }).collect());
        let scrolls = tidy_segments(parse_optional_timing(chart_scrolls, global_scrolls, parse_segments, cleaned).into_iter().map(q).collect());
        let fakes = tidy_row_segments(parse_optional_timing(chart_fakes, global_fakes, parse_segments_positive, cleaned).into_iter().map(qv).collect());

        Self::build(song_offset + beat0_adj, global_offset, bpms, stops, delays, warps, speeds, scrolls, fakes)
    }

    /// Compatibility wrapper for pre-cleaned inputs
    #[allow(clippy::too_many_arguments)]
    pub fn from_chart_data_cleaned(
        song_offset: f64, global_offset: f64,
        chart_bpms: Option<&str>, global_bpms: &str,
        chart_stops: Option<&str>, global_stops: &str,
        chart_delays: Option<&str>, global_delays: &str,
        chart_warps: Option<&str>, global_warps: &str,
        chart_speeds: Option<&str>, global_speeds: &str,
        chart_scrolls: Option<&str>, global_scrolls: &str,
        chart_fakes: Option<&str>, global_fakes: &str,
        format: TimingFormat,
    ) -> Self {
        Self::from_chart_data(
            song_offset, global_offset,
            chart_bpms, global_bpms, chart_stops, global_stops,
            chart_delays, global_delays, chart_warps, global_warps,
            chart_speeds, global_speeds, chart_scrolls, global_scrolls,
            chart_fakes, global_fakes, format, true,
        )
    }

    fn build(
        song_offset: f64, global_offset: f64,
        bpms: Vec<(f64, f64)>, stops: Vec<Segment>, delays: Vec<Segment>,
        warps: Vec<Segment>, speeds: Vec<SpeedSegment>, scrolls: Vec<Segment>, fakes: Vec<Segment>,
    ) -> Self {
        let mut beat_to_time = Vec::with_capacity(bpms.len());
        let mut current_time = 0.0;
        let mut last_beat = 0.0;
        let mut last_bpm = bpms[0].1;
        let mut max_bpm = 0.0_f64;

        for &(beat, bpm) in &bpms {
            if beat > last_beat && last_bpm > 0.0 {
                current_time += (beat - last_beat) * 60.0 / last_bpm;
            }
            beat_to_time.push(BeatTimePoint { beat, time_sec: song_offset + current_time, bpm });
            if bpm.is_finite() && bpm > max_bpm { max_bpm = bpm; }
            last_beat = beat;
            last_bpm = bpm;
        }

        let stop_rows = build_segment_rows(&stops, true);
        let delay_rows = build_segment_rows(&delays, true);
        let warp_start_rows = build_segment_rows(&warps, false);
        let fake_start_rows = build_segment_rows(&fakes, false);

        let mut timing = Self {
            beat_to_time, stops, stop_rows, delays, delay_rows, warps, warp_start_rows,
            speeds, scrolls, fakes, fake_start_rows,
            speed_runtime: Vec::new(), scroll_prefix: Vec::new(),
            beat0_offset_sec: song_offset, global_offset_sec: global_offset, max_bpm,
        };

        timing.beat_to_time = timing.beat_to_time.iter()
            .map(|p| BeatTimePoint { time_sec: timing.get_time_internal(p.beat), ..*p })
            .collect();

        if !timing.speeds.is_empty() {
            let mut prev_ratio = 1.0;
            timing.speed_runtime = timing.speeds.iter().map(|seg| {
                let start = timing.get_time_for_beat(seg.beat);
                let end = if seg.delay <= 0.0 { start }
                    else if seg.unit == SpeedUnit::Seconds { start + seg.delay }
                    else { timing.get_time_for_beat(seg.beat + seg.delay) };
                let rt = SpeedRuntime { start_time: start, end_time: end, prev_ratio };
                prev_ratio = seg.ratio;
                rt
            }).collect();
        }

        if !timing.scrolls.is_empty() {
            let mut cum = 0.0;
            let mut last_beat = 0.0;
            let mut last_ratio = 1.0;
            timing.scroll_prefix = timing.scrolls.iter().map(|seg| {
                cum += (seg.beat - last_beat) * last_ratio;
                let p = ScrollPrefix { beat: seg.beat, cum_displayed: cum, ratio: seg.value };
                last_beat = seg.beat;
                last_ratio = seg.value;
                p
            }).collect();
        }

        timing
    }

    #[inline(always)]
    pub fn beat0_offset_seconds(&self) -> f64 { self.beat0_offset_sec }
    #[inline(always)]
    pub fn beat0_group_offset_seconds(&self) -> f64 { self.global_offset_sec }
    #[inline(always)]
    pub fn warps(&self) -> &[Segment] { &self.warps }
    #[inline(always)]
    pub fn stops(&self) -> &[Segment] { &self.stops }
    #[inline(always)]
    pub fn delays(&self) -> &[Segment] { &self.delays }
    #[inline(always)]
    pub fn speeds(&self) -> &[SpeedSegment] { &self.speeds }
    #[inline(always)]
    pub fn scrolls(&self) -> &[Segment] { &self.scrolls }
    #[inline(always)]
    pub fn fakes(&self) -> &[Segment] { &self.fakes }

    pub fn bpm_segments(&self) -> Vec<(f64, f64)> {
        self.beat_to_time.iter().map(|p| (p.beat, p.bpm)).collect()
    }

    #[inline(always)]
    pub fn is_fake_at_beat(&self, beat: f64) -> bool {
        self.is_in_range_segment(&self.fakes, &self.fake_start_rows, beat)
    }

    #[inline(always)]
    pub fn is_fake_at_row(&self, row: i32) -> bool {
        self.is_in_range_segment(&self.fakes, &self.fake_start_rows, note_row_to_beat(row))
    }

    #[inline(always)]
    pub fn is_warp_at_beat(&self, beat: f64) -> bool {
        self.is_warp_at_row(beat_to_note_row_f32(beat as f32))
    }

    #[inline(always)]
    pub fn is_warp_at_row(&self, row: i32) -> bool {
        let Some(idx) = segment_index_at_row(&self.warp_start_rows, row) else { return false };
        let seg = self.warps[idx];
        if !(seg.value.is_finite() && seg.value > 0.0) { return false; }
        let beat_row = note_row_to_beat(row) as f32;
        let seg_beat = seg.beat as f32;
        if !(seg_beat <= beat_row && beat_row < seg_beat + seg.value as f32) { return false; }
        !(has_row(&self.stop_rows, row) || has_row(&self.delay_rows, row))
    }

    fn is_in_range_segment(&self, segs: &[Segment], rows: &[i32], beat: f64) -> bool {
        let row = beat_to_note_row_f32(beat as f32);
        let Some(idx) = segment_index_at_row(rows, row) else { return false };
        let seg = segs[idx];
        if !seg.value.is_finite() { return false; }
        let beat_f = note_row_to_beat(row) as f32;
        beat_f >= seg.beat as f32 && beat_f < (seg.beat + seg.value) as f32
    }

    #[inline(always)]
    pub fn is_judgable_at_row(&self, row: i32) -> bool {
        !self.is_warp_at_row(row) && !self.is_fake_at_row(row)
    }

    #[inline(always)]
    pub fn is_judgable_at_beat(&self, beat: f64) -> bool {
        self.is_judgable_at_row(beat_to_note_row_f32(beat as f32))
    }

    pub fn get_beat_info_from_time(&self, time: f64) -> BeatInfo {
        let elapsed = time + self.global_offset_sec;
        let start_time = -self.beat0_offset_sec - self.global_offset_sec;
        self.get_beat_internal(elapsed, start_time)
    }

    pub fn get_beat_for_time(&self, time: f64) -> f64 {
        self.get_beat_info_from_time(time).beat
    }

    pub fn get_time_for_beat(&self, beat: f64) -> f64 {
        self.get_time_internal(beat) - self.global_offset_sec
    }

    pub(crate) fn get_time_for_beat_f32(&self, target_beat: f64) -> f64 {
        let mut state = GetBeatStateF32::default();
        state.last_time = (-self.beat0_offset_sec - self.global_offset_sec) as f32;
        self.get_elapsed_time_f32(&mut state, target_beat as f32);
        state.last_time as f64 - self.global_offset_sec
    }

    fn get_time_internal(&self, target_beat: f64) -> f64 {
        let mut state = GetBeatState::default();
        state.last_time = -self.beat0_offset_sec - self.global_offset_sec;
        self.get_elapsed_time(&mut state, target_beat);
        state.last_time
    }

    fn get_beat_internal(&self, elapsed: f64, start_time: f64) -> BeatInfo {
        let mut state = GetBeatState { last_time: start_time, ..Default::default() };
        let mut bps = self.get_bpm_for_beat(0.0) / 60.0;

        loop {
            let (event_row, event_type) = self.find_next_event(&state, 0.0, false);
            if event_type == TimingEvent::NotFound { break; }

            let time_to_event = if state.is_warping { 0.0 } else { note_row_to_beat(event_row - state.last_row) / bps };
            let next_time = state.last_time + time_to_event;
            if elapsed < next_time { break; }
            state.last_time = next_time;

            match event_type {
                TimingEvent::WarpDest => state.is_warping = false,
                TimingEvent::Bpm => { bps = self.beat_to_time[state.bpm_idx].bpm / 60.0; state.bpm_idx += 1; }
                TimingEvent::Delay | TimingEvent::StopDelay => {
                    let d = self.delays[state.delay_idx].value;
                    if elapsed < state.last_time + d {
                        return BeatInfo { beat: self.delays[state.delay_idx].beat, is_in_delay: true, is_in_freeze: false };
                    }
                    state.last_time += d; state.delay_idx += 1;
                    if event_type == TimingEvent::Delay { state.last_row = event_row; continue; }
                }
                TimingEvent::Stop => {
                    let d = self.stops[state.stop_idx].value;
                    if elapsed < state.last_time + d {
                        return BeatInfo { beat: self.stops[state.stop_idx].beat, is_in_freeze: true, is_in_delay: false };
                    }
                    state.last_time += d; state.stop_idx += 1;
                }
                TimingEvent::Warp => {
                    state.is_warping = true;
                    let w = &self.warps[state.warp_idx];
                    state.warp_destination = state.warp_destination.max(w.beat + w.value);
                    state.warp_idx += 1;
                }
                _ => {}
            }
            state.last_row = event_row;
        }

        BeatInfo { beat: note_row_to_beat(state.last_row) + (elapsed - state.last_time) * bps, is_in_freeze: false, is_in_delay: false }
    }

    fn get_elapsed_time(&self, state: &mut GetBeatState, target_beat: f64) {
        let find_marker = target_beat < f64::MAX;
        let mut bps = self.get_bpm_for_beat(note_row_to_beat(state.last_row)) / 60.0;

        loop {
            let (event_row, event_type) = self.find_next_event(state, target_beat, find_marker);
            if event_type == TimingEvent::NotFound { break; }

            let dt = if state.is_warping { 0.0 } else { note_row_to_beat(event_row - state.last_row) / bps };
            state.last_time += dt;

            match event_type {
                TimingEvent::WarpDest => state.is_warping = false,
                TimingEvent::Bpm => { bps = self.beat_to_time[state.bpm_idx].bpm / 60.0; state.bpm_idx += 1; }
                TimingEvent::Stop | TimingEvent::StopDelay => { state.last_time += self.stops[state.stop_idx].value; state.stop_idx += 1; }
                TimingEvent::Delay => { state.last_time += self.delays[state.delay_idx].value; state.delay_idx += 1; }
                TimingEvent::Marker => return,
                TimingEvent::Warp => {
                    state.is_warping = true;
                    let w = &self.warps[state.warp_idx];
                    state.warp_destination = state.warp_destination.max(w.beat + w.value);
                    state.warp_idx += 1;
                }
                _ => {}
            }
            state.last_row = event_row;
        }
    }

    fn get_elapsed_time_f32(&self, state: &mut GetBeatStateF32, target_beat: f32) {
        let find_marker = target_beat < f32::MAX;
        let mut bps = self.get_bpm_for_row_f32(state.last_row) / 60.0;
        let mut curr_segment = state.bpm_idx + state.warp_idx + state.stop_idx + state.delay_idx;

        while curr_segment < u32::MAX as usize {
            let (event_row, event_type) = self.find_next_event_f32(state, target_beat, find_marker);
            if event_type == TimingEvent::NotFound { break; }

            let dt = if state.is_warping { 0.0 } else { note_row_to_beat_f32(event_row - state.last_row) / bps };
            state.last_time += dt;

            match event_type {
                TimingEvent::WarpDest => state.is_warping = false,
                TimingEvent::Bpm => {
                    bps = self.beat_to_time[state.bpm_idx].bpm as f32 / 60.0;
                    state.bpm_idx += 1;
                    curr_segment += 1;
                }
                TimingEvent::Stop | TimingEvent::StopDelay => {
                    state.last_time += self.stops[state.stop_idx].value as f32;
                    state.stop_idx += 1;
                    curr_segment += 1;
                }
                TimingEvent::Delay => {
                    state.last_time += self.delays[state.delay_idx].value as f32;
                    state.delay_idx += 1;
                    curr_segment += 1;
                }
                TimingEvent::Marker => return,
                TimingEvent::Warp => {
                    state.is_warping = true;
                    let w = &self.warps[state.warp_idx];
                    let warp_sum = w.value as f32 + w.beat as f32;
                    if warp_sum > state.warp_destination { state.warp_destination = warp_sum; }
                    state.warp_idx += 1;
                    curr_segment += 1;
                }
                _ => {}
            }
            state.last_row = event_row;
        }
    }

    fn find_next_event(&self, state: &GetBeatState, beat: f64, find_marker: bool) -> (i32, TimingEvent) {
        let mut row = i32::MAX;
        let mut event = TimingEvent::NotFound;

        if state.is_warping {
            let r = beat_to_note_row(state.warp_destination);
            if r < row { row = r; event = TimingEvent::WarpDest; }
        }
        if state.bpm_idx < self.beat_to_time.len() {
            let r = beat_to_note_row(self.beat_to_time[state.bpm_idx].beat);
            if r < row { row = r; event = TimingEvent::Bpm; }
        }
        if state.delay_idx < self.delays.len() {
            let r = beat_to_note_row(self.delays[state.delay_idx].beat);
            if r < row { row = r; event = TimingEvent::Delay; }
        }
        if find_marker {
            let r = beat_to_note_row(beat);
            if r < row { row = r; event = TimingEvent::Marker; }
        }
        if state.stop_idx < self.stops.len() {
            let r = beat_to_note_row(self.stops[state.stop_idx].beat);
            if r < row { row = r; event = TimingEvent::Stop; }
        }
        if state.warp_idx < self.warps.len() {
            let r = beat_to_note_row(self.warps[state.warp_idx].beat);
            if r < row { row = r; event = TimingEvent::Warp; }
        }

        (row, event)
    }

    fn find_next_event_f32(&self, state: &GetBeatStateF32, beat: f32, find_marker: bool) -> (i32, TimingEvent) {
        let mut row = i32::MAX;
        let mut event = TimingEvent::NotFound;

        if state.is_warping {
            let r = beat_to_note_row_f32_exact(state.warp_destination);
            if r < row { row = r; event = TimingEvent::WarpDest; }
        }
        if state.bpm_idx < self.beat_to_time.len() {
            let r = beat_to_note_row_f32_exact(self.beat_to_time[state.bpm_idx].beat as f32);
            if r < row { row = r; event = TimingEvent::Bpm; }
        }
        if state.delay_idx < self.delays.len() {
            let r = beat_to_note_row_f32_exact(self.delays[state.delay_idx].beat as f32);
            if r < row { row = r; event = TimingEvent::Delay; }
        }
        if find_marker {
            let r = beat_to_note_row_f32_exact(beat);
            if r < row { row = r; event = TimingEvent::Marker; }
        }
        if state.stop_idx < self.stops.len() {
            let r = beat_to_note_row_f32_exact(self.stops[state.stop_idx].beat as f32);
            if r < row { row = r; event = TimingEvent::Stop; }
        }
        if state.warp_idx < self.warps.len() {
            let r = beat_to_note_row_f32_exact(self.warps[state.warp_idx].beat as f32);
            if r < row { row = r; event = TimingEvent::Warp; }
        }

        (row, event)
    }

    fn get_bpm_for_row_f32(&self, row: i32) -> f32 {
        if self.beat_to_time.is_empty() { return DEFAULT_BPM as f32; }
        let pos = self.beat_to_time.partition_point(|p| beat_to_note_row_f32_exact(p.beat as f32) <= row);
        if pos == 0 { self.beat_to_time[0].bpm as f32 } else { self.beat_to_time[pos - 1].bpm as f32 }
    }

    pub fn get_bpm_for_beat(&self, beat: f64) -> f64 {
        if self.beat_to_time.is_empty() { return DEFAULT_BPM; }
        let idx = self.beat_to_time.partition_point(|p| p.beat <= beat);
        self.beat_to_time[idx.saturating_sub(1).max(0)].bpm
    }

    pub fn get_capped_max_bpm(&self, cap: Option<f64>) -> f64 {
        let mut max = self.max_bpm.max(self.beat_to_time.iter().map(|p| p.bpm).filter(|b| b.is_finite() && *b > 0.0).fold(0.0, f64::max));
        if let Some(c) = cap { if c > 0.0 { max = max.min(c); } }
        if max > 0.0 { max } else { DEFAULT_BPM }
    }

    pub fn get_displayed_beat(&self, beat: f64) -> f64 {
        if self.scroll_prefix.is_empty() || beat < self.scroll_prefix[0].beat { return beat; }
        let idx = self.scroll_prefix.partition_point(|p| p.beat <= beat).saturating_sub(1);
        let p = self.scroll_prefix[idx];
        p.cum_displayed + (beat - p.beat) * p.ratio
    }

    pub fn get_speed_multiplier(&self, beat: f64, time: f64) -> f64 {
        if self.speeds.is_empty() { return 1.0; }
        let idx = self.speeds.partition_point(|s| s.beat <= beat);
        if idx == 0 { return 1.0; }
        let i = idx - 1;
        let seg = &self.speeds[i];
        let rt = &self.speed_runtime[i];

        if time >= rt.end_time || seg.delay <= 0.0 { return seg.ratio; }
        if time < rt.start_time { return rt.prev_ratio; }
        let progress = (time - rt.start_time) / (rt.end_time - rt.start_time);
        rt.prev_ratio + (seg.ratio - rt.prev_ratio) * progress
    }
}

use std::borrow::Cow;

use crate::parse::{
    decode_bytes,
    extract_sections,
    parse_version,
    split_notes_fields,
    unescape_trim,
    ParsedChartEntry,
    ParsedSimfileData,
};
use crate::timing::{
    compute_timing_segments,
    format_bpm_segments_like_itg,
    round_sig_figs_itg,
    roundtrip_bpm_itg,
    steps_timing_allowed,
    TimingData,
    TimingFormat,
    ROWS_PER_BEAT,
};

fn normalize_decimal(s: &str) -> Option<String> {
    let value: f64 = if s.chars().any(|c| c.is_control()) {
        let mut cleaned = String::with_capacity(s.len());
        cleaned.extend(s.chars().filter(|c| !c.is_control()));
        cleaned.trim().parse().ok()?
    } else {
        s.trim().parse().ok()?
    };

    let mult = 1000.0;
    let temp = value * mult + 0.5;
    let rounded = (temp - temp.rem_euclid(1.0)) / mult;

    Some(format!("{:.3}", rounded))
}

fn normalize_entry(beat_bpm: &str) -> String {
    let trimmed = beat_bpm.trim();
    if let Some((beat_str, bpm_str)) = trimmed.split_once('=') {
        if let (Some(beat), Some(bpm)) = (normalize_decimal(beat_str), normalize_decimal(bpm_str)) {
            return format!("{}={}", beat, bpm);
        }
    }
    trimmed.to_string()
}

pub(crate) fn parse_beat_or_row(raw: &str) -> Option<f64> {
    let mut trimmed = raw.trim();
    let mut is_row = false;
    if let Some(stripped) = trimmed.strip_suffix('r').or_else(|| trimmed.strip_suffix('R')) {
        trimmed = stripped.trim_end();
        is_row = true;
    }
    let value = trimmed.parse::<f64>().ok()?;
    let value_f32 = value as f32;
    if !value_f32.is_finite() {
        return None;
    }
    if is_row {
        Some((value_f32 / ROWS_PER_BEAT as f32) as f64)
    } else {
        Some(value_f32 as f64)
    }
}

pub fn normalize_float_digits(param: &str) -> String {
    let mut out = String::with_capacity(param.len());
    for entry in param.split(',') {
        let trimmed = entry.trim();
        if trimmed.is_empty() {
            continue;
        }
        let normalized = normalize_entry(trimmed);
        if !out.is_empty() {
            out.push(',');
        }
        out.push_str(&normalized);
    }
    out
}

pub fn clean_timing_map(param: &str) -> String {
    let mut out = String::with_capacity(param.len());
    let mut scratch = String::new();

    for entry in param.split(',') {
        if entry.is_empty() {
            continue;
        }

        let trimmed = if entry.chars().any(|c| c.is_control()) {
            scratch.clear();
            scratch.extend(entry.chars().filter(|c| !c.is_control()));
            scratch.trim()
        } else {
            entry.trim()
        };

        if trimmed.is_empty() {
            continue;
        }
        if !out.is_empty() {
            out.push(',');
        }
        out.push_str(trimmed);
    }

    out
}

pub fn clean_timing_map_cow(param: &str) -> Cow<'_, str> {
    if param.is_empty() {
        return Cow::Borrowed("");
    }
    for entry in param.split(',') {
        if entry.is_empty() || entry.trim() != entry || entry.chars().any(|c| c.is_control()) {
            return Cow::Owned(clean_timing_map(param));
        }
    }
    Cow::Borrowed(param)
}

pub fn normalize_chart_tag(tag: Option<Vec<u8>>) -> Option<String> {
    normalize_chart_tag_bytes(tag.as_deref())
}

fn normalize_chart_tag_bytes(tag: Option<&[u8]>) -> Option<String> {
    tag.and_then(|bytes| std::str::from_utf8(bytes).ok())
        .map(normalize_float_digits)
        .filter(|s| !s.is_empty())
}

fn normalize_tag_bytes(tag: Option<&[u8]>) -> String {
    tag.and_then(|bytes| std::str::from_utf8(bytes).ok())
        .map(normalize_float_digits)
        .unwrap_or_default()
}

fn clean_tag_bytes(tag: Option<&[u8]>) -> String {
    tag.and_then(|bytes| std::str::from_utf8(bytes).ok())
        .map(clean_timing_map)
        .unwrap_or_default()
}

fn clean_chart_tag_bytes(tag: Option<&[u8]>) -> Option<String> {
    tag.and_then(|bytes| std::str::from_utf8(bytes).ok())
        .map(clean_timing_map)
        .filter(|s| !s.is_empty())
}

fn decode_display_bpm_tag(tag: Option<&[u8]>) -> Option<String> {
    let text = tag.map(|bytes| unescape_trim(decode_bytes(bytes).as_ref()))?;
    if text.is_empty() {
        return None;
    }
    Some(text)
}

fn parse_display_bpm(tag: &str) -> Option<(f64, f64)> {
    let trimmed = tag.trim();
    if trimmed.is_empty() || trimmed == "*" {
        return None;
    }
    let mut parts = trimmed.split(':');
    let min_str = parts.next()?.trim();
    if min_str.is_empty() {
        return None;
    }
    let max_str = parts.next().unwrap_or("").trim();

    let min_raw = min_str.parse::<f64>().unwrap_or(0.0);
    let min_raw = if min_raw.is_finite() { min_raw } else { 0.0 };
    let min = (min_raw as f32) as f64;

    let max_raw = if max_str.is_empty() {
        min
    } else {
        max_str.parse::<f64>().unwrap_or(0.0)
    };
    let max_raw = if max_raw.is_finite() { max_raw } else { 0.0 };
    let max = (max_raw as f32) as f64;

    Some((min, max))
}

fn format_bpm_sl(bpm: f64, music_rate: f64) -> String {
    if music_rate == 1.0 {
        return format!("{:.0}", bpm);
    }
    let mut s = format!("{:.1}", bpm);
    if s.ends_with(".0") {
        s.truncate(s.len() - 2);
    }
    s
}

fn join_display_bpm_sl(bpm_min: f64, bpm_max: f64, music_rate: f64) -> String {
    let lo = format_bpm_sl(bpm_min, music_rate);
    if bpm_min == bpm_max {
        return lo;
    }
    let hi = format_bpm_sl(bpm_max, music_rate);
    format!("{} - {}", lo, hi)
}

pub(crate) fn resolve_display_bpm(
    chart_tag: Option<&str>,
    actual_min: f64,
    actual_max: f64,
    music_rate: f64,
) -> (f64, f64, String) {
    let chart_tag = chart_tag.map(|s| s.trim()).filter(|s| !s.is_empty());
    let (mut min, mut max) = match chart_tag {
        Some(tag) => match parse_display_bpm(tag) {
            Some((min, max)) => (min, max),
            None => (actual_min, actual_max),
        },
        None => (actual_min, actual_max),
    };

    if min <= 0.0 || max <= 0.0 {
        min = actual_min;
        max = actual_max;
    }

    let scaled_min = min * music_rate;
    let scaled_max = max * music_rate;
    let display = join_display_bpm_sl(scaled_min, scaled_max, music_rate);

    (scaled_min, scaled_max, display)
}

#[derive(Debug, Clone)]
pub struct ChartBpmSnapshot {
    pub step_type: String,
    pub difficulty: String,
    pub hash_bpms: String,
    pub bpms_formatted: String,
    pub bpm_min: f64,
    pub bpm_max: f64,
    pub display_bpm: String,
    pub display_bpm_min: f64,
    pub display_bpm_max: f64,
}

#[derive(Clone)]
struct TimingGlobals {
    bpms_raw: String,
    stops_raw: String,
    delays_raw: String,
    warps_raw: String,
    speeds_raw: String,
    scrolls_raw: String,
    fakes_raw: String,
    bpms_norm: String,
    display_bpm_raw: Option<String>,
    timing_format: TimingFormat,
    allow_steps_timing: bool,
}

#[derive(Clone)]
struct ChartTimingTags {
    bpms_raw: Option<String>,
    stops_raw: Option<String>,
    delays_raw: Option<String>,
    warps_raw: Option<String>,
    speeds_raw: Option<String>,
    scrolls_raw: Option<String>,
    fakes_raw: Option<String>,
    bpms_norm: Option<String>,
}

fn timing_globals(parsed: &ParsedSimfileData<'_>, extension: &str) -> TimingGlobals {
    let timing_format = TimingFormat::from_extension(extension);
    let allow_steps_timing =
        steps_timing_allowed(parse_version(parsed.version, timing_format), timing_format);

    TimingGlobals {
        bpms_raw: clean_tag_bytes(parsed.bpms),
        stops_raw: clean_tag_bytes(parsed.stops),
        delays_raw: clean_tag_bytes(parsed.delays),
        warps_raw: clean_tag_bytes(parsed.warps),
        speeds_raw: clean_tag_bytes(parsed.speeds),
        scrolls_raw: clean_tag_bytes(parsed.scrolls),
        fakes_raw: clean_tag_bytes(parsed.fakes),
        bpms_norm: normalize_tag_bytes(parsed.bpms),
        display_bpm_raw: decode_display_bpm_tag(parsed.display_bpm),
        timing_format,
        allow_steps_timing,
    }
}

fn chart_timing_tags(entry: &ParsedChartEntry<'_>) -> ChartTimingTags {
    ChartTimingTags {
        bpms_raw: clean_chart_tag_bytes(entry.chart_bpms.as_deref()),
        stops_raw: clean_chart_tag_bytes(entry.chart_stops.as_deref()),
        delays_raw: clean_chart_tag_bytes(entry.chart_delays.as_deref()),
        warps_raw: clean_chart_tag_bytes(entry.chart_warps.as_deref()),
        speeds_raw: clean_chart_tag_bytes(entry.chart_speeds.as_deref()),
        scrolls_raw: clean_chart_tag_bytes(entry.chart_scrolls.as_deref()),
        fakes_raw: clean_chart_tag_bytes(entry.chart_fakes.as_deref()),
        bpms_norm: normalize_chart_tag_bytes(entry.chart_bpms.as_deref()),
    }
}

fn chart_metadata(fields: &[&[u8]], timing_format: TimingFormat) -> Option<(String, String)> {
    if fields.len() < 4 {
        return None;
    }
    let step_type = unescape_trim(decode_bytes(fields[0]).as_ref());
    if step_type == "lights-cabinet" {
        return None;
    }
    let description = unescape_trim(decode_bytes(fields[1]).as_ref());
    let difficulty_raw = unescape_trim(decode_bytes(fields[2]).as_ref());
    let meter_raw = unescape_trim(decode_bytes(fields[3]).as_ref());
    let extension = if timing_format == TimingFormat::Sm {
        "sm"
    } else {
        "ssc"
    };
    let difficulty = crate::resolve_difficulty_label(&difficulty_raw, &description, &meter_raw, extension);
    Some((step_type, difficulty))
}

fn chart_bpm_snapshot(
    entry: &ParsedChartEntry<'_>,
    globals: &TimingGlobals,
) -> Option<ChartBpmSnapshot> {
    let (fields, _chart_data) = split_notes_fields(&entry.notes);
    let (step_type, difficulty) = chart_metadata(&fields, globals.timing_format)?;
    let tags = chart_timing_tags(entry);
    let hash_bpms = tags
        .bpms_norm
        .clone()
        .unwrap_or_else(|| globals.bpms_norm.clone());
    let use_chart = globals.allow_steps_timing;
    let segments = compute_timing_segments(
        if use_chart { tags.bpms_raw.as_deref() } else { None },
        &globals.bpms_raw,
        if use_chart { tags.stops_raw.as_deref() } else { None },
        &globals.stops_raw,
        if use_chart { tags.delays_raw.as_deref() } else { None },
        &globals.delays_raw,
        if use_chart { tags.warps_raw.as_deref() } else { None },
        &globals.warps_raw,
        if use_chart { tags.speeds_raw.as_deref() } else { None },
        &globals.speeds_raw,
        if use_chart { tags.scrolls_raw.as_deref() } else { None },
        &globals.scrolls_raw,
        if use_chart { tags.fakes_raw.as_deref() } else { None },
        &globals.fakes_raw,
        globals.timing_format,
    );

    let mut bpms = Vec::with_capacity(segments.bpms.len());
    for (beat, bpm) in segments.bpms {
        bpms.push((beat as f64, bpm as f64));
    }
    let bpms_formatted = format_bpm_segments_like_itg(&bpms);
    let (bpm_min_raw, bpm_max_raw) = actual_bpm_range_raw(&bpms);
    let bpm_min = round_sig_figs_itg(bpm_min_raw);
    let bpm_max = round_sig_figs_itg(bpm_max_raw);
    let chart_display_bpm = decode_display_bpm_tag(entry.chart_display_bpm.as_deref());
    let display_tag = chart_display_bpm
        .as_deref()
        .or_else(|| globals.display_bpm_raw.as_deref());
    let (display_bpm_min_raw, display_bpm_max_raw, display_bpm) = resolve_display_bpm(
        display_tag,
        bpm_min_raw,
        bpm_max_raw,
        1.0,
    );
    let display_bpm_min = round_sig_figs_itg(display_bpm_min_raw);
    let display_bpm_max = round_sig_figs_itg(display_bpm_max_raw);

    Some(ChartBpmSnapshot {
        step_type,
        difficulty,
        hash_bpms,
        bpms_formatted,
        bpm_min,
        bpm_max,
        display_bpm,
        display_bpm_min,
        display_bpm_max,
    })
}

pub fn chart_bpm_snapshots(
    simfile_data: &[u8],
    extension: &str,
) -> Result<Vec<ChartBpmSnapshot>, String> {
    let parsed_data = extract_sections(simfile_data, extension).map_err(|e| e.to_string())?;
    let globals = timing_globals(&parsed_data, extension);
    Ok(parsed_data
        .notes_list
        .iter()
        .filter_map(|entry| chart_bpm_snapshot(entry, &globals))
        .collect())
}

fn normalized_3dp_to_thousandths(s: &str) -> Option<i64> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }

    let (sign, body) = s.strip_prefix('-').map_or((1i64, s), |rest| (-1i64, rest));
    let (int_part, frac_part) = body.split_once('.').unwrap_or((body, "0"));

    if int_part.is_empty() || !int_part.as_bytes().iter().all(|b| b.is_ascii_digit()) {
        return None;
    }

    let int_value: i64 = int_part.parse().ok()?;
    let mut frac_value: i64 = 0;
    let mut frac_digits = 0usize;
    for &b in frac_part.as_bytes() {
        if !b.is_ascii_digit() {
            return None;
        }
        if frac_digits < 3 {
            frac_value = frac_value * 10 + (b - b'0') as i64;
            frac_digits += 1;
        }
    }
    while frac_digits < 3 {
        frac_value *= 10;
        frac_digits += 1;
    }

    Some(sign * (int_value * 1000 + frac_value))
}

#[derive(Clone)]
struct NormalizedTimingEntry {
    beat_thousandths: i64,
    beat_str: String,
    value_thousandths: i64,
    value_str: String,
    index: usize,
}

fn parse_and_normalize_timing_entry(entry: &str, index: usize) -> Option<NormalizedTimingEntry> {
    let trimmed = entry.trim();
    let (beat_raw, value_raw) = trimmed.split_once('=')?;
    let beat_str = normalize_decimal(beat_raw)?;
    let value_str = normalize_decimal(value_raw)?;
    Some(NormalizedTimingEntry {
        beat_thousandths: normalized_3dp_to_thousandths(&beat_str)?,
        beat_str,
        value_thousandths: normalized_3dp_to_thousandths(&value_str)?,
        value_str,
        index,
    })
}

pub fn normalize_and_tidy_bpms(param: &str) -> String {
    let mut entries: Vec<NormalizedTimingEntry> = Vec::with_capacity(
        param.as_bytes().iter().filter(|&&b| b == b',').count() + 1,
    );
    for (i, entry) in param.split(',').enumerate() {
        if let Some(parsed) = parse_and_normalize_timing_entry(entry, i) {
            entries.push(parsed);
        }
    }

    if entries.is_empty() {
        return "0.000=60.000".to_string();
    }

    entries.sort_by(|a, b| a
        .beat_thousandths
        .cmp(&b.beat_thousandths)
        .then_with(|| a.index.cmp(&b.index)));

    let mut last_per_beat: Vec<NormalizedTimingEntry> = Vec::with_capacity(entries.len());
    for entry in entries {
        if let Some(last) = last_per_beat.last_mut() {
            if last.beat_thousandths == entry.beat_thousandths {
                *last = entry;
                continue;
            }
        }
        last_per_beat.push(entry);
    }

    if let Some(first) = last_per_beat.first_mut() {
        if first.beat_thousandths != 0 {
            first.beat_thousandths = 0;
            first.beat_str = "0.000".to_string();
        }
    }

    let mut tidied: Vec<NormalizedTimingEntry> = Vec::with_capacity(last_per_beat.len());
    let mut last_value: Option<i64> = None;
    for entry in last_per_beat {
        if last_value == Some(entry.value_thousandths) {
            continue;
        }
        last_value = Some(entry.value_thousandths);
        tidied.push(entry);
    }

    let mut out = String::new();
    for entry in tidied {
        if !out.is_empty() {
            out.push(',');
        }
        out.push_str(&entry.beat_str);
        out.push('=');
        out.push_str(&entry.value_str);
    }
    out
}

pub fn parse_bpm_map(normalized_bpms: &str) -> Vec<(f64, f64)> {
    let mut bpms_vec: Vec<(f64, f64)> = Vec::with_capacity(
        normalized_bpms.as_bytes().iter().filter(|&&b| b == b',').count() + 1,
    );
    for chunk in normalized_bpms.split(',') {
        let chunk = chunk.trim();
        if chunk.is_empty() {
            continue;
        }
        let Some((left, right)) = chunk.split_once('=') else {
            continue;
        };
        let beat = parse_beat_or_row(left.trim());
        let bpm = right.trim().parse::<f64>().ok();
        if let (Some(beat), Some(bpm)) = (beat, bpm) {
            let bpm = bpm as f32 as f64;
            bpms_vec.push((beat, bpm));
        }
    }

    bpms_vec.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    bpms_vec
}

/// Alias for parsing generic beat=value timing maps (Stops, Delays, Warps).
pub fn parse_timing_map(normalized: &str) -> Vec<(f64, f64)> {
    parse_bpm_map(normalized)
}

/// Returns the BPM in effect at a given beat.
/// This is used for actual timing calculations and is NOT filtered.
pub fn get_current_bpm(beat: f64, bpm_map: &[(f64, f64)]) -> f64 {
    if bpm_map.is_empty() {
        return 0.0;
    }

    // `partition_point` returns the index of the first element for which the predicate is false.
    // It's equivalent to a binary search for the insertion point to maintain order.
    let pos = bpm_map.partition_point(|&(b, _)| b <= beat);

    if pos == 0 {
        // If the beat is before the very first BPM change, the effective BPM is that first change.
        bpm_map[0].1
    } else {
        // `pos` is the index of the first BPM change *after* the given beat.
        // The correct BPM is the one at the previous index.
        bpm_map[pos - 1].1
    }
}

#[inline(always)]
pub(crate) fn for_each_measure_bpm<F>(
    measure_count: usize,
    bpm_map: &[(f64, f64)],
    beats_per_measure: f64,
    mut f: F,
) where
    F: FnMut(usize, f64),
{
    if measure_count == 0 || bpm_map.is_empty() {
        return;
    }

    let mut bpm_idx = 0usize;
    let mut cur_bpm = bpm_map[0].1;
    let mut next_change = bpm_map
        .get(1)
        .map(|(beat, _)| *beat)
        .unwrap_or(f64::INFINITY);

    for idx in 0..measure_count {
        let beat = idx as f64 * beats_per_measure;
        while beat >= next_change {
            bpm_idx += 1;
            cur_bpm = bpm_map[bpm_idx].1;
            next_change = bpm_map
                .get(bpm_idx + 1)
                .map(|(b, _)| *b)
                .unwrap_or(f64::INFINITY);
        }
        f(idx, cur_bpm);
    }
}

/// Threshold for determining if a BPM is a "gimmick" (warp/visual effect) vs playable.
/// Matches Simply Love's logic roughly (SL uses 0.12s/measure which is ~2000 BPM).
/// We use 10,000 here to be conservative but catch the millions.
const GIMMICK_BPM_THRESHOLD: f64 = 10000.0;

/// Determines if a BPM is considered "playable" for stats/display purposes.
/// Filters out stops (<= 0) and visual gimmick warps (>= 10000).
fn is_display_bpm(bpm: f64) -> bool {
    bpm > 0.0 && bpm < GIMMICK_BPM_THRESHOLD
}

/// Computes the min/max BPM range for display purposes.
///
/// Applies a heuristic to ignore "gimmick" BPMs (e.g., <= 0 or >= 10,000) which are
/// often used for visual effects or stops, unless no valid BPMs remain.
pub fn compute_bpm_range(bpm_map: &[(f64, f64)]) -> (i32, i32) {
    if bpm_map.is_empty() {
        return (0, 0);
    }

    let mut min_bpm = f64::MAX;
    let mut max_bpm = f64::MIN;
    let mut count = 0;

    for &(_, bpm) in bpm_map {
        if is_display_bpm(bpm) {
            min_bpm = min_bpm.min(bpm);
            max_bpm = max_bpm.max(bpm);
            count += 1;
        }
    }

    if count == 0 {
        // Fallback: if all BPMs were filtered out (e.g., gimmicks only), include everything.
        min_bpm = f64::MAX;
        max_bpm = f64::MIN;
        for &(_, bpm) in bpm_map {
            min_bpm = min_bpm.min(bpm);
            max_bpm = max_bpm.max(bpm);
        }
    }

    (min_bpm.round() as i32, max_bpm.round() as i32)
}

pub fn compute_actual_bpm_range(bpm_map: &[(f64, f64)]) -> (f64, f64) {
    let (min_bpm, max_bpm) = actual_bpm_range_raw(bpm_map);
    (round_sig_figs_itg(min_bpm), round_sig_figs_itg(max_bpm))
}

pub(crate) fn actual_bpm_range_raw(bpm_map: &[(f64, f64)]) -> (f64, f64) {
    if bpm_map.is_empty() {
        return (0.0, 0.0);
    }

    let mut min_bpm = f64::MAX;
    let mut max_bpm = 0.0;

    for &(_, bpm) in bpm_map {
        if !bpm.is_finite() {
            continue;
        }
        let bpm = roundtrip_bpm_itg(bpm);
        if !bpm.is_finite() {
            continue;
        }
        if bpm < min_bpm {
            min_bpm = bpm;
        }
        if bpm > max_bpm {
            max_bpm = bpm;
        }
    }

    if !min_bpm.is_finite() {
        min_bpm = 0.0;
    }
    if max_bpm < 0.0 {
        max_bpm = 0.0;
    }

    (min_bpm, max_bpm)
}

/// Calculates the accurate cumulative time to reach a target beat, accounting for
/// BPM changes, Stops, Delays, and Warps.
///
/// Logic mimics StepMania/ITGmania's `GetElapsedTimeFromBeat`:
/// - Beats advance time based on current BPM.
/// - Warps skip beats instantly (time doesn't advance).
/// - Stops/Delays add time instantly (beats don't advance).
pub fn get_elapsed_time(
    target_beat: f64,
    bpm_map: &[(f64, f64)],
    stop_map: &[(f64, f64)],
    delay_map: &[(f64, f64)],
    warp_map: &[(f64, f64)],
) -> f64 {
    if stop_map.is_empty() && delay_map.is_empty() && warp_map.is_empty() {
        if bpm_map.is_empty() {
            return 0.0;
        }

        let mut current_time = 0.0;
        let mut current_beat = 0.0;
        let mut current_bpm = if bpm_map[0].0 <= 0.0 { bpm_map[0].1 } else { 60.0 };

        let mut idx = 0usize;
        while idx < bpm_map.len() && bpm_map[idx].0 <= 0.0 {
            current_bpm = bpm_map[idx].1;
            idx += 1;
        }

        while idx < bpm_map.len() {
            let (beat, bpm) = bpm_map[idx];
            if beat > target_beat {
                break;
            }
            if beat > current_beat && current_bpm > 0.0 {
                current_time += (beat - current_beat) * (60.0 / current_bpm);
            }
            current_beat = beat;
            current_bpm = bpm;
            idx += 1;
        }

        if target_beat > current_beat && current_bpm > 0.0 {
            current_time += (target_beat - current_beat) * (60.0 / current_bpm);
        }

        return current_time;
    }

    // Event priority: 0=BPM, 1=Stop/Delay, 2=Warp
    let mut events = Vec::with_capacity(bpm_map.len() + stop_map.len() + delay_map.len() + warp_map.len());
    for &(b, v) in bpm_map { events.push((b, 0, v)); }
    for &(b, v) in stop_map { events.push((b, 1, v)); }
    for &(b, v) in delay_map { events.push((b, 1, v)); }
    for &(b, v) in warp_map { events.push((b, 2, v)); }

    // Sort by beat, then priority
    events.sort_by(|a, b| {
        a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal)
           .then_with(|| a.1.cmp(&b.1))
    });

    let mut current_time = 0.0;
    let mut current_beat = 0.0;
    let mut current_bpm = if !bpm_map.is_empty() && bpm_map[0].0 <= 0.0 { bpm_map[0].1 } else { 60.0 };
    let mut warp_end_beat = 0.0;

    for (event_beat, priority, value) in events {
        // Optimization: if we are past target and not currently warping, we can stop.
        if event_beat > target_beat && warp_end_beat <= target_beat {
            break;
        }

        // Advance time to the event beat
        if event_beat > current_beat {
            // We only accumulate time for beats that are NOT inside a warp.
            let effective_start = current_beat.max(warp_end_beat);
            if event_beat > effective_start {
                let valid_dist = event_beat - effective_start;
                if current_bpm > 0.0 {
                    current_time += valid_dist * (60.0 / current_bpm);
                }
            }
            current_beat = event_beat;
        }

        match priority {
            0 => current_bpm = value,
            1 => current_time += value, // Stop/Delay adds time
            2 => {
                // Warp skips beats instantly.
                let end = event_beat + value;
                if end > warp_end_beat { warp_end_beat = end; }
            }
            _ => {}
        }
    }

    // Final advance to target beat
    let effective_start = current_beat.max(warp_end_beat);
    if target_beat > effective_start {
        let valid_dist = target_beat - effective_start;
        if current_bpm > 0.0 {
            current_time += valid_dist * (60.0 / current_bpm);
        }
    }

    current_time
}

#[inline(always)]
fn trim_cr(line: &[u8]) -> &[u8] {
    if line.last() == Some(&b'\r') {
        &line[..line.len() - 1]
    } else {
        line
    }
}

fn compute_last_beat_impl<const LANES: usize>(minimized_note_data: &[u8]) -> f64 {
    let mut hold_depths = [0u32; LANES];
    let mut last_measure_idx: Option<usize> = None;
    let mut last_row_in_measure: usize = 0;
    let mut last_rows_in_measure: usize = 0;
    let mut measure_idx = 0usize;
    let mut row_in_measure = 0usize;
    let mut saw_terminator = false;

    for line_raw in minimized_note_data.split(|&b| b == b'\n') {
        let line = trim_cr(line_raw);
        if line.is_empty() {
            continue;
        }

        match line[0] {
            b',' => {
                if last_measure_idx == Some(measure_idx) {
                    last_rows_in_measure = row_in_measure;
                }
                measure_idx += 1;
                row_in_measure = 0;
                continue;
            }
            b';' => {
                if last_measure_idx == Some(measure_idx) {
                    last_rows_in_measure = row_in_measure;
                }
                saw_terminator = true;
                break;
            }
            _ => {}
        }

        if line.len() < LANES {
            continue;
        }

        let mut has_object = false;
        for (col, &ch) in line[..LANES].iter().enumerate() {
            match ch {
                b'1' | b'M' | b'K' | b'L' | b'F' => {
                    has_object = true;
                }
                b'2' | b'4' => {
                    hold_depths[col] = hold_depths[col].saturating_add(1);
                }
                b'3' => {
                    if hold_depths[col] > 0 {
                        hold_depths[col] -= 1;
                        has_object = true;
                    }
                }
                _ => {}
            }
        }

        if has_object {
            last_measure_idx = Some(measure_idx);
            last_row_in_measure = row_in_measure;
        }
        row_in_measure += 1;
    }

    if !saw_terminator && last_measure_idx == Some(measure_idx) {
        last_rows_in_measure = row_in_measure;
    }

    let Some(measure_idx) = last_measure_idx else {
        return 0.0;
    };

    let total_rows_in_measure = last_rows_in_measure.max(1) as f64;
    let row_index = last_row_in_measure as f64;
    let beats_into_measure = 4.0 * (row_index / total_rows_in_measure);
    let beat = (measure_idx as f64) * 4.0 + beats_into_measure;
    let row = crate::timing::beat_to_note_row(beat);
    crate::timing::note_row_to_beat(row)
}

/// Computes the beat of the last playable object in the chart from minimized note data.
///
/// The minimized format produced by `minimize_chart_and_count_with_lanes` is:
///   - fixed-width note rows (per-chart lane count) followed by '\n'
///   - ",\n" as a measure separator
/// Measures are assumed to be 4 beats long, matching StepMania's default behavior.
pub fn compute_last_beat(minimized_note_data: &[u8], lanes: usize) -> f64 {
    match lanes {
        4 => compute_last_beat_impl::<4>(minimized_note_data),
        8 => compute_last_beat_impl::<8>(minimized_note_data),
        _ => compute_last_beat_impl::<4>(minimized_note_data),
    }
}

pub fn compute_total_chart_length(
    minimized_note_data: &[u8],
    lanes: usize,
    bpm_map: &[(f64, f64)],
    stop_map: &[(f64, f64)],
    delay_map: &[(f64, f64)],
    warp_map: &[(f64, f64)],
) -> i32 {
    let target_beat = compute_last_beat(minimized_note_data, lanes);
    if target_beat <= 0.0 || bpm_map.is_empty() {
        return 0;
    }

    get_elapsed_time(target_beat, bpm_map, stop_map, delay_map, warp_map).floor() as i32
}

/// Computes the number of mines that are actually judgable, i.e. not inside
/// warp ranges or #FAKES ranges. Uses the minimized chart data format
/// produced by `minimize_chart_and_count`.
pub fn compute_mines_nonfake(
    minimized_note_data: &[u8],
    lanes: usize,
    warp_map: &[(f64, f64)],
    fake_map: &[(f64, f64)],
) -> u32 {
    #[derive(Clone, Copy)]
    struct RowInfo {
        measure_idx: usize,
        row_in_measure: usize,
        is_mine: bool,
    }

    let mut rows: Vec<RowInfo> = Vec::new();
    let mut rows_per_measure: Vec<usize> = Vec::new();
    let mut current_rows: usize = 0;
    let mut measure_idx: usize = 0;
    let mut row_in_measure: usize = 0;

    let lanes = lanes.max(1);

    for line in minimized_note_data.split(|&b| b == b'\n') {
        if line.is_empty() {
            continue;
        }
        if line[0] == b',' {
            rows_per_measure.push(current_rows);
            measure_idx += 1;
            current_rows = 0;
            row_in_measure = 0;
            continue;
        }
        if line.len() < lanes {
            continue;
        }
        let is_mine = line[..lanes]
            .iter()
            .any(|&b| b == b'M' || b == b'm');

        rows.push(RowInfo {
            measure_idx,
            row_in_measure,
            is_mine,
        });
        current_rows += 1;
        row_in_measure += 1;
    }
    rows_per_measure.push(current_rows);

    if rows.is_empty() {
        return 0;
    }

    let mut beats: Vec<f64> = Vec::with_capacity(rows.len());
    for info in &rows {
        let total_rows = rows_per_measure
            .get(info.measure_idx)
            .copied()
            .unwrap_or(0)
            .max(1) as f64;
        let row_index = info.row_in_measure as f64;
        let beats_into_measure = 4.0 * (row_index / total_rows);
        let beat = (info.measure_idx as f64) * 4.0 + beats_into_measure;
        beats.push(beat);
    }

    #[inline]
    fn is_active_at_beat(beat: f64, segments: &[(f64, f64)]) -> bool {
        if segments.is_empty() {
            return false;
        }
        let idx = segments.partition_point(|(seg_beat, _)| *seg_beat <= beat);
        if idx == 0 {
            return false;
        }
        let (start, len) = segments[idx - 1];
        if !len.is_finite() || len <= 0.0 {
            return false;
        }
        beat >= start && beat < start + len
    }

    let mut count: u32 = 0;
    for (info, beat) in rows.iter().zip(beats.iter()) {
        if !info.is_mine {
            continue;
        }
        let b = *beat;
        if !is_active_at_beat(b, warp_map) && !is_active_at_beat(b, fake_map) {
            count = count.saturating_add(1);
        }
    }

    count
}

pub fn compute_measure_nps_vec(measure_densities: &[usize], bpm_map: &[(f64, f64)]) -> Vec<f64> {
    let mut out = Vec::with_capacity(measure_densities.len());
    if measure_densities.is_empty() {
        return out;
    }

    let mut bpm_idx = 0usize;
    let mut curr_bpm = bpm_map.get(0).map(|&(_, bpm)| bpm).unwrap_or(0.0);
    let mut measure_start_beat = 0.0_f64;

    for &density in measure_densities {
        while bpm_idx + 1 < bpm_map.len() && bpm_map[bpm_idx + 1].0 <= measure_start_beat {
            bpm_idx += 1;
            curr_bpm = bpm_map[bpm_idx].1;
        }

        // For NPS calculation, if the BPM is a gimmick (too high),
        // it implies the measure passes instantly (warp), so effective NPS
        // for a human reading it is treated as 0/unplayable, matching Simply Love.
        let nps = if density == 0 || !is_display_bpm(curr_bpm) {
            0.0
        } else {
            // NPS = density / (4 * 60 / BPM) = density * BPM / 240
            density as f64 * curr_bpm / 240.0
        };
        out.push(nps);
        measure_start_beat += 4.0;
    }
    out
}

/// Computes NPS per measure using TimingData (matches Simply Love timing semantics).
pub fn compute_measure_nps_vec_with_timing(
    measure_densities: &[usize],
    timing: &TimingData,
) -> Vec<f64> {
    let mut out = Vec::with_capacity(measure_densities.len());
    if measure_densities.is_empty() {
        return out;
    }
    let mut start_beat = 0.0_f64;
    let mut end_beat = 4.0_f64;

    for &density in measure_densities {
        if density == 0 {
            out.push(0.0);
            start_beat = end_beat;
            end_beat += 4.0;
            continue;
        }

        let start_time = timing.get_time_for_beat_f32(start_beat);
        let end_time = timing.get_time_for_beat_f32(end_beat);
        let duration = end_time - start_time;

        if duration <= 0.12_f64 {
            out.push(0.0);
        } else {
            out.push(density as f64 / duration);
        }

        start_beat = end_beat;
        end_beat += 4.0;
    }
    out
}

/// Computes median of a pre-sorted slice of f64.
fn median_of_sorted(sorted: &[f64]) -> f64 {
    let len = sorted.len();
    if len == 0 {
        return 0.0;
    }
    if len % 2 == 0 {
        (sorted[len / 2 - 1] + sorted[len / 2]) / 2.0
    } else {
        sorted[len / 2]
    }
}

/// A small helper to compute median of a slice of f64.
fn median(arr: &[f64]) -> f64 {
    if arr.is_empty() {
        return 0.0;
    }
    if arr.iter().all(|v| v.is_finite()) {
        let mut data = arr.to_vec();
        let len = data.len();
        let mid = len / 2;
        let mid_val = {
            let (_, mid_val, _) = data.select_nth_unstable_by(mid, |a, b| {
                a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal)
            });
            *mid_val
        };
        if len % 2 == 1 {
            mid_val
        } else {
            let lower_max = data[..mid]
                .iter()
                .copied()
                .fold(f64::MIN, f64::max);
            (lower_max + mid_val) / 2.0
        }
    } else {
        let mut sorted = arr.to_vec();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        median_of_sorted(&sorted)
    }
}

pub fn get_nps_stats(measure_nps_vec: &[f64]) -> (f64, f64) {
    let max_nps = measure_nps_vec
        .iter()
        .fold(f64::MIN, |a, &b| a.max(b))
        .max(0.0);
    let median_nps = median(measure_nps_vec);
    (max_nps, median_nps)
}

/// Computes median and average BPM, filtering out gimmick values unless unavoidable.
pub fn compute_bpm_stats(bpm_values: &[f64]) -> (f64, f64) {
    if bpm_values.is_empty() {
        return (0.0, 0.0);
    }

    // Filter out gimmick BPMs for stats
    let mut sorted: Vec<f64> = Vec::with_capacity(bpm_values.len());
    for &bpm in bpm_values {
        if is_display_bpm(bpm) {
            sorted.push(bpm);
        }
    }

    // Fallback if everything was filtered
    if sorted.is_empty() {
        sorted.extend_from_slice(bpm_values);
    }

    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let median = median_of_sorted(&sorted);
    let average = sorted.iter().sum::<f64>() / sorted.len() as f64;
    (median, average)
}

pub fn compute_tier_bpm(
    measure_densities: &[usize],
    bpm_map: &[(f64, f64)],
    beats_per_measure: f64,
) -> f64 {
    use crate::stats::categorize_measure_density;
    use crate::stats::RunDensity;

    // Filter max BPM search
    let max_bpm = bpm_map
        .iter()
        .map(|&(_, bpm)| bpm)
        .filter(|&bpm| is_display_bpm(bpm))
        .fold(f64::NEG_INFINITY, f64::max);
    
    // If we filtered everything out (e.g. all gimmicks), just fallback to 0 or whatever is there
    let max_bpm = if max_bpm.is_finite() { max_bpm } else { 
        bpm_map.iter().map(|&(_, bpm)| bpm).fold(f64::NEG_INFINITY, f64::max)
    };

    let mut max_e = 0.0f64;
    let mut run_cat = RunDensity::Break;
    let mut run_len = 0usize;
    let mut run_max_e = 0.0f64;

    for_each_measure_bpm(measure_densities.len(), bpm_map, beats_per_measure, |idx, bpm| {
        let density = measure_densities[idx];
        let cat = categorize_measure_density(density);
        if cat == RunDensity::Break {
            if run_len >= 4 {
                max_e = max_e.max(run_max_e);
            }
            run_cat = RunDensity::Break;
            run_len = 0;
            run_max_e = 0.0;
        } else {
            if run_len == 0 || cat != run_cat {
                if run_len >= 4 {
                    max_e = max_e.max(run_max_e);
                }
                run_cat = cat;
                run_len = 0;
                run_max_e = 0.0;
            }
            run_len += 1;
            if is_display_bpm(bpm) {
                let e_k = (density as f64 * bpm) / 16.0;
                if e_k > run_max_e {
                    run_max_e = e_k;
                }
            }
        }
    });

    if run_len >= 4 {
        max_e = max_e.max(run_max_e);
    }

    if max_e > 0.0 {
        max_e
    } else {
        max_bpm
    }
}

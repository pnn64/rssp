use std::borrow::Cow;

use crate::math::{fmt_dec3_half_up, round_sig_figs_itg, roundtrip_bpm_itg};
use crate::parse::{
    ParsedChartEntry, ParsedSimfileData, decode_bytes, extract_sections, parse_version,
    unescape_trim,
};
use crate::timing::{
    ROWS_PER_BEAT, TimingFormat, compute_timing_segments, format_bpm_segments_like_itg,
    steps_timing_allowed, timing_format_from_ext,
};

const GIMMICK_BPM_THRESHOLD: f64 = 10000.0;

#[inline]
pub(crate) fn is_display_bpm(bpm: f64) -> bool {
    bpm > 0.0 && bpm < GIMMICK_BPM_THRESHOLD
}

pub use crate::nps::{compute_measure_nps_vec, compute_measure_nps_vec_with_timing, get_nps_stats};

#[inline]
fn has_control(s: &str) -> bool {
    s.bytes()
        .any(|b| b < 0x20 && !matches!(b, b'\t' | b'\n' | b'\r'))
}

fn strip_control(s: &str) -> Cow<'_, str> {
    if has_control(s) {
        Cow::Owned(s.chars().filter(|c| !c.is_control()).collect())
    } else {
        Cow::Borrowed(s)
    }
}

fn normalize_decimal(s: &str) -> Option<String> {
    let value: f64 = strip_control(s).trim().parse().ok()?;
    Some(fmt_dec3_half_up(value))
}

pub(crate) fn parse_beat_or_row(raw: &str) -> Option<f64> {
    let mut s = raw.trim();
    let is_row = s
        .strip_suffix(['r', 'R'])
        .map(|r| {
            s = r.trim_end();
            true
        })
        .unwrap_or(false);
    let v = s.parse::<f32>().ok().filter(|v| v.is_finite())?;
    Some(if is_row {
        v as f64 / ROWS_PER_BEAT as f64
    } else {
        v as f64
    })
}

pub fn normalize_float_digits(param: &str) -> String {
    let mut out = String::with_capacity(param.len());
    for entry in param.split(',') {
        let t = entry.trim();
        if t.is_empty() {
            continue;
        }
        if let Some((b, v)) = t.split_once('=') {
            if let (Some(b), Some(v)) = (normalize_decimal(b), normalize_decimal(v)) {
                if !out.is_empty() {
                    out.push(',');
                }
                out.push_str(&b);
                out.push('=');
                out.push_str(&v);
            }
        }
    }
    out
}

pub fn clean_timing_map(param: &str) -> String {
    let mut out = String::with_capacity(param.len());
    for entry in param.split(',') {
        if entry.is_empty() {
            continue;
        }
        let t = strip_control(entry);
        let t = t.trim();
        if t.is_empty() {
            continue;
        }
        if !out.is_empty() {
            out.push(',');
        }
        out.push_str(t);
    }
    out
}

pub fn clean_timing_map_cow(param: &str) -> Cow<'_, str> {
    if param.is_empty() {
        return Cow::Borrowed("");
    }
    let dirty = param
        .split(',')
        .any(|e| e.is_empty() || e.trim() != e || has_control(e));
    if dirty {
        Cow::Owned(clean_timing_map(param))
    } else {
        Cow::Borrowed(param)
    }
}

// Unified tag processing - replaces 6 functions
#[inline]
fn map_tag<F: FnOnce(&str) -> String>(tag: Option<&[u8]>, f: F) -> String {
    tag.and_then(|b| std::str::from_utf8(b).ok())
        .map(f)
        .unwrap_or_default()
}

#[inline]
fn map_tag_opt<F: FnOnce(&str) -> String>(tag: Option<&[u8]>, f: F) -> Option<String> {
    tag.and_then(|b| std::str::from_utf8(b).ok())
        .map(f)
        .filter(|s| !s.is_empty())
}

pub fn normalize_chart_tag(tag: Option<Vec<u8>>) -> Option<String> {
    map_tag_opt(tag.as_deref(), normalize_float_digits)
}

fn decode_display_bpm_tag(tag: Option<&[u8]>) -> Option<String> {
    tag.map(decode_bytes)
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn split_display_bpm_params(tag: &str) -> (&str, Option<&str>) {
    let mut depth = 0usize;
    for (i, c) in tag.char_indices() {
        match c {
            '\\' => {
                depth = 1;
                continue;
            }
            ':' if depth == 0 => return (tag[..i].trim(), Some(tag[i + 1..].trim())),
            _ => {
                depth = 0;
            }
        }
    }
    (tag.trim(), None)
}

fn parse_float_prefix(s: &str) -> Option<f64> {
    let b = s.trim_start().as_bytes();
    let mut i = if b.first().map_or(false, |&c| c == b'+' || c == b'-') {
        1
    } else {
        0
    };
    let start = i;
    while i < b.len() && b[i].is_ascii_digit() {
        i += 1;
    }
    if i < b.len() && b[i] == b'.' {
        i += 1;
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
    }
    if i == start || (i == start + 1 && !b[start].is_ascii_digit()) {
        return None;
    }
    if i < b.len() && matches!(b[i], b'e' | b'E') {
        let e = i;
        i += 1;
        if i < b.len() && matches!(b[i], b'+' | b'-') {
            i += 1;
        }
        let ed = i;
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
        if ed == i {
            i = e;
        }
    }
    std::str::from_utf8(&b[..i])
        .ok()?
        .parse()
        .ok()
        .map(|v: f64| if v.is_finite() { v } else { 0.0 })
}

fn parse_display_bpm(tag: &str) -> Option<(f64, f64)> {
    let t = tag.trim();
    if t.is_empty() || t == "*" {
        return None;
    }
    let (min_s, max_s) = split_display_bpm_params(t);
    if min_s.is_empty() {
        return None;
    }
    let min = (parse_float_prefix(min_s).unwrap_or(0.0) as f32) as f64;
    let max = max_s
        .filter(|s| !s.is_empty())
        .and_then(parse_float_prefix)
        .map(|v| (v as f32) as f64)
        .unwrap_or(min);
    Some((min, max))
}

pub(crate) fn resolve_display_bpm(
    chart_tag: Option<&str>,
    actual_min: f64,
    actual_max: f64,
    rate: f64,
) -> (f64, f64, String) {
    let (mut min, mut max) = chart_tag
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .and_then(parse_display_bpm)
        .unwrap_or((actual_min, actual_max));
    if min <= 0.0 || max <= 0.0 {
        min = actual_min;
        max = actual_max;
    }
    let (smin, smax) = (min * rate, max * rate);
    let fmt = |v: f64| {
        if rate == 1.0 {
            format!("{:.0}", v)
        } else {
            let s = format!("{:.1}", v);
            s.strip_suffix(".0").map_or(s.clone(), |t| t.to_string())
        }
    };
    let display = if smin == smax {
        fmt(smin)
    } else {
        format!("{} - {}", fmt(smin), fmt(smax))
    };
    (smin, smax, display)
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

// Unified timing tags - single struct with optional chart overlay
#[derive(Clone, Default)]
struct TimingTags {
    bpms: String,
    stops: String,
    delays: String,
    warps: String,
    speeds: String,
    scrolls: String,
    fakes: String,
}

fn timing_tags_from_global(p: &ParsedSimfileData<'_>) -> TimingTags {
    TimingTags {
        bpms: map_tag(p.bpms, clean_timing_map),
        stops: map_tag(p.stops, clean_timing_map),
        delays: map_tag(p.delays, clean_timing_map),
        warps: map_tag(p.warps, clean_timing_map),
        speeds: map_tag(p.speeds, clean_timing_map),
        scrolls: map_tag(p.scrolls, clean_timing_map),
        fakes: map_tag(p.fakes, clean_timing_map),
    }
}

#[derive(Clone, Default)]
struct ChartTags {
    bpms: Option<String>,
    stops: Option<String>,
    delays: Option<String>,
    warps: Option<String>,
    speeds: Option<String>,
    scrolls: Option<String>,
    fakes: Option<String>,
}

fn chart_tags_from_entry(e: &ParsedChartEntry<'_>) -> ChartTags {
    ChartTags {
        bpms: map_tag_opt(e.chart_bpms.as_deref(), clean_timing_map),
        stops: map_tag_opt(e.chart_stops.as_deref(), clean_timing_map),
        delays: map_tag_opt(e.chart_delays.as_deref(), clean_timing_map),
        warps: map_tag_opt(e.chart_warps.as_deref(), clean_timing_map),
        speeds: map_tag_opt(e.chart_speeds.as_deref(), clean_timing_map),
        scrolls: map_tag_opt(e.chart_scrolls.as_deref(), clean_timing_map),
        fakes: map_tag_opt(e.chart_fakes.as_deref(), clean_timing_map),
    }
}

fn resolve_chart_tags<'a>(
    chart: &'a ChartTags,
    global: &'a TimingTags,
    use_chart: bool,
) -> [(&'a str, Option<&'a str>); 7] {
    macro_rules! pair {
        ($f:ident) => {
            (
                &global.$f,
                if use_chart { chart.$f.as_deref() } else { None },
            )
        };
    }
    [
        pair!(bpms),
        pair!(stops),
        pair!(delays),
        pair!(warps),
        pair!(speeds),
        pair!(scrolls),
        pair!(fakes),
    ]
}

fn chart_metadata(fields: &[&[u8]], fmt: TimingFormat) -> Option<(String, String)> {
    if fields.len() < 4 {
        return None;
    }
    let _lanes = crate::analysis::supported_stepstype_lanes_bytes(fields[0])?;
    let step_type = unescape_trim(decode_bytes(fields[0]).as_ref());
    let desc = unescape_trim(decode_bytes(fields[1]).as_ref());
    let diff_raw = unescape_trim(decode_bytes(fields[2]).as_ref());
    let meter = unescape_trim(decode_bytes(fields[3]).as_ref());
    let ext = if fmt == TimingFormat::Sm { "sm" } else { "ssc" };
    Some((
        step_type,
        crate::resolve_difficulty_label(&diff_raw, &desc, &meter, ext),
    ))
}

fn chart_bpm_snapshot(
    entry: &ParsedChartEntry<'_>,
    global: &TimingTags,
    bpms_norm: &str,
    fmt: TimingFormat,
    use_chart: bool,
) -> Option<ChartBpmSnapshot> {
    if entry.field_count < 4 {
        return None;
    }
    let (step_type, difficulty) = chart_metadata(&entry.fields, fmt)?;
    let chart = chart_tags_from_entry(entry);
    let hash_bpms = chart
        .bpms
        .as_ref()
        .map(|s| normalize_float_digits(s))
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| bpms_norm.to_string());

    let r = resolve_chart_tags(&chart, global, use_chart);
    let segments = compute_timing_segments(
        r[0].1, r[0].0, r[1].1, r[1].0, r[2].1, r[2].0, r[3].1, r[3].0, r[4].1, r[4].0, r[5].1,
        r[5].0, r[6].1, r[6].0, fmt, true,
    );

    let bpms: Vec<_> = segments
        .bpms
        .iter()
        .map(|&(b, v)| (b as f64, v as f64))
        .collect();
    let bpms_formatted = format_bpm_segments_like_itg(&bpms);
    let (bpm_min_raw, bpm_max_raw) = actual_bpm_range_raw(&bpms);
    let chart_dbpm = decode_display_bpm_tag(entry.chart_display_bpm.as_deref());
    let (display_bpm_min_raw, display_bpm_max_raw, display_bpm) =
        resolve_display_bpm(chart_dbpm.as_deref(), bpm_min_raw, bpm_max_raw, 1.0);

    Some(ChartBpmSnapshot {
        step_type,
        difficulty,
        hash_bpms,
        bpms_formatted,
        bpm_min: round_sig_figs_itg(bpm_min_raw),
        bpm_max: round_sig_figs_itg(bpm_max_raw),
        display_bpm,
        display_bpm_min: round_sig_figs_itg(display_bpm_min_raw),
        display_bpm_max: round_sig_figs_itg(display_bpm_max_raw),
    })
}

pub fn chart_bpm_snapshots(data: &[u8], ext: &str) -> Result<Vec<ChartBpmSnapshot>, String> {
    let parsed = extract_sections(data, ext).map_err(|e| e.to_string())?;
    let fmt = timing_format_from_ext(ext);
    let use_chart = steps_timing_allowed(parse_version(parsed.version, fmt), fmt);
    let global = timing_tags_from_global(&parsed);
    let bpms_norm = map_tag(parsed.bpms, normalize_float_digits);
    Ok(parsed
        .notes_list
        .iter()
        .filter_map(|e| chart_bpm_snapshot(e, &global, &bpms_norm, fmt, use_chart))
        .collect())
}

// BPM parsing - consolidated
pub fn parse_bpm_map(s: &str) -> Vec<(f64, f64)> {
    let mut v: Vec<_> = s
        .split(',')
        .filter_map(|c| {
            let c = c.trim();
            let (l, r) = c.split_once('=')?;
            let beat = parse_beat_or_row(l)?;
            let bpm = r.trim().parse::<f64>().ok().map(|v| v as f32 as f64)?;
            Some((beat, bpm))
        })
        .collect();
    v.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    v
}

pub fn get_current_bpm(beat: f64, map: &[(f64, f64)]) -> f64 {
    if map.is_empty() {
        return 0.0;
    }
    let pos = map.partition_point(|&(b, _)| b <= beat);
    map[pos
        .saturating_sub(1)
        .max(if pos == 0 { 0 } else { pos - 1 })]
    .1
}

#[inline(always)]
pub(crate) fn for_each_measure_bpm<F: FnMut(usize, f64)>(
    count: usize,
    map: &[(f64, f64)],
    bpm: f64,
    mut f: F,
) {
    if count == 0 || map.is_empty() {
        return;
    }
    let (mut idx, mut cur, mut next) = (0, map[0].1, map.get(1).map_or(f64::INFINITY, |m| m.0));
    for i in 0..count {
        let beat = i as f64 * bpm;
        while beat >= next {
            idx += 1;
            cur = map[idx].1;
            next = map.get(idx + 1).map_or(f64::INFINITY, |m| m.0);
        }
        f(i, cur);
    }
}

pub fn compute_bpm_range(map: &[(f64, f64)]) -> (i32, i32) {
    bpm_range_filtered(map, is_display_bpm, |v| v.round() as i32)
}

pub fn compute_actual_bpm_range(map: &[(f64, f64)]) -> (f64, f64) {
    let (min, max) = actual_bpm_range_raw(map);
    (round_sig_figs_itg(min), round_sig_figs_itg(max))
}

pub(crate) fn actual_bpm_range_raw(map: &[(f64, f64)]) -> (f64, f64) {
    bpm_range_filtered(map, |b| b.is_finite(), |v| roundtrip_bpm_itg(v))
}

fn bpm_range_filtered<T: PartialOrd + Default + Copy, F: Fn(f64) -> bool, M: Fn(f64) -> T>(
    map: &[(f64, f64)],
    filter: F,
    transform: M,
) -> (T, T) {
    if map.is_empty() {
        return (T::default(), T::default());
    }
    let (mut min, mut max, mut count) = (f64::MAX, f64::MIN, 0);
    for &(_, bpm) in map {
        if filter(bpm) {
            min = min.min(bpm);
            max = max.max(bpm);
            count += 1;
        }
    }
    if count == 0 {
        for &(_, bpm) in map {
            min = min.min(bpm);
            max = max.max(bpm);
        }
    }
    (transform(min.max(0.0)), transform(max.max(0.0)))
}

pub fn get_elapsed_time(
    target: f64,
    bpms: &[(f64, f64)],
    stops: &[(f64, f64)],
    delays: &[(f64, f64)],
    warps: &[(f64, f64)],
) -> f64 {
    if stops.is_empty() && delays.is_empty() && warps.is_empty() {
        return elapsed_bpm_only(target, bpms);
    }
    elapsed_with_events(target, bpms, stops, delays, warps)
}

fn elapsed_bpm_only(target: f64, bpms: &[(f64, f64)]) -> f64 {
    if bpms.is_empty() {
        return 0.0;
    }
    let (mut time, mut beat, mut bpm, mut i) = (0.0, 0.0, 60.0, 0);
    while i < bpms.len() && bpms[i].0 <= 0.0 {
        bpm = bpms[i].1;
        i += 1;
    }
    while i < bpms.len() && bpms[i].0 <= target {
        if bpms[i].0 > beat && bpm > 0.0 {
            time += (bpms[i].0 - beat) * 60.0 / bpm;
        }
        beat = bpms[i].0;
        bpm = bpms[i].1;
        i += 1;
    }
    if target > beat && bpm > 0.0 {
        time += (target - beat) * 60.0 / bpm;
    }
    time
}

fn elapsed_with_events(
    target: f64,
    bpms: &[(f64, f64)],
    stops: &[(f64, f64)],
    delays: &[(f64, f64)],
    warps: &[(f64, f64)],
) -> f64 {
    let mut events: Vec<_> = bpms
        .iter()
        .map(|&(b, v)| (b, 0u8, v))
        .chain(stops.iter().chain(delays).map(|&(b, v)| (b, 1, v)))
        .chain(warps.iter().map(|&(b, v)| (b, 2, v)))
        .collect();
    events.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap().then(a.1.cmp(&b.1)));

    let (mut time, mut beat, mut bpm, mut warp_end) = (
        0.0,
        0.0,
        if !bpms.is_empty() && bpms[0].0 <= 0.0 {
            bpms[0].1
        } else {
            60.0
        },
        0.0,
    );

    for (eb, pri, val) in events {
        if eb > target && warp_end <= target {
            break;
        }
        if eb > beat {
            let eff = beat.max(warp_end);
            if eb > eff && bpm > 0.0 {
                time += (eb - eff) * 60.0 / bpm;
            }
            beat = eb;
        }
        match pri {
            0 => bpm = val,
            1 => time += val,
            2 => warp_end = warp_end.max(eb + val),
            _ => {}
        }
    }
    let eff = beat.max(warp_end);
    if target > eff && bpm > 0.0 {
        time += (target - eff) * 60.0 / bpm;
    }
    time
}

pub fn compute_last_beat(data: &[u8], lanes: usize) -> f64 {
    let (_, _, _, _, last_beat) = crate::stats::minimize_chart_count_rows(data, lanes);
    last_beat
}

pub fn compute_total_chart_length(
    data: &[u8],
    lanes: usize,
    bpms: &[(f64, f64)],
    stops: &[(f64, f64)],
    delays: &[(f64, f64)],
    warps: &[(f64, f64)],
) -> i32 {
    let beat = compute_last_beat(data, lanes);
    if beat <= 0.0 || bpms.is_empty() {
        0
    } else {
        get_elapsed_time(beat, bpms, stops, delays, warps).floor() as i32
    }
}

pub fn compute_mines_nonfake(
    data: &[u8],
    lanes: usize,
    warps: &[(f64, f64)],
    fakes: &[(f64, f64)],
) -> u32 {
    let lanes = if lanes == 8 { 8 } else { 4 };
    let minimized = crate::stats::minimize_chart_for_hash(data, lanes);
    let mut rows = Vec::new();
    let mut per_measure = Vec::new();
    let (mut m, mut r, mut cnt) = (0usize, 0usize, 0usize);

    for line in minimized.split(|&b| b == b'\n') {
        if line.is_empty() {
            continue;
        }
        if line[0] == b',' {
            per_measure.push(cnt);
            m += 1;
            r = 0;
            cnt = 0;
            continue;
        }
        if line.len() < lanes {
            continue;
        }
        let mine = line[..lanes].iter().any(|&b| matches!(b, b'M' | b'm'));
        rows.push((m, r, mine));
        cnt += 1;
        r += 1;
    }
    per_measure.push(cnt);

    let in_range = |b: f64, segs: &[(f64, f64)]| -> bool {
        let i = segs.partition_point(|(s, _)| *s <= b);
        i > 0 && {
            let (s, l) = segs[i - 1];
            l > 0.0 && l.is_finite() && b < s + l
        }
    };

    rows.iter()
        .filter(|&&(m, r, mine)| {
            if !mine {
                return false;
            }
            let t = per_measure.get(m).copied().unwrap_or(1).max(1) as f64;
            let beat = m as f64 * 4.0 + 4.0 * (r as f64 / t);
            !in_range(beat, warps) && !in_range(beat, fakes)
        })
        .count() as u32
}

pub fn compute_bpm_stats(values: &[f64]) -> (f64, f64) {
    if values.is_empty() {
        return (0.0, 0.0);
    }
    let mut v: Vec<_> = values
        .iter()
        .copied()
        .filter(|&b| is_display_bpm(b))
        .collect();
    if v.is_empty() {
        v.extend_from_slice(values);
    }
    v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let med = if v.len() % 2 == 0 {
        (v[v.len() / 2 - 1] + v[v.len() / 2]) / 2.0
    } else {
        v[v.len() / 2]
    };
    (med, v.iter().sum::<f64>() / v.len() as f64)
}

pub fn compute_tier_bpm(densities: &[usize], bpms: &[(f64, f64)], bpm: f64) -> f64 {
    use crate::stats::{RunDensity, categorize_measure_density};

    let max_bpm = bpms
        .iter()
        .map(|&(_, b)| b)
        .filter(|&b| is_display_bpm(b))
        .fold(f64::NEG_INFINITY, f64::max);
    let max_bpm = if max_bpm.is_finite() {
        max_bpm
    } else {
        bpms.iter()
            .map(|&(_, b)| b)
            .fold(f64::NEG_INFINITY, f64::max)
    };

    let (mut max_e, mut cat, mut len, mut run_e) = (0.0f64, RunDensity::Break, 0usize, 0.0f64);
    for_each_measure_bpm(densities.len(), bpms, bpm, |i, b| {
        let c = categorize_measure_density(densities[i]);
        if c == RunDensity::Break {
            if len >= 4 {
                max_e = max_e.max(run_e);
            }
            cat = RunDensity::Break;
            len = 0;
            run_e = 0.0;
        } else {
            if len == 0 || c != cat {
                if len >= 4 {
                    max_e = max_e.max(run_e);
                }
                cat = c;
                len = 0;
                run_e = 0.0;
            }
            len += 1;
            if is_display_bpm(b) {
                run_e = run_e.max(densities[i] as f64 * b / 16.0);
            }
        }
    });
    if len >= 4 {
        max_e = max_e.max(run_e);
    }
    if max_e > 0.0 { max_e } else { max_bpm }
}

pub fn normalize_and_tidy_bpms(param: &str) -> String {
    let mut entries: Vec<_> = param
        .split(',')
        .enumerate()
        .filter_map(|(i, e)| {
            let (b, v) = e.trim().split_once('=')?;
            let (bs, vs) = (normalize_decimal(b)?, normalize_decimal(v)?);
            Some((bs.parse::<i64>().ok()?, bs, vs.parse::<i64>().ok()?, vs, i))
        })
        .collect();

    if entries.is_empty() {
        return "0.000=60.000".to_string();
    }
    entries.sort_by_key(|e| (e.0, e.4));

    // Dedupe by beat, keeping last
    let mut deduped = Vec::with_capacity(entries.len());
    for e in entries {
        if deduped
            .last()
            .map_or(false, |l: &(i64, String, i64, String, usize)| l.0 == e.0)
        {
            *deduped.last_mut().unwrap() = e;
        } else {
            deduped.push(e);
        }
    }

    if let Some(f) = deduped.first_mut() {
        if f.0 != 0 {
            f.0 = 0;
            f.1 = "0.000".into();
        }
    }

    // Remove consecutive same values
    let mut out = String::new();
    let mut last_v = None;
    for (_, bs, vt, vs, _) in deduped {
        if last_v == Some(vt) {
            continue;
        }
        last_v = Some(vt);
        if !out.is_empty() {
            out.push(',');
        }
        out.push_str(&bs);
        out.push('=');
        out.push_str(&vs);
    }
    out
}

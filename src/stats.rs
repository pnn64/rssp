use crate::timing::{TimingData, beat_to_note_row, note_row_to_beat};

pub use crate::nps::measure_equally_spaced;
pub use crate::streams::{
    BreakdownMode, RunDensity, StreamBreakdownLevel, StreamCounts, StreamSegment, Token,
    categorize_measure_density, compute_stream_counts, format_run_symbol, generate_breakdown,
    stream_breakdown, stream_sequences,
};

// ============================================================================
// Data Structures
// ============================================================================

#[derive(Debug, Default, Clone, PartialEq)]
pub struct ArrowStats {
    pub total_arrows: u32,
    pub left: u32,
    pub down: u32,
    pub up: u32,
    pub right: u32,
    pub total_steps: u32,
    pub jumps: u32,
    pub hands: u32,
    pub mines: u32,
    pub holds: u32,
    pub rolls: u32,
    pub lifts: u32,
    pub fakes: u32,
    pub holding: i32,
}

pub const RADAR_CATEGORY_COUNT: usize = 14;

const HOLD_END_NONE: usize = usize::MAX;

// ============================================================================
// Lane Dispatch Macro
// ============================================================================

macro_rules! dispatch_lanes {
    ($lanes:expr, $func:ident($($arg:expr),*)) => {
        match $lanes {
            8 => $func::<8>($($arg),*),
            _ => $func::<4>($($arg),*),
        }
    };
}

// ============================================================================
// Core Utilities
// ============================================================================

#[inline(always)]
fn trim_cr(line: &[u8]) -> &[u8] {
    line.strip_suffix(&[b'\r']).unwrap_or(line)
}

#[inline(always)]
fn skip_ws(mut line: &[u8]) -> &[u8] {
    while let [b, rest @ ..] = line {
        if !b.is_ascii_whitespace() {
            break;
        }
        line = rest;
    }
    line
}

#[inline(always)]
fn is_hold_blocker(ch: u8) -> bool {
    matches!(ch, b'1' | b'M' | b'L' | b'F')
}

#[inline(always)]
fn is_note(ch: u8) -> bool {
    matches!(ch, b'1' | b'2' | b'4')
}

#[inline(always)]
fn is_all_zero<const L: usize>(line: &[u8; L]) -> bool {
    line.iter().all(|&b| b == b'0')
}

#[inline(always)]
fn bump_dir(stats: &mut ArrowStats, col: usize) {
    match col & 3 {
        0 => stats.left += 1,
        1 => stats.down += 1,
        2 => stats.up += 1,
        _ => stats.right += 1,
    }
}

#[inline(always)]
fn has_step<const L: usize>(line: &[u8]) -> bool {
    line.iter().take(L).any(|&b| is_note(b))
}

// ============================================================================
// Unified Hold Tracking
// ============================================================================

const HOLD_STACK_CAP: usize = 8;

fn track_holds_core<const L: usize>(
    rows: impl Iterator<Item = impl AsRef<[u8]>>,
    row_count: usize,
) -> Vec<[usize; L]> {
    let mut stacks: [[usize; HOLD_STACK_CAP]; L] = [[0; HOLD_STACK_CAP]; L];
    let mut depths: [usize; L] = [0; L];
    let mut ends = Vec::with_capacity(row_count);

    for (i, row) in rows.enumerate() {
        let r = row.as_ref();
        ends.push([HOLD_END_NONE; L]);
        for c in 0..L.min(r.len()) {
            match r[c] {
                ch if is_hold_blocker(ch) => depths[c] = 0,
                b'2' | b'4' => {
                    let d = depths[c];
                    if d < HOLD_STACK_CAP {
                        stacks[c][d] = i;
                        depths[c] = d + 1;
                    }
                }
                b'3' => {
                    if depths[c] > 0 {
                        depths[c] -= 1;
                        let start = stacks[c][depths[c]];
                        ends[start][c] = i;
                    }
                }
                _ => {}
            }
        }
    }
    ends
}

fn scan_hold_ends<const L: usize>(rows: &[[u8; L]]) -> Vec<[usize; L]> {
    track_holds_core::<L>(rows.iter().map(|r| r.as_slice()), rows.len())
}

fn match_hold_ends<const L: usize>(rows: &[[u8; L]]) -> Vec<[Option<usize>; L]> {
    scan_hold_ends(rows)
        .into_iter()
        .map(|r| r.map(|e| (e != HOLD_END_NONE).then_some(e)))
        .collect()
}

fn has_phantoms<const L: usize>(data: &[u8]) -> bool {
    let mut depths = [0u32; L];
    for raw in data.split(|&b| b == b'\n') {
        let line = trim_cr(raw);
        if line.len() < L || matches!(line.first(), Some(b',' | b';') | None) {
            continue;
        }
        for c in 0..L {
            match line[c] {
                ch if is_hold_blocker(ch) => {
                    if depths[c] != 0 {
                        return true;
                    }
                }
                b'2' | b'4' => depths[c] += 1,
                b'3' => depths[c] = depths[c].saturating_sub(1),
                _ => {}
            }
        }
    }
    depths.iter().any(|&d| d != 0)
}

// ============================================================================
// Row Parsing
// ============================================================================

pub(crate) fn parse_minimized_rows<const L: usize>(data: &[u8]) -> Vec<[u8; L]> {
    let mut rows = Vec::with_capacity(data.len() / (L + 1));
    for raw in data.split(|&b| b == b'\n') {
        let line = trim_cr(raw);
        if line.len() >= L && !matches!(line.first(), Some(b',' | b';') | None) {
            let mut arr = [0u8; L];
            arr.copy_from_slice(&line[..L]);
            rows.push(arr);
        }
    }
    rows
}

// ============================================================================
// Stats Counting
// ============================================================================

#[inline(always)]
fn count_line<const L: usize>(
    line: &[u8; L],
    stats: &mut ArrowStats,
    holds_started: &mut u32,
    ends_seen: &mut u32,
) -> bool {
    let (mut note_mask, mut hold_mask, mut end_mask) = (0u8, 0u8, 0u8);

    for (i, &ch) in line.iter().enumerate() {
        match ch {
            b'1' | b'2' | b'4' => {
                note_mask |= 1 << i;
                stats.total_arrows += 1;
                bump_dir(stats, i);
                match ch {
                    b'2' => {
                        hold_mask |= 1 << i;
                        stats.holds += 1;
                    }
                    b'4' => {
                        hold_mask |= 1 << i;
                        stats.rolls += 1;
                    }
                    _ => {}
                }
            }
            b'3' => end_mask |= 1 << i,
            b'M' => stats.mines += 1,
            b'L' => stats.lifts += 1,
            b'F' => stats.fakes += 1,
            _ => {}
        }
    }

    *holds_started += hold_mask.count_ones();
    *ends_seen += end_mask.count_ones();

    let notes = note_mask.count_ones();
    let active = stats.holding;

    if notes == 0 {
        stats.holding = (stats.holding - end_mask.count_ones() as i32).max(0);
        return false;
    }

    stats.total_steps += 1;
    if notes >= 2 {
        stats.jumps += 1;
    }
    if notes as i32 + active >= 3 {
        stats.hands += 1;
    }
    stats.holding =
        (stats.holding + hold_mask.count_ones() as i32 - end_mask.count_ones() as i32).max(0);
    true
}

fn recalc_without_phantoms<const L: usize>(
    rows: &[[u8; L]],
    ends: &[[Option<usize>; L]],
) -> ArrowStats {
    let fixed: Vec<_> = rows
        .iter()
        .enumerate()
        .map(|(i, row)| {
            let mut new = *row;
            for (c, b) in new.iter_mut().enumerate() {
                if ends[i][c].is_none() && matches!(*b, b'2' | b'4') {
                    *b = b'0';
                }
            }
            new
        })
        .collect();

    let (mut stats, mut h, mut e) = (ArrowStats::default(), 0, 0);
    for line in &fixed {
        count_line(line, &mut stats, &mut h, &mut e);
    }
    stats
}

// ============================================================================
// Measure Minimization
// ============================================================================

#[inline(always)]
pub fn minimize_measure<const L: usize>(m: &mut Vec<[u8; L]>) {
    while m.len() >= 2 && m.len() % 2 == 0 && m.iter().skip(1).step_by(2).all(is_all_zero) {
        let half = m.len() / 2;
        for i in 0..half {
            m[i] = m[i * 2];
        }
        m.truncate(half);
    }
}

#[inline(always)]
fn append_row_beats(beats: &mut Vec<f32>, midx: usize, rows: usize) {
    if rows == 0 {
        return;
    }
    beats.reserve(rows);
    let (start, step) = (midx as f32 * 4.0, 4.0 / rows as f32);
    for r in 0..rows {
        beats.push(start + r as f32 * step);
    }
}

#[inline(always)]
pub(crate) fn calc_last_beat(midx: Option<usize>, row: usize, rows: usize) -> f64 {
    let m = midx.unwrap_or(0);
    let beat = m as f64 * 4.0 + 4.0 * row as f64 / rows.max(1) as f64;
    note_row_to_beat(beat_to_note_row(beat))
}

// ============================================================================
// Chart Processing - Unified Implementation
// ============================================================================

fn finalize_measure<const L: usize, R, N, C>(
    m: &mut Vec<[u8; L]>,
    idx: usize,
    output: &mut Vec<u8>,
    densities: &mut Vec<usize>,
    on_rows: &mut R,
    on_line: &mut N,
    on_count: &mut C,
) where
    R: FnMut(usize, usize),
    N: FnMut(&[u8; L], usize, usize, usize),
    C: FnMut(&[u8; L]) -> bool,
{
    if m.is_empty() {
        densities.push(0);
        return;
    }
    minimize_measure(m);
    on_rows(idx, m.len());
    output.reserve(m.len() * (L + 1));
    let rows = m.len();
    let mut density = 0;
    for (i, line) in m.iter().enumerate() {
        on_line(line, idx, i, rows);
        if on_count(line) {
            density += 1;
        }
        output.extend_from_slice(line);
        output.push(b'\n');
    }
    m.clear();
    densities.push(density);
}

fn minimize_chart_core<const L: usize, R, N, C>(
    data: &[u8],
    on_rows: &mut R,
    on_line: &mut N,
    on_count: &mut C,
) -> (Vec<u8>, Vec<usize>)
where
    R: FnMut(usize, usize),
    N: FnMut(&[u8; L], usize, usize, usize),
    C: FnMut(&[u8; L]) -> bool,
{
    let mut output = Vec::with_capacity(data.len());
    let mut measure = Vec::with_capacity(64);
    let mut densities = Vec::new();
    let (mut midx, mut done) = (0usize, false);

    for raw in data.split(|&b| b == b'\n') {
        let line = skip_ws(raw);
        if line.is_empty() || line[0] == b'/' {
            continue;
        }

        match line[0] {
            b',' => {
                finalize_measure(
                    &mut measure,
                    midx,
                    &mut output,
                    &mut densities,
                    on_rows,
                    on_line,
                    on_count,
                );
                output.extend_from_slice(b",\n");
                midx += 1;
            }
            b';' => {
                finalize_measure(
                    &mut measure,
                    midx,
                    &mut output,
                    &mut densities,
                    on_rows,
                    on_line,
                    on_count,
                );
                done = true;
                break;
            }
            _ if line.len() >= L => {
                let mut arr = [0u8; L];
                arr.copy_from_slice(&line[..L]);
                measure.push(arr);
            }
            _ => {}
        }
    }

    if !done {
        finalize_measure(
            &mut measure,
            midx,
            &mut output,
            &mut densities,
            on_rows,
            on_line,
            on_count,
        );
    }

    (output, densities)
}

fn process_chart<const L: usize, R, N>(
    data: &[u8],
    on_rows: &mut R,
    on_line: &mut N,
) -> (Vec<u8>, ArrowStats, Vec<usize>)
where
    R: FnMut(usize, usize),
    N: FnMut(&[u8; L], usize, usize, usize),
{
    let mut stats = ArrowStats::default();
    let (mut holds, mut ends) = (0u32, 0u32);
    let mut count = |line: &[u8; L]| count_line(line, &mut stats, &mut holds, &mut ends);
    let (output, densities) = minimize_chart_core(data, on_rows, on_line, &mut count);

    // Fix phantom holds
    if holds > 0 && (holds != ends || has_phantoms::<L>(&output)) {
        let rows = parse_minimized_rows::<L>(&output);
        let hold_ends = match_hold_ends(&rows);
        let step_count = stats.total_steps;
        stats = recalc_without_phantoms(&rows, &hold_ends);
        stats.total_steps = step_count;
    }

    (output, stats, densities)
}

pub fn minimize_chart_and_count_with_lanes(
    data: &[u8],
    lanes: usize,
) -> (Vec<u8>, ArrowStats, Vec<usize>) {
    match lanes {
        8 => {
            let (mut nr, mut nl) = (|_, _| {}, |_: &[u8; 8], _, _, _| {});
            process_chart::<8, _, _>(data, &mut nr, &mut nl)
        }
        _ => {
            let (mut nr, mut nl) = (|_, _| {}, |_: &[u8; 4], _, _, _| {});
            process_chart::<4, _, _>(data, &mut nr, &mut nl)
        }
    }
}

pub(crate) fn minimize_chart_count_rows(
    data: &[u8],
    lanes: usize,
) -> (Vec<u8>, ArrowStats, Vec<usize>, Vec<f32>, f64) {
    dispatch_lanes!(lanes, minimize_rows_impl(data))
}

fn minimize_rows_impl<const L: usize>(
    data: &[u8],
) -> (Vec<u8>, ArrowStats, Vec<usize>, Vec<f32>, f64) {
    let mut beats = Vec::with_capacity(data.len() / (L + 1));
    let mut depths = [0u32; L];
    let (mut last_m, mut last_r, mut last_rows) = (None, 0, 0);

    let mut on_rows = |m, r| append_row_beats(&mut beats, m, r);
    let mut on_line = |line: &[u8; L], m, r, rows| {
        if line_has_object::<L>(line, &mut depths) {
            (last_m, last_r, last_rows) = (Some(m), r, rows);
        }
    };

    let (out, stats, dens) = process_chart::<L, _, _>(data, &mut on_rows, &mut on_line);
    let last = calc_last_beat(last_m, last_r, last_rows);
    (out, stats, dens, beats, last)
}

#[inline(always)]
pub(crate) fn line_has_object<const L: usize>(line: &[u8], depths: &mut [u32; L]) -> bool {
    let mut has = false;
    for c in 0..L {
        match line[c] {
            b'1' | b'M' | b'L' | b'F' | b'K' => {
                has = true;
                depths[c] = 0;
            }
            b'2' | b'4' => depths[c] = depths[c].saturating_add(1),
            b'3' if depths[c] > 0 => {
                depths[c] -= 1;
                has = true;
            }
            _ => {}
        }
    }
    has
}

pub(crate) fn minimize_chart_rows_bits(
    data: &[u8],
) -> (Vec<u8>, ArrowStats, Vec<usize>, Vec<f32>, f64, Vec<u8>) {
    let mut beats = Vec::with_capacity(data.len() / 5);
    let mut depths = [0u32; 4];
    let mut bits = Vec::with_capacity(beats.capacity());
    let (mut last_m, mut last_r, mut last_rows) = (None, 0, 0);

    let mut on_rows = |m, r| append_row_beats(&mut beats, m, r);
    let mut on_line = |line: &[u8; 4], m, r, rows| {
        let mask = (0..4).fold(0u8, |acc, i| {
            acc | if is_note(line[i]) { 1 << i } else { 0 }
        });
        bits.push(mask);
        if line_has_object::<4>(line, &mut depths) {
            (last_m, last_r, last_rows) = (Some(m), r, rows);
        }
    };

    let (out, stats, dens) = process_chart::<4, _, _>(data, &mut on_rows, &mut on_line);
    let last = calc_last_beat(last_m, last_r, last_rows);
    (out, stats, dens, beats, last, bits)
}

pub fn minimize_chart_for_hash(data: &[u8], lanes: usize) -> Vec<u8> {
    dispatch_lanes!(lanes, minimize_hash_impl(data))
}

fn minimize_hash_impl<const L: usize>(data: &[u8]) -> Vec<u8> {
    let mut on_rows = |_, _| {};
    let mut on_line = |_: &[u8; L], _, _, _| {};
    let mut on_count = |_: &[u8; L]| false;
    let (output, _) = minimize_chart_core(data, &mut on_rows, &mut on_line, &mut on_count);
    output
}

// ============================================================================
// Timing-Aware Stats
// ============================================================================

pub fn compute_timing_aware_stats(data: &[u8], lanes: usize, timing: &TimingData) -> ArrowStats {
    let (minimized, _stats, _densities, beats, _last_beat) =
        minimize_chart_count_rows(data, lanes);
    compute_timing_aware_stats_with_row_to_beat(&minimized, lanes, timing, &beats)
}

pub(crate) fn compute_timing_aware_stats_with_row_to_beat(
    data: &[u8],
    lanes: usize,
    timing: &TimingData,
    beats: &[f32],
) -> ArrowStats {
    match lanes {
        8 => {
            let rows = parse_minimized_rows::<8>(data);
            compute_timing_aware_stats_from_rows_with_row_to_beat::<8>(&rows, timing, beats)
        }
        _ => {
            let rows = parse_minimized_rows::<4>(data);
            compute_timing_aware_stats_from_rows_with_row_to_beat::<4>(&rows, timing, beats)
        }
    }
}

pub(crate) fn compute_timing_aware_stats_from_rows_with_row_to_beat<const L: usize>(
    rows: &[[u8; L]],
    timing: &TimingData,
    beats: &[f32],
) -> ArrowStats {
    if rows.is_empty() {
        return ArrowStats::default();
    }
    let ends = scan_hold_ends(rows);
    process_timing_rows::<L>(rows.iter(), &ends, timing, beats)
}

fn process_timing_rows<'a, const L: usize>(
    rows: impl Iterator<Item = &'a [u8; L]>,
    ends: &[[usize; L]],
    timing: &TimingData,
    beats: &[f32],
) -> ArrowStats {
    let mut stats = ArrowStats::default();
    let mut ends_per = vec![0u32; ends.len()];
    let mut active = 0i32;

    for (ridx, line) in rows.enumerate() {
        if ridx > 0 {
            active -= ends_per[ridx - 1] as i32;
        }
        process_timing_row::<L>(
            line,
            &ends[ridx],
            timing.is_judgable_at_beat(beats[ridx] as f64),
            &mut stats,
            &mut ends_per,
            &mut active,
        );
    }
    stats
}

#[inline(always)]
fn process_timing_row<const L: usize>(
    line: &[u8],
    hold_ends: &[usize; L],
    judgable: bool,
    stats: &mut ArrowStats,
    ends_per: &mut [u32],
    active: &mut i32,
) {
    if !judgable {
        for c in 0..L {
            match line[c] {
                b'1' | b'L' | b'l' | b'M' | b'm' | b'F' | b'f' => stats.fakes += 1,
                b'2' | b'4' if hold_ends[c] != HOLD_END_NONE => stats.fakes += 1,
                _ => {}
            }
        }
        return;
    }

    let (mut notes, mut new_h) = (0u32, 0u32);
    let mut has_note = false;

    for c in 0..L {
        match line[c] {
            b'1' => {
                has_note = true;
                notes += 1;
                stats.total_arrows += 1;
                bump_dir(stats, c);
            }
            b'2' | b'4' if hold_ends[c] != HOLD_END_NONE => {
                has_note = true;
                notes += 1;
                new_h += 1;
                stats.total_arrows += 1;
                ends_per[hold_ends[c]] += 1;
                if line[c] == b'2' {
                    stats.holds += 1;
                } else {
                    stats.rolls += 1;
                }
                bump_dir(stats, c);
            }
            b'L' | b'l' => {
                has_note = true;
                notes += 1;
                stats.total_arrows += 1;
                stats.lifts += 1;
                bump_dir(stats, c);
            }
            b'M' | b'm' => {
                has_note = true;
                stats.mines += 1;
            }
            b'F' | b'f' => {
                has_note = true;
                stats.fakes += 1;
            }
            _ => {}
        }
    }

    if notes > 0 {
        stats.total_steps += 1;
        if notes >= 2 {
            stats.jumps += 1;
        }
    }

    if has_note && (notes as i32 + *active >= 3) {
        stats.hands += 1;
    }

    if new_h > 0 {
        *active += new_h as i32;
    }
}

// ============================================================================
// Measure Density Analysis
// ============================================================================

pub fn measure_densities(data: &[u8], lanes: usize) -> Vec<usize> {
    dispatch_lanes!(lanes, densities_impl(data))
}

fn densities_impl<const L: usize>(data: &[u8]) -> Vec<usize> {
    let mut on_rows = |_, _| {};
    let mut on_line = |_: &[u8; L], _, _, _| {};
    let mut on_count = |line: &[u8; L]| has_step::<L>(line);
    let (_, densities) = minimize_chart_core(data, &mut on_rows, &mut on_line, &mut on_count);
    densities
}

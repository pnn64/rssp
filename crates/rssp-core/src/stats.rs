use crate::timing::{
    TimingData, beat_to_note_row, has_nonjudgable_rows, is_judgable_at_beat, note_row_to_beat,
};

pub use crate::nps::measure_equally_spaced;
pub use crate::streams::{
    BreakdownMode, RunDensity, StreamBreakdownLevel, StreamCounts, StreamSegment, Token,
    categorize_measure_density, compute_stream_counts, format_run_symbol, generate_breakdown,
    generate_breakdowns, stream_breakdown, stream_breakdowns, stream_sequences,
};

// ============================================================================
// Data Structures
// ============================================================================

#[derive(Debug, Default, Clone, PartialEq, Eq)]
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

#[derive(Clone, Copy)]
struct RowCount {
    density: bool,
    object: bool,
}

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
    line.strip_suffix(b"\r").unwrap_or(line)
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
const fn is_hold_blocker(ch: u8) -> bool {
    matches!(ch, b'1' | b'M' | b'L' | b'F')
}

#[inline(always)]
const fn is_note(ch: u8) -> bool {
    ch == b'1' || ch == b'2' || ch == b'4'
}

#[inline(always)]
fn is_all_zero<const L: usize>(line: &[u8; L]) -> bool {
    *line == [b'0'; L]
}

#[inline(always)]
const fn bump_dir(stats: &mut ArrowStats, col: usize) {
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

#[inline(always)]
fn byte_hits(word: u64, byte: u8) -> u64 {
    const LO: u64 = 0x0101_0101_0101_0101;
    const HI: u64 = 0x8080_8080_8080_8080;
    let x = word ^ (u64::from(byte) * LO);
    x.wrapping_sub(LO) & !x & HI
}

#[inline(always)]
fn find_byte(slice: &[u8], needle: u8) -> Option<usize> {
    let mut i = 0usize;
    let (chunks, rem) = slice.as_chunks::<8>();
    for chunk in chunks {
        let hits = byte_hits(u64::from_le_bytes(*chunk), needle);
        if hits != 0 {
            return Some(i + hits.trailing_zeros() as usize / 8);
        }
        i += 8;
    }
    for (j, &b) in rem.iter().enumerate() {
        if b == needle {
            return Some(i + j);
        }
    }
    None
}

#[inline(always)]
fn next_line<'a>(data: &'a [u8], offset: &mut usize) -> Option<&'a [u8]> {
    if *offset > data.len() {
        return None;
    }
    let start = *offset;
    let end = find_byte(&data[start..], b'\n').map_or(data.len(), |rel| start + rel);
    *offset = if end == data.len() {
        data.len() + 1
    } else {
        end + 1
    };
    Some(&data[start..end])
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
    track_holds_core::<L>(rows.iter().map(<[u8; L]>::as_slice), rows.len())
}

fn match_hold_ends<const L: usize>(rows: &[[u8; L]]) -> Vec<[Option<usize>; L]> {
    scan_hold_ends(rows)
        .into_iter()
        .map(|r| r.map(|e| (e != HOLD_END_NONE).then_some(e)))
        .collect()
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

#[inline(always)]
fn row_has_hold_head<const L: usize>(line: &[u8; L]) -> bool {
    line.iter().any(|&b| b == b'2' || b == b'4')
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
    phantom_depths: &mut [u32; L],
    has_phantom: &mut bool,
    object_depths: &mut [u32; L],
) -> RowCount {
    let (mut notes, mut new_holds, mut ends) = (0u32, 0i32, 0i32);
    let mut object = false;

    for (i, &ch) in line.iter().enumerate() {
        match ch {
            b'1' => {
                object = true;
                object_depths[i] = 0;
                *has_phantom |= phantom_depths[i] != 0;
                notes += 1;
                stats.total_arrows += 1;
                bump_dir(stats, i);
            }
            b'2' => {
                object_depths[i] = object_depths[i].saturating_add(1);
                phantom_depths[i] = phantom_depths[i].saturating_add(1);
                notes += 1;
                new_holds += 1;
                stats.total_arrows += 1;
                stats.holds += 1;
                bump_dir(stats, i);
            }
            b'4' => {
                object_depths[i] = object_depths[i].saturating_add(1);
                phantom_depths[i] = phantom_depths[i].saturating_add(1);
                notes += 1;
                new_holds += 1;
                stats.total_arrows += 1;
                stats.rolls += 1;
                bump_dir(stats, i);
            }
            b'3' => {
                if object_depths[i] > 0 {
                    object_depths[i] -= 1;
                    object = true;
                }
                phantom_depths[i] = phantom_depths[i].saturating_sub(1);
                ends += 1;
            }
            b'M' => {
                object = true;
                object_depths[i] = 0;
                *has_phantom |= phantom_depths[i] != 0;
                stats.mines += 1;
            }
            b'L' => {
                object = true;
                object_depths[i] = 0;
                *has_phantom |= phantom_depths[i] != 0;
                stats.lifts += 1;
            }
            b'F' => {
                object = true;
                object_depths[i] = 0;
                *has_phantom |= phantom_depths[i] != 0;
                stats.fakes += 1;
            }
            b'K' => {
                object = true;
                object_depths[i] = 0;
            }
            b'm' => stats.mines += 1,
            b'l' => stats.lifts += 1,
            b'f' => stats.fakes += 1,
            _ => {}
        }
    }

    *holds_started += new_holds as u32;
    *ends_seen += ends as u32;
    let active = stats.holding;

    if notes == 0 {
        stats.holding = (stats.holding - ends).max(0);
        return RowCount {
            density: false,
            object,
        };
    }

    stats.total_steps += 1;
    if notes >= 2 {
        stats.jumps += 1;
    }
    if notes as i32 + active >= 3 {
        stats.hands += 1;
    }
    stats.holding = (stats.holding + new_holds - ends).max(0);
    RowCount {
        density: true,
        object,
    }
}

fn recalc_without_phantoms<const L: usize>(
    rows: &[[u8; L]],
    ends: &[[Option<usize>; L]],
) -> ArrowStats {
    let mut stats = ArrowStats::default();
    for (i, line) in rows.iter().enumerate() {
        let phantom_mask = ends[i]
            .iter()
            .enumerate()
            .fold(0u8, |m, (c, e)| if e.is_none() { m | (1 << c) } else { m });
        count_line_masked(line, &mut stats, phantom_mask);
    }
    stats
}

#[inline(always)]
fn count_line_masked<const L: usize>(line: &[u8; L], stats: &mut ArrowStats, phantom_mask: u8) {
    let (mut note_mask, mut hold_mask, mut end_mask) = (0u8, 0u8, 0u8);

    for (i, &ch) in line.iter().enumerate() {
        let bit = 1u8 << i;
        let is_phantom = (phantom_mask & bit) != 0;
        match ch {
            b'1' => {
                note_mask |= bit;
                stats.total_arrows += 1;
                bump_dir(stats, i);
            }
            b'2' | b'4' if !is_phantom => {
                note_mask |= bit;
                hold_mask |= bit;
                stats.total_arrows += 1;
                bump_dir(stats, i);
                if ch == b'2' {
                    stats.holds += 1;
                } else {
                    stats.rolls += 1;
                }
            }
            b'3' => end_mask |= 1 << i,
            b'M' | b'm' => stats.mines += 1,
            b'L' | b'l' => stats.lifts += 1,
            b'F' | b'f' => stats.fakes += 1,
            _ => {}
        }
    }

    let notes = note_mask.count_ones();
    let active = stats.holding;

    if notes == 0 {
        stats.holding = (stats.holding - end_mask.count_ones() as i32).max(0);
        return;
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
}

// ============================================================================
// Measure Minimization
// ============================================================================

#[inline(always)]
pub fn minimize_measure<const L: usize>(m: &mut Vec<[u8; L]>) {
    let shift = measure_reduce_shift(m);
    if shift == 0 {
        return;
    }

    let step = 1usize << shift;
    let len = m.len() >> shift;
    for i in 1..len {
        m[i] = m[i * step];
    }
    m.truncate(len);
}

#[inline(always)]
fn measure_reduce_shift<const L: usize>(m: &[[u8; L]]) -> usize {
    if m.len() < 2 {
        return 0;
    }

    let mut shift = 0usize;
    let mut step = 2usize;
    for _ in 0..m.len().trailing_zeros() {
        let mut i = step / 2;
        while i < m.len() {
            if !is_all_zero(&m[i]) {
                return shift;
            }
            i += step;
        }
        shift += 1;
        step <<= 1;
    }
    shift
}

#[inline(always)]
fn append_row_beats(beats: &mut Vec<f32>, midx: usize, rows: usize) {
    if rows == 0 {
        return;
    }
    let (start, step) = (midx as f32 * 4.0, 4.0 / rows as f32);
    for r in 0..rows {
        beats.push((r as f32).mul_add(step, start));
    }
}

#[inline(always)]
pub(crate) fn calc_last_beat(midx: Option<usize>, row: usize, rows: usize) -> f64 {
    let m = midx.unwrap_or(0);
    let beat = (m as f64).mul_add(4.0, 4.0 * row as f64 / rows.max(1) as f64);
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
    N: FnMut(&[u8; L], usize, usize, usize, bool),
    C: FnMut(&[u8; L]) -> RowCount,
{
    if m.is_empty() {
        densities.push(0);
        return;
    }
    minimize_measure(m);
    on_rows(idx, m.len());
    let rows = m.len();
    let mut density = 0;
    for (i, line) in m.iter().enumerate() {
        let row_count = on_count(line);
        on_line(line, idx, i, rows, row_count.object);
        if row_count.density {
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
    N: FnMut(&[u8; L], usize, usize, usize, bool),
    C: FnMut(&[u8; L]) -> RowCount,
{
    let mut output = Vec::with_capacity(data.len());
    let mut measure = Vec::with_capacity(64);
    let mut densities = Vec::with_capacity(data.len() / ((L + 1) * 4) + 1);
    let (mut midx, mut done) = (0usize, false);

    let mut line_off = 0usize;
    while let Some(raw) = next_line(data, &mut line_off) {
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
    N: FnMut(&[u8; L], usize, usize, usize, bool),
{
    let mut stats = ArrowStats::default();
    let (mut holds, mut ends) = (0u32, 0u32);
    let mut phantom_depths = [0u32; L];
    let mut object_depths = [0u32; L];
    let mut has_phantom = false;
    let mut count = |line: &[u8; L]| {
        count_line(
            line,
            &mut stats,
            &mut holds,
            &mut ends,
            &mut phantom_depths,
            &mut has_phantom,
            &mut object_depths,
        )
    };
    let (output, densities) = minimize_chart_core(data, on_rows, on_line, &mut count);
    has_phantom |= phantom_depths.iter().any(|&d| d != 0);

    // Fix phantom holds
    if holds > 0 && (holds != ends || has_phantom) {
        let rows = parse_minimized_rows::<L>(&output);
        let hold_ends = match_hold_ends(&rows);
        let step_count = stats.total_steps;
        stats = recalc_without_phantoms(&rows, &hold_ends);
        stats.total_steps = step_count;
    }

    (output, stats, densities)
}

#[must_use]
pub fn minimize_chart_and_count_with_lanes(
    data: &[u8],
    lanes: usize,
) -> (Vec<u8>, ArrowStats, Vec<usize>) {
    if lanes == 8 {
        let (mut nr, mut nl) = (|_, _| {}, |_: &[u8; 8], _, _, _, _| {});
        process_chart::<8, _, _>(data, &mut nr, &mut nl)
    } else {
        let (mut nr, mut nl) = (|_, _| {}, |_: &[u8; 4], _, _, _, _| {});
        process_chart::<4, _, _>(data, &mut nr, &mut nl)
    }
}

#[must_use]
pub fn minimize_chart_count_rows(
    data: &[u8],
    lanes: usize,
) -> (Vec<u8>, ArrowStats, Vec<usize>, Vec<f32>, f64) {
    dispatch_lanes!(lanes, minimize_rows_plain(data))
}

fn minimize_rows_plain<const L: usize>(
    data: &[u8],
) -> (Vec<u8>, ArrowStats, Vec<usize>, Vec<f32>, f64) {
    minimize_rows_basic::<L>(data)
}

fn minimize_rows_basic<const L: usize>(
    data: &[u8],
) -> (Vec<u8>, ArrowStats, Vec<usize>, Vec<f32>, f64) {
    let mut beats = Vec::with_capacity(data.len() / (L + 1));
    let (mut last_m, mut last_r, mut last_rows) = (None, 0, 0);

    let mut on_rows = |m, r| append_row_beats(&mut beats, m, r);
    let mut on_line = |_: &[u8; L], m, r, row_count, has_object| {
        if has_object {
            (last_m, last_r, last_rows) = (Some(m), r, row_count);
        }
    };

    let (out, stats, dens) = process_chart::<L, _, _>(data, &mut on_rows, &mut on_line);
    let last = calc_last_beat(last_m, last_r, last_rows);
    (out, stats, dens, beats, last)
}

fn push_timing_measure<const L: usize>(
    measure: &mut Vec<[u8; L]>,
    midx: usize,
    rows: &mut Vec<[u8; L]>,
    beats: &mut Vec<f32>,
    has_holds: &mut bool,
) {
    if measure.is_empty() {
        return;
    }

    minimize_measure(measure);
    append_row_beats(beats, midx, measure.len());
    for line in measure.drain(..) {
        *has_holds |= row_has_hold_head(&line);
        rows.push(line);
    }
}

fn minimize_timing_rows<const L: usize>(data: &[u8]) -> (Vec<[u8; L]>, Vec<f32>, bool) {
    let cap = data.len() / (L + 1);
    let mut rows = Vec::with_capacity(cap);
    let mut beats = Vec::with_capacity(cap);
    let mut measure = Vec::with_capacity(64);
    let mut has_holds = false;
    let (mut midx, mut done) = (0usize, false);

    let mut line_off = 0usize;
    while let Some(raw) = next_line(data, &mut line_off) {
        let line = skip_ws(raw);
        if line.is_empty() || line[0] == b'/' {
            continue;
        }

        match line[0] {
            b',' => {
                push_timing_measure(&mut measure, midx, &mut rows, &mut beats, &mut has_holds);
                midx += 1;
            }
            b';' => {
                push_timing_measure(&mut measure, midx, &mut rows, &mut beats, &mut has_holds);
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
        push_timing_measure(&mut measure, midx, &mut rows, &mut beats, &mut has_holds);
    }

    (rows, beats, has_holds)
}

pub fn minimize_rows_typed<const L: usize>(
    data: &[u8],
) -> (Vec<u8>, ArrowStats, Vec<usize>, Vec<[u8; L]>, Vec<f32>, f64) {
    let mut beats = Vec::with_capacity(data.len() / (L + 1));
    let mut rows = Vec::with_capacity(beats.capacity());
    let (mut last_m, mut last_r, mut last_rows) = (None, 0, 0);

    let mut on_rows = |m, r| append_row_beats(&mut beats, m, r);
    let mut on_line = |line: &[u8; L], m, r, row_count, has_object| {
        rows.push(*line);
        if has_object {
            (last_m, last_r, last_rows) = (Some(m), r, row_count);
        }
    };

    let (out, stats, dens) = process_chart::<L, _, _>(data, &mut on_rows, &mut on_line);
    let last = calc_last_beat(last_m, last_r, last_rows);
    (out, stats, dens, rows, beats, last)
}

pub fn minimize_chart_rows_bits(
    data: &[u8],
) -> (
    Vec<u8>,
    ArrowStats,
    Vec<usize>,
    Vec<[u8; 4]>,
    Vec<f32>,
    f64,
    Vec<u8>,
) {
    let mut beats = Vec::with_capacity(data.len() / 5);
    let mut rows = Vec::with_capacity(beats.capacity());
    let mut bits = Vec::with_capacity(beats.capacity());
    let (mut last_m, mut last_r, mut last_rows) = (None, 0, 0);

    let mut on_rows = |m, r| append_row_beats(&mut beats, m, r);
    let mut on_line = |line: &[u8; 4], m, r, row_count, has_object| {
        rows.push(*line);
        let mask = u8::from(is_note(line[0]))
            | (u8::from(is_note(line[1])) << 1)
            | (u8::from(is_note(line[2])) << 2)
            | (u8::from(is_note(line[3])) << 3);
        bits.push(mask);
        if has_object {
            (last_m, last_r, last_rows) = (Some(m), r, row_count);
        }
    };

    let (out, stats, dens) = process_chart::<4, _, _>(data, &mut on_rows, &mut on_line);
    let last = calc_last_beat(last_m, last_r, last_rows);
    (out, stats, dens, rows, beats, last, bits)
}

#[must_use]
pub fn minimize_chart_for_hash(data: &[u8], lanes: usize) -> Vec<u8> {
    dispatch_lanes!(lanes, minimize_hash_impl(data))
}

fn minimize_hash_impl<const L: usize>(data: &[u8]) -> Vec<u8> {
    let mut on_rows = |_, _| {};
    let mut on_line = |_: &[u8; L], _, _, _, _| {};
    let mut on_count = |_: &[u8; L]| RowCount {
        density: false,
        object: false,
    };
    let (output, _) = minimize_chart_core(data, &mut on_rows, &mut on_line, &mut on_count);
    output
}

// ============================================================================
// Timing-Aware Stats
// ============================================================================

#[must_use]
pub fn compute_timing_aware_stats(data: &[u8], lanes: usize, timing: &TimingData) -> ArrowStats {
    dispatch_lanes!(lanes, timing_stats_typed(data, timing))
}

fn timing_stats_typed<const L: usize>(data: &[u8], timing: &TimingData) -> ArrowStats {
    let (rows, beats, has_holds) = minimize_timing_rows::<L>(data);
    if has_holds {
        compute_timing_aware_stats_from_rows_with_row_to_beat::<L>(&rows, timing, &beats)
    } else {
        compute_timing_aware_stats_no_holds_from_rows::<L>(&rows, timing, &beats)
    }
}

pub fn compute_timing_aware_stats_with_row_to_beat(
    data: &[u8],
    lanes: usize,
    timing: &TimingData,
    beats: &[f32],
) -> ArrowStats {
    if lanes == 8 {
        let rows = parse_minimized_rows::<8>(data);
        compute_timing_aware_stats_from_rows_with_row_to_beat::<8>(&rows, timing, beats)
    } else {
        let rows = parse_minimized_rows::<4>(data);
        compute_timing_aware_stats_from_rows_with_row_to_beat::<4>(&rows, timing, beats)
    }
}

pub fn compute_timing_aware_stats_from_rows_with_row_to_beat<const L: usize>(
    rows: &[[u8; L]],
    timing: &TimingData,
    beats: &[f32],
) -> ArrowStats {
    if rows.is_empty() {
        return ArrowStats::default();
    }
    let ends = scan_hold_ends(rows);
    if !has_nonjudgable_rows(timing) {
        return process_timing_rows_all_judgable::<L>(rows.iter(), &ends);
    }
    process_timing_rows::<L>(rows.iter(), &ends, timing, beats)
}

pub fn compute_timing_aware_stats_no_holds_from_rows<const L: usize>(
    rows: &[[u8; L]],
    timing: &TimingData,
    beats: &[f32],
) -> ArrowStats {
    if !has_nonjudgable_rows(timing) {
        return process_timing_rows_no_holds_all_judgable::<L>(rows.iter());
    }
    process_timing_rows_no_holds::<L>(rows.iter(), timing, beats)
}

fn process_timing_rows_no_holds_all_judgable<'a, const L: usize>(
    rows: impl Iterator<Item = &'a [u8; L]>,
) -> ArrowStats {
    let mut stats = ArrowStats::default();
    for line in rows {
        process_timing_row_no_holds_judgable(line, &mut stats);
    }
    stats
}

fn process_timing_rows_no_holds<'a, const L: usize>(
    rows: impl Iterator<Item = &'a [u8; L]>,
    timing: &TimingData,
    beats: &[f32],
) -> ArrowStats {
    let mut stats = ArrowStats::default();
    for (ridx, line) in rows.enumerate() {
        process_timing_row_no_holds(
            line,
            is_judgable_at_beat(timing, f64::from(beats[ridx])),
            &mut stats,
        );
    }
    stats
}

#[inline(always)]
fn process_timing_row_no_holds<const L: usize>(
    line: &[u8; L],
    judgable: bool,
    stats: &mut ArrowStats,
) {
    if !judgable {
        for &ch in line {
            if matches!(ch, b'1' | b'L' | b'l' | b'M' | b'm' | b'F' | b'f') {
                stats.fakes += 1;
            }
        }
        return;
    }

    process_timing_row_no_holds_judgable(line, stats);
}

#[inline(always)]
fn process_timing_row_no_holds_judgable<const L: usize>(line: &[u8; L], stats: &mut ArrowStats) {
    let (mut notes, mut has_note) = (0u32, false);
    for (c, &ch) in line.iter().enumerate() {
        match ch {
            b'1' => {
                has_note = true;
                notes += 1;
                stats.total_arrows += 1;
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
    if has_note && notes >= 3 {
        stats.hands += 1;
    }
}

fn process_timing_rows_all_judgable<'a, const L: usize>(
    rows: impl Iterator<Item = &'a [u8; L]>,
    ends: &[[usize; L]],
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
            true,
            &mut stats,
            &mut ends_per,
            &mut active,
        );
    }
    stats
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
            is_judgable_at_beat(timing, f64::from(beats[ridx])),
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

#[must_use]
pub fn measure_densities(data: &[u8], lanes: usize) -> Vec<usize> {
    dispatch_lanes!(lanes, densities_impl(data))
}

fn densities_impl<const L: usize>(data: &[u8]) -> Vec<usize> {
    let mut on_rows = |_, _| {};
    let mut on_line = |_: &[u8; L], _, _, _, _| {};
    let mut on_count = |line: &[u8; L]| RowCount {
        density: has_step::<L>(line),
        object: false,
    };
    let (_, densities) = minimize_chart_core(data, &mut on_rows, &mut on_line, &mut on_count);
    densities
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::timing::{TimingFormat, timing_data_from_chart_data};

    fn timing(fakes: Option<&str>) -> TimingData {
        timing_data_from_chart_data(
            0.0,
            0.0,
            None,
            "0.000=120.000",
            None,
            "",
            None,
            "",
            None,
            "",
            None,
            "",
            None,
            "",
            fakes,
            "",
            TimingFormat::Ssc,
            true,
        )
    }

    fn stats_from_typed(data: &[u8], timing: &TimingData) -> ArrowStats {
        let (_, _, _, rows, beats, _) = minimize_rows_typed::<4>(data);
        compute_timing_aware_stats_from_rows_with_row_to_beat::<4>(&rows, timing, &beats)
    }

    #[test]
    fn timing_stats_row_only_matches_typed_minimize() {
        let data = b"0000
1000
0000
0000
,
2000
0000
3000
0000
,
1100
0000
0011
0000
;";
        let timing = timing(None);
        assert_eq!(
            compute_timing_aware_stats(data, 4, &timing),
            stats_from_typed(data, &timing)
        );
    }

    #[test]
    fn timing_stats_no_hold_fake_rows_match_typed_minimize() {
        let data = b"1000
0000
0100
0000
,
0010
0000
0001
0000
;";
        let timing = timing(Some("4.000=4.000"));
        assert_eq!(
            compute_timing_aware_stats(data, 4, &timing),
            stats_from_typed(data, &timing)
        );
    }
}

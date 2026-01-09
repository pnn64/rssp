use crate::timing::{beat_to_note_row, note_row_to_beat, TimingData};
use std::fmt::Write;

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

#[derive(Debug, Default)]
pub struct StreamCounts {
    pub run16_streams: u32,
    pub run20_streams: u32,
    pub run24_streams: u32,
    pub run32_streams: u32,
    pub total_breaks: u32,
    pub sn_breaks: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RunDensity {
    Run32,
    Run24,
    Run20,
    Run16,
    Break,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BreakdownMode {
    Detailed,
    Partial,
    Simplified,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum StreamBreakdownLevel {
    Detailed,
    Partial,
    Simple,
    Total,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StreamSegment {
    pub start: usize,
    pub end: usize,
    pub is_break: bool,
}

#[derive(Debug, Clone, Copy)]
pub enum Token {
    Run(RunDensity, usize),
    Break(usize),
}

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

fn track_holds_core<const L: usize>(
    rows: impl Iterator<Item = impl AsRef<[u8]>>,
    row_count: usize,
) -> Vec<[usize; L]> {
    let mut stacks: [Vec<usize>; L] = std::array::from_fn(|_| Vec::new());
    let mut ends = Vec::with_capacity(row_count);

    for (i, row) in rows.enumerate() {
        let r = row.as_ref();
        ends.push([HOLD_END_NONE; L]);
        for c in 0..L.min(r.len()) {
            match r[c] {
                ch if is_hold_blocker(ch) => stacks[c].clear(),
                b'2' | b'4' => stacks[c].push(i),
                b'3' => {
                    if let Some(start) = stacks[c].pop() {
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

fn process_chart<const L: usize, R, N>(
    data: &[u8],
    on_rows: &mut R,
    on_line: &mut N,
) -> (Vec<u8>, ArrowStats, Vec<usize>)
where
    R: FnMut(usize, usize),
    N: FnMut(&[u8; L], usize, usize, usize),
{
    let mut output = Vec::with_capacity(data.len());
    let mut measure = Vec::with_capacity(64);
    let mut stats = ArrowStats::default();
    let mut densities = Vec::new();
    let (mut holds, mut ends, mut midx, mut done) = (0u32, 0u32, 0usize, false);

    let finalize = |m: &mut Vec<[u8; L]>,
                    out: &mut Vec<u8>,
                    s: &mut ArrowStats,
                    d: &mut Vec<usize>,
                    hs: &mut u32,
                    es: &mut u32,
                    idx: usize,
                    or: &mut R,
                    ol: &mut N| {
        if m.is_empty() {
            d.push(0);
            return;
        }
        minimize_measure(m);
        or(idx, m.len());
        out.reserve(m.len() * (L + 1));
        let rows = m.len();
        let mut density = 0;
        for (i, line) in m.iter().enumerate() {
            ol(line, idx, i, rows);
            if count_line(line, s, hs, es) {
                density += 1;
            }
            out.extend_from_slice(line);
            out.push(b'\n');
        }
        m.clear();
        d.push(density);
    };

    for raw in data.split(|&b| b == b'\n') {
        let line = skip_ws(raw);
        if line.is_empty() || line[0] == b'/' {
            continue;
        }

        match line[0] {
            b',' => {
                finalize(
                    &mut measure,
                    &mut output,
                    &mut stats,
                    &mut densities,
                    &mut holds,
                    &mut ends,
                    midx,
                    on_rows,
                    on_line,
                );
                output.extend_from_slice(b",\n");
                midx += 1;
            }
            b';' => {
                finalize(
                    &mut measure,
                    &mut output,
                    &mut stats,
                    &mut densities,
                    &mut holds,
                    &mut ends,
                    midx,
                    on_rows,
                    on_line,
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
        finalize(
            &mut measure,
            &mut output,
            &mut stats,
            &mut densities,
            &mut holds,
            &mut ends,
            midx,
            on_rows,
            on_line,
        );
    }

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

pub fn minimize_chart_and_count(data: &[u8]) -> (Vec<u8>, ArrowStats, Vec<usize>) {
    minimize_chart_and_count_with_lanes(data, 4)
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
    let mut output = Vec::with_capacity(data.len());
    let mut measure = Vec::with_capacity(64);

    for raw in data.split(|&b| b == b'\n') {
        let line = skip_ws(raw);
        if line.is_empty() || line[0] == b' ' || line[0] == b'/' {
            continue;
        }

        match line[0] {
            b',' => {
                flush_hash_measure::<L>(&mut measure, &mut output);
                output.extend_from_slice(b",\n");
            }
            b';' => {
                flush_hash_measure::<L>(&mut measure, &mut output);
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
    output
}

fn flush_hash_measure<const L: usize>(m: &mut Vec<[u8; L]>, out: &mut Vec<u8>) {
    if m.is_empty() {
        return;
    }
    minimize_measure(m);
    out.reserve(m.len() * (L + 1));
    for line in m.iter() {
        out.extend_from_slice(line);
        out.push(b'\n');
    }
    m.clear();
}

// ============================================================================
// Timing-Aware Stats
// ============================================================================

pub fn compute_timing_aware_stats(data: &[u8], lanes: usize, timing: &TimingData) -> ArrowStats {
    dispatch_lanes!(lanes, timing_stats_impl(data, timing))
}

pub(crate) fn compute_timing_aware_stats_with_row_to_beat(
    data: &[u8],
    lanes: usize,
    timing: &TimingData,
    beats: &[f32],
) -> ArrowStats {
    dispatch_lanes!(lanes, timing_stats_beats_impl(data, timing, beats))
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

fn timing_stats_impl<const L: usize>(data: &[u8], timing: &TimingData) -> ArrowStats {
    let (ends, measures) = scan_holds_with_measures::<L>(data);
    if ends.is_empty() {
        return ArrowStats::default();
    }

    let mut stats = ArrowStats::default();
    let mut ends_per = vec![0u32; ends.len()];
    let (mut ridx, mut midx, mut rim, mut active) = (0, 0, 0, 0i32);
    let mut rows_m = *measures.first().unwrap_or(&0);
    let (mut rows_f, mut m_f) = (rows_m as f32, 0.0f32);

    for raw in data.split(|&b| b == b'\n') {
        let line = trim_cr(raw);
        if line.is_empty() {
            continue;
        }

        match line[0] {
            b',' => {
                midx += 1;
                rim = 0;
                rows_m = *measures.get(midx).unwrap_or(&0);
                rows_f = rows_m as f32;
                m_f = midx as f32;
                continue;
            }
            b';' => break,
            _ => {}
        }
        if line.len() < L {
            continue;
        }

        if ridx > 0 {
            active -= ends_per[ridx - 1] as i32;
        }

        let row = if rows_m > 0 {
            beat_to_note_row(((m_f + rim as f32 / rows_f) * 4.0) as f64)
        } else {
            0
        };

        process_timing_row::<L>(
            line,
            &ends[ridx],
            timing.is_judgable_at_row(row),
            &mut stats,
            &mut ends_per,
            &mut active,
        );
        ridx += 1;
        rim += 1;
    }
    stats
}

fn timing_stats_beats_impl<const L: usize>(
    data: &[u8],
    timing: &TimingData,
    beats: &[f32],
) -> ArrowStats {
    let ends = scan_holds_data::<L>(data);
    if ends.is_empty() {
        return ArrowStats::default();
    }

    let mut stats = ArrowStats::default();
    let mut ends_per = vec![0u32; ends.len()];
    let (mut ridx, mut active) = (0, 0i32);

    for raw in data.split(|&b| b == b'\n') {
        let line = trim_cr(raw);
        if line.is_empty() || matches!(line[0], b',' | b';') {
            if line.first() == Some(&b';') {
                break;
            }
            continue;
        }
        if line.len() < L {
            continue;
        }

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
        ridx += 1;
    }
    stats
}

fn scan_holds_data<const L: usize>(data: &[u8]) -> Vec<[usize; L]> {
    scan_holds_with_measures::<L>(data).0
}

fn scan_holds_with_measures<const L: usize>(data: &[u8]) -> (Vec<[usize; L]>, Vec<usize>) {
    let mut stacks: [Vec<usize>; L] = std::array::from_fn(|_| Vec::new());
    let mut ends = Vec::new();
    let mut measures = Vec::new();
    let (mut ridx, mut mrows) = (0, 0);

    for raw in data.split(|&b| b == b'\n') {
        let line = trim_cr(raw);
        if line.is_empty() {
            continue;
        }

        match line[0] {
            b',' => {
                measures.push(mrows);
                mrows = 0;
                continue;
            }
            b';' => {
                measures.push(mrows);
                break;
            }
            _ => {}
        }
        if line.len() < L {
            continue;
        }

        ends.push([HOLD_END_NONE; L]);
        for c in 0..L {
            match line[c] {
                ch if is_hold_blocker(ch) => stacks[c].clear(),
                b'2' | b'4' => stacks[c].push(ridx),
                b'3' => {
                    if let Some(s) = stacks[c].pop() {
                        ends[s][c] = ridx;
                    }
                }
                _ => {}
            }
        }
        ridx += 1;
        mrows += 1;
    }
    (ends, measures)
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
    let mut densities = Vec::new();
    let mut density = 0;
    let mut saw_term = false;

    for raw in data.split(|&b| b == b'\n') {
        let line = skip_ws(raw);
        if line.is_empty() || line[0] == b'/' {
            continue;
        }

        match line[0] {
            b',' => {
                densities.push(density);
                density = 0;
            }
            b';' => {
                densities.push(density);
                saw_term = true;
                break;
            }
            _ if line.len() >= L && has_step::<L>(line) => density += 1,
            _ => {}
        }
    }

    if !saw_term {
        densities.push(density);
    }

    densities
}

pub fn measure_equally_spaced(data: &[u8], lanes: usize) -> Vec<bool> {
    dispatch_lanes!(lanes, equally_spaced_impl(data))
}

fn equally_spaced_impl<const L: usize>(data: &[u8]) -> Vec<bool> {
    let mut results = Vec::new();
    let (mut rows, mut notes) = (0, 0);
    let mut saw_term = false;

    for raw in data.split(|&b| b == b'\n') {
        let line = trim_cr(raw);
        if line.is_empty() {
            continue;
        }

        match line[0] {
            b',' => {
                results.push(notes == rows);
                rows = 0;
                notes = 0;
            }
            b';' => {
                results.push(notes == rows);
                saw_term = true;
                break;
            }
            _ if line.len() >= L => {
                rows += 1;
                if has_step::<L>(line) {
                    notes += 1;
                }
            }
            _ => {}
        }
    }

    if !saw_term {
        results.push(notes == rows);
    }

    results
}

// ============================================================================
// Stream Analysis
// ============================================================================

#[inline]
pub fn categorize_measure_density(d: usize) -> RunDensity {
    match d {
        32.. => RunDensity::Run32,
        24..=31 => RunDensity::Run24,
        20..=23 => RunDensity::Run20,
        16..=19 => RunDensity::Run16,
        _ => RunDensity::Break,
    }
}

const STREAM_THRESHOLD: usize = 16;

pub fn stream_sequences(measures: &[usize]) -> Vec<StreamSegment> {
    let streams: Vec<_> = measures
        .iter()
        .enumerate()
        .filter(|(_, n)| **n >= STREAM_THRESHOLD)
        .map(|(i, _)| i + 1)
        .collect();

    if streams.is_empty() {
        return Vec::new();
    }

    let mut segs = Vec::new();
    let first_break = streams[0].saturating_sub(1);
    if first_break >= 2 {
        segs.push(StreamSegment {
            start: 0,
            end: first_break,
            is_break: true,
        });
    }

    let (mut count, mut end) = (1, None);
    for (i, &cur) in streams.iter().enumerate() {
        let next = streams.get(i + 1).copied().unwrap_or(usize::MAX);

        if cur + 1 == next {
            count += 1;
            end = Some(cur + 1);
            continue;
        }

        let e = end.unwrap_or(cur);
        segs.push(StreamSegment {
            start: e - count,
            end: e,
            is_break: false,
        });

        let bstart = cur;
        let bend = if next == usize::MAX {
            measures.len()
        } else {
            next - 1
        };
        if bend >= bstart + 2 {
            segs.push(StreamSegment {
                start: bstart,
                end: bend,
                is_break: true,
            });
        }

        count = 1;
        end = None;
    }
    segs
}

pub fn compute_stream_counts(measures: &[usize]) -> StreamCounts {
    let cats: Vec<_> = measures
        .iter()
        .map(|&d| categorize_measure_density(d))
        .collect();
    let (start, end) = match (
        cats.iter().position(|&c| c != RunDensity::Break),
        cats.iter().rposition(|&c| c != RunDensity::Break),
    ) {
        (Some(s), Some(e)) => (s, e),
        _ => return StreamCounts::default(),
    };

    let mut sc = StreamCounts::default();
    for &cat in &cats[start..=end] {
        match cat {
            RunDensity::Run16 => sc.run16_streams += 1,
            RunDensity::Run20 => sc.run20_streams += 1,
            RunDensity::Run24 => sc.run24_streams += 1,
            RunDensity::Run32 => sc.run32_streams += 1,
            RunDensity::Break => sc.sn_breaks += 1,
        }
    }
    sc.total_breaks = stream_sequences(measures)
        .iter()
        .filter(|s| s.is_break)
        .map(|s| (s.end - s.start) as u32)
        .sum();
    sc
}

// ============================================================================
// Breakdown Formatting
// ============================================================================

pub fn generate_breakdown(measures: &[usize], mode: BreakdownMode) -> String {
    let (start, end) = match active_range(measures) {
        Some(r) => r,
        None => return String::new(),
    };

    let tokens = tokenize(&measures[start..=end]);
    let threshold = match mode {
        BreakdownMode::Detailed => 0,
        BreakdownMode::Partial => 1,
        BreakdownMode::Simplified => 4,
    };

    let mut out = String::new();
    let mut i = 0;

    while i < tokens.len() {
        match tokens[i] {
            Token::Run(cat, _) => {
                let (total, star, next) = merge_runs(&tokens, i, cat, threshold, mode);
                if !out.is_empty() {
                    out.push(' ');
                }
                write_run(&mut out, cat, total, star);
                i = next;
            }
            Token::Break(n) => {
                format_break(&mut out, n, mode);
                i += 1;
            }
        }
    }
    out
}

fn active_range(m: &[usize]) -> Option<(usize, usize)> {
    let s = m
        .iter()
        .position(|&d| categorize_measure_density(d) != RunDensity::Break)?;
    let e = m
        .iter()
        .rposition(|&d| categorize_measure_density(d) != RunDensity::Break)?;
    Some((s, e))
}

fn tokenize(dens: &[usize]) -> Vec<Token> {
    if dens.is_empty() {
        return Vec::new();
    }

    let mut tokens = Vec::with_capacity(dens.len());
    let mut cur = categorize_measure_density(dens[0]);
    let mut count = 1;

    for &d in &dens[1..] {
        let next = categorize_measure_density(d);
        if next == cur {
            count += 1;
        } else {
            tokens.push(match cur {
                RunDensity::Break => Token::Break(count),
                c => Token::Run(c, count),
            });
            cur = next;
            count = 1;
        }
    }
    tokens.push(match cur {
        RunDensity::Break => Token::Break(count),
        c => Token::Run(c, count),
    });
    tokens
}

fn merge_runs(
    tokens: &[Token],
    start: usize,
    cat: RunDensity,
    thresh: usize,
    mode: BreakdownMode,
) -> (usize, bool, usize) {
    let Token::Run(_, init) = tokens[start] else {
        unreachable!()
    };
    let (mut total, mut star, mut next) = (init, false, start + 1);

    while next + 1 < tokens.len() {
        let Token::Break(bk) = tokens[next] else {
            break;
        };
        if bk > thresh {
            break;
        }
        let Token::Run(nc, nl) = tokens[next + 1] else {
            break;
        };
        if nc == cat {
            total += bk + nl;
            star = true;
            next += 2;
        } else {
            if mode == BreakdownMode::Simplified && bk > 1 && bk <= 4 {
                total += bk;
                star = true;
            }
            next += 1;
            break;
        }
    }
    (total, star, next)
}

fn write_run(out: &mut String, cat: RunDensity, len: usize, star: bool) {
    let (pre, suf) = match cat {
        RunDensity::Run16 => ("", ""),
        RunDensity::Run20 => ("~", "~"),
        RunDensity::Run24 => ("\\", "\\"),
        RunDensity::Run32 => ("=", "="),
        RunDensity::Break => unreachable!(),
    };
    let _ = write!(out, "{}{}{}", pre, len, suf);
    if star {
        out.push('*');
    }
}

fn format_break(out: &mut String, n: usize, mode: BreakdownMode) {
    let sym = match mode {
        BreakdownMode::Detailed if n > 1 => {
            if !out.is_empty() {
                out.push(' ');
            }
            let _ = write!(out, "({})", n);
            return;
        }
        BreakdownMode::Partial => match n {
            1 => None,
            2..=4 => Some("-"),
            5..=32 => Some("/"),
            _ => Some("|"),
        },
        BreakdownMode::Simplified => match n {
            1..=4 => None,
            5..=32 => Some("/"),
            _ => Some("|"),
        },
        _ => None,
    };
    if let Some(s) = sym {
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(s);
    }
}

pub fn format_run_symbol(cat: RunDensity, len: usize, star: bool) -> String {
    let mut out = String::new();
    write_run(&mut out, cat, len, star);
    out
}

pub fn stream_breakdown(measures: &[usize], level: StreamBreakdownLevel) -> String {
    if measures.is_empty() {
        return "No Streams!".into();
    }

    let segs = stream_sequences(measures);
    if segs.is_empty() {
        return "No Streams!".into();
    }

    let mut out = String::new();
    let (mut sum, mut broken, mut total) = (0, false, 0);

    for (i, seg) in segs.iter().enumerate() {
        let size = seg.end - seg.start;
        if seg.is_break {
            if i != 0 && i + 1 != segs.len() {
                flush_stream(&mut out, &mut sum, &mut broken, &mut total, level, size);
            }
        } else {
            match level {
                StreamBreakdownLevel::Simple | StreamBreakdownLevel::Total => {
                    if i > 0 && !segs[i - 1].is_break {
                        broken = true;
                        if level == StreamBreakdownLevel::Simple {
                            sum += 1;
                        }
                    }
                    sum += size;
                }
                _ => {
                    if i > 0 && !segs[i - 1].is_break {
                        out.push('-');
                    }
                    let _ = write!(out, "{}", size);
                }
            }
        }
    }

    if sum != 0 {
        match level {
            StreamBreakdownLevel::Simple => {
                let _ = write!(out, "{}", sum);
                if broken {
                    out.push('*');
                }
            }
            StreamBreakdownLevel::Total => total += sum,
            _ => {}
        }
    }

    if level == StreamBreakdownLevel::Total {
        return format!("{} Total", total);
    }
    if out.is_empty() {
        "No Streams!".into()
    } else {
        out
    }
}

fn flush_stream(
    out: &mut String,
    sum: &mut usize,
    broken: &mut bool,
    total: &mut usize,
    level: StreamBreakdownLevel,
    size: usize,
) {
    let sym = match size {
        1..=4 => "-",
        5..=31 => "/",
        _ => " | ",
    };

    if level == StreamBreakdownLevel::Detailed {
        let _ = write!(out, " ({}) ", size);
        return;
    }

    if *sum != 0 && level == StreamBreakdownLevel::Simple {
        let _ = write!(out, "{}", *sum);
        if *broken {
            out.push('*');
        }
    } else if level == StreamBreakdownLevel::Total {
        *total += *sum;
    }

    if level != StreamBreakdownLevel::Total {
        out.push_str(sym);
    }

    *sum = 0;
    *broken = false;
}

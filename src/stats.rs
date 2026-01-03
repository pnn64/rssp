use crate::timing::{beat_to_note_row, note_row_to_beat, TimingData};
use std::fmt::Write;

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

#[inline]
fn is_all_zero<const LANES: usize>(line: &[u8; LANES]) -> bool {
    line.iter().all(|&b| b == b'0')
}

#[inline(always)]
fn is_hold_blocker(ch: u8) -> bool {
    matches!(ch, b'1' | b'M' | b'L' | b'F')
}

fn match_hold_ends<const LANES: usize>(
    lines: &[[u8; LANES]],
) -> Vec<[Option<usize>; LANES]> {
    let mut stacks: [Vec<usize>; LANES] = std::array::from_fn(|_| Vec::new());
    let mut hold_ends = vec![[None; LANES]; lines.len()];

    for (row_idx, line) in lines.iter().enumerate() {
        for (col, &ch) in line.iter().enumerate() {
            match ch {
                ch if is_hold_blocker(ch) => stacks[col].clear(),
                b'2' | b'4' => stacks[col].push(row_idx),
                b'3' => {
                    if let Some(start_idx) = stacks[col].pop() {
                        hold_ends[start_idx][col] = Some(row_idx);
                    }
                }
                _ => {}
            }
        }
    }

    hold_ends
}

fn strip_phantom_holds<const LANES: usize>(
    lines: &[[u8; LANES]],
    hold_ends: &[[Option<usize>; LANES]],
) -> Vec<[u8; LANES]> {
    lines
        .iter()
        .enumerate()
        .map(|(row_idx, line)| {
            let mut new_line = *line;
            for (col, byte) in new_line.iter_mut().enumerate() {
                if hold_ends[row_idx][col].is_none() && matches!(*byte, b'2' | b'4') {
                    *byte = b'0';
                }
            }
            new_line
        })
        .collect()
}

#[inline(always)]
fn has_phantom_holds<const LANES: usize>(minimized: &[u8]) -> bool {
    let mut depths = [0u32; LANES];
    for line_raw in minimized.split(|&b| b == b'\n') {
        let line = trim_cr(line_raw);
        if line.is_empty() {
            continue;
        }
        match line[0] {
            b',' | b';' => continue,
            _ => {}
        }
        if line.len() < LANES {
            continue;
        }
        for col in 0..LANES {
            match line[col] {
                ch if is_hold_blocker(ch) => {
                    if depths[col] != 0 {
                        return true;
                    }
                    depths[col] = 0;
                }
                b'2' | b'4' => depths[col] += 1,
                b'3' => {
                    if depths[col] > 0 {
                        depths[col] -= 1;
                    }
                }
                _ => {}
            }
        }
    }
    depths.iter().any(|&depth| depth != 0)
}

const HOLD_END_NONE: usize = usize::MAX;

#[inline(always)]
fn trim_cr(line: &[u8]) -> &[u8] {
    if line.last() == Some(&b'\r') {
        &line[..line.len() - 1]
    } else {
        line
    }
}

#[inline(always)]
fn trim_leading_ws(line: &[u8]) -> &[u8] {
    if line.is_empty() || !line[0].is_ascii_whitespace() {
        return line;
    }
    let mut start = 1usize;
    while start < line.len() && line[start].is_ascii_whitespace() {
        start += 1;
    }
    &line[start..]
}

#[inline(always)]
fn has_step<const LANES: usize>(line: &[u8]) -> bool {
    for i in 0..LANES {
        let b = line[i];
        if b == b'1' || b == b'2' || b == b'4' {
            return true;
        }
    }
    false
}

fn scan_minimized_rows_for_holds<const LANES: usize>(
    minimized_note_data: &[u8],
) -> (Vec<[usize; LANES]>, Vec<usize>) {
    let estimated_rows = minimized_note_data.len() / (LANES + 1);
    let mut hold_ends = Vec::with_capacity(estimated_rows);
    let mut measure_rows = Vec::new();
    let mut stacks: [Vec<usize>; LANES] = std::array::from_fn(|_| Vec::new());
    let mut current_measure_rows = 0usize;
    let mut row_idx = 0usize;
    let mut saw_terminator = false;

    for line_raw in minimized_note_data.split(|&b| b == b'\n') {
        let line = trim_cr(line_raw);
        if line.is_empty() {
            continue;
        }

        match line[0] {
            b',' => {
                measure_rows.push(current_measure_rows);
                current_measure_rows = 0;
                continue;
            }
            b';' => {
                measure_rows.push(current_measure_rows);
                saw_terminator = true;
                break;
            }
            _ => {}
        }

        if line.len() < LANES {
            continue;
        }

        hold_ends.push([HOLD_END_NONE; LANES]);
        for (col, &ch) in line[..LANES].iter().enumerate() {
            match ch {
                ch if is_hold_blocker(ch) => stacks[col].clear(),
                b'2' | b'4' => stacks[col].push(row_idx),
                b'3' => {
                    if let Some(start_idx) = stacks[col].pop() {
                        hold_ends[start_idx][col] = row_idx;
                    }
                }
                _ => {}
            }
        }

        row_idx += 1;
        current_measure_rows += 1;
    }

    if !saw_terminator {
        measure_rows.push(current_measure_rows);
    }

    (hold_ends, measure_rows)
}

/// Minimizes measure lines if every other line is all-zero.
#[inline(always)]
pub fn minimize_measure<const LANES: usize>(measure: &mut Vec<[u8; LANES]>) {
    while measure.len() >= 2 && measure.len() % 2 == 0 {
        if measure.iter().skip(1).step_by(2).any(|line| !is_all_zero(line)) {
            break;
        }
        let half_len = measure.len() / 2;
        for i in 0..half_len {
            measure[i] = measure[i * 2];
        }
        measure.truncate(half_len);
    }
}

/// Counts basic notes and objects on a line, returning masks for further processing.
#[inline(always)]
fn count_line_objects<const LANES: usize>(
    line: &[u8; LANES],
    stats: &mut ArrowStats,
) -> (u8, u8, u8) {
    let mut note_mask = 0u8;
    let mut hold_start_mask = 0u8;
    let mut end_mask = 0u8;

    for (i, &ch) in line.iter().enumerate() {
        match ch {
            b'1' | b'2' | b'4' => {
                note_mask |= 1 << i;
                stats.total_arrows += 1;
                if ch == b'2' {
                    hold_start_mask |= 1 << i;
                    stats.holds += 1;
                } else if ch == b'4' {
                    hold_start_mask |= 1 << i;
                    stats.rolls += 1;
                }
                match i & 3 {
                    0 => stats.left += 1,
                    1 => stats.down += 1,
                    2 => stats.up += 1,
                    3 => stats.right += 1,
                    _ => unreachable!(),
                }
            }
            b'3' => end_mask |= 1 << i,
            b'M' => stats.mines += 1,
            b'L' => stats.lifts += 1,
            b'F' => stats.fakes += 1,
            _ => {}
        }
    }
    (note_mask, hold_start_mask, end_mask)
}

#[inline(always)]
fn count_line<const LANES: usize>(
    line: &[u8; LANES],
    stats: &mut ArrowStats,
    holds_started: &mut u32,
    ends_seen: &mut u32,
) -> bool {
    // Phase 1: Count simple objects and get masks for complex logic.
    let (note_mask, hold_start_mask, end_mask) = count_line_objects(line, stats);

    *holds_started += hold_start_mask.count_ones();
    *ends_seen += end_mask.count_ones();

    // Phase 2: Handle step counting and state updates.
    let notes_on_line = note_mask.count_ones();
    let active_holds = stats.holding;
    if notes_on_line == 0 {
        stats.holding = (stats.holding - end_mask.count_ones() as i32).max(0);
        return false; // No steps on this line.
    }

    stats.total_steps += 1;
    if notes_on_line >= 2 {
        stats.jumps += 1;
    }
    if (notes_on_line as i32 + active_holds) >= 3 {
        stats.hands += 1;
    }

    // Update the number of currently held notes.
    let new_holds = hold_start_mask.count_ones() as i32;
    let released_holds = end_mask.count_ones() as i32;
    stats.holding = (stats.holding + new_holds - released_holds).max(0);

    true // There was a step on this line.
}

/// Recalculates chart stats after identifying and ignoring phantom (unclosed) holds.
fn recalculate_stats_without_phantom_holds<const LANES: usize>(
    all_lines_buffer: &[[u8; LANES]],
    hold_ends: &[[Option<usize>; LANES]],
) -> ArrowStats {
    let fixed_lines = strip_phantom_holds(all_lines_buffer, &hold_ends);

    // Pass 3: Recalculate stats using the fixed lines.
    let mut new_stats = ArrowStats::default();
    let mut dummy_holds = 0;
    let mut dummy_ends = 0;
    for line in &fixed_lines {
        count_line(line, &mut new_stats, &mut dummy_holds, &mut dummy_ends);
    }

    new_stats
}

pub fn compute_timing_aware_stats(
    minimized_note_data: &[u8],
    lanes: usize,
    timing: &TimingData,
) -> ArrowStats {
    match lanes {
        4 => compute_timing_aware_stats_impl::<4>(minimized_note_data, timing),
        8 => compute_timing_aware_stats_impl::<8>(minimized_note_data, timing),
        _ => compute_timing_aware_stats_impl::<4>(minimized_note_data, timing),
    }
}

fn compute_timing_aware_stats_impl<const LANES: usize>(
    minimized_note_data: &[u8],
    timing: &TimingData,
) -> ArrowStats {
    let (hold_ends, measure_rows) = scan_minimized_rows_for_holds::<LANES>(minimized_note_data);
    if hold_ends.is_empty() {
        return ArrowStats::default();
    }

    let mut stats = ArrowStats::default();
    let mut ends_per_row = vec![0u32; hold_ends.len()];
    let mut row_idx = 0usize;
    let mut measure_idx = 0usize;
    let mut row_in_measure = 0usize;
    let mut active_holds = 0i32;
    let mut rows_in_measure = *measure_rows.get(0).unwrap_or(&0);
    let mut rows_in_measure_f = rows_in_measure as f32;
    let mut measure_f = 0.0_f32;

    for line_raw in minimized_note_data.split(|&b| b == b'\n') {
        let line = trim_cr(line_raw);
        if line.is_empty() {
            continue;
        }

        match line[0] {
            b',' => {
                measure_idx += 1;
                row_in_measure = 0;
                rows_in_measure = *measure_rows.get(measure_idx).unwrap_or(&0);
                rows_in_measure_f = rows_in_measure as f32;
                measure_f = measure_idx as f32;
                continue;
            }
            b';' => break,
            _ => {}
        }

        if line.len() < LANES {
            continue;
        }

        if row_idx > 0 {
            active_holds -= ends_per_row[row_idx - 1] as i32;
            debug_assert!(active_holds >= 0);
        }

        let note_row = if rows_in_measure > 0 {
            let percent = row_in_measure as f32 / rows_in_measure_f;
            let beat = (measure_f + percent) * 4.0;
            beat_to_note_row(beat as f64)
        } else {
            0
        };
        let judgable = timing.is_judgable_at_row(note_row);
        let row_hold_ends = &hold_ends[row_idx];

        if judgable {
            let mut notes_on_line = 0u32;
            let mut has_note = false;
            let mut new_holds = 0u32;

            for (col, &ch) in line[..LANES].iter().enumerate() {
                match ch {
                    b'1' => {
                        has_note = true;
                        notes_on_line += 1;
                        stats.total_arrows += 1;
                        match col & 3 {
                            0 => stats.left += 1,
                            1 => stats.down += 1,
                            2 => stats.up += 1,
                            3 => stats.right += 1,
                            _ => unreachable!(),
                        }
                    }
                    b'2' | b'4' => {
                        let end_row = row_hold_ends[col];
                        if end_row != HOLD_END_NONE {
                            has_note = true;
                            notes_on_line += 1;
                            stats.total_arrows += 1;
                            new_holds += 1;
                            ends_per_row[end_row] += 1;
                            if ch == b'2' {
                                stats.holds += 1;
                            } else {
                                stats.rolls += 1;
                            }
                            match col & 3 {
                                0 => stats.left += 1,
                                1 => stats.down += 1,
                                2 => stats.up += 1,
                                3 => stats.right += 1,
                                _ => unreachable!(),
                            }
                        }
                    }
                    b'L' | b'l' => {
                        has_note = true;
                        notes_on_line += 1;
                        stats.total_arrows += 1;
                        stats.lifts += 1;
                        match col & 3 {
                            0 => stats.left += 1,
                            1 => stats.down += 1,
                            2 => stats.up += 1,
                            3 => stats.right += 1,
                            _ => unreachable!(),
                        }
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

            if notes_on_line > 0 {
                stats.total_steps += 1;
                if notes_on_line >= 2 {
                    stats.jumps += 1;
                }
            }

            if has_note && (notes_on_line as i32 + active_holds) >= 3 {
                stats.hands += 1;
            }

            if new_holds > 0 {
                active_holds += new_holds as i32;
            }
        } else {
            for (col, &ch) in line[..LANES].iter().enumerate() {
                match ch {
                    b'1' | b'L' | b'l' | b'M' | b'm' | b'F' | b'f' => stats.fakes += 1,
                    b'2' | b'4' => {
                        if row_hold_ends[col] != HOLD_END_NONE {
                            stats.fakes += 1;
                        }
                    }
                    _ => {}
                }
            }
        }

        row_idx += 1;
        row_in_measure += 1;
    }

    stats
}

/// Helper to process a completed measure: minimize, count stats, and update buffers.
#[inline(always)]
fn append_row_to_beat(row_to_beat: &mut Vec<f32>, measure_idx: usize, rows: usize) {
    if rows == 0 {
        return;
    }
    row_to_beat.reserve(rows);
    let rows_f = rows as f32;
    let measure_start = measure_idx as f32 * 4.0;
    let row_step = 4.0 / rows_f;
    for row_in_measure in 0..rows {
        let beat = measure_start + row_in_measure as f32 * row_step;
        row_to_beat.push(beat);
    }
}

/// Helper to process a completed measure: minimize, count stats, and update buffers.
fn finalize_and_process_measure<
    const LANES: usize,
    F: FnMut(usize, usize),
    G: FnMut(&[u8; LANES], usize, usize, usize),
>(
    measure: &mut Vec<[u8; LANES]>,
    output: &mut Vec<u8>,
    stats: &mut ArrowStats,
    measure_densities: &mut Vec<usize>,
    total_holds_started: &mut u32,
    total_ends_seen: &mut u32,
    measure_idx: usize,
    on_rows: &mut F,
    on_line: &mut G,
) {
    if measure.is_empty() {
        measure_densities.push(0);
        return;
    }
    minimize_measure(measure);
    on_rows(measure_idx, measure.len());
    output.reserve(measure.len() * (LANES + 1));

    let mut density = 0;
    let rows_in_measure = measure.len();
    for (row_idx, mline) in measure.iter().enumerate() {
        on_line(mline, measure_idx, row_idx, rows_in_measure);
        if count_line(mline, stats, total_holds_started, total_ends_seen) {
            density += 1;
        }
        output.extend_from_slice(mline);
        output.push(b'\n');
    }
    measure.clear();
    measure_densities.push(density);
}

fn finalize_measure_for_hash<const LANES: usize>(
    measure: &mut Vec<[u8; LANES]>,
    output: &mut Vec<u8>,
) {
    if measure.is_empty() {
        return;
    }
    minimize_measure(measure);
    output.reserve(measure.len() * (LANES + 1));
    for mline in measure.iter() {
        output.extend_from_slice(mline);
        output.push(b'\n');
    }
    measure.clear();
}

fn minimize_chart_for_hash_impl<const LANES: usize>(notes_data: &[u8]) -> Vec<u8> {
    let mut output = Vec::with_capacity(notes_data.len());
    let mut measure = Vec::with_capacity(64);
    let mut saw_semicolon = false;

    for line_raw in notes_data.split(|&b| b == b'\n') {
        let mut start = 0usize;
        while start < line_raw.len() && line_raw[start].is_ascii_whitespace() {
            start += 1;
        }
        let line = &line_raw[start..];

        if line.is_empty() || line.first() == Some(&b' ') || line.first() == Some(&b'/') {
            continue;
        }

        match line.first() {
            Some(b',') => {
                finalize_measure_for_hash(&mut measure, &mut output);
                output.extend_from_slice(b",\n");
            }
            Some(b';') => {
                finalize_measure_for_hash(&mut measure, &mut output);
                saw_semicolon = true;
                break;
            }
            Some(_) if line.len() >= LANES => {
                let mut arr = [0u8; LANES];
                arr.copy_from_slice(&line[..LANES]);
                measure.push(arr);
            }
            _ => {}
        }
    }

    if !saw_semicolon && !measure.is_empty() {
        finalize_measure_for_hash(&mut measure, &mut output);
    }

    output
}

fn minimize_chart_and_count_impl<
    const LANES: usize,
    F: FnMut(usize, usize),
    G: FnMut(&[u8; LANES], usize, usize, usize),
>(
    notes_data: &[u8],
    on_rows: &mut F,
    on_line: &mut G,
) -> (Vec<u8>, ArrowStats, Vec<usize>) {
    let mut output = Vec::with_capacity(notes_data.len());
    let mut measure = Vec::with_capacity(64);
    let mut stats = ArrowStats::default();
    let mut measure_densities = Vec::new();
    let mut total_holds_started = 0u32;
    let mut total_ends_seen = 0u32;
    let mut saw_semicolon = false;
    let mut measure_idx = 0usize;

    for line_raw in notes_data.split(|&b| b == b'\n') {
        let mut start = 0usize;
        while start < line_raw.len() && line_raw[start].is_ascii_whitespace() {
            start += 1;
        }
        let line = &line_raw[start..];

        if line.is_empty() || line.first() == Some(&b'/') {
            continue;
        }

        match line.first() {
            Some(b',') => {
                finalize_and_process_measure(
                    &mut measure,
                    &mut output,
                    &mut stats,
                    &mut measure_densities,
                    &mut total_holds_started,
                    &mut total_ends_seen,
                    measure_idx,
                    on_rows,
                    on_line,
                );
                output.extend_from_slice(b",\n");
                measure_idx += 1;
            }
            Some(b';') => {
                finalize_and_process_measure(
                    &mut measure,
                    &mut output,
                    &mut stats,
                    &mut measure_densities,
                    &mut total_holds_started,
                    &mut total_ends_seen,
                    measure_idx,
                    on_rows,
                    on_line,
                );
                saw_semicolon = true;
                measure_idx += 1;
                break;
            }
            Some(_) if line.len() >= LANES => {
                let mut arr = [0u8; LANES];
                arr.copy_from_slice(&line[..LANES]);
                measure.push(arr);
            }
            _ => { /* Ignore short lines or other cases */ }
        }
    }

    if !saw_semicolon {
        finalize_and_process_measure(
            &mut measure,
            &mut output,
            &mut stats,
            &mut measure_densities,
            &mut total_holds_started,
            &mut total_ends_seen,
            measure_idx,
            on_rows,
            on_line,
        );
    }

    if total_holds_started > 0 {
        let raw_total_steps = stats.total_steps;
        let needs_cleanup = if total_holds_started != total_ends_seen {
            true
        } else {
            has_phantom_holds::<LANES>(&output)
        };
        if needs_cleanup {
            let mut all_lines_buffer = Vec::with_capacity(output.len() / (LANES + 1));
            for line in output.split(|&b| b == b'\n') {
                if line.is_empty() {
                    continue;
                }
                match line[0] {
                    b',' | b';' => continue,
                    _ => {}
                }
                if line.len() < LANES {
                    continue;
                }
                let mut arr = [0u8; LANES];
                arr.copy_from_slice(&line[..LANES]);
                all_lines_buffer.push(arr);
            }
            let hold_ends = match_hold_ends(&all_lines_buffer);
            let mut cleaned = recalculate_stats_without_phantom_holds(&all_lines_buffer, &hold_ends);
            cleaned.total_steps = raw_total_steps;
            stats = cleaned;
        }
    }

    (output, stats, measure_densities)
}

pub fn minimize_chart_and_count(notes_data: &[u8]) -> (Vec<u8>, ArrowStats, Vec<usize>) {
    minimize_chart_and_count_with_lanes(notes_data, 4)
}

pub fn minimize_chart_and_count_with_lanes(
    notes_data: &[u8],
    lanes: usize,
) -> (Vec<u8>, ArrowStats, Vec<usize>) {
    let mut noop_rows = |_, _| {};
    match lanes {
        4 => {
            let mut noop_line = |_: &[u8; 4], _: usize, _: usize, _: usize| {};
            minimize_chart_and_count_impl::<4, _, _>(notes_data, &mut noop_rows, &mut noop_line)
        }
        8 => {
            let mut noop_line = |_: &[u8; 8], _: usize, _: usize, _: usize| {};
            minimize_chart_and_count_impl::<8, _, _>(notes_data, &mut noop_rows, &mut noop_line)
        }
        _ => {
            let mut noop_line = |_: &[u8; 4], _: usize, _: usize, _: usize| {};
            minimize_chart_and_count_impl::<4, _, _>(notes_data, &mut noop_rows, &mut noop_line)
        }
    }
}

#[inline(always)]
pub(crate) fn line_has_object<const LANES: usize>(
    line: &[u8],
    hold_depths: &mut [u32; LANES],
) -> bool {
    let mut has_object = false;
    for col in 0..LANES {
        let ch = line[col];
        match ch {
            b'1' | b'M' | b'L' | b'F' => {
                has_object = true;
                hold_depths[col] = 0;
            }
            b'K' => {
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
    has_object
}

#[inline(always)]
pub(crate) fn calc_last_beat(
    last_measure_idx: Option<usize>,
    last_row_in_measure: usize,
    last_rows_in_measure: usize,
) -> f64 {
    let Some(measure_idx) = last_measure_idx else {
        return 0.0;
    };
    let total_rows_in_measure = last_rows_in_measure.max(1) as f64;
    let row_index = last_row_in_measure as f64;
    let beats_into_measure = 4.0 * (row_index / total_rows_in_measure);
    let beat = measure_idx as f64 * 4.0 + beats_into_measure;
    let row = beat_to_note_row(beat);
    note_row_to_beat(row)
}

fn minimize_chart_count_rows_impl<const LANES: usize>(
    notes_data: &[u8],
) -> (Vec<u8>, ArrowStats, Vec<usize>, Vec<f32>, f64) {
    let mut row_to_beat = Vec::with_capacity(notes_data.len() / (LANES + 1));
    let mut hold_depths = [0u32; LANES];
    let mut last_measure_idx: Option<usize> = None;
    let mut last_row_in_measure = 0usize;
    let mut last_rows_in_measure = 0usize;
    let mut on_rows = |measure_idx: usize, rows: usize| {
        append_row_to_beat(&mut row_to_beat, measure_idx, rows);
    };
    let mut on_line = |line: &[u8; LANES], measure_idx: usize, row_idx: usize, rows: usize| {
        if line_has_object(line, &mut hold_depths) {
            last_measure_idx = Some(measure_idx);
            last_row_in_measure = row_idx;
            last_rows_in_measure = rows;
        }
    };
    let (output, stats, measure_densities) =
        minimize_chart_and_count_impl::<LANES, _, _>(notes_data, &mut on_rows, &mut on_line);
    let last_beat = calc_last_beat(last_measure_idx, last_row_in_measure, last_rows_in_measure);
    (output, stats, measure_densities, row_to_beat, last_beat)
}

pub(crate) fn minimize_chart_count_rows(
    notes_data: &[u8],
    lanes: usize,
) -> (Vec<u8>, ArrowStats, Vec<usize>, Vec<f32>, f64) {
    match lanes {
        4 => minimize_chart_count_rows_impl::<4>(notes_data),
        8 => minimize_chart_count_rows_impl::<8>(notes_data),
        _ => minimize_chart_count_rows_impl::<4>(notes_data),
    }
}

pub(crate) fn minimize_chart_rows_bits(
    notes_data: &[u8],
) -> (Vec<u8>, ArrowStats, Vec<usize>, Vec<f32>, f64, Vec<u8>) {
    let mut row_to_beat = Vec::with_capacity(notes_data.len() / (4 + 1));
    let mut hold_depths = [0u32; 4];
    let mut last_measure_idx: Option<usize> = None;
    let mut last_row_in_measure = 0usize;
    let mut last_rows_in_measure = 0usize;
    let mut bitmasks = Vec::with_capacity(row_to_beat.capacity());
    let mut on_rows = |measure_idx: usize, rows: usize| {
        append_row_to_beat(&mut row_to_beat, measure_idx, rows);
    };
    let mut on_line = |line: &[u8; 4], measure_idx: usize, row_idx: usize, rows: usize| {
        let mut mask = 0u8;
        let ch0 = line[0];
        let ch1 = line[1];
        let ch2 = line[2];
        let ch3 = line[3];
        if ch0 == b'1' || ch0 == b'2' || ch0 == b'4' {
            mask |= 1;
        }
        if ch1 == b'1' || ch1 == b'2' || ch1 == b'4' {
            mask |= 1 << 1;
        }
        if ch2 == b'1' || ch2 == b'2' || ch2 == b'4' {
            mask |= 1 << 2;
        }
        if ch3 == b'1' || ch3 == b'2' || ch3 == b'4' {
            mask |= 1 << 3;
        }
        bitmasks.push(mask);
        if line_has_object(line, &mut hold_depths) {
            last_measure_idx = Some(measure_idx);
            last_row_in_measure = row_idx;
            last_rows_in_measure = rows;
        }
    };
    let (output, stats, measure_densities) =
        minimize_chart_and_count_impl::<4, _, _>(notes_data, &mut on_rows, &mut on_line);
    let last_beat = calc_last_beat(last_measure_idx, last_row_in_measure, last_rows_in_measure);
    (output, stats, measure_densities, row_to_beat, last_beat, bitmasks)
}

fn measure_densities_impl<const LANES: usize>(notes_data: &[u8]) -> Vec<usize> {
    let mut densities = Vec::new();
    let mut density = 0usize;
    let mut saw_semicolon = false;

    for line_raw in notes_data.split(|&b| b == b'\n') {
        let line = trim_leading_ws(line_raw);
        if line.is_empty() {
            continue;
        }
        let first = line[0];
        if first == b'/' {
            continue;
        }

        match first {
            b',' => {
                densities.push(density);
                density = 0;
            }
            b';' => {
                densities.push(density);
                saw_semicolon = true;
                break;
            }
            _ if line.len() >= LANES => {
                if has_step::<LANES>(line) {
                    density += 1;
                }
            }
            _ => {}
        }
    }

    if !saw_semicolon {
        densities.push(density);
    }

    densities
}

pub fn measure_densities(notes_data: &[u8], lanes: usize) -> Vec<usize> {
    match lanes {
        4 => measure_densities_impl::<4>(notes_data),
        8 => measure_densities_impl::<8>(notes_data),
        _ => measure_densities_impl::<4>(notes_data),
    }
}

pub fn minimize_chart_for_hash(notes_data: &[u8], lanes: usize) -> Vec<u8> {
    match lanes {
        4 => minimize_chart_for_hash_impl::<4>(notes_data),
        8 => minimize_chart_for_hash_impl::<8>(notes_data),
        _ => minimize_chart_for_hash_impl::<4>(notes_data),
    }
}

#[inline]
pub fn categorize_measure_density(d: usize) -> RunDensity {
    match d {
        d if d >= 32 => RunDensity::Run32,
        d if d >= 24 => RunDensity::Run24,
        d if d >= 20 => RunDensity::Run20,
        d if d >= 16 => RunDensity::Run16,
        _ => RunDensity::Break,
    }
}

pub fn compute_stream_counts(measure_densities: &[usize]) -> StreamCounts {
    let mut sc = StreamCounts::default();

    let cats: Vec<RunDensity> = measure_densities
        .iter()
        .map(|&d| categorize_measure_density(d))
        .collect();

    let first_run = cats.iter().position(|&c| c != RunDensity::Break);
    let last_run  = cats.iter().rposition(|&c| c != RunDensity::Break);
    if first_run.is_none() || last_run.is_none() {
        return sc;
    }

    let start_idx = first_run.unwrap();
    let end_idx   = last_run.unwrap();

    for &cat in &cats[start_idx..=end_idx] {
        match cat {
            RunDensity::Run16 => sc.run16_streams += 1,
            RunDensity::Run20 => sc.run20_streams += 1,
            RunDensity::Run24 => sc.run24_streams += 1,
            RunDensity::Run32 => sc.run32_streams += 1,
            RunDensity::Break => sc.sn_breaks += 1,
        }
    }

    sc.total_breaks = stream_sequences(measure_densities)
        .iter()
        .filter(|segment| segment.is_break)
        .map(|segment| (segment.end - segment.start) as u32)
        .sum();

    sc
}

#[derive(Debug, Clone, Copy)]
pub enum Token {
    Run(RunDensity, usize),
    Break(usize),
}

pub fn generate_breakdown(measure_densities: &[usize], mode: BreakdownMode) -> String {
    let mut start_idx = None;
    let mut end_idx = None;
    for (idx, &d) in measure_densities.iter().enumerate() {
        if categorize_measure_density(d) != RunDensity::Break {
            if start_idx.is_none() {
                start_idx = Some(idx);
            }
            end_idx = Some(idx);
        }
    }

    let Some(start_idx) = start_idx else {
        return String::new();
    };
    let end_idx = end_idx.unwrap();

    let dens = &measure_densities[start_idx..=end_idx];

    let mut tokens = Vec::with_capacity(dens.len());
    let mut iter = dens.iter();
    let Some(&first) = iter.next() else {
        return String::new();
    };

    let mut cur = categorize_measure_density(first);
    let mut count = 1usize;
    for &d in iter {
        let next = categorize_measure_density(d);
        if next == cur {
            count += 1;
        } else {
            tokens.push(match cur {
                RunDensity::Break => Token::Break(count),
                other => Token::Run(other, count),
            });
            cur = next;
            count = 1;
        }
    }
    tokens.push(match cur {
        RunDensity::Break => Token::Break(count),
        other => Token::Run(other, count),
    });

    // Determine the break threshold.
    let threshold = match mode {
        BreakdownMode::Partial => 1,
        BreakdownMode::Simplified => 4,
        BreakdownMode::Detailed => 0,
    };

    let mut out = String::new();
    let mut idx = 0usize;

    while idx < tokens.len() {
        match tokens[idx] {
            Token::Run(cat, len) => {
                let mut total = len;
                let mut star = false;
                let mut next_idx = idx + 1;

                while next_idx + 1 < tokens.len() {
                    let Token::Break(bk) = tokens[next_idx] else { break; };
                    if bk > threshold {
                        break;
                    }
                    let Token::Run(next_cat, next_len) = tokens[next_idx + 1] else { break; };
                    if next_cat == cat {
                        total += bk + next_len;
                        star = true;
                        next_idx += 2;
                        continue;
                    }
                    if mode == BreakdownMode::Simplified && bk != 1 && bk <= 4 {
                        total += bk;
                        star = true;
                    }
                    next_idx += 1;
                    break;
                }

                if !out.is_empty() {
                    out.push(' ');
                }
                write_run_symbol(&mut out, cat, total, star);
                idx = next_idx;
            }
            Token::Break(bk) => {
                match mode {
                    BreakdownMode::Detailed => {
                        if bk > 1 {
                            if !out.is_empty() {
                                out.push(' ');
                            }
                            out.push('(');
                            let _ = write!(out, "{}", bk);
                            out.push(')');
                        }
                    }
                    BreakdownMode::Partial => {
                        let sym = match bk {
                            1 => None,
                            2..=4 => Some("-"),
                            5..=32 => Some("/"),
                            _ => Some("|"),
                        };
                        if let Some(sym) = sym {
                            if !out.is_empty() {
                                out.push(' ');
                            }
                            out.push_str(sym);
                        }
                    }
                    BreakdownMode::Simplified => {
                        let sym = match bk {
                            1..=4 => None,
                            5..=32 => Some("/"),
                            _ => Some("|"),
                        };
                        if let Some(sym) = sym {
                            if !out.is_empty() {
                                out.push(' ');
                            }
                            out.push_str(sym);
                        }
                    }
                }
                idx += 1;
            }
        }
    }

    out
}

fn write_run_symbol(out: &mut String, cat: RunDensity, length: usize, star: bool) {
    match cat {
        RunDensity::Run16 => {
            let _ = write!(out, "{}", length);
        }
        RunDensity::Run20 => {
            out.push('~');
            let _ = write!(out, "{}", length);
            out.push('~');
        }
        RunDensity::Run24 => {
            out.push('\\');
            let _ = write!(out, "{}", length);
            out.push('\\');
        }
        RunDensity::Run32 => {
            out.push('=');
            let _ = write!(out, "{}", length);
            out.push('=');
        }
        RunDensity::Break => unreachable!(),
    }
    if star {
        out.push('*');
    }
}

pub fn format_run_symbol(cat: RunDensity, length: usize, star: bool) -> String {
    let mut out = String::new();
    write_run_symbol(&mut out, cat, length, star);
    out
}

#[derive(Debug, Clone, Copy)]
struct StreamSegment {
    start: usize,
    end: usize,
    is_break: bool,
}

const STREAM_NOTES_THRESHOLD: usize = 16;
const STREAM_SEQUENCE_THRESHOLD: usize = 1;
const STREAM_BREAK_THRESHOLD: usize = 2;

fn stream_sequences(notes_per_measure: &[usize]) -> Vec<StreamSegment> {
    let mut stream_measures = Vec::new();
    for (idx, &n) in notes_per_measure.iter().enumerate() {
        if n >= STREAM_NOTES_THRESHOLD {
            stream_measures.push(idx + 1);
        }
    }

    let mut sequences = Vec::new();

    if let Some(&first) = stream_measures.first() {
        let break_start = 0usize;
        let break_end = first.saturating_sub(1);
        if break_end >= break_start + STREAM_BREAK_THRESHOLD {
            sequences.push(StreamSegment {
                start: break_start,
                end: break_end,
                is_break: true,
            });
        }
    }

    let mut counter = 1usize;
    let mut stream_end: Option<usize> = None;

    for (idx, &cur_val) in stream_measures.iter().enumerate() {
        let next_val = stream_measures.get(idx + 1).copied().unwrap_or(usize::MAX);

        if cur_val + 1 == next_val {
            counter += 1;
            stream_end = Some(cur_val + 1);
            continue;
        }

        if counter >= STREAM_SEQUENCE_THRESHOLD {
            let end_val = stream_end.unwrap_or(cur_val);
            let stream_start = end_val - counter;
            sequences.push(StreamSegment {
                start: stream_start,
                end: end_val,
                is_break: false,
            });
        }

        let break_start = cur_val;
        let break_end = if next_val != usize::MAX {
            next_val - 1
        } else {
            notes_per_measure.len()
        };
        if break_end >= break_start + STREAM_BREAK_THRESHOLD {
            sequences.push(StreamSegment {
                start: break_start,
                end: break_end,
                is_break: true,
            });
        }

        counter = 1;
        stream_end = None;
    }

    sequences
}

fn add_stream_notation(
    level: StreamBreakdownLevel,
    notation: &str,
    segment_size: usize,
    out: &mut String,
    segment_sum: &mut usize,
    is_broken: &mut bool,
    total_sum: &mut usize,
) {
    if level == StreamBreakdownLevel::Detailed {
        out.push_str(" (");
        let _ = write!(out, "{}", segment_size);
        out.push_str(") ");
        return;
    }

    if *segment_sum != 0 {
        match level {
            StreamBreakdownLevel::Simple => {
                let _ = write!(out, "{}", *segment_sum);
                if *is_broken {
                    out.push('*');
                }
            }
            StreamBreakdownLevel::Total => {
                *total_sum += *segment_sum;
            }
            _ => {}
        }
    }

    if level != StreamBreakdownLevel::Total {
        out.push_str(notation);
    }

    *is_broken = false;
    *segment_sum = 0;
}

pub fn stream_breakdown(
    notes_per_measure: &[usize],
    level: StreamBreakdownLevel,
) -> String {
    if notes_per_measure.is_empty() {
        return "No Streams!".to_string();
    }

    let segments = stream_sequences(notes_per_measure);
    if segments.is_empty() {
        return "No Streams!".to_string();
    }

    let mut out = String::new();
    let mut segment_sum = 0usize;
    let mut is_broken = false;
    let mut total_sum = 0usize;

    for (idx, segment) in segments.iter().enumerate() {
        let segment_size = segment.end - segment.start;
        if segment.is_break {
            if idx != 0 && idx + 1 != segments.len() {
                if segment_size <= 4 {
                    add_stream_notation(
                        level,
                        "-",
                        segment_size,
                        &mut out,
                        &mut segment_sum,
                        &mut is_broken,
                        &mut total_sum,
                    );
                } else if segment_size < 32 {
                    add_stream_notation(
                        level,
                        "/",
                        segment_size,
                        &mut out,
                        &mut segment_sum,
                        &mut is_broken,
                        &mut total_sum,
                    );
                } else {
                    add_stream_notation(
                        level,
                        " | ",
                        segment_size,
                        &mut out,
                        &mut segment_sum,
                        &mut is_broken,
                        &mut total_sum,
                    );
                }
            }
        } else {
            match level {
                StreamBreakdownLevel::Simple | StreamBreakdownLevel::Total => {
                    if idx > 0 && !segments[idx - 1].is_break {
                        is_broken = true;
                        if level == StreamBreakdownLevel::Simple {
                            segment_sum += 1;
                        }
                    }
                    segment_sum += segment_size;
                }
                _ => {
                    if idx > 0 && !segments[idx - 1].is_break {
                        out.push('-');
                    }
                    let _ = write!(out, "{}", segment_size);
                }
            }
        }
    }

    if segment_sum != 0 {
        match level {
            StreamBreakdownLevel::Simple => {
                let _ = write!(out, "{}", segment_sum);
                if is_broken {
                    out.push('*');
                }
            }
            StreamBreakdownLevel::Total => {
                total_sum += segment_sum;
            }
            _ => {}
        }
    }

    if level == StreamBreakdownLevel::Total {
        let mut out = String::new();
        let _ = write!(out, "{} Total", total_sum);
        return out;
    }

    if out.is_empty() {
        "No Streams!".to_string()
    } else {
        out
    }
}

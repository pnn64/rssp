use crate::timing::{beat_to_note_row, TimingData};

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

#[inline]
fn is_all_zero<const LANES: usize>(line: &[u8; LANES]) -> bool {
    line.iter().all(|&b| b == b'0')
}

fn match_hold_ends<const LANES: usize>(
    lines: &[[u8; LANES]],
) -> Vec<[Option<usize>; LANES]> {
    let mut stacks: [Vec<usize>; LANES] = std::array::from_fn(|_| Vec::new());
    let mut hold_ends = vec![[None; LANES]; lines.len()];

    for (row_idx, line) in lines.iter().enumerate() {
        for (col, &ch) in line.iter().enumerate() {
            match ch {
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

const HOLD_END_NONE: usize = usize::MAX;

#[inline(always)]
fn trim_cr(line: &[u8]) -> &[u8] {
    if line.last() == Some(&b'\r') {
        &line[..line.len() - 1]
    } else {
        line
    }
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
) -> ArrowStats {
    let hold_ends = match_hold_ends(all_lines_buffer);
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
fn finalize_and_process_measure<const LANES: usize>(
    measure: &mut Vec<[u8; LANES]>,
    output: &mut Vec<u8>,
    stats: &mut ArrowStats,
    measure_densities: &mut Vec<usize>,
    all_lines_buffer: &mut Vec<[u8; LANES]>,
    total_holds_started: &mut u32,
    total_ends_seen: &mut u32,
) {
    if measure.is_empty() {
        measure_densities.push(0);
        return;
    }
    minimize_measure(measure);
    output.reserve(measure.len() * (LANES + 1));

    let mut density = 0;
    for mline in measure.iter() {
        all_lines_buffer.push(*mline);
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

fn minimize_chart_and_count_impl<const LANES: usize>(
    notes_data: &[u8],
) -> (Vec<u8>, ArrowStats, Vec<usize>) {
    let mut output = Vec::with_capacity(notes_data.len());
    let mut measure = Vec::with_capacity(64);
    let mut stats = ArrowStats::default();
    let mut measure_densities = Vec::new();
    let mut all_lines_buffer = Vec::new();
    let mut total_holds_started = 0u32;
    let mut total_ends_seen = 0u32;
    let mut saw_semicolon = false;

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
                    &mut all_lines_buffer,
                    &mut total_holds_started,
                    &mut total_ends_seen,
                );
                output.extend_from_slice(b",\n");
            }
            Some(b';') => {
                finalize_and_process_measure(
                    &mut measure,
                    &mut output,
                    &mut stats,
                    &mut measure_densities,
                    &mut all_lines_buffer,
                    &mut total_holds_started,
                    &mut total_ends_seen,
                );
                saw_semicolon = true;
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

    if !saw_semicolon && !measure.is_empty() {
        finalize_and_process_measure(
            &mut measure,
            &mut output,
            &mut stats,
            &mut measure_densities,
            &mut all_lines_buffer,
            &mut total_holds_started,
            &mut total_ends_seen,
        );
    }

    if total_holds_started != total_ends_seen {
        let raw_total_steps = stats.total_steps;
        let mut cleaned = recalculate_stats_without_phantom_holds(&all_lines_buffer);
        cleaned.total_steps = raw_total_steps;
        stats = cleaned;
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
    match lanes {
        4 => minimize_chart_and_count_impl::<4>(notes_data),
        8 => minimize_chart_and_count_impl::<8>(notes_data),
        _ => minimize_chart_and_count_impl::<4>(notes_data),
    }
}

fn measure_densities_impl<const LANES: usize>(notes_data: &[u8]) -> Vec<usize> {
    let mut densities = Vec::new();
    let mut density = 0usize;
    let mut saw_semicolon = false;
    let mut measure_has_rows = false;

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
                densities.push(density);
                density = 0;
                measure_has_rows = false;
            }
            Some(b';') => {
                densities.push(density);
                saw_semicolon = true;
                break;
            }
            Some(_) if line.len() >= LANES => {
                measure_has_rows = true;
                if line[..LANES]
                    .iter()
                    .any(|&b| matches!(b, b'1' | b'2' | b'4'))
                {
                    density += 1;
                }
            }
            _ => {}
        }
    }

    if !saw_semicolon && measure_has_rows {
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
            RunDensity::Break => sc.total_breaks += 1,
        }
    }

    sc
}

#[derive(Debug)]
pub enum Token {
    Run(super::stats::RunDensity, usize),
    Break(usize),
}

pub fn generate_breakdown(measure_densities: &[usize], mode: BreakdownMode) -> String {
    // Convert densities into categories.
    let cats: Vec<RunDensity> = measure_densities
        .iter()
        .map(|&d| categorize_measure_density(d))
        .collect();

    // Trim leading/trailing Breaks.
    let start = cats.iter().position(|&c| c != RunDensity::Break);
    let end = cats.iter().rposition(|&c| c != RunDensity::Break);
    if start.is_none() || end.is_none() {
        return String::new();
    }
    let cats = &cats[start.unwrap()..=end.unwrap()];

    // Group consecutive identical categories into tokens.
    #[derive(Debug)]
    enum Token {
        Run(RunDensity, usize),
        Break(usize),
    }
    let tokens: Vec<Token> = {
        let mut tokens = Vec::new();
        let mut iter = cats.iter().cloned().peekable();
        while let Some(cat) = iter.next() {
            let mut count = 1;
            while let Some(&next) = iter.peek() {
                if next == cat {
                    count += 1;
                    iter.next();
                } else {
                    break;
                }
            }
            tokens.push(match cat {
                RunDensity::Break => Token::Break(count),
                other => Token::Run(other, count),
            });
        }
        tokens
    };

    // Determine the break threshold.
    let threshold = match mode {
        BreakdownMode::Partial => 1,
        BreakdownMode::Simplified => 4,
        BreakdownMode::Detailed => 0,
    };

    // Merge tokens—when a Run is separated from a subsequent Run of the same type
    // by a short Break (<= threshold), merge them.
    #[derive(Debug)]
    enum MToken {
        Run(RunDensity, usize, bool), // (category, total length, star flag)
        Break(usize),
    }
    let merged: Vec<MToken> = {
        let mut merged = Vec::new();
        let mut iter = tokens.into_iter().peekable();
        while let Some(tok) = iter.next() {
            match tok {
                Token::Run(cat, len) => {
                    let mut total = len;
                    let mut star = false;
                    // While a short Break is found...
                    while let Some(Token::Break(bk)) = iter.peek() {
                        if *bk > threshold {
                            break;
                        }
                        // Consume the Break.
                        let Token::Break(bk) = iter.next().unwrap() else { unreachable!() };
                        // If followed by a Run...
                        if let Some(Token::Run(next_cat, next_len)) = iter.peek() {
                            if *next_cat == cat {
                                total += bk + *next_len;
                                star = true;
                                iter.next(); // consume the next Run
                                continue;
                            } else {
                                // In Simplified mode, if the break length is >1 and ≤4, merge it.
                                if bk != 1 && mode == BreakdownMode::Simplified && bk <= 4 {
                                    total += bk;
                                    star = true;
                                }
                                break;
                            }
                        } else {
                            break;
                        }
                    }
                    merged.push(MToken::Run(cat, total, star));
                }
                Token::Break(bk) => merged.push(MToken::Break(bk)),
            }
        }
        merged
    };

    // Map merged tokens into output strings.
    let output: Vec<String> = merged
        .into_iter()
        .filter_map(|mt| match mt {
            MToken::Run(cat, len, star) => Some(format_run_symbol(cat, len, star)),
            MToken::Break(bk) => match mode {
                BreakdownMode::Detailed if bk > 1 => Some(format!("({})", bk)),
                BreakdownMode::Partial => match bk {
                    1 => None,
                    2..=4 => Some("-".to_owned()),
                    5..=32 => Some("/".to_owned()),
                    _ => Some("|".to_owned()),
                },
                BreakdownMode::Simplified => match bk {
                    1..=4 => None,
                    5..=32 => Some("/".to_owned()),
                    _ => Some("|".to_owned()),
                },
                _ => None,
            },
        })
        .collect();

    output.join(" ")
}

pub fn format_run_symbol(cat: RunDensity, length: usize, star: bool) -> String {
    let base = match cat {
        RunDensity::Run16 => format!("{}", length),
        RunDensity::Run20 => format!("~{}~", length),
        RunDensity::Run24 => format!(r"\{}\", length),
        RunDensity::Run32 => format!("={}=", length),
        RunDensity::Break => unreachable!(),
    };
    if star {
        format!("{}*", base)
    } else {
        base
    }
}

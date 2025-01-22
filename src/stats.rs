#[derive(Default)]
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
    pub holding: i32,
}

#[derive(Default)]
pub struct StreamCounts {
    pub run16_streams: u32,
    pub run20_streams: u32,
    pub run24_streams: u32,
    pub run32_streams: u32,
    pub total_breaks: u32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
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
fn is_all_zero(line: &[u8; 4]) -> bool {
    u32::from_ne_bytes(*line) == 0x30303030
}

/// Minimizes measure lines if every other line is all-zero.
#[inline]
pub fn minimize_measure(measure: &mut Vec<[u8; 4]>) {
    while measure.len() >= 2 && measure.len() % 2 == 0 {
        if (1..measure.len()).step_by(2).any(|i| !is_all_zero(&measure[i])) {
            break;
        }
        let half_len = measure.len() / 2;
        for i in 0..half_len {
            measure[i] = measure[i * 2];
        }
        measure.truncate(half_len);
    }

    // If everything is zero, keep only 1 line
    if !measure.is_empty() && measure.iter().all(is_all_zero) {
        measure.truncate(1);
    }
}

#[inline]
fn count_line(
    line: &[u8; 4],
    stats: &mut ArrowStats,
    holds_started: &mut u32,
    ends_seen: &mut u32,
) -> bool {
    // Count mines
    stats.mines += line.iter().filter(|&&c| c == b'M').count() as u32;

    // Count how many new presses on this line
    let notes_on_line = line
        .iter()
        .filter(|&&c| matches!(c, b'1' | b'2' | b'4'))
        .count() as u32;

    // Also track how many 2/4 we see for possible holds
    *holds_started += line.iter().filter(|&&c| matches!(c, b'2' | b'4')).count() as u32;

    // How many '3' ends we see
    *ends_seen += line.iter().filter(|&&c| c == b'3').count() as u32;

    if notes_on_line == 0 {
        // If no new arrow, we might end some holds
        for &ch in line {
            if ch == b'3' && stats.holding > 0 {
                stats.holding -= 1;
            }
        }
        return false;
    }

    // At least one arrow => this line is a step
    stats.total_steps += 1;

    if notes_on_line >= 2 {
        stats.jumps += 1;
    }
    if notes_on_line >= 3 {
        stats.hands += 1;
    }

    // If we were already holding something, that might form an extra hand
    let holding_val = stats.holding;
    if (holding_val == 1 && notes_on_line >= 2) || (holding_val >= 2 && notes_on_line >= 1) {
        stats.hands += 1;
    }

    // Process each column
    for (i, &ch) in line.iter().enumerate() {
        match ch {
            b'1' => {
                stats.total_arrows += 1;
            }
            b'2' => {
                stats.total_arrows += 1;
                stats.holds += 1;
            }
            b'4' => {
                stats.total_arrows += 1;
                stats.rolls += 1;
            }
            b'3' => {
                if stats.holding > 0 {
                    stats.holding -= 1;
                }
            }
            _ => {}
        }

        // directions
        if matches!(ch, b'1' | b'2' | b'4') {
            match i {
                0 => stats.left += 1,
                1 => stats.down += 1,
                2 => stats.up += 1,
                3 => stats.right += 1,
                _ => {}
            }
        }
    }

    // Increase our holding if we see '2'/'4'
    stats.holding += line.iter().filter(|&&ch| matches!(ch, b'2' | b'4')).count() as i32;

    true
}

pub fn minimize_chart_and_count(notes_data: &[u8]) -> (Vec<u8>, ArrowStats, Vec<usize>) {
    let mut output = Vec::with_capacity(notes_data.len());
    let mut measure = Vec::with_capacity(64);

    let mut stats = ArrowStats::default();
    let mut measure_densities = Vec::new();
    let mut saw_semicolon = false;

    // We'll store all lines in case we need second pass
    let mut all_lines_buffer = Vec::new();

    // We'll track how many hold starts (2 or 4) vs how many ends (3)
    let mut total_holds_started = 0u32;
    let mut total_ends_seen = 0u32;

    #[inline]
    fn finalize_measure(
        measure: &mut Vec<[u8; 4]>,
        output: &mut Vec<u8>,
        stats: &mut ArrowStats,
        measure_densities: &mut Vec<usize>,
        all_lines_buffer: &mut Vec<[u8; 4]>,
        total_holds_started: &mut u32,
        total_ends_seen: &mut u32,
    ) {
        if measure.is_empty() {
            measure_densities.push(0);
            return;
        }
        minimize_measure(measure);
        output.reserve(measure.len() * 5);

        let mut density = 0usize;
        for mline in measure.iter() {
            // store line in buffer for possible second pass
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

    for line in notes_data.split(|&b| b == b'\n') {
        if line.is_empty() {
            continue;
        }
        match line[0] {
            b',' => {
                finalize_measure(
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
            b';' => {
                finalize_measure(
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
            b' ' => {
                // skip lines of only spaces
            }
            b'/' => {
                // skip lines starting with comment
            }
            _ => {
                if line.len() < 4 {
                    continue;
                }
                let mut arr = [0u8; 4];
                arr.copy_from_slice(&line[..4]);
                measure.push(arr);
            }
        }
    }

    if !saw_semicolon && !measure.is_empty() {
        finalize_measure(
            &mut measure,
            &mut output,
            &mut stats,
            &mut measure_densities,
            &mut all_lines_buffer,
            &mut total_holds_started,
            &mut total_ends_seen,
        );
    }

    // remove trailing ",\n"
    if output.ends_with(b",\n") {
        output.truncate(output.len() - 2);
    }

    // Now check if broken => total_holds_started != total_ends_seen
    if total_holds_started != total_ends_seen {
        // We do a second pass ignoring phantom holds and phantom rolls

        let mut col_stacks: [Vec<usize>; 4] = Default::default();
        use std::collections::HashSet;
        let mut phantom_positions = HashSet::new();

        for (line_idx, line) in all_lines_buffer.iter().enumerate() {
            for (col, &ch) in line.iter().enumerate() {
                match ch {
                    b'2' | b'4' => {
                        // Start hold in this column
                        col_stacks[col].push(line_idx);
                    }
                    b'3' => {
                        // End hold in this column
                        if let Some(_start_idx) = col_stacks[col].pop() {
                            // That was a valid hold from start_idx..line_idx
                        } else {
                            // We saw a '3' but there's no open hold => oh well, do nothing
                        }
                    }
                    _ => {}
                }
            }
        }
        // Anything left in col_stacks => phantom hold(s)
        // Mark them in phantom_positions
        for (col, stack) in col_stacks.iter_mut().enumerate() {
            while let Some(start_idx) = stack.pop() {
                // That start_idx, col => phantom
                phantom_positions.insert((start_idx, col));
            }
        }

        // 2) Build a new lines array ignoring those phantom positions
        let mut fixed_lines = Vec::with_capacity(all_lines_buffer.len());
        for (i, line) in all_lines_buffer.iter().enumerate() {
            let mut new_line = *line;
            // For each col that is phantom, set '2'/'4' => '0'
            for (col, byte) in new_line.iter_mut().enumerate() {
                if phantom_positions.contains(&(i, col)) {
                    if matches!(*byte, b'2' | b'4') {
                        *byte = b'0';
                    }
                }
            }
            fixed_lines.push(new_line);
        }

        // 3) Re-run the single pass stats with the new lines
        let mut new_stats = ArrowStats::default();

        let mut dummy_holds = 0u32;
        let mut dummy_ends = 0u32;

        for line in &fixed_lines {
            count_line(line, &mut new_stats, &mut dummy_holds, &mut dummy_ends);
        }

        stats = new_stats; // overwrite old stats with the new fixed stats
    }

    (output, stats, measure_densities)
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
    let cats: Vec<RunDensity> = measure_densities
        .iter()
        .map(|&d| categorize_measure_density(d))
        .collect();

    let first_run = cats.iter().position(|&c| c != RunDensity::Break);
    let last_run  = cats.iter().rposition(|&c| c != RunDensity::Break);
    if first_run.is_none() || last_run.is_none() {
        return String::new();
    }

    let mut tokens = Vec::new();
    {
        let mut i = first_run.unwrap();
        let end = last_run.unwrap();
        while i <= end {
            let cat = cats[i];
            let mut length = 1;
            let mut j = i + 1;
            while j <= end && cats[j] == cat {
                length += 1;
                j += 1;
            }
            if cat == RunDensity::Break {
                tokens.push(Token::Break(length));
            } else {
                tokens.push(Token::Run(cat, length));
            }
            i = j;
        }
    }

    let mut output = Vec::new();
    let mut idx = 0;

    let threshold = match mode {
        BreakdownMode::Partial => 1,
        BreakdownMode::Simplified => 4,
        BreakdownMode::Detailed => 0,
    };

    while idx < tokens.len() {
        match tokens[idx] {
            Token::Run(curr_cat, mut curr_len) => {
                let mut star = false;

                if mode != BreakdownMode::Detailed {
                    'merge_loop: loop {
                        if idx + 1 >= tokens.len() {
                            break;
                        }
                        let Token::Break(bk_len) = tokens[idx + 1] else {
                            break;
                        };
                        if bk_len > threshold {
                            break;
                        }
                        if idx + 2 >= tokens.len() {
                            break;
                        }

                        if let Token::Run(next_cat, next_len) = tokens[idx + 2] {
                            if next_cat == curr_cat {
                                curr_len += bk_len + next_len;
                                star = true;
                                tokens.remove(idx + 1);
                                tokens.remove(idx + 1);
                                continue 'merge_loop;
                            } else {
                                if bk_len == 1 {
                                    break 'merge_loop;
                                }
                                if mode == BreakdownMode::Simplified && bk_len <= 4 {
                                    curr_len += bk_len;
                                    star = true;
                                    tokens.remove(idx + 1);
                                }
                                break 'merge_loop;
                            }
                        } else {
                            break 'merge_loop;
                        }
                    }
                }

                let s = format_run_symbol(curr_cat, curr_len, star);
                output.push(s);
                idx += 1;
            }
            Token::Break(bk_len) => {
                match mode {
                    BreakdownMode::Detailed => {
                        if bk_len > 1 {
                            output.push(format!("({})", bk_len));
                        }
                    }
                    BreakdownMode::Partial => {
                        if bk_len == 1 {
                            // skip
                        } else if bk_len <= 4 {
                            output.push("-".to_string());
                        } else if bk_len <= 32 {
                            output.push("/".to_string());
                        } else {
                            output.push("|".to_string());
                        }
                    }
                    BreakdownMode::Simplified => {
                        if bk_len <= 4 {
                            // skip
                        } else if bk_len <= 32 {
                            output.push("/".to_string());
                        } else {
                            output.push("|".to_string());
                        }
                    }
                }
                idx += 1;
            }
        }
    }

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

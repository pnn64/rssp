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
    line.iter().all(|&b| b == b'0')
}

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
fn count_line(line: &[u8; 4], stats: &mut ArrowStats) -> bool
{
    for &ch in line {
        if ch == b'M' {
            stats.mines += 1;
        }
    }

    let notes_on_line = line.iter()
        .filter(|&&c| matches!(c, b'1' | b'2' | b'4' ))
        .count();

    if notes_on_line == 0 {
        for &ch in line {
            // Still need to handle release of holds (b'3') if present:
            if ch == b'3' && stats.holding > 0 {
                stats.holding -= 1;
            }
        }
        return false;
    }

    stats.total_steps += 1;

    if notes_on_line >= 2 {
        stats.jumps += 1;
    }

    if notes_on_line >= 3 {
        stats.hands += 1;
    }

    if stats.holding == 1 && notes_on_line >= 2 {
        stats.hands += 1;
    }
    if stats.holding == 2 && notes_on_line >= 1 {
        stats.hands += 1;
    }
    if stats.holding == 3 && notes_on_line >= 1 {
        stats.hands += 1;
    }

    for &ch in line {
        match ch {
            b'1' => {
                stats.total_arrows += 1;
            }
            b'2' => {
                stats.total_arrows += 1;
                stats.holds += 1;  // Starting a freeze
            }
            b'4' => {
                stats.total_arrows += 1;
                stats.rolls += 1;  // Starting a roll
            }
            b'3' => {
                if stats.holding > 0 {
                    stats.holding -= 1;
                }
            }
            _ => {}
        }
    }

    if matches!(line[0], b'1'|b'2'|b'4') { stats.left  += 1; }
    if matches!(line[1], b'1'|b'2'|b'4') { stats.down  += 1; }
    if matches!(line[2], b'1'|b'2'|b'4') { stats.up  += 1; }
    if matches!(line[3], b'1'|b'2'|b'4') { stats.right += 1; }

    for &ch in line {
        if ch == b'2' || ch == b'4' {
            stats.holding += 1;
        }
    }

    true
}

/// Minimizes chart + counts arrows, returning (final chart bytes, arrow stats, measure densities).
pub fn minimize_chart_and_count(notes_data: &[u8]) -> (Vec<u8>, ArrowStats, Vec<usize>) {
    let mut output = Vec::with_capacity(notes_data.len());
    let mut measure = Vec::with_capacity(64);

    let mut stats = ArrowStats::default();
    let mut measure_densities = Vec::new();
    let mut saw_semicolon = false;

    #[inline]
    fn finalize_measure(
        measure: &mut Vec<[u8; 4]>,
        output: &mut Vec<u8>,
        stats: &mut ArrowStats,
        measure_densities: &mut Vec<usize>,
    ) {
        if measure.is_empty() {
            measure_densities.push(0);
            return;
        }
        minimize_measure(measure);
        output.reserve(measure.len() * 5);

        let mut density = 0usize;
        for mline in measure.iter() {
            if count_line(mline, stats) {
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
                finalize_measure(&mut measure, &mut output, &mut stats, &mut measure_densities);
                output.extend_from_slice(b",\n");
            }
            b';' => {
                finalize_measure(&mut measure, &mut output, &mut stats, &mut measure_densities);
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
        finalize_measure(&mut measure, &mut output, &mut stats, &mut measure_densities);
    }

    // remove trailing ",\n"
    if output.ends_with(&[b',', b'\n']) {
        output.truncate(output.len() - 2);
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

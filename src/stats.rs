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
        if measure.iter().skip(1).step_by(2).any(|line| !is_all_zero(line)) {
            break;
        }
        let half_len = measure.len() / 2;
        for i in 0..half_len {
            measure[i] = measure[i * 2];
        }
        measure.truncate(half_len);
    }

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
    let mut note_mask = 0u8;
    let mut hold_start_mask = 0u8;
    let mut end_mask = 0u8;
    let mut mine_count = 0u32;

    for (i, &ch) in line.iter().enumerate() {
        match ch {
            b'1' | b'2' | b'4' => {
                note_mask |= 1 << i;
                if ch == b'2' || ch == b'4' {
                    hold_start_mask |= 1 << i;
                }
                match i {
                    0 => stats.left += 1,
                    1 => stats.down += 1,
                    2 => stats.up += 1,
                    3 => stats.right += 1,
                    _ => {}
                }
                stats.total_arrows += 1;
                if ch == b'2' {
                    stats.holds += 1;
                } else if ch == b'4' {
                    stats.rolls += 1;
                }
            }
            b'3' => end_mask |= 1 << i,
            b'M' => mine_count += 1,
            _ => {}
        }
    }

    stats.mines += mine_count;
    let notes_on_line = note_mask.count_ones() as u32;
    *holds_started += hold_start_mask.count_ones() as u32;
    *ends_seen += end_mask.count_ones() as u32;

    if notes_on_line == 0 {
        let ends = end_mask.count_ones() as i32;
        stats.holding = (stats.holding - ends).max(0);
        return false;
    }

    stats.total_steps += 1;
    if notes_on_line >= 2 {
        stats.jumps += 1;
    }
    if notes_on_line >= 3 {
        stats.hands += 1;
    }

    let holding_val = stats.holding;
    if (holding_val == 1 && notes_on_line >= 2) || (holding_val >= 2 && notes_on_line >= 1) {
        stats.hands += 1;
    }

    let new_holds = hold_start_mask.count_ones() as i32;
    stats.holding = (stats.holding + new_holds - end_mask.count_ones() as i32).max(0);
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Direction {
    Left,
    Right,
}

#[derive(Debug, Clone, Copy)]
enum FacingState {
    Waiting { count: usize },
    Left    { count: usize },
    Right   { count: usize },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Foot {
    LeftFoot,
    RightFoot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Arrow {
    L, // must use LeftFoot
    R, // must use RightFoot
    U, // can be either foot
    D, // can be either foot
}

/// Convert bitmask to Arrow. This part is the same as you already have.
fn map_bitmask_to_arrow(mask: u8) -> Option<Arrow> {
    // Exactly one bit set => identify which arrow
    // L=0b0001, D=0b0010, U=0b0100, R=0b1000
    match mask {
        0b0001 => Some(Arrow::L),
        0b0010 => Some(Arrow::D),
        0b0100 => Some(Arrow::U),
        0b1000 => Some(Arrow::R),
        _      => None, // either no arrows or multiple arrows => not a single-arrow step
    }
}


/// Returns Some(Left) if (L->U), (D->R), (R->D), (U->L).
/// Returns Some(Right) if (L->D), (U->R), (R->U), (D->L).
/// Returns None if it's neutral (L->R, R->L, U->D, D->U) or doesn't match.
fn transition_direction(a: Arrow, b: Arrow) -> Option<Direction> {
    use Arrow::*;
    use Direction::*;

    match (a, b) {
        (L, U) | (D, R) | (R, D) | (U, L) => Some(Left),
        (L, D) | (U, R) | (R, U) | (D, L) => Some(Right),
        _ => None,
    }
}

/// This function **only** handles a slice of single-arrow steps (i.e. `[Arrow, Arrow, ...]`)
/// and returns `(left_count, right_count)` using your existing logic.
fn count_facing_steps_in_arrows(arrows: &[Arrow]) -> (u32, u32) {
    use Foot::*;
    const MONO_THRESHOLD: usize = 4;

    if arrows.is_empty() {
        return (0, 0);
    }

    let mut final_left = 0u32;
    let mut final_right = 0u32;

    // This is your old FacingState logic
    let mut state = FacingState::Waiting { count: 1 };

    // NEW: track foot usage for each arrow
    // None => not yet assigned
    // Some(LeftFoot) or Some(RightFoot) => assigned
    let mut foot_usage = vec![None; arrows.len()];

    // Helper to finalize the old segment if needed
    fn finalize_segment(
        old_state: &mut FacingState,
        final_left: &mut u32,
        final_right: &mut u32,
    ) {
        match *old_state {
            FacingState::Waiting { count: _ } => {
                // do nothing
            }
            FacingState::Left { count } => {
                if count >= MONO_THRESHOLD {
                    *final_left += count as u32;
                }
            }
            FacingState::Right { count } => {
                if count >= MONO_THRESHOLD {
                    *final_right += count as u32;
                }
            }
        }
        *old_state = FacingState::Waiting { count: 0 }; // start new segment
    }

    // Back-propagate foot assignment from arrow `idx` backward for `pending_count` arrows.
    fn back_propagate_feet(
        foot_usage: &mut [Option<Foot>],
        idx: usize,
        pending_count: usize,
    ) {
        let mut i = idx as i32 - 1;
        while (idx as i32 - i) <= (pending_count as i32) && i >= 0 {
            let prev_foot = foot_usage[(i + 1) as usize].unwrap(); 
            let this_foot = match prev_foot {
                Foot::LeftFoot => Foot::RightFoot,
                Foot::RightFoot => Foot::LeftFoot,
            };
            foot_usage[i as usize] = Some(this_foot);
            i -= 1;
        }
    }

    // Forward-assign foot usage for arrow i, alternating from arrow i-1's foot.
    // Returns Some(true) if conflict => we need a segment break.
    fn forward_assign_foot(
        arrows: &[Arrow],
        foot_usage: &mut [Option<Foot>],
        i: usize,
    ) -> Option<bool> {
        let prev_foot = foot_usage[i - 1].unwrap();
        let next_foot = match prev_foot {
            Foot::LeftFoot => Foot::RightFoot,
            Foot::RightFoot => Foot::LeftFoot,
        };
        match (arrows[i], next_foot) {
            (Arrow::L, Foot::RightFoot) => Some(true), // conflict
            (Arrow::R, Foot::LeftFoot)  => Some(true), // conflict
            _ => {
                foot_usage[i] = Some(next_foot);
                None
            }
        }
    }

    // 1) Handle the very first arrow
    match arrows[0] {
        Arrow::L => foot_usage[0] = Some(LeftFoot),
        Arrow::R => foot_usage[0] = Some(RightFoot),
        Arrow::U | Arrow::D => {
            // remain None => “pending”
        }
    }
    let mut pending_count = if foot_usage[0].is_none() { 1 } else { 0 };

    // 2) Iterate from the second arrow on
    for i in 1..arrows.len() {
        let prev = arrows[i - 1];
        let curr = arrows[i];
        let trans = transition_direction(prev, curr);

        // STEP A: foot assignment logic
        if foot_usage[i - 1].is_none() {
            // The previous arrow was still “pending”.
            if matches!(curr, Arrow::L | Arrow::R) {
                // We have a forced foot => assign foot_usage[i]
                foot_usage[i] = match curr {
                    Arrow::L => Some(Foot::LeftFoot),
                    Arrow::R => Some(Foot::RightFoot),
                    _        => None,
                };
                // back-propagate for pending_count+1
                back_propagate_feet(&mut foot_usage, i, pending_count);
                pending_count = 0;
            } else {
                // still U/D => remain pending
                foot_usage[i] = None;
                pending_count += 1;
            }
        } else {
            // The previous arrow foot is assigned => alternate forward
            if matches!(curr, Arrow::L | Arrow::R) {
                let needed_foot = match curr {
                    Arrow::L => Foot::LeftFoot,
                    Arrow::R => Foot::RightFoot,
                    _ => unreachable!(),
                };
                // The foot we want if we alternate from i-1
                let alt_foot = match foot_usage[i - 1].unwrap() {
                    Foot::LeftFoot => Foot::RightFoot,
                    Foot::RightFoot => Foot::LeftFoot,
                };
                if needed_foot != alt_foot {
                    // conflict => finalize old facing segment
                    finalize_segment(&mut state, &mut final_left, &mut final_right);
                    // now start new => we forcibly assign arrow i:
                    foot_usage[i] = Some(needed_foot);
                } else {
                    foot_usage[i] = Some(alt_foot);
                }
            } else {
                // arrow i is U or D
                if let Some(conflict) = forward_assign_foot(arrows, &mut foot_usage, i) {
                    if conflict {
                        // conflict => finalize old segment
                        finalize_segment(&mut state, &mut final_left, &mut final_right);
                        // treat this arrow as new pending block?
                        foot_usage[i] = None;
                        pending_count = 1;
                    }
                }
            }
        }

        // STEP B: your existing facing direction logic
        match state {
            FacingState::Waiting { count } => {
                match trans {
                    Some(Direction::Left) => {
                        let new_count = count + 1;
                        state = FacingState::Left { count: new_count };
                    }
                    Some(Direction::Right) => {
                        let new_count = count + 1;
                        state = FacingState::Right { count: new_count };
                    }
                    None => {
                        let new_count = count + 1;
                        state = FacingState::Waiting { count: new_count };
                    }
                }
            }
            FacingState::Left { count: seg_count } => {
                match trans {
                    Some(Direction::Left) | None => {
                        state = FacingState::Left { count: seg_count + 1 };
                    }
                    Some(Direction::Right) => {
                        if seg_count >= MONO_THRESHOLD {
                            final_left += seg_count as u32;
                        }
                        state = FacingState::Right { count: 1 };
                    }
                }
            }
            FacingState::Right { count: seg_count } => {
                match trans {
                    Some(Direction::Right) | None => {
                        state = FacingState::Right { count: seg_count + 1 };
                    }
                    Some(Direction::Left) => {
                        if seg_count >= MONO_THRESHOLD {
                            final_right += seg_count as u32;
                        }
                        state = FacingState::Left { count: 1 };
                    }
                }
            }
        }
    }

    // 3) finalize leftover
    match state {
        FacingState::Waiting { count: _ } => {
            // no direction => discard
        }
        FacingState::Left { count } => {
            if count >= MONO_THRESHOLD {
                final_left += count as u32;
            }
        }
        FacingState::Right { count } => {
            if count >= MONO_THRESHOLD {
                final_right += count as u32;
            }
        }
    }

    (final_left, final_right)
}


pub fn count_facing_steps(bitmasks: &[u8]) -> (u32, u32) {
    let mut final_left = 0u32;
    let mut final_right = 0u32;

    // Temporary buffer of consecutive single-arrow lines
    let mut current_arrows = Vec::new();

    // Go line by line
    for &mask in bitmasks {
        if let Some(arrow) = map_bitmask_to_arrow(mask) {
            // Exactly one arrow => accumulate it
            current_arrows.push(arrow);
        } else {
            // "Break" line (0 arrows or multiple arrows)
            // => finalize the chunk so far
            let (l, r) = count_facing_steps_in_arrows(&current_arrows);
            final_left += l;
            final_right += r;
            current_arrows.clear();
        }
    }

    // End of file => finalize leftover arrows
    let (l, r) = count_facing_steps_in_arrows(&current_arrows);
    final_left += l;
    final_right += r;

    (final_left, final_right)
}

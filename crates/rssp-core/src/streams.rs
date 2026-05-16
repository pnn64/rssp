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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BreakdownMode {
    Detailed,
    Partial,
    Simplified,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

#[inline]
#[must_use]
pub const fn categorize_measure_density(d: usize) -> RunDensity {
    match d {
        32.. => RunDensity::Run32,
        24..=31 => RunDensity::Run24,
        20..=23 => RunDensity::Run20,
        16..=19 => RunDensity::Run16,
        _ => RunDensity::Break,
    }
}

const STREAM_THRESHOLD: usize = 16;

#[inline(always)]
const fn is_stream_measure(d: usize) -> bool {
    d >= STREAM_THRESHOLD
}

#[must_use]
pub fn stream_sequences(measures: &[usize]) -> Vec<StreamSegment> {
    let mut segs = Vec::with_capacity(measures.len() / 2 + 1);
    let mut i = 0usize;
    let mut prev_stream_end = None;

    while i < measures.len() {
        if !is_stream_measure(measures[i]) {
            i += 1;
            continue;
        }

        let start = i;
        while i + 1 < measures.len() && is_stream_measure(measures[i + 1]) {
            i += 1;
        }
        let end = i + 1;

        match prev_stream_end {
            Some(prev_end) => {
                let gap = start - prev_end;
                if gap >= 2 {
                    segs.push(StreamSegment {
                        start: prev_end,
                        end: start,
                        is_break: true,
                    });
                }
            }
            None if start >= 2 => {
                segs.push(StreamSegment {
                    start: 0,
                    end: start,
                    is_break: true,
                });
            }
            _ => {}
        }

        segs.push(StreamSegment {
            start,
            end,
            is_break: false,
        });
        prev_stream_end = Some(end);
        i += 1;
    }

    if let Some(last_end) = prev_stream_end {
        let tail = measures.len() - last_end;
        if tail >= 2 {
            segs.push(StreamSegment {
                start: last_end,
                end: measures.len(),
                is_break: true,
            });
        }
    }

    segs
}

#[must_use]
pub fn compute_stream_counts(measures: &[usize]) -> StreamCounts {
    let mut sc = StreamCounts::default();
    let (mut seen_stream, mut leading_breaks, mut pending_breaks) = (false, 0usize, 0usize);

    for &d in measures {
        match categorize_measure_density(d) {
            RunDensity::Run16 => sc.run16_streams += 1,
            RunDensity::Run20 => sc.run20_streams += 1,
            RunDensity::Run24 => sc.run24_streams += 1,
            RunDensity::Run32 => sc.run32_streams += 1,
            RunDensity::Break => {
                if seen_stream {
                    pending_breaks += 1;
                } else {
                    leading_breaks += 1;
                }
                continue;
            }
        }

        if seen_stream {
            if pending_breaks != 0 {
                sc.sn_breaks += pending_breaks as u32;
                if pending_breaks >= 2 {
                    sc.total_breaks += pending_breaks as u32;
                }
                pending_breaks = 0;
            }
        } else {
            seen_stream = true;
            if leading_breaks >= 2 {
                sc.total_breaks += leading_breaks as u32;
            }
        }
    }

    if !seen_stream {
        return StreamCounts::default();
    }
    if pending_breaks >= 2 {
        sc.total_breaks += pending_breaks as u32;
    }
    sc
}

#[must_use]
pub fn generate_breakdown(measures: &[usize], mode: BreakdownMode) -> String {
    let Some((start, end)) = active_range(measures) else {
        return String::new();
    };

    let tokens = tokenize(&measures[start..=end]);
    format_breakdown_tokens(&tokens, mode)
}

#[must_use]
pub fn generate_breakdowns(measures: &[usize]) -> (String, String, String) {
    let Some((start, end)) = active_range(measures) else {
        return (String::new(), String::new(), String::new());
    };

    let tokens = tokenize(&measures[start..=end]);
    (
        format_breakdown_tokens(&tokens, BreakdownMode::Detailed),
        format_breakdown_tokens(&tokens, BreakdownMode::Partial),
        format_breakdown_tokens(&tokens, BreakdownMode::Simplified),
    )
}

fn format_breakdown_tokens(tokens: &[Token], mode: BreakdownMode) -> String {
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
                let (total, star, next) = merge_runs(tokens, i, cat, threshold, mode);
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
    let s = m.iter().position(|&d| is_stream_measure(d))?;
    let e = m.iter().rposition(|&d| is_stream_measure(d))?;
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
    out.push_str(pre);
    push_usize(out, len);
    out.push_str(suf);
    if star {
        out.push('*');
    }
}

fn push_usize(out: &mut String, mut n: usize) {
    if n == 0 {
        out.push('0');
        return;
    }

    let mut buf = [0u8; 20];
    let mut i = buf.len();
    while n != 0 {
        i -= 1;
        buf[i] = b'0' + (n % 10) as u8;
        n /= 10;
    }
    for &b in &buf[i..] {
        out.push(char::from(b));
    }
}

fn format_break(out: &mut String, n: usize, mode: BreakdownMode) {
    let sym = match mode {
        BreakdownMode::Detailed if n > 1 => {
            if !out.is_empty() {
                out.push(' ');
            }
            out.push('(');
            push_usize(out, n);
            out.push(')');
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
        BreakdownMode::Detailed => None,
    };

    if let Some(s) = sym {
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(s);
    }
}

#[must_use]
pub fn format_run_symbol(cat: RunDensity, len: usize, star: bool) -> String {
    let mut out = String::new();
    write_run(&mut out, cat, len, star);
    out
}

#[must_use]
pub fn stream_breakdown(measures: &[usize], level: StreamBreakdownLevel) -> String {
    if measures.is_empty() {
        return "No Streams!".into();
    }

    let segs = stream_sequences(measures);
    if segs.is_empty() {
        return "No Streams!".into();
    }

    format_stream_segments(&segs, level)
}

#[must_use]
pub fn stream_breakdowns(measures: &[usize]) -> (String, String, String) {
    if measures.is_empty() {
        return no_streams3();
    }

    let segs = stream_sequences(measures);
    if segs.is_empty() {
        return no_streams3();
    }

    (
        format_stream_segments(&segs, StreamBreakdownLevel::Detailed),
        format_stream_segments(&segs, StreamBreakdownLevel::Partial),
        format_stream_segments(&segs, StreamBreakdownLevel::Simple),
    )
}

fn no_streams3() -> (String, String, String) {
    (
        "No Streams!".into(),
        "No Streams!".into(),
        "No Streams!".into(),
    )
}

fn format_stream_segments(segs: &[StreamSegment], level: StreamBreakdownLevel) -> String {
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
                    push_usize(&mut out, size);
                }
            }
        }
    }

    if sum != 0 {
        match level {
            StreamBreakdownLevel::Simple => {
                push_usize(&mut out, sum);
                if broken {
                    out.push('*');
                }
            }
            StreamBreakdownLevel::Total => total += sum,
            _ => {}
        }
    }

    if level == StreamBreakdownLevel::Total {
        let mut out = String::new();
        push_usize(&mut out, total);
        out.push_str(" Total");
        return out;
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
        out.push_str(" (");
        push_usize(out, size);
        out.push_str(") ");
        return;
    }

    if *sum != 0 && level == StreamBreakdownLevel::Simple {
        push_usize(out, *sum);
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

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_counts(
        measures: &[usize],
        runs: (u32, u32, u32, u32),
        total_breaks: u32,
        sn_breaks: u32,
    ) {
        let counts = compute_stream_counts(measures);
        assert_eq!(counts.run16_streams, runs.0);
        assert_eq!(counts.run20_streams, runs.1);
        assert_eq!(counts.run24_streams, runs.2);
        assert_eq!(counts.run32_streams, runs.3);
        assert_eq!(counts.total_breaks, total_breaks);
        assert_eq!(counts.sn_breaks, sn_breaks);
    }

    #[test]
    fn run_symbol_formatting() {
        assert_eq!(format_run_symbol(RunDensity::Run16, 12, true), "12*");
        assert_eq!(format_run_symbol(RunDensity::Run20, 12, true), "~12~*");
        assert_eq!(format_run_symbol(RunDensity::Run24, 12, false), "\\12\\");
        assert_eq!(format_run_symbol(RunDensity::Run32, 12, false), "=12=");
    }

    #[test]
    fn generated_breakdowns_match_expected_strings() {
        let measures = [
            16, 16, 0, 16, 16, 0, 0, 20, 20, 20, 0, 24, 0, 0, 0, 32, 32, 32, 32, 32,
        ];
        let (detailed, partial, simple) = generate_breakdowns(&measures);

        assert_eq!(detailed, "2 2 (2) ~3~ \\1\\ (3) =5=");
        assert_eq!(partial, "5* - ~3~ \\1\\ - =5=");
        assert_eq!(simple, "7* ~3~ \\4\\* =5=");
    }

    #[test]
    fn stream_counts_empty_without_streams() {
        assert_counts(&[], (0, 0, 0, 0), 0, 0);
        assert_counts(&[0, 12, 15, 0], (0, 0, 0, 0), 0, 0);
    }

    #[test]
    fn stream_counts_match_break_rules() {
        assert_counts(&[0, 0, 16, 20, 0, 24, 0, 0, 32, 0, 0], (1, 1, 1, 1), 6, 3);
    }

    #[test]
    fn stream_counts_ignore_short_edge_breaks() {
        assert_counts(&[0, 16, 17, 23, 24, 31, 32, 0], (2, 1, 2, 1), 0, 0);
    }
}

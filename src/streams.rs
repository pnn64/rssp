use std::fmt::Write;

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

use std::env::args;
use std::fs::File;
use std::io::{self, Read};
use std::time::Instant;
use std::fmt::Write; // For writing to String in normalize_float_digits.
use sha1::{Digest, Sha1};

#[inline(always)]
fn extract_sections(
    data: &[u8],
) -> io::Result<(
    Option<&[u8]>, 
    Option<&[u8]>, 
    Option<&[u8]>, 
    Option<&[u8]>, 
    Option<&[u8]>
)> {
    let mut title    = None;
    let mut subtitle = None;
    let mut artist   = None;
    let mut bpms     = None;
    let mut notes    = None;

    let mut i = 0;
    while i < data.len() {
        if title.is_some() 
            && subtitle.is_some() 
            && artist.is_some() 
            && bpms.is_some() 
            && notes.is_some() 
        {
            break;
        }

        #[inline(always)]
        fn parse_tag<'a>(
            data: &'a [u8], 
            index: &mut usize, 
            tag_len: usize
        ) -> Option<&'a [u8]> {
            let start_idx = *index + tag_len; 
            if start_idx > data.len() {
                return None;
            }
            if let Some(end_off) = data[start_idx..].iter().position(|&b| b == b';') {
                let result = &data[start_idx..start_idx + end_off];
                *index = start_idx + end_off + 1; // Move past the semicolon
                Some(result)
            } else {
                None
            }
        }

        let slice = &data[i..];

        if slice.starts_with(b"#TITLE:") && title.is_none() {
            title = parse_tag(data, &mut i, b"#TITLE:".len());
            continue;
        } 
        else if slice.starts_with(b"#SUBTITLE:") && subtitle.is_none() {
            subtitle = parse_tag(data, &mut i, b"#SUBTITLE:".len());
            continue;
        } 
        else if slice.starts_with(b"#ARTIST:") && artist.is_none() {
            artist = parse_tag(data, &mut i, b"#ARTIST:".len());
            continue;
        } 
        else if slice.starts_with(b"#BPMS:") && bpms.is_none() {
            bpms = parse_tag(data, &mut i, b"#BPMS:".len());
            continue;
        } 
        else if slice.starts_with(b"#NOTES:") && notes.is_none() {
            // For #NOTES, we usually parse until the end or next data chunk. 
            // We'll parse all data after "#NOTES:" in case it goes to multiple lines.
            let start_idx = i + b"#NOTES:".len();
            if start_idx < data.len() {
                notes = Some(&data[start_idx..]);
            }
            break; // #NOTES is usually the last part we need
        }

        i += 1;
    }

    Ok((title, subtitle, artist, bpms, notes))
}

#[inline]
fn split_notes_fields<'a>(notes_block: &'a [u8]) -> (Vec<&'a [u8]>, &'a [u8]) {
    let mut fields = Vec::with_capacity(5);
    let mut colon_count = 0;
    let mut start = 0;
    for (i, &b) in notes_block.iter().enumerate() {
        if b == b':' {
            fields.push(&notes_block[start..i]);
            start = i + 1;
            colon_count += 1;
            if colon_count == 5 {
                let remainder = &notes_block[start..];
                return (fields, remainder);
            }
        }
    }
    (fields, &notes_block[notes_block.len()..])
}

#[inline(always)]
fn is_all_zero(line: &[u8; 4]) -> bool {
    line.iter().all(|&b| b == b'0')
}

#[inline(always)]
fn minimize_measure(measure: &mut Vec<[u8; 4]>) {
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

    if !measure.is_empty() && measure.iter().all(is_all_zero) {
        measure.truncate(1);
    }
}

#[inline]
fn minimize_chart(notes_data: &[u8]) -> Vec<u8> {
    let mut output = Vec::with_capacity(notes_data.len());
    let mut measure = Vec::with_capacity(64);
    let mut saw_semicolon = false;

    #[inline(always)]
    fn finalize_measure(measure: &mut Vec<[u8; 4]>, output: &mut Vec<u8>) {
        if measure.is_empty() {
            return;
        }
        minimize_measure(measure);
        output.reserve(measure.len() * 5); 
        for mline in measure.iter() {
            output.extend_from_slice(mline);
            output.push(b'\n');
        }
        measure.clear();
    }

    for line in notes_data.split(|&b| b == b'\n') {
        if line.is_empty() {
            continue;
        }
        match line[0] {
            b',' => {
                finalize_measure(&mut measure, &mut output);
                output.extend_from_slice(b",\n");
            }
            b';' => {
                finalize_measure(&mut measure, &mut output);
                saw_semicolon = true;
                break;
            }
            b' ' => {
                // skip space-only lines
            }
            _ => {
                // skip malformed line if < 4 bytes
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
        finalize_measure(&mut measure, &mut output);
    }

    // Remove a trailing ",\n" if present.
    if output.ends_with(&[b',', b'\n']) {
        output.truncate(output.len() - 2);
    }

    output
}

#[inline]
fn normalize_float_digits(param: &str) -> String {
    let mut output = String::with_capacity(param.len());
    let mut first = true;

    for beat_bpm in param.split(',').map(str::trim).filter(|s| !s.is_empty()) {
        if !first {
            output.push(',');
        } else {
            first = false;
        }

        let mut eq_split = beat_bpm.split('=');
        let beat_str = eq_split.next().unwrap_or("").trim_matches(|c: char| c.is_control());
        let bpm_str  = eq_split.next().unwrap_or("").trim_matches(|c: char| c.is_control());

        if let (Ok(beat_val), Ok(bpm_val)) = (beat_str.parse::<f64>(), bpm_str.parse::<f64>()) {
            let beat_rounded = (beat_val * 1000.0).round() / 1000.0;
            let bpm_rounded  = (bpm_val * 1000.0).round() / 1000.0;
            // We ignore the `write!` result (no I/O error here since we're writing to String).
            let _ = write!(&mut output, "{:.3}={:.3}", beat_rounded, bpm_rounded);
        } else {
            output.push_str(beat_bpm);
        }
    }
    output
}

fn main() -> io::Result<()> {
    let before = Instant::now();
    let args: Vec<String> = args().collect();

    if args.len() < 2 {
        eprintln!("Usage: {} <simfile_path>", args[0]);
        std::process::exit(1);
    }

    let simfile_path = &args[1];
    let mut file = File::open(simfile_path)?;
    let mut simfile_data = Vec::new();
    file.read_to_end(&mut simfile_data)?;

    // Single-pass extraction of #TITLE, #SUBTITLE, #ARTIST, #BPMS, #NOTES
    let (title_opt, subtitle_opt, artist_opt, bpms_opt, notes_opt) =
        extract_sections(&simfile_data)?;

    // Convert each to UTF-8 with default placeholders if missing.
    let title_str = std::str::from_utf8(title_opt.unwrap_or(b"<invalid-title>"))
        .unwrap_or("<invalid-title>");
    let subtitle_str = std::str::from_utf8(subtitle_opt.unwrap_or(b"<invalid-subtitle>"))
        .unwrap_or("<invalid-subtitle>");
    let artist_str = std::str::from_utf8(artist_opt.unwrap_or(b"<invalid-artist>"))
        .unwrap_or("<invalid-artist>");
    let bpms_raw = std::str::from_utf8(bpms_opt.unwrap_or(b"<invalid-bpms>"))
        .unwrap_or("<invalid-bpms>");

    let normalized_bpms = normalize_float_digits(bpms_raw);

    let notes_bytes = notes_opt.unwrap_or(b"<invalid-notes>");

    // Split the first 5 colon-delimited fields from #NOTES
    let (fields, chart_data) = split_notes_fields(notes_bytes);
    if fields.len() < 5 {
        eprintln!("#NOTES section is incomplete.");
        std::process::exit(1);
    }

    // We apply trim() to remove extra whitespace or newlines if needed.
    let step_type_str   = std::str::from_utf8(fields[0]).unwrap_or("").trim();
    let difficulty_str  = std::str::from_utf8(fields[2]).unwrap_or("").trim();
    let rating_str      = std::str::from_utf8(fields[3]).unwrap_or("").trim();

    // Minimize the chart data
    let mut minimized_chart = minimize_chart(chart_data);

    // Remove trailing newlines
    if let Some(pos) = minimized_chart.iter().rposition(|&x| x != b'\n') {
        minimized_chart.truncate(pos + 1);
    }

    // Hash minimized chart + normalized BPMs
    let mut hasher = Sha1::new();
    hasher.update(&minimized_chart);
    hasher.update(normalized_bpms.as_bytes());
    let hash_result = hasher.finalize();
    let hash_hex = hex::encode(hash_result);
    let short_hash = &hash_hex[..16];

    println!("Elapsed time: {:.2?}", before.elapsed());
    println!("Title: {}", title_str);
    println!("Subtitle: {}", subtitle_str);
    println!("Artist: {}", artist_str);
    println!("Normalized BPMs: {}", normalized_bpms);
    println!("Steptype: {}", step_type_str);
    println!("Difficulty: {}", difficulty_str);
    println!("Rating: {}", rating_str);
    println!("Hash (first 16 hex chars): {}", short_hash);

    Ok(())
}

use crate::perf::measure;
use std::hint::black_box;
use std::io::{self, BufWriter, Write};

fn hashes(count: usize, kind: &str) -> Vec<String> {
    (0..count)
        .map(|index| match kind {
            "plain" => format!("{index:016x}"),
            "special" => ["", "a|b", "comma,quote\"", "歌\\é\n"][index % 4].into(),
            "long" => "abcdef0123456789".repeat(64),
            _ => unreachable!("fixture kind"),
        })
        .collect()
}

#[test]
fn csv_hash_edges() {
    let mut course = super::course_reports::summary(16);
    course.sha1_hashes = vec![String::new(), "a|b".into(), "歌".into(), String::new()];
    course.bpm_neutral_sha1_hashes = vec!["z,".into(), "q\"".into()];
    let mut expected = Vec::new();
    super::super::write_csv_course(&mut expected, &course).expect("Vec write");
    let text = std::str::from_utf8(&expected).expect("UTF-8 report");
    assert!(text.contains(",0,|a|b|歌|,z,|q\","), "{text}");
    let begin = text.find("|a|b|歌|").expect("hash field");
    // Exercise every byte boundary in both hash fields, including UTF-8.
    for limit in begin..=begin + "|a|b|歌|,z,|q\"".len() {
        let mut writer = Limited {
            bytes: Vec::new(),
            limit,
            calls_after_error: 0,
            failed: false,
        };
        let error =
            super::super::write_csv_course(&mut writer, &course).expect_err("limited writer");
        assert_eq!(error.kind(), io::ErrorKind::BrokenPipe);
        assert_eq!(writer.bytes, expected[..limit]);
        assert_eq!(writer.calls_after_error, 0);
    }
    for first in [vec![], vec![String::new()]] {
        course.sha1_hashes = first;
        course.bpm_neutral_sha1_hashes.clear();
        let mut output = Vec::new();
        super::super::write_csv_course(&mut output, &course).expect("Vec write");
        let text = std::str::from_utf8(&output).expect("UTF-8 report");
        assert!(text.contains(",0,,,"));
    }
}

struct Limited {
    bytes: Vec<u8>,
    limit: usize,
    calls_after_error: usize,
    failed: bool,
}

impl Write for Limited {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self.failed {
            self.calls_after_error += 1;
        }
        let len = bytes.len().min(self.limit - self.bytes.len());
        if len == 0 {
            self.failed = true;
            return Err(io::ErrorKind::BrokenPipe.into());
        }
        self.bytes.extend_from_slice(&bytes[..len]);
        Ok(len)
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[test]
#[ignore = "explicit CSV course hash benchmark"]
fn csv_hotpath() {
    let mut output = Vec::with_capacity(262_144);
    let mut buffered = BufWriter::with_capacity(8192, Vec::with_capacity(262_144));
    for (count, kind) in [
        (0, "plain"),
        (1, "plain"),
        (32, "plain"),
        (256, "plain"),
        (4096, "plain"),
        (1, "special"),
        (32, "special"),
        (32, "long"),
    ] {
        let mut course = super::course_reports::summary(16);
        course.sha1_hashes = hashes(count, kind);
        course.bpm_neutral_sha1_hashes = hashes(count / 2, kind);
        for use_buffer in [false, true] {
            measure(
                &format!("csv_hash/{count}_{kind}_{use_buffer}"),
                count,
                || {
                    if use_buffer {
                        buffered.get_mut().clear();
                        super::super::write_course_reports(
                            black_box(&course),
                            super::super::OutputMode::CSV,
                            black_box(&mut buffered),
                        )
                        .expect("Vec write");
                        buffered.flush().expect("Vec flush");
                        black_box(buffered.get_ref());
                    } else {
                        output.clear();
                        super::super::write_course_reports(
                            black_box(&course),
                            super::super::OutputMode::CSV,
                            black_box(&mut output),
                        )
                        .expect("Vec write");
                        black_box(&output);
                    }
                },
            );
        }
    }
}

#[test]
#[ignore = "explicit CSV byte comparison"]
fn csv_trace() {
    for count in [0, 1, 32, 256, 4096] {
        for kind in ["plain", "special", "long"] {
            let mut course = super::course_reports::summary(16);
            course.sha1_hashes = hashes(count, kind);
            course.bpm_neutral_sha1_hashes = hashes(count / 2, kind);
            let mut output = Vec::new();
            super::super::write_course_reports(&course, super::super::OutputMode::CSV, &mut output)
                .expect("Vec write");
            println!("csv-hashes {count} {kind} {output:?}");
        }
    }
}

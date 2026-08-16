use std::io;
use std::sync::Arc;

use super::report_timing_bench;

pub struct SerializeFixture {
    pub summary: rssp::SimfileSummary,
    pub output_len: usize,
}

pub struct EscapeFixture {
    pub summary: rssp::SimfileSummary,
    pub output_len: usize,
    pub legacy_calls: usize,
    pub current_calls: usize,
}

pub struct BufferFixture {
    pub summary: rssp::SimfileSummary,
    pub output_len: usize,
    pub legacy_calls: usize,
    pub current_calls: usize,
}

#[derive(Default)]
struct WriteCounter {
    bytes: usize,
    calls: usize,
}

impl io::Write for WriteCounter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.bytes += buf.len();
        self.calls += 1;
        Ok(buf.len())
    }

    fn write_all(&mut self, buf: &[u8]) -> io::Result<()> {
        self.bytes += buf.len();
        self.calls += 1;
        Ok(())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

pub fn write(summary: &rssp::SimfileSummary, output: &mut Vec<u8>, legacy: bool) -> usize {
    rssp::serialize::profile_serialize_simfile(summary, "ssc", output, legacy)
        .expect("benchmark summary should serialize")
}

pub fn write_escape(summary: &rssp::SimfileSummary, output: &mut Vec<u8>, legacy: bool) -> usize {
    rssp::serialize::profile_serialize_simfile_escape(summary, "ssc", output, legacy)
        .expect("escape benchmark summary should serialize")
}

pub fn write_escape_field(bytes: &[u8], output: &mut Vec<u8>, legacy: bool) -> usize {
    rssp::serialize::profile_sm_escape(bytes, output, legacy)
        .expect("escape benchmark field should serialize")
}

pub fn write_buffered(summary: &rssp::SimfileSummary, output: &mut Vec<u8>, legacy: bool) -> usize {
    rssp::serialize::profile_serialize_simfile_buffered(summary, "ssc", output, legacy)
        .expect("buffer benchmark summary should serialize")
}

fn assert_same(summary: &rssp::SimfileSummary, extension: &str) -> usize {
    let mut legacy = Vec::new();
    let legacy_len =
        rssp::serialize::profile_serialize_simfile(summary, extension, &mut legacy, true)
            .expect("legacy benchmark summary should serialize");
    let mut current = Vec::new();
    let current_len =
        rssp::serialize::profile_serialize_simfile(summary, extension, &mut current, false)
            .expect("benchmark summary should serialize");
    assert_eq!(current_len, legacy_len);
    assert_eq!(current_len, current.len());
    assert_eq!(current, legacy);

    let mut public = Vec::new();
    let public_len = rssp::serialize::serialize_simfile(summary, extension, &mut public)
        .expect("public serializer should succeed");
    assert_eq!(public_len, current_len);
    assert_eq!(public, current);
    current_len
}

fn assert_escape_same(summary: &rssp::SimfileSummary, extension: &str) -> usize {
    let mut legacy = Vec::new();
    let legacy_len =
        rssp::serialize::profile_serialize_simfile_escape(summary, extension, &mut legacy, true)
            .expect("legacy escape benchmark summary should serialize");
    let mut current = Vec::new();
    let current_len =
        rssp::serialize::profile_serialize_simfile_escape(summary, extension, &mut current, false)
            .expect("current escape benchmark summary should serialize");
    assert_eq!(current_len, legacy_len);
    assert_eq!(current_len, current.len());
    assert_eq!(current, legacy);

    let mut public = Vec::new();
    let public_len = rssp::serialize::serialize_simfile(summary, extension, &mut public)
        .expect("public serializer should succeed");
    assert_eq!(public_len, current_len);
    assert_eq!(public, current);
    current_len
}

fn assert_buffer_same(summary: &rssp::SimfileSummary, extension: &str) -> usize {
    let mut legacy = Vec::new();
    let legacy_len =
        rssp::serialize::profile_serialize_simfile_buffered(summary, extension, &mut legacy, true)
            .expect("unbuffered benchmark summary should serialize");
    let mut current = Vec::new();
    let current_len = rssp::serialize::profile_serialize_simfile_buffered(
        summary,
        extension,
        &mut current,
        false,
    )
    .expect("stack-buffered benchmark summary should serialize");
    assert_eq!(current_len, legacy_len);
    assert_eq!(current_len, current.len());
    assert_eq!(current, legacy);

    let mut public = Vec::new();
    let public_len = rssp::serialize::serialize_simfile(summary, extension, &mut public)
        .expect("public serializer should succeed");
    assert_eq!(public_len, current_len);
    assert_eq!(public, current);
    current_len
}

fn write_calls(summary: &rssp::SimfileSummary, legacy: bool) -> (usize, usize) {
    let mut counter = WriteCounter::default();
    let written =
        rssp::serialize::profile_serialize_simfile_escape(summary, "ssc", &mut counter, legacy)
            .expect("escape benchmark summary should serialize");
    assert_eq!(written, counter.bytes);
    (counter.calls, counter.bytes)
}

fn buffer_write_calls(summary: &rssp::SimfileSummary, legacy: bool) -> (usize, usize) {
    let mut counter = WriteCounter::default();
    let written =
        rssp::serialize::profile_serialize_simfile_buffered(summary, "ssc", &mut counter, legacy)
            .expect("buffer benchmark summary should serialize");
    assert_eq!(written, counter.bytes);
    (counter.calls, counter.bytes)
}

fn assert_escape_field_same(bytes: &[u8]) {
    let mut legacy = Vec::new();
    let legacy_len = write_escape_field(bytes, &mut legacy, true);
    let mut current = Vec::new();
    let current_len = write_escape_field(bytes, &mut current, false);
    assert_eq!(current_len, legacy_len);
    assert_eq!(current_len, current.len());
    assert_eq!(current, legacy);
}

impl SerializeFixture {
    pub fn new() -> Self {
        let input = report_timing_bench::fixture();
        let summary = rssp::analyze(input.as_bytes(), "ssc", &report_timing_bench::options())
            .expect("serialization benchmark fixture should analyze");
        let output_len = assert_same(&summary, "ssc");
        Self {
            summary,
            output_len,
        }
    }
}

impl EscapeFixture {
    pub fn new() -> Self {
        let mut summary = SerializeFixture::new().summary;
        const CHUNK: &str = "A long ordinary metadata field with UTF-8 Café 二 and path / value // comment : section ; slash\\ tail. ";
        let mut title = String::with_capacity(CHUNK.len() * 4096);
        for _ in 0..4096 {
            title.push_str(CHUNK);
        }
        summary.title_str = title;
        let output_len = assert_escape_same(&summary, "ssc");
        let (legacy_calls, legacy_bytes) = write_calls(&summary, true);
        let (current_calls, current_bytes) = write_calls(&summary, false);
        assert_eq!(legacy_bytes, output_len);
        assert_eq!(current_bytes, output_len);
        assert!(current_calls < legacy_calls);
        Self {
            summary,
            output_len,
            legacy_calls,
            current_calls,
        }
    }
}

impl BufferFixture {
    pub fn new() -> Self {
        let summary = SerializeFixture::new().summary;
        let output_len = assert_buffer_same(&summary, "ssc");
        let (legacy_calls, legacy_bytes) = buffer_write_calls(&summary, true);
        let (current_calls, current_bytes) = buffer_write_calls(&summary, false);
        assert_eq!(legacy_bytes, output_len);
        assert_eq!(current_bytes, output_len);
        assert!(current_calls < legacy_calls);
        Self {
            summary,
            output_len,
            legacy_calls,
            current_calls,
        }
    }
}

pub fn assert_behavior(fixture: &SerializeFixture) {
    assert_same(&fixture.summary, "ssc");
    assert_same(&fixture.summary, "sm");

    let mut edge = fixture.summary.clone();
    edge.ssc_version = -12.345;
    edge.offset = -0.0;
    edge.sample_start = f64::NAN;
    edge.sample_length = f64::INFINITY;
    edge.last_second_hint = Some(f64::NEG_INFINITY);
    let timing = Arc::make_mut(&mut edge.global_timing_segments);
    timing.bpms = vec![
        (-0.0, f32::NAN),
        (f32::INFINITY, f32::NEG_INFINITY),
        (f32::MAX, f32::MIN_POSITIVE),
    ];
    timing.speeds = vec![
        (
            -0.0,
            f32::NAN,
            f32::INFINITY,
            rssp::timing::SpeedUnit::Seconds,
        ),
        (
            f32::NEG_INFINITY,
            f32::MAX,
            f32::MIN_POSITIVE,
            rssp::timing::SpeedUnit::Beats,
        ),
    ];
    edge.charts[0].cached_radar_values = Some([f32::NAN; rssp::stats::RADAR_CATEGORY_COUNT]);
    assert_same(&edge, "ssc");
    assert_same(&edge, "sm");
}

pub fn assert_escape_behavior(fixture: &EscapeFixture) {
    assert_escape_same(&fixture.summary, "ssc");
    assert_escape_same(&fixture.summary, "sm");

    let mut edge = fixture.summary.clone();
    for title in [
        "",
        "plain ascii",
        "/",
        "//",
        "///",
        "////",
        "\\:;",
        "a//b:c;d\\e",
        "UTF-8 Café 二 // : ; \\",
    ] {
        assert_escape_field_same(title.as_bytes());
        edge.title_str = title.to_owned();
        assert_escape_same(&edge, "ssc");
        assert_escape_same(&edge, "sm");
    }
    assert_escape_field_same(fixture.summary.title_str.as_bytes());

    println!(
        "serialize escape writer calls: byte-at-a-time={} batched-spans={} output_bytes={}",
        fixture.legacy_calls, fixture.current_calls, fixture.output_len
    );
}

pub fn assert_buffer_behavior(fixture: &BufferFixture) {
    assert_buffer_same(&fixture.summary, "ssc");
    assert_buffer_same(&fixture.summary, "sm");

    let mut edge = fixture.summary.clone();
    edge.title_str = "UTF-8 Café 二 // : ; \\".repeat(1024);
    edge.offset = -0.0;
    edge.sample_start = f64::NAN;
    edge.sample_length = f64::INFINITY;
    edge.last_second_hint = Some(f64::NEG_INFINITY);
    assert_buffer_same(&edge, "ssc");
    assert_buffer_same(&edge, "sm");

    println!(
        "serialize buffer writer calls: unbuffered={} stack-buffered={} output_bytes={}",
        fixture.legacy_calls, fixture.current_calls, fixture.output_len
    );
}

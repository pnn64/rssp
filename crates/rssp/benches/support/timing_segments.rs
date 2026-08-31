use std::fmt::Write;

pub const ENTRY_COUNT: usize = 3_840;

pub fn fixture() -> String {
    let mut map = String::with_capacity(32 * 1_024);
    for idx in 0..ENTRY_COUNT {
        if idx != 0 {
            map.push(',');
        }
        write!(&mut map, "{}=.5", idx * 4).expect("writing to a String cannot fail");
    }
    assert!(map.len() < 32 * 1_024);
    map
}

pub fn parse(map: &str, legacy_count: bool) -> Vec<rssp::timing::Segment> {
    rssp::timing::parse_segments_for_bench(map, legacy_count)
}

pub fn assert_behavior(map: &str) {
    let legacy = parse(map, true);
    let chunked = parse(map, false);
    assert_eq!(legacy.len(), ENTRY_COUNT);
    assert_eq!(chunked.len(), legacy.len());
    for (actual, expected) in chunked.iter().zip(legacy) {
        assert_eq!(actual.beat.to_bits(), expected.beat.to_bits());
        assert_eq!(actual.value.to_bits(), expected.value.to_bits());
    }
}

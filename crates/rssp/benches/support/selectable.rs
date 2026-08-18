use std::hint::black_box;

pub const BATCH: usize = 4_096;
pub const HOT_CASES: [Option<&[u8]>; 8] = [
    Some(b"YES"),
    Some(b"NO"),
    Some(b"YES"),
    Some(b"MAYBE"),
    None,
    Some(b"no"),
    Some(b" NO "),
    Some(b"YES"),
];

const EDGE_CASES: [(Option<&[u8]>, bool); 12] = [
    (None, true),
    (Some(b""), true),
    (Some(b"NO"), false),
    (Some(b"YES"), true),
    (Some(b"no"), true),
    (Some(b" NO "), true),
    (Some(b"N\\O"), false),
    (Some(b"\\NO"), false),
    (Some(b"N\\\\O"), true),
    (Some(b"NO\\"), true),
    (Some(b"N\x80O"), true),
    (Some(b"\x80"), true),
];

pub fn assert_behavior() {
    for (tag, expected) in EDGE_CASES {
        let legacy = rssp::profile::selectable(tag, true);
        let current = rssp::profile::selectable(tag, false);
        assert_eq!(legacy, expected, "legacy mismatch for {tag:?}");
        assert_eq!(current, expected, "current mismatch for {tag:?}");
        assert_eq!(current, legacy, "implementation mismatch for {tag:?}");
    }
}

pub fn run<const LEGACY: bool>() -> usize {
    let mut selectable = 0usize;
    for index in 0..BATCH {
        let tag = black_box(HOT_CASES[index % HOT_CASES.len()]);
        selectable += rssp::profile::selectable(tag, LEGACY) as usize;
    }
    black_box(selectable)
}

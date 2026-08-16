use std::borrow::Cow;
use std::fmt::Write as _;
use std::hint::black_box;

pub const PAIR_MAPS: usize = 6;
pub const ENTRIES: usize = 512;

pub struct TimingMaps {
    pairs: [String; PAIR_MAPS],
    speeds: String,
    bytes: u64,
}

impl TimingMaps {
    pub fn new() -> Self {
        let pairs = std::array::from_fn(|map| pair_map(map));
        let speeds = speed_map();
        let bytes = pairs.iter().map(String::len).sum::<usize>() + speeds.len();
        Self {
            pairs,
            speeds,
            bytes: bytes as u64,
        }
    }

    pub fn bytes(&self) -> u64 {
        self.bytes
    }

    pub fn owned(&self) -> usize {
        let mut checksum = 0usize;
        for map in &self.pairs {
            let result = rssp::bpm::clean_and_normalize_float_digits(black_box(map));
            checksum = checksum.wrapping_add(result.0.len() ^ result.1.len().rotate_left(5));
            black_box(result);
        }
        let result = rssp::bpm::clean_and_normalize_speeds_float_digits(black_box(&self.speeds));
        checksum = checksum.wrapping_add(result.0.len() ^ result.1.len().rotate_left(5));
        black_box(result);
        checksum
    }

    pub fn borrowed(&self) -> usize {
        let mut checksum = 0usize;
        for map in &self.pairs {
            let result = rssp::bpm::clean_norm_map_cow(black_box(map));
            checksum = checksum.wrapping_add(result.0.len() ^ result.1.len().rotate_left(5));
            black_box(result);
        }
        let result = rssp::bpm::clean_norm_speeds_cow(black_box(&self.speeds));
        checksum = checksum.wrapping_add(result.0.len() ^ result.1.len().rotate_left(5));
        black_box(result);
        checksum
    }
}

fn pair_map(map: usize) -> String {
    let mut out = String::with_capacity(ENTRIES * 18);
    for index in 0..ENTRIES {
        if index != 0 {
            out.push(',');
        }
        write!(
            &mut out,
            "{}={}.{}",
            index * 4,
            60 + (index * (map + 3)) % 300,
            map
        )
        .expect("writing to a String cannot fail");
    }
    out
}

fn speed_map() -> String {
    let mut out = String::with_capacity(ENTRIES * 24);
    for index in 0..ENTRIES {
        if index != 0 {
            out.push(',');
        }
        write!(
            &mut out,
            "{}={}.{}={}.{}={}",
            index * 4,
            1 + index % 7,
            index % 10,
            1 + index % 4,
            index % 10,
            index & 1
        )
        .expect("writing to a String cannot fail");
    }
    out
}

fn assert_pair(raw: &str) {
    let expected = rssp::bpm::clean_and_normalize_float_digits(raw);
    let actual = rssp::bpm::clean_norm_map_cow(raw);
    assert_eq!(actual.0.as_ref(), expected.0);
    assert_eq!(actual.1, expected.1);
}

fn assert_speed(raw: &str) {
    let expected = rssp::bpm::clean_and_normalize_speeds_float_digits(raw);
    let actual = rssp::bpm::clean_norm_speeds_cow(raw);
    assert_eq!(actual.0.as_ref(), expected.0);
    assert_eq!(actual.1, expected.1);
}

pub fn assert_behavior(maps: &TimingMaps) {
    for raw in [
        "",
        "0=120",
        "0=120,4=180",
        ",0=120,,4=180,",
        " \u{b}0=120\u{b} ",
    ] {
        assert_pair(raw);
    }
    for raw in ["", "0=1=0=0", "0=1=0=0,4=2=1=1", ",0=1=0=0,,"] {
        assert_speed(raw);
    }
    for raw in &maps.pairs {
        assert_pair(raw);
        assert!(matches!(
            rssp::bpm::clean_norm_map_cow(raw),
            (Cow::Borrowed(_), _)
        ));
    }
    assert_speed(&maps.speeds);
    assert!(matches!(
        rssp::bpm::clean_norm_speeds_cow(&maps.speeds),
        (Cow::Borrowed(_), _)
    ));
    assert!(matches!(
        rssp::bpm::clean_norm_map_cow(",0=120,,4=180,"),
        (Cow::Owned(_), _)
    ));
    assert_eq!(maps.owned(), maps.borrowed());
}

use std::path::PathBuf;

pub const PATH_COUNT: usize = 4_096;

pub fn paths() -> Vec<PathBuf> {
    let mut seed = 0x9e37_79b9u32;
    let mut paths = Vec::with_capacity(PATH_COUNT);
    for index in 0..PATH_COUNT {
        seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        let mut name = String::with_capacity(32);
        for shift in [0, 5, 10, 15, 20, 25] {
            let mut byte = b'a' + ((seed >> shift) % 26) as u8;
            if seed & (1 << (shift / 5)) != 0 {
                byte.make_ascii_uppercase();
            }
            name.push(byte as char);
        }
        name.push_str(&format!("-{index:04}-{:08x}.ssc", seed.rotate_left(11)));
        paths.push(PathBuf::from("Songs").join(name));
    }
    paths
}

fn assert_sorted(input: &[PathBuf]) {
    let mut legacy = input.to_vec();
    let mut direct = input.to_vec();
    let mut stable = input.to_vec();
    let mut in_place = input.to_vec();
    rssp::profile::sort_paths_ci(&mut legacy, true);
    rssp::profile::sort_paths_ci(&mut direct, false);
    rssp::profile::sort_paths_ci_in_place(&mut stable, false);
    rssp::profile::sort_paths_ci_in_place(&mut in_place, true);
    assert_eq!(direct, legacy);
    assert_eq!(in_place, stable);
    assert_eq!(in_place, direct);
}

pub fn assert_behavior() {
    assert_sorted(&[
        PathBuf::from("Songs/Alpha.ssc"),
        PathBuf::from("Songs/alpha.SM"),
        PathBuf::from("Songs/BETA.ssc"),
        PathBuf::from("Songs/beta.ssc"),
        PathBuf::from("Songs/éclair.ssc"),
        PathBuf::from("Songs/Äther.sm"),
    ]);
    assert_sorted(&paths());
}

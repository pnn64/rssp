pub const ENTRY_COUNT: usize = 4_096;

pub fn fixture() -> Vec<(f64, f64)> {
    (0..ENTRY_COUNT)
        .map(|index| {
            (
                index as f64 * 4.0,
                60.125 + ((index * 977) % 1_000) as f64 / 8.0,
            )
        })
        .collect()
}

pub fn compute(map: &[(f64, f64)], values: &mut Vec<f64>, legacy: bool) -> (i32, i32, f64, f64) {
    rssp::bpm::compute_bpm_summary_for_bench(map, values, legacy)
}

fn assert_same(map: &[(f64, f64)]) {
    let mut legacy_values = Vec::with_capacity(map.len());
    let mut current_values = Vec::with_capacity(map.len());
    let legacy = compute(map, &mut legacy_values, true);
    let current = compute(map, &mut current_values, false);
    assert_eq!(current.0, legacy.0);
    assert_eq!(current.1, legacy.1);
    assert_eq!(current.2.to_bits(), legacy.2.to_bits());
    assert_eq!(current.3.to_bits(), legacy.3.to_bits());
}

pub fn assert_behavior(map: &[(f64, f64)]) {
    assert_same(map);
    assert_same(&[]);
    assert_same(&[(0.0, 120.0), (4.0, 60.0), (8.0, 180.0)]);
    assert_same(
        &(0..64)
            .map(|index| (index as f64, (index * 37 % 211) as f64 + 0.25))
            .collect::<Vec<_>>(),
    );
    assert_same(&[
        (0.0, f64::NAN),
        (4.0, f64::INFINITY),
        (8.0, -120.0),
        (12.0, 10_000.0),
    ]);
}

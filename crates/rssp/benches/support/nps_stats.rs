pub const VALUE_COUNT: u64 = 16_385;

pub fn values() -> Vec<f64> {
    (0..VALUE_COUNT as usize)
        .map(|index| {
            if index % 17 == 0 {
                0.0
            } else {
                ((index * 37) % 2_003) as f64 / 13.0
            }
        })
        .collect()
}

fn assert_same(input: &[f64]) {
    let expected = rssp::bpm::get_nps_stats(input);
    let mut owned = input.to_vec();
    let actual = rssp::bpm::get_nps_stats_in_place(&mut owned);
    assert_eq!(
        (actual.0.to_bits(), actual.1.to_bits()),
        (expected.0.to_bits(), expected.1.to_bits())
    );
}

pub fn assert_behavior() {
    assert_same(&[]);
    assert_same(&[8.0, 2.0, 4.0, 16.0]);
    assert_same(&vec![7.5; 1_025]);

    let mut mostly_zero = vec![0.0; 1_025];
    mostly_zero[900] = 10.0;
    mostly_zero[1_024] = 20.0;
    assert_same(&mostly_zero);

    let mut special = values();
    special[3] = f64::INFINITY;
    special[7] = f64::NEG_INFINITY;
    special[11] = f64::NAN;
    assert_same(&special);
    assert_same(&values());
}

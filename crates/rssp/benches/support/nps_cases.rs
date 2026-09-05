// Cover the one/two-value fast paths, scratch storage, and median scan shortcuts.
pub fn cases() -> Vec<(String, Vec<f64>)> {
    let mut cases = Vec::new();
    for len in [1u32, 2, 3, 32, 63, 64, 65, 256, 4096] {
        let values = (0..len).map(|i| f64::from((i * 37) % 257)).collect();
        cases.push((format!("mixed_{len}"), values));
    }
    cases.push(("constant_32".into(), vec![7.5; 32]));
    cases.push(("constant_256".into(), vec![7.5; 256]));
    let mut sparse = vec![0.0; 256];
    sparse[0] = 12.0;
    sparse[255] = 18.0;
    cases.push(("sparse_256".into(), sparse));
    cases
}

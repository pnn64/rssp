use sha1::{Digest, Sha1};
use std::fmt::Write as FmtWrite;

/// Computes a short (first 16 hex characters) SHA-1 hash
/// for the given chart data + normalized BPMs.
pub fn compute_chart_hash(chart_data: &[u8], normalized_bpms: &str) -> String {
    let mut hasher = Sha1::new();
    hasher.update(chart_data);
    hasher.update(normalized_bpms.as_bytes());
    let hash = hasher.finalize();

    let mut hex = String::with_capacity(16);
    for byte in hash.iter().take(8) {
        write!(&mut hex, "{:02x}", byte).unwrap();
    }
    hex
}

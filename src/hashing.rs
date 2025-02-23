use sha1::{Digest, Sha1};

pub fn compute_chart_hash(chart_data: &[u8], normalized_bpms: &str) -> String {
    let mut hasher = Sha1::new();
    hasher.update(chart_data);
    hasher.update(normalized_bpms.as_bytes());
    hasher.finalize()[..8].iter().map(|b| format!("{:02x}", b)).collect()
}

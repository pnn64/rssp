use sha1::{Digest, Sha1};

pub fn compute_chart_hash(chart_data: &[u8], normalized_bpms: &str) -> String {
    let mut hasher = Sha1::new();
    hasher.update(chart_data);
    hasher.update(normalized_bpms.as_bytes());
    let digest = hasher.finalize();
    let mut out = String::with_capacity(16);
    const HEX: &[u8; 16] = b"0123456789abcdef";
    for &byte in digest[..8].iter() {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}

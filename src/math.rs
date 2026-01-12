const POW10: [f64; 19] = [
    1e0, 1e1, 1e2, 1e3, 1e4, 1e5, 1e6, 1e7, 1e8, 1e9,
    1e10, 1e11, 1e12, 1e13, 1e14, 1e15, 1e16, 1e17, 1e18,
];

#[inline(always)]
fn round_sig_figs_6_fmt(value: f64, fallback: f64) -> f64 {
    let formatted = format!("{:.5e}", value);
    formatted.parse::<f64>().unwrap_or(fallback)
}

#[inline(always)]
pub fn round_dp(value: f64, dp: usize) -> f64 {
    if !value.is_finite() {
        return value;
    }
    if dp < POW10.len() {
        let scale = POW10[dp];
        (value * scale).round_ties_even() / scale
    } else {
        // Fallback for dp >= 19 (rare)
        let scale = 10_f64.powi(dp as i32);
        (value * scale).round_ties_even() / scale
    }
}

#[inline(always)]
pub fn round_sig_figs_6(value: f64) -> f64 {
    if !value.is_finite() || value == 0.0 {
        return value;
    }
    round_sig_figs_6_fmt(value, value)
}

#[inline(always)]
pub fn round_sig_figs_itg(value: f64) -> f64 {
    if !value.is_finite() || value == 0.0 {
        return value;
    }
    round_sig_figs_6_fmt(value as f32 as f64, value)
}

#[inline(always)]
pub(crate) fn fmt_dec3_itg(value: f64) -> String {
    format!("{:.3}", (value as f32 * 1000.0).round() / 1000.0)
}

#[inline(always)]
pub(crate) fn fmt_dec3_half_up(value: f64) -> String {
    format!("{:.3}", ((value * 1000.0 + 0.5).floor()) / 1000.0)
}

#[inline(always)]
pub fn lrint_f64(v: f64) -> f64 {
    if !v.is_finite() { 0.0 } else { v.round_ties_even() }
}

#[inline(always)]
pub fn lrint_f32(v: f32) -> i32 {
    if !v.is_finite() { 0 } else { v.round_ties_even() as i32 }
}

#[inline(always)]
pub fn roundtrip_bpm_itg(bpm: f64) -> f64 {
    let bpm_f = bpm as f32;
    if bpm_f.is_finite() { (bpm_f / 60.0 * 60.0) as f64 } else { 0.0 }
}
use std::cmp::Ordering;

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
    let formatted = format!("{:.*}", dp, value);
    formatted.parse::<f64>().unwrap_or(value)
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
pub(crate) fn roundtrip_bpm_itg(bpm: f64) -> f64 {
    let bpm_f = bpm as f32;
    if !bpm_f.is_finite() {
        0.0
    } else {
        (bpm_f / 60.0 * 60.0) as f64
    }
}

#[inline(always)]
pub(crate) fn fmt_dec3_itg(value: f64) -> String {
    format!("{:.3}", (value as f32 * 1000.0).round() / 1000.0)
}

#[inline(always)]
pub(crate) fn fmt_dec3_half_up(value: f64) -> String {
    format!("{:.3}", ((value * 1000.0 + 0.5).floor()) / 1000.0)
}

// Rust 1.77+ version (preferred - uses CPU rounding instructions):
#[inline(always)]
pub(crate) fn lrint_f64(v: f64) -> f64 {
    if !v.is_finite() {
        return 0.0;
    }
    v.round_ties_even()
}

#[inline(always)]
pub(crate) fn lrint_f32(v: f32) -> i32 {
    if !v.is_finite() {
        return 0;
    }
    v.round_ties_even() as i32
}

#[inline(always)]
pub fn round_sig_figs_6(value: f64) -> f64 {
    if !value.is_finite() || value == 0.0 {
        return value;
    }
    let formatted = format!("{:.5e}", value);
    formatted.parse::<f64>().unwrap_or(value)
}

#[inline(always)]
pub fn round_2(value: f64) -> f64 {
    if !value.is_finite() {
        return value;
    }
    let formatted = format!("{:.2}", value);
    formatted.parse::<f64>().unwrap_or(value)
}

#[inline(always)]
pub fn round_3(value: f64) -> f64 {
    if !value.is_finite() {
        return value;
    }
    let formatted = format!("{:.3}", value);
    formatted.parse::<f64>().unwrap_or(value)
}

const POW10: [f64; 19] = [
    1e0, 1e1, 1e2, 1e3, 1e4, 1e5, 1e6, 1e7, 1e8, 1e9, 1e10, 1e11, 1e12, 1e13, 1e14, 1e15, 1e16,
    1e17, 1e18,
];

#[inline(always)]
fn round_sig_figs_6_impl(value: f64, fallback: f64) -> f64 {
    if value == 0.0 || !value.is_finite() {
        return value;
    }
    if let Some(scale) = sig_figs_6_scale(value.abs()) {
        let rounded = (value * scale).round_ties_even() / scale;
        return if rounded.is_finite() {
            rounded
        } else {
            fallback
        };
    }

    let magnitude = value.abs().log10().floor() as i32;
    let power = 5 - magnitude;

    if !(-300..=300).contains(&power) {
        return fallback;
    }

    let scale = 10f64.powi(power);
    let rounded = (value * scale).round_ties_even() / scale;

    if rounded.is_finite() {
        rounded
    } else {
        fallback
    }
}

#[inline(always)]
fn sig_figs_6_scale(abs: f64) -> Option<f64> {
    if !(1.0..1_000_000.0).contains(&abs) {
        return None;
    }
    Some(if abs < 10.0 {
        100_000.0
    } else if abs < 100.0 {
        10_000.0
    } else if abs < 1_000.0 {
        1_000.0
    } else if abs < 10_000.0 {
        100.0
    } else if abs < 100_000.0 {
        10.0
    } else {
        1.0
    })
}

#[inline(always)]
#[must_use]
pub fn round_dp(value: f64, dp: usize) -> f64 {
    if !value.is_finite() {
        return value;
    }
    if dp < POW10.len() {
        let scale = POW10[dp];
        (value * scale).round_ties_even() / scale
    } else {
        let scale = 10_f64.powi(i32::try_from(dp).unwrap_or(0));
        (value * scale).round_ties_even() / scale
    }
}

#[inline(always)]
#[must_use]
pub fn round_sig_figs_6(value: f64) -> f64 {
    round_sig_figs_6_impl(value, value)
}

#[inline(always)]
#[must_use]
pub fn round_sig_figs_itg(value: f64) -> f64 {
    round_sig_figs_6_impl(f64::from(value as f32), value)
}

#[inline(always)]
pub fn fmt_dec6_itg(value: f64) -> String {
    format!("{:.6}", value as f32)
}

#[inline]
pub(crate) fn fmt_dec3_half_up(value: f64) -> String {
    let mut out = String::with_capacity(16);
    push_dec3_half_up(&mut out, value);
    out
}

pub(crate) fn push_dec3_half_up(out: &mut String, value: f64) {
    let scaled = value.mul_add(1000.0, 0.5).floor();
    if !scaled.is_finite() || scaled <= i64::MIN as f64 || scaled >= i64::MAX as f64 {
        out.push_str(&format!("{:.3}", scaled / 1000.0));
        return;
    }

    let n = scaled as i64;
    let neg = n < 0;
    let abs = if neg { (-n) as u64 } else { n as u64 };
    let whole = abs / 1000;
    let frac = abs % 1000;

    if neg {
        out.push('-');
    }
    push_u64(out, whole);
    out.push('.');
    out.push(char::from(b'0' + (frac / 100) as u8));
    out.push(char::from(b'0' + ((frac / 10) % 10) as u8));
    out.push(char::from(b'0' + (frac % 10) as u8));
}

fn push_u64(out: &mut String, mut n: u64) {
    if n == 0 {
        out.push('0');
        return;
    }

    let mut buf = [0u8; 20];
    let mut i = buf.len();
    while n != 0 {
        i -= 1;
        buf[i] = b'0' + (n % 10) as u8;
        n /= 10;
    }
    for &b in &buf[i..] {
        out.push(char::from(b));
    }
}

#[inline(always)]
#[must_use]
pub const fn lrint_f64(v: f64) -> f64 {
    if v.is_finite() {
        v.round_ties_even()
    } else {
        0.0
    }
}

#[inline(always)]
#[must_use]
pub const fn lrint_f32(v: f32) -> i32 {
    if v.is_finite() {
        v.round_ties_even() as i32
    } else {
        0
    }
}

#[inline(always)]
#[must_use]
pub fn roundtrip_bpm_itg(bpm: f64) -> f64 {
    let bpm_f = bpm as f32;
    if bpm_f.is_finite() {
        f64::from(bpm_f / 60.0 * 60.0)
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::{fmt_dec3_half_up, round_sig_figs_6};

    #[test]
    fn round_sig_figs_common_range() {
        assert_eq!(round_sig_figs_6(1.2345678), 1.23457);
        assert_eq!(round_sig_figs_6(12.345678), 12.3457);
        assert_eq!(round_sig_figs_6(123.45678), 123.457);
        assert_eq!(round_sig_figs_6(1234.5678), 1234.57);
        assert_eq!(round_sig_figs_6(12345.678), 12345.7);
        assert_eq!(round_sig_figs_6(123456.78), 123457.0);
    }

    #[test]
    fn round_sig_figs_keeps_fallback_range() {
        assert_eq!(round_sig_figs_6(0.12345678), 0.123457);
        assert_eq!(round_sig_figs_6(1_234_567.8), 1_234_570.0);
    }

    #[test]
    fn dec3_half_up_matches_format() {
        let values: [f64; 12] = [
            0.0,
            -0.0,
            0.0004,
            0.0005,
            -0.0004,
            -0.0005,
            1.2344,
            1.2345,
            -1.2344,
            -1.2345,
            12_345.6789,
            -12_345.6789,
        ];

        for value in values {
            let rounded = (value.mul_add(1000.0, 0.5).floor()) / 1000.0;
            assert_eq!(fmt_dec3_half_up(value), format!("{rounded:.3}"));
        }
    }
}

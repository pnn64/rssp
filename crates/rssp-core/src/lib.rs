// These lints conflict with deliberate hot-path codegen, exact ITG-compatible
// float behavior, range-constrained chart values, or the stable public API.
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::float_cmp,
    clippy::inline_always,
    clippy::too_many_arguments,
    clippy::trivially_copy_pass_by_ref
)]

pub mod bpm;
pub mod duration;
pub mod hash;
pub mod math;
pub mod matrix;
pub mod meta;
pub mod nps;
pub mod parse;
pub mod patterns;
pub mod stats;
pub mod step_parity;
pub mod streams;
pub mod tech;
pub mod timing;

pub use bpm::chart_timing_tag_raw;
pub use duration::{ChartDuration, TimingOffsets, compute_chart_durations};
pub use meta::{
    normalize_difficulty_label, resolve_difficulty_label, step_type_lanes,
    supported_stepstype_lanes_bytes,
};
pub use nps::{ChartNpsInfo, compute_chart_peak_nps};
pub use step_parity::{Foot, RowAnnotation, TechCounts};

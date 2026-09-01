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

pub mod analysis;
pub mod assets;
pub mod course;
pub mod pack;
pub mod report;
pub mod serialize;
pub mod simfile;
pub mod translate;

pub use rssp_core::{
    bpm, duration, hash, math, matrix, nps, parse, patterns, stats, step_parity, streams, tech,
    timing,
};
pub use rssp_core::{
    normalize_difficulty_label, resolve_difficulty_label, step_type_lanes,
    supported_stepstype_lanes_bytes,
};

pub mod rounding {
    pub use rssp_core::math::{round_dp, round_sig_figs_6, round_sig_figs_itg};
}

pub const RSSP_VERSION: &str = env!("CARGO_PKG_VERSION");

pub use analysis::{
    AnalysisOptions, AnalysisScratch, ChartHashInfo, ChartNoteType, ParsedChartNote,
    PreparedAnalysis, analyze, analyze_prepared_in, analyze_prepared_in_with_notes,
    analyze_with_scratch, compute_all_hashes, display_metadata,
};
pub(crate) use rssp_core::chart_timing_tag_raw;

pub use report::{ChartSummary, SimfileSummary};
pub use report::{CourseEntrySummary, CourseSummary};
pub use rssp_core::{ChartDuration, ChartNpsInfo, Foot, RowAnnotation, TechCounts, TimingOffsets};
pub use rssp_core::{compute_chart_durations, compute_chart_peak_nps};

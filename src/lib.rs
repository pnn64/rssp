pub mod analysis;
pub mod assets;
pub mod bpm;
pub mod course;
pub mod duration;
pub mod graph;
pub mod hash;
pub mod math;
pub mod matrix;
pub mod nps;
pub mod parse;
pub mod patterns;
pub mod pack;
pub mod report;
pub mod simfile;
pub mod stats;
pub mod step_parity;
pub mod streams;
pub mod tech;
pub mod timing;
pub mod translate;

pub mod rounding {
    pub use crate::math::{round_dp, round_sig_figs_6, round_sig_figs_itg};
}

pub const RSSP_VERSION: &str = env!("CARGO_PKG_VERSION");

pub(crate) use analysis::chart_timing_tag_raw;
pub use analysis::{
    AnalysisOptions, ChartHashInfo, ChartNpsInfo, analyze, compute_all_hashes, display_metadata,
    normalize_difficulty_label, resolve_difficulty_label, step_type_lanes,
};

pub use duration::{ChartDuration, TimingOffsets, compute_chart_durations};
pub use nps::compute_chart_peak_nps;
pub use report::{ChartSummary, SimfileSummary};
pub use report::{CourseEntrySummary, CourseSummary};
pub use step_parity::TechCounts;

pub mod analysis;
pub mod assets;
pub mod course;
pub mod pack;
pub mod report;
pub mod simfile;
pub mod translate;

#[cfg(feature = "profile")]
#[doc(hidden)]
pub mod profile {
    use std::path::{Path, PathBuf};

    #[must_use]
    pub fn first_path_ci(paths: &[PathBuf]) -> Option<&Path> {
        let mut first = None;
        for path in paths {
            let key = crate::assets::lc_name(path);
            if first.as_ref().is_none_or(|(first_key, _)| key < *first_key) {
                first = Some((key, path.as_path()));
            }
        }
        first.map(|(_, path)| path)
    }

    #[must_use]
    pub fn match_mask_ci(name: &str, mask: &str) -> bool {
        crate::assets::match_mask_ci(name, mask)
    }
}

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

pub use analysis::{AnalysisOptions, ChartHashInfo, analyze, compute_all_hashes, display_metadata};
pub(crate) use rssp_core::chart_timing_tag_raw;

pub use report::{ChartSummary, SimfileSummary};
pub use report::{CourseEntrySummary, CourseSummary};
pub use rssp_core::{ChartDuration, ChartNpsInfo, Foot, RowAnnotation, TechCounts, TimingOffsets};
pub use rssp_core::{compute_chart_durations, compute_chart_peak_nps};

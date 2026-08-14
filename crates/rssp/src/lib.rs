pub mod analysis;
pub mod assets;
pub mod course;
pub mod pack;
pub mod report;
pub mod serialize;
pub mod simfile;
pub mod translate;

#[cfg(feature = "profile")]
#[doc(hidden)]
pub mod profile {
    use std::ffi::OsStr;
    use std::path::{Path, PathBuf};

    use crate::pack::{ScanError, ScanOpt, SongScan};

    pub type PackRootResult = (Option<PathBuf>, Option<PathBuf>, Vec<SongScan>);

    #[must_use]
    pub fn first_path_ci(paths: &[PathBuf]) -> Option<&Path> {
        paths
            .iter()
            .map(PathBuf::as_path)
            .min_by(|left, right| crate::assets::cmp_name_ci(left, right))
    }

    #[must_use]
    pub fn first_two_paths_ci(paths: &[PathBuf]) -> (Option<&Path>, Option<&Path>) {
        let mut first = None;
        let mut second = None;
        for path in paths.iter().map(PathBuf::as_path) {
            if first.is_none_or(|candidate| {
                crate::assets::cmp_name_ci(path, candidate) == std::cmp::Ordering::Less
            }) {
                second = first.replace(path);
            } else if second.is_none_or(|candidate| {
                crate::assets::cmp_name_ci(path, candidate) == std::cmp::Ordering::Less
            }) {
                second = Some(path);
            }
        }
        (first, second)
    }

    #[must_use]
    pub fn match_mask_ci(name: &str, mask: &str) -> bool {
        crate::assets::match_mask_ci(name, mask)
    }

    #[must_use]
    pub fn file_ci(dir: &Path, name: &str) -> Option<PathBuf> {
        crate::assets::is_file_ci(dir, name)
    }

    #[must_use]
    pub fn name_eq_ci(actual: &OsStr, expected: &str) -> bool {
        crate::assets::name_eq_ci(actual, expected)
    }

    pub fn pack_root(
        dir: &Path,
        opt: ScanOpt,
        banner: &str,
        background: &str,
    ) -> Result<PackRootResult, ScanError> {
        crate::pack::profile_pack_root(dir, opt, banner, background, false)
    }

    pub fn pack_root_legacy(
        dir: &Path,
        opt: ScanOpt,
        banner: &str,
        background: &str,
    ) -> Result<PackRootResult, ScanError> {
        crate::pack::profile_pack_root(dir, opt, banner, background, true)
    }

    #[must_use]
    pub fn background_changes_legacy(
        song_dir: &Path,
        simfile_data: &[u8],
    ) -> Vec<crate::assets::ResolvedBackgroundChange> {
        crate::assets::profile_bgchanges_legacy(song_dir, simfile_data)
    }

    #[must_use]
    pub fn background_changes_double_find(
        song_dir: &Path,
        simfile_data: &[u8],
    ) -> Vec<crate::assets::ResolvedBackgroundChange> {
        crate::assets::profile_bgchanges_double_find(song_dir, simfile_data)
    }

    #[must_use]
    #[inline]
    pub fn bg_delimiter(rem: &str) -> Option<usize> {
        crate::assets::profile_find_bg_delimiter(rem)
    }

    #[must_use]
    #[inline]
    pub fn bg_delimiter_legacy(rem: &str) -> Option<usize> {
        crate::assets::profile_find_bg_delimiter_legacy(rem)
    }

    pub fn write_json_materialized<W: std::io::Write>(
        simfile: &crate::report::SimfileSummary,
        writer: &mut W,
    ) -> std::io::Result<()> {
        crate::report::profile_write_json_materialized(simfile, writer)
    }

    pub fn write_json_timing<W: std::io::Write>(
        writer: &mut W,
        chart: &crate::report::ChartSummary,
        simfile: &crate::report::SimfileSummary,
    ) -> std::io::Result<()> {
        crate::report::profile_write_json_timing::<W, false>(writer, chart, simfile)
    }

    pub fn write_json_timing_materialized<W: std::io::Write>(
        writer: &mut W,
        chart: &crate::report::ChartSummary,
        simfile: &crate::report::SimfileSummary,
    ) -> std::io::Result<()> {
        crate::report::profile_write_json_timing::<W, true>(writer, chart, simfile)
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

pub use analysis::{
    AnalysisOptions, AnalysisScratch, ChartHashInfo, analyze, analyze_with_scratch,
    compute_all_hashes, display_metadata,
};
pub(crate) use rssp_core::chart_timing_tag_raw;

pub use report::{ChartSummary, SimfileSummary};
pub use report::{CourseEntrySummary, CourseSummary};
pub use rssp_core::{ChartDuration, ChartNpsInfo, Foot, RowAnnotation, TechCounts, TimingOffsets};
pub use rssp_core::{compute_chart_durations, compute_chart_peak_nps};

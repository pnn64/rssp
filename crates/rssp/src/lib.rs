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

    pub fn sort_paths_ci(paths: &mut [PathBuf], legacy: bool) {
        crate::pack::profile_sort_paths_ci(paths, legacy);
    }

    pub fn sort_paths_ci_in_place(paths: &mut [PathBuf], in_place: bool) {
        crate::pack::profile_sort_paths_ci_in_place(paths, in_place);
    }

    pub fn sort_bg_files(files: &mut [String], in_place: bool) {
        crate::assets::profile_sort_bg_files(files, in_place);
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
    pub fn relative_asset_path(dir: &Path, rel: &str, legacy: bool) -> Option<PathBuf> {
        crate::assets::profile_resolve_rel_ci(dir, rel, legacy)
    }

    #[must_use]
    pub fn relative_asset_parts_hash(rel: &str, legacy: bool) -> u64 {
        crate::assets::profile_rel_parts_hash(rel, legacy)
    }

    #[must_use]
    pub fn relative_asset_parts_match(rel: &str) -> bool {
        crate::assets::profile_rel_parts_match(rel)
    }

    #[must_use]
    pub fn relative_path_join(base: &Path, relative: &str, prealloc: bool) -> PathBuf {
        crate::assets::profile_join_rel(base, relative, prealloc)
    }

    #[must_use]
    pub fn song_assets_legacy(
        song_dir: &Path,
        banner: &str,
        background: &str,
    ) -> (Option<PathBuf>, Option<PathBuf>) {
        crate::assets::profile_resolve_song_assets_legacy(song_dir, banner, background)
    }

    #[must_use]
    pub fn music_path_legacy(song_dir: &Path, music_tag: &str) -> Option<PathBuf> {
        crate::assets::profile_resolve_music_path_legacy(song_dir, music_tag)
    }

    #[must_use]
    pub fn name_eq_ci(actual: &OsStr, expected: &str) -> bool {
        crate::assets::name_eq_ci(actual, expected)
    }

    pub fn merge_course_patterns_legacy(
        total: &mut Vec<crate::patterns::CustomPatternSummary>,
        chart: &[crate::patterns::CustomPatternSummary],
    ) {
        crate::course::profile_merge_custom_patterns_legacy(total, chart);
    }

    pub fn merge_course_patterns(
        total: &mut Vec<crate::patterns::CustomPatternSummary>,
        chart: &[crate::patterns::CustomPatternSummary],
    ) {
        crate::course::profile_merge_custom_patterns(total, chart);
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

    pub fn pack_root_full_paths(
        dir: &Path,
        opt: ScanOpt,
        banner: &str,
        background: &str,
    ) -> Result<PackRootResult, ScanError> {
        crate::pack::profile_pack_root_full_paths(dir, opt, banner, background)
    }

    pub fn scan_song_dir_full_paths(
        dir: &Path,
        opt: ScanOpt,
    ) -> Result<Option<SongScan>, ScanError> {
        crate::pack::profile_scan_song_dir_full_paths(dir, opt)
    }

    pub fn scan_song_dir_joined_paths(
        dir: &Path,
        opt: ScanOpt,
    ) -> Result<Option<SongScan>, ScanError> {
        crate::pack::profile_scan_song_dir_joined_paths(dir, opt)
    }

    pub fn scan_song_dir_growing_names(
        dir: &Path,
        opt: ScanOpt,
    ) -> Result<Option<SongScan>, ScanError> {
        crate::pack::profile_scan_song_dir_growing_names(dir, opt)
    }

    #[must_use]
    pub fn find_simfiles_legacy(root: &Path, opt: ScanOpt) -> Vec<PathBuf> {
        crate::pack::profile_find_simfiles_legacy(root, opt)
    }

    pub fn scan_songs_dir_legacy(
        dir: &Path,
        opt: ScanOpt,
    ) -> Result<Vec<crate::pack::PackScan>, ScanError> {
        crate::pack::profile_scan_songs_dir_legacy(dir, opt)
    }

    #[must_use]
    pub fn pack_parent_img(pack_dir: &Path, group_name: &str) -> Option<PathBuf> {
        crate::pack::profile_pick_pack_parent_img(pack_dir, group_name, false)
    }

    #[must_use]
    pub fn pack_parent_img_legacy(pack_dir: &Path, group_name: &str) -> Option<PathBuf> {
        crate::pack::profile_pick_pack_parent_img(pack_dir, group_name, true)
    }

    #[must_use]
    pub fn pack_subdir_img(pack_dir: &Path, hint: &str) -> Option<PathBuf> {
        crate::pack::profile_pick_subdir_img(pack_dir, hint, false)
    }

    #[must_use]
    pub fn pack_subdir_img_legacy(pack_dir: &Path, hint: &str) -> Option<PathBuf> {
        crate::pack::profile_pick_subdir_img(pack_dir, hint, true)
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
    pub fn background_changes_materialized(
        song_dir: &Path,
        simfile_data: &[u8],
    ) -> Vec<crate::assets::ResolvedBackgroundChange> {
        crate::assets::profile_bgchanges_materialized(song_dir, simfile_data)
    }

    #[must_use]
    pub fn background_changes_linear_upsert(
        song_dir: &Path,
        simfile_data: &[u8],
    ) -> Vec<crate::assets::ResolvedBackgroundChange> {
        crate::assets::profile_bgchanges_linear_upsert(song_dir, simfile_data)
    }

    #[must_use]
    pub fn background_changes_path_metadata(
        song_dir: &Path,
        simfile_data: &[u8],
    ) -> Vec<crate::assets::ResolvedBackgroundChange> {
        crate::assets::profile_bgchanges_path_metadata(song_dir, simfile_data)
    }

    #[must_use]
    pub fn background_changes_always_sort(
        song_dir: &Path,
        simfile_data: &[u8],
    ) -> Vec<crate::assets::ResolvedBackgroundChange> {
        crate::assets::profile_bgchanges_always_sort(song_dir, simfile_data)
    }

    #[must_use]
    pub fn background_changes_growing_paths(
        song_dir: &Path,
        simfile_data: &[u8],
    ) -> Vec<crate::assets::ResolvedBackgroundChange> {
        crate::assets::profile_bgchanges_growing_paths(song_dir, simfile_data)
    }

    #[must_use]
    pub fn background_changes_catalog_sort(
        song_dir: &Path,
        simfile_data: &[u8],
        in_place: bool,
    ) -> Vec<crate::assets::ResolvedBackgroundChange> {
        crate::assets::profile_bgchanges_catalog_sort(song_dir, simfile_data, in_place)
    }

    pub fn sort_background_changes(
        changes: &mut [crate::assets::ResolvedBackgroundChange],
        beats_ordered: bool,
        legacy: bool,
    ) {
        crate::assets::profile_sort_bgchanges(changes, beats_ordered, legacy);
    }

    pub fn analyze_with_allocating_bpms(
        simfile_data: &[u8],
        extension: &str,
        options: &crate::AnalysisOptions,
        scratch: &mut crate::AnalysisScratch,
    ) -> Result<crate::SimfileSummary, String> {
        crate::analysis::profile_analyze_with_allocating_bpms(
            simfile_data,
            extension,
            options,
            scratch,
        )
    }

    pub fn analyze_owned_timing(
        simfile_data: &[u8],
        extension: &str,
        options: &crate::AnalysisOptions,
        scratch: &mut crate::AnalysisScratch,
    ) -> Result<crate::SimfileSummary, String> {
        crate::analysis::profile_analyze_owned(simfile_data, extension, options, scratch)
    }

    #[must_use]
    pub fn selectable(tag: Option<&[u8]>, legacy: bool) -> bool {
        crate::analysis::profile_selectable(tag, legacy)
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

    pub fn write_text_report<W: std::io::Write>(
        simfile: &crate::report::SimfileSummary,
        writer: &mut W,
        full: bool,
        legacy: bool,
    ) -> std::io::Result<()> {
        crate::report::profile_write_text(simfile, writer, full, legacy)
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

    pub type TimingText = (
        Vec<(f64, i32, i32)>,
        Vec<(f64, String)>,
        Vec<(f64, i32)>,
        Vec<(f64, i32, i32)>,
    );

    #[must_use]
    pub fn timing_text(
        time_signatures: &str,
        labels: &str,
        tickcounts: &str,
        combos: &str,
        legacy: bool,
    ) -> TimingText {
        crate::report::profile_timing_text(time_signatures, labels, tickcounts, combos, legacy)
    }

    pub fn write_json_bpm_text_report_materialized<W: std::io::Write>(
        simfile: &crate::report::SimfileSummary,
        writer: &mut W,
    ) -> std::io::Result<()> {
        crate::report::profile_write_json_bpm_text_materialized(simfile, writer)
    }

    pub fn write_json_bpm_text<W: std::io::Write>(
        writer: &mut W,
        chart: &crate::report::ChartSummary,
        simfile: &crate::report::SimfileSummary,
    ) -> std::io::Result<()> {
        crate::report::profile_write_json_bpm_text::<W, false>(writer, chart, simfile)
    }

    pub fn write_json_bpm_text_materialized<W: std::io::Write>(
        writer: &mut W,
        chart: &crate::report::ChartSummary,
        simfile: &crate::report::SimfileSummary,
    ) -> std::io::Result<()> {
        crate::report::profile_write_json_bpm_text::<W, true>(writer, chart, simfile)
    }

    pub fn write_json_nps_report_materialized<W: std::io::Write>(
        simfile: &crate::report::SimfileSummary,
        writer: &mut W,
    ) -> std::io::Result<()> {
        crate::report::profile_write_json_nps_materialized(simfile, writer)
    }

    pub fn write_json_nps<W: std::io::Write>(
        writer: &mut W,
        chart: &crate::report::ChartSummary,
    ) -> std::io::Result<()> {
        crate::report::profile_write_json_nps::<W, false>(writer, chart)
    }

    pub fn write_json_nps_materialized<W: std::io::Write>(
        writer: &mut W,
        chart: &crate::report::ChartSummary,
    ) -> std::io::Result<()> {
        crate::report::profile_write_json_nps::<W, true>(writer, chart)
    }

    pub fn write_json_streams_report_materialized<W: std::io::Write>(
        simfile: &crate::report::SimfileSummary,
        writer: &mut W,
    ) -> std::io::Result<()> {
        crate::report::profile_write_json_streams_materialized(simfile, writer)
    }

    pub fn write_json_streams<W: std::io::Write>(
        writer: &mut W,
        chart: &crate::report::ChartSummary,
    ) -> std::io::Result<()> {
        crate::report::profile_write_json_streams::<W, false>(writer, chart)
    }

    pub fn write_json_streams_materialized<W: std::io::Write>(
        writer: &mut W,
        chart: &crate::report::ChartSummary,
    ) -> std::io::Result<()> {
        crate::report::profile_write_json_streams::<W, true>(writer, chart)
    }

    pub fn write_json_custom_report_materialized<W: std::io::Write>(
        simfile: &crate::report::SimfileSummary,
        writer: &mut W,
    ) -> std::io::Result<()> {
        crate::report::profile_write_json_custom_report_materialized(simfile, writer)
    }

    pub fn write_json_custom_patterns<W: std::io::Write>(
        writer: &mut W,
        chart: &crate::report::ChartSummary,
    ) -> std::io::Result<()> {
        crate::report::profile_write_json_pattern_counts::<W, false>(writer, chart)
    }

    pub fn write_json_custom_patterns_materialized<W: std::io::Write>(
        writer: &mut W,
        chart: &crate::report::ChartSummary,
    ) -> std::io::Result<()> {
        crate::report::profile_write_json_pattern_counts::<W, true>(writer, chart)
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
    AnalysisOptions, AnalysisScratch, ChartHashInfo, ChartNoteType, ParsedChartNote,
    PreparedAnalysis, analyze, analyze_prepared_in, analyze_prepared_in_with_notes,
    analyze_with_scratch, compute_all_hashes, display_metadata,
};
pub(crate) use rssp_core::chart_timing_tag_raw;

pub use report::{ChartSummary, SimfileSummary};
pub use report::{CourseEntrySummary, CourseSummary};
pub use rssp_core::{ChartDuration, ChartNpsInfo, Foot, RowAnnotation, TechCounts, TimingOffsets};
pub use rssp_core::{compute_chart_durations, compute_chart_peak_nps};

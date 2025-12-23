use crate::bpm::{normalize_float_digits, parse_bpm_map};
use std::cmp::Ordering;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimingFormat {
    Sm,
    Ssc,
}

impl TimingFormat {
    pub fn from_extension(extension: &str) -> Self {
        if extension.eq_ignore_ascii_case("sm") {
            Self::Sm
        } else {
            Self::Ssc
        }
    }
}

const DEFAULT_BPM: f64 = 60.0;
const FAST_BPM_WARP: f64 = 9_999_999.0;

pub const ROWS_PER_BEAT: i32 = 48;

#[inline(always)]
fn note_row_to_beat(row: i32) -> f64 {
    row as f64 / ROWS_PER_BEAT as f64
}

#[inline(always)]
fn beat_to_note_row(beat: f64) -> i32 {
    (beat * ROWS_PER_BEAT as f64).round() as i32
}

pub fn compute_row_to_beat(minimized_note_data: &[u8]) -> Vec<f32> {
    let mut row_to_beat = Vec::new();
    let mut measure_index = 0usize;

    for measure_bytes in minimized_note_data.split(|&b| b == b',') {
        let num_rows_in_measure = measure_bytes
            .split(|&b| b == b'\n')
            .filter(|line| {
                let trimmed = line.strip_suffix(b"\r").unwrap_or(line);
                !trimmed.is_empty()
                    && !trimmed.iter().all(|c| c.is_ascii_whitespace())
            })
            .count();
        if num_rows_in_measure == 0 {
            continue;
        }

        let rows = num_rows_in_measure as f32;
        let measure_start = measure_index as f32 * 4.0;
        for row_in_measure in 0..num_rows_in_measure {
            let beat = measure_start + (row_in_measure as f32 / rows * 4.0);
            row_to_beat.push(beat);
        }
        measure_index += 1;
    }

    row_to_beat
}

fn parse_optional_timing<T, F>(chart_val: Option<&str>, global_val: &str, parser: F) -> Vec<T>
where
    F: Fn(&str) -> Result<Vec<T>, &'static str>,
{
    let s = chart_val.filter(|s| !s.is_empty()).unwrap_or(global_val);
    parser(s).unwrap_or_else(|_| vec![])
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SpeedUnit {
    Beats,
    Seconds,
}

#[derive(Debug, Clone)]
pub struct TimingSegments {
    pub beat0_offset_adjust: f32,
    pub bpms: Vec<(f32, f32)>,
    pub stops: Vec<(f32, f32)>,
    pub delays: Vec<(f32, f32)>,
    pub warps: Vec<(f32, f32)>,
    pub speeds: Vec<(f32, f32, f32, SpeedUnit)>,
    pub scrolls: Vec<(f32, f32)>,
    pub fakes: Vec<(f32, f32)>,
}

#[allow(clippy::too_many_arguments)]
pub fn compute_timing_segments(
    chart_bpms: Option<&str>,
    global_bpms: &str,
    chart_stops: Option<&str>,
    global_stops: &str,
    chart_delays: Option<&str>,
    global_delays: &str,
    chart_warps: Option<&str>,
    global_warps: &str,
    chart_speeds: Option<&str>,
    global_speeds: &str,
    chart_scrolls: Option<&str>,
    global_scrolls: &str,
    chart_fakes: Option<&str>,
    global_fakes: &str,
    format: TimingFormat,
) -> TimingSegments {
    let bpms_str = chart_bpms.filter(|s| !s.is_empty()).unwrap_or(global_bpms);
    let normalized_bpms = normalize_float_digits(bpms_str);
    let mut parsed_bpms: Vec<(f64, f64)> = parse_bpm_map(&normalized_bpms);

    if parsed_bpms.is_empty() {
        parsed_bpms.push((0.0, DEFAULT_BPM));
    }

    let raw_stops = parse_optional_timing(chart_stops, global_stops, parse_stops);
    let (mut parsed_bpms, stops, extra_warps, beat0_offset_adjust) =
        process_bpms_and_stops(format, &parsed_bpms, &raw_stops);

    if parsed_bpms.is_empty() {
        parsed_bpms.push((0.0, DEFAULT_BPM));
    }

    let delays = parse_optional_timing(chart_delays, global_delays, parse_delays);
    let mut warps = parse_optional_timing(chart_warps, global_warps, parse_warps);
    warps.extend(extra_warps);
    let mut speeds = parse_optional_timing(chart_speeds, global_speeds, parse_speeds);
    let mut scrolls = parse_optional_timing(chart_scrolls, global_scrolls, parse_scrolls);
    let mut fakes = parse_optional_timing(chart_fakes, global_fakes, parse_fakes);

    speeds.sort_by(|a, b| a.beat.partial_cmp(&b.beat).unwrap_or(Ordering::Less));
    scrolls.sort_by(|a, b| a.beat.partial_cmp(&b.beat).unwrap_or(Ordering::Less));
    warps.sort_by(|a, b| a.beat.partial_cmp(&b.beat).unwrap_or(Ordering::Less));
    fakes.sort_by(|a, b| a.beat.partial_cmp(&b.beat).unwrap_or(Ordering::Less));

    TimingSegments {
        beat0_offset_adjust: beat0_offset_adjust as f32,
        bpms: parsed_bpms
            .iter()
            .map(|(beat, bpm)| (*beat as f32, *bpm as f32))
            .collect(),
        stops: stops
            .iter()
            .map(|seg| (seg.beat as f32, seg.duration as f32))
            .collect(),
        delays: delays
            .iter()
            .map(|seg| (seg.beat as f32, seg.duration as f32))
            .collect(),
        warps: warps
            .iter()
            .map(|seg| (seg.beat as f32, seg.length as f32))
            .collect(),
        speeds: speeds
            .iter()
            .map(|seg| {
                (
                    seg.beat as f32,
                    seg.ratio as f32,
                    seg.delay as f32,
                    seg.unit,
                )
            })
            .collect(),
        scrolls: scrolls
            .iter()
            .map(|seg| (seg.beat as f32, seg.ratio as f32))
            .collect(),
        fakes: fakes
            .iter()
            .map(|seg| (seg.beat as f32, seg.length as f32))
            .collect(),
    }
}

#[derive(Debug, Clone, Copy)]
pub struct StopSegment {
    pub beat: f64,
    pub duration: f64,
}

#[derive(Debug, Clone, Copy)]
pub struct DelaySegment {
    pub beat: f64,
    pub duration: f64,
}

#[derive(Debug, Clone, Copy)]
pub struct WarpSegment {
    pub beat: f64,
    pub length: f64,
}

#[derive(Debug, Clone, Copy)]
pub struct SpeedSegment {
    pub beat: f64,
    pub ratio: f64,
    pub delay: f64,
    pub unit: SpeedUnit,
}

#[derive(Debug, Clone, Copy)]
pub struct ScrollSegment {
    pub beat: f64,
    pub ratio: f64,
}

#[derive(Debug, Clone, Copy)]
pub struct FakeSegment {
    pub beat: f64,
    pub length: f64,
}

#[derive(Debug, Clone, Copy)]
struct SpeedRuntime {
    start_time: f64,
    end_time: f64,
    prev_ratio: f64,
}

#[derive(Debug, Clone, Copy)]
struct ScrollPrefix {
    beat: f64,
    cum_displayed: f64,
    ratio: f64,
}

#[derive(Debug, Clone, Copy)]
struct BeatTimePoint {
    beat: f64,
    time_sec: f64,
    bpm: f64,
}

#[derive(Debug, Clone, Copy)]
struct GetBeatStarts {
    bpm_idx: usize,
    stop_idx: usize,
    delay_idx: usize,
    warp_idx: usize,
    last_row: i32,
    last_time: f64,
    warp_destination: f64,
    is_warping: bool,
}

impl Default for GetBeatStarts {
    fn default() -> Self {
        Self {
            bpm_idx: 0,
            stop_idx: 0,
            delay_idx: 0,
            warp_idx: 0,
            last_row: 0,
            last_time: 0.0,
            warp_destination: 0.0,
            is_warping: false,
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct GetBeatArgs {
    pub elapsed_time: f64,
    pub beat: f64,
    pub bps_out: f64,
    pub warp_dest_out: f64,
    pub warp_begin_out: i32,
    pub freeze_out: bool,
    pub delay_out: bool,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct BeatInfo {
    pub beat: f64,
    pub is_in_freeze: bool,
    pub is_in_delay: bool,
}

#[derive(PartialEq, Eq)]
enum TimingEvent {
    Bpm,
    Stop,
    Delay,
    StopDelay,
    Warp,
    WarpDest,
    Marker,
    NotFound,
}

#[derive(Debug, Clone, Default)]
pub struct TimingData {
    beat_to_time: Vec<BeatTimePoint>,
    stops: Vec<StopSegment>,
    delays: Vec<DelaySegment>,
    warps: Vec<WarpSegment>,
    speeds: Vec<SpeedSegment>,
    scrolls: Vec<ScrollSegment>,
    fakes: Vec<FakeSegment>,
    speed_runtime: Vec<SpeedRuntime>,
    scroll_prefix: Vec<ScrollPrefix>,
    global_offset_sec: f64,
    max_bpm: f64,
}

impl TimingData {
    #[allow(clippy::too_many_arguments)]
    pub fn from_chart_data(
        song_offset_sec: f64,
        global_offset_sec: f64,
        chart_bpms: Option<&str>,
        global_bpms: &str,
        chart_stops: Option<&str>,
        global_stops: &str,
        chart_delays: Option<&str>,
        global_delays: &str,
        chart_warps: Option<&str>,
        global_warps: &str,
        chart_speeds: Option<&str>,
        global_speeds: &str,
        chart_scrolls: Option<&str>,
        global_scrolls: &str,
        chart_fakes: Option<&str>,
        global_fakes: &str,
        format: TimingFormat,
    ) -> Self {
        let bpms_str = chart_bpms.filter(|s| !s.is_empty()).unwrap_or(global_bpms);
        let normalized_bpms = normalize_float_digits(bpms_str);
        let mut parsed_bpms: Vec<(f64, f64)> = parse_bpm_map(&normalized_bpms);

        if parsed_bpms.is_empty() {
            parsed_bpms.push((0.0, DEFAULT_BPM));
        }

        let raw_stops = parse_optional_timing(chart_stops, global_stops, parse_stops);

        let (mut parsed_bpms, stops, extra_warps, beat0_offset_adjust) =
            process_bpms_and_stops(format, &parsed_bpms, &raw_stops);

        if parsed_bpms.is_empty() {
            parsed_bpms.push((0.0, DEFAULT_BPM));
        }

        let song_offset_sec = song_offset_sec + beat0_offset_adjust;

        let mut beat_to_time = Vec::with_capacity(parsed_bpms.len());
        let mut current_time = 0.0;
        let mut last_beat = 0.0;
        let mut last_bpm = parsed_bpms[0].1;
        let mut max_bpm = 0.0;

        for &(beat, bpm) in &parsed_bpms {
            if beat > last_beat && last_bpm > 0.0 {
                current_time += (beat - last_beat) * (60.0 / last_bpm);
            }
            beat_to_time.push(BeatTimePoint {
                beat,
                time_sec: song_offset_sec + current_time,
                bpm,
            });
            if bpm.is_finite() && bpm > max_bpm {
                max_bpm = bpm;
            }
            last_beat = beat;
            last_bpm = bpm;
        }

        let delays = parse_optional_timing(chart_delays, global_delays, parse_delays);
        let mut warps = parse_optional_timing(chart_warps, global_warps, parse_warps);
        warps.extend(extra_warps);
        let mut speeds = parse_optional_timing(chart_speeds, global_speeds, parse_speeds);
        let mut scrolls = parse_optional_timing(chart_scrolls, global_scrolls, parse_scrolls);
        let mut fakes = parse_optional_timing(chart_fakes, global_fakes, parse_fakes);

        speeds.sort_by(|a, b| a.beat.partial_cmp(&b.beat).unwrap_or(Ordering::Less));
        scrolls.sort_by(|a, b| a.beat.partial_cmp(&b.beat).unwrap_or(Ordering::Less));
        warps.sort_by(|a, b| a.beat.partial_cmp(&b.beat).unwrap_or(Ordering::Less));
        fakes.sort_by(|a, b| a.beat.partial_cmp(&b.beat).unwrap_or(Ordering::Less));

        let mut timing = Self {
            beat_to_time,
            stops,
            delays,
            warps,
            speeds,
            scrolls,
            fakes,
            speed_runtime: Vec::new(),
            scroll_prefix: Vec::new(),
            global_offset_sec,
            max_bpm,
        };

        let re_beat_to_time: Vec<_> = timing
            .beat_to_time
            .iter()
            .map(|point| {
                let mut new_point = *point;
                new_point.time_sec = timing.get_time_for_beat_internal(point.beat);
                new_point
            })
            .collect();
        timing.beat_to_time = re_beat_to_time;

        if !timing.speeds.is_empty() {
            let mut runtime = Vec::with_capacity(timing.speeds.len());
            let mut prev_ratio = 1.0_f64;
            for seg in &timing.speeds {
                let start_time = timing.get_time_for_beat(seg.beat);
                let end_time = if seg.delay <= 0.0 {
                    start_time
                } else if seg.unit == SpeedUnit::Seconds {
                    start_time + seg.delay
                } else {
                    timing.get_time_for_beat(seg.beat + seg.delay)
                };
                runtime.push(SpeedRuntime {
                    start_time,
                    end_time,
                    prev_ratio,
                });
                prev_ratio = seg.ratio;
            }
            timing.speed_runtime = runtime;
        }

        if !timing.scrolls.is_empty() {
            let mut prefixes = Vec::with_capacity(timing.scrolls.len());
            let mut cum_displayed = 0.0_f64;
            let mut last_real_beat = 0.0_f64;
            let mut last_ratio = 1.0_f64;
            for seg in &timing.scrolls {
                cum_displayed += (seg.beat - last_real_beat) * last_ratio;
                prefixes.push(ScrollPrefix {
                    beat: seg.beat,
                    cum_displayed,
                    ratio: seg.ratio,
                });
                last_real_beat = seg.beat;
                last_ratio = seg.ratio;
            }
            timing.scroll_prefix = prefixes;
        }

        timing
    }

    #[inline(always)]
    pub fn beat0_offset_seconds(&self) -> f64 {
        self.beat_to_time.first().map_or(0.0, |p| p.time_sec)
    }

    #[inline(always)]
    pub fn beat0_group_offset_seconds(&self) -> f64 {
        self.global_offset_sec
    }

    #[inline(always)]
    pub fn warps(&self) -> &[WarpSegment] {
        &self.warps
    }

    #[inline(always)]
    pub fn stops(&self) -> &[StopSegment] {
        &self.stops
    }

    #[inline(always)]
    pub fn delays(&self) -> &[DelaySegment] {
        &self.delays
    }

    #[inline(always)]
    pub fn speeds(&self) -> &[SpeedSegment] {
        &self.speeds
    }

    #[inline(always)]
    pub fn scrolls(&self) -> &[ScrollSegment] {
        &self.scrolls
    }

    #[inline(always)]
    pub fn fakes(&self) -> &[FakeSegment] {
        &self.fakes
    }

    pub fn bpm_segments(&self) -> Vec<(f64, f64)> {
        self.beat_to_time
            .iter()
            .map(|point| (point.beat, point.bpm))
            .collect()
    }

    #[inline(always)]
    pub fn is_fake_at_beat(&self, beat: f64) -> bool {
        if self.fakes.is_empty() {
            return false;
        }
        let idx = self.fakes.partition_point(|seg| seg.beat <= beat);
        if idx == 0 {
            return false;
        }
        let seg = self.fakes[idx - 1];
        beat >= seg.beat && beat < seg.beat + seg.length
    }

    #[inline(always)]
    pub fn is_warp_at_beat(&self, beat: f64) -> bool {
        if self.warps.is_empty() {
            return false;
        }
        let idx = self.warps.partition_point(|seg| seg.beat <= beat);
        if idx == 0 {
            return false;
        }
        let seg = self.warps[idx - 1];
        if !(seg.length.is_finite() && seg.length > 0.0) {
            return false;
        }
        beat >= seg.beat && beat < seg.beat + seg.length
    }

    #[inline(always)]
    pub fn is_judgable_at_beat(&self, beat: f64) -> bool {
        !self.is_warp_at_beat(beat) && !self.is_fake_at_beat(beat)
    }

    pub fn get_beat_info_from_time(&self, target_time_sec: f64) -> BeatInfo {
        let mut args = GetBeatArgs::default();
        args.elapsed_time = target_time_sec + self.global_offset_sec;

        let mut start = GetBeatStarts::default();
        start.last_time = -self.beat0_offset_seconds() - self.beat0_group_offset_seconds();

        self.get_beat_internal(start, &mut args, u32::MAX as usize);

        BeatInfo {
            beat: args.beat,
            is_in_freeze: args.freeze_out,
            is_in_delay: args.delay_out,
        }
    }

    pub fn get_beat_for_time(&self, target_time_sec: f64) -> f64 {
        self.get_beat_info_from_time(target_time_sec).beat
    }

    pub fn get_time_for_beat(&self, target_beat: f64) -> f64 {
        self.get_time_for_beat_internal(target_beat) - self.global_offset_sec
    }

    pub fn get_bpm_for_beat(&self, target_beat: f64) -> f64 {
        let points = &self.beat_to_time;
        if points.is_empty() {
            return DEFAULT_BPM;
        }
        let point_idx = self.get_bpm_point_index_for_beat(target_beat);
        points[point_idx].bpm
    }

    pub fn get_capped_max_bpm(&self, cap: Option<f64>) -> f64 {
        let mut max_bpm = self.max_bpm.max(0.0);
        if max_bpm == 0.0 {
            max_bpm = self
                .beat_to_time
                .iter()
                .map(|point| point.bpm)
                .filter(|bpm| bpm.is_finite() && *bpm > 0.0)
                .fold(0.0, f64::max);
        }

        if let Some(cap_value) = cap
            && cap_value > 0.0
        {
            max_bpm = max_bpm.min(cap_value);
        }

        if max_bpm > 0.0 { max_bpm } else { DEFAULT_BPM }
    }

    pub fn get_displayed_beat(&self, beat: f64) -> f64 {
        if self.scroll_prefix.is_empty() {
            return beat;
        }
        if beat < self.scroll_prefix[0].beat {
            return beat;
        }
        let idx = self.scroll_prefix.partition_point(|p| p.beat <= beat);
        let i = idx.saturating_sub(1);
        let p = self.scroll_prefix[i];
        p.cum_displayed + (beat - p.beat) * p.ratio
    }

    pub fn get_speed_multiplier(&self, beat: f64, time: f64) -> f64 {
        if self.speeds.is_empty() {
            return 1.0;
        }
        let segment_index = self.get_speed_segment_index_at_beat(beat);
        if segment_index < 0 {
            return 1.0;
        }
        let i = segment_index as usize;
        let seg = self.speeds[i];
        let rt = self.speed_runtime.get(i).copied().unwrap_or(SpeedRuntime {
            start_time: self.get_time_for_beat(seg.beat),
            end_time: if seg.unit == SpeedUnit::Seconds {
                self.get_time_for_beat(seg.beat) + seg.delay
            } else {
                self.get_time_for_beat(seg.beat + seg.delay)
            },
            prev_ratio: if i > 0 { self.speeds[i - 1].ratio } else { 1.0 },
        });

        if time >= rt.end_time || seg.delay <= 0.0 {
            return seg.ratio;
        }
        if time < rt.start_time {
            return rt.prev_ratio;
        }
        let progress = (time - rt.start_time) / (rt.end_time - rt.start_time);
        rt.prev_ratio + (seg.ratio - rt.prev_ratio) * progress
    }

    fn get_bpm_point_index_for_beat(&self, target_beat: f64) -> usize {
        let points = &self.beat_to_time;
        if points.is_empty() {
            return 0;
        }

        match points.binary_search_by(|p| {
            p.beat
                .partial_cmp(&target_beat)
                .unwrap_or(Ordering::Less)
        }) {
            Ok(i) => i,
            Err(i) => i.saturating_sub(1),
        }
    }

    fn get_time_for_beat_internal(&self, target_beat: f64) -> f64 {
        let mut starts = GetBeatStarts::default();
        starts.last_time = -self.beat0_offset_seconds() - self.beat0_group_offset_seconds();
        self.get_elapsed_time_internal(&mut starts, target_beat)
    }

    fn get_elapsed_time_internal(&self, starts: &mut GetBeatStarts, beat: f64) -> f64 {
        let mut start = *starts;
        self.get_elapsed_time_internal_mut(&mut start, beat, u32::MAX as usize);
        start.last_time
    }

    fn get_beat_internal(
        &self,
        mut start: GetBeatStarts,
        args: &mut GetBeatArgs,
        max_segment: usize,
    ) {
        let bpms = &self.beat_to_time;
        let warps = &self.warps;
        let stops = &self.stops;
        let delays = &self.delays;

        let mut curr_segment = start.bpm_idx + start.warp_idx + start.stop_idx + start.delay_idx;
        let mut bps = self.get_bpm_for_beat(note_row_to_beat(start.last_row)) / 60.0;
        while curr_segment < max_segment {
            let mut event_row = i32::MAX;
            let mut event_type = TimingEvent::NotFound;
            find_event(
                &mut event_row,
                &mut event_type,
                start,
                0.0,
                false,
                bpms,
                warps,
                stops,
                delays,
            );
            if event_type == TimingEvent::NotFound {
                break;
            }
            let time_to_next_event = if start.is_warping {
                0.0
            } else {
                note_row_to_beat(event_row - start.last_row) / bps
            };
            let next_event_time = start.last_time + time_to_next_event;
            if args.elapsed_time < next_event_time {
                break;
            }
            start.last_time = next_event_time;

            match event_type {
                TimingEvent::WarpDest => start.is_warping = false,
                TimingEvent::Bpm => {
                    bps = bpms[start.bpm_idx].bpm / 60.0;
                    start.bpm_idx += 1;
                    curr_segment += 1;
                }
                TimingEvent::Delay | TimingEvent::StopDelay => {
                    let delay = delays[start.delay_idx];
                    if args.elapsed_time < start.last_time + delay.duration {
                        args.delay_out = true;
                        args.beat = delay.beat;
                        args.bps_out = bps;
                        return;
                    }
                    start.last_time += delay.duration;
                    start.delay_idx += 1;
                    curr_segment += 1;
                    if event_type == TimingEvent::Delay {
                        continue;
                    }
                }
                TimingEvent::Stop => {
                    let stop = stops[start.stop_idx];
                    if args.elapsed_time < start.last_time + stop.duration {
                        args.freeze_out = true;
                        args.beat = stop.beat;
                        args.bps_out = bps;
                        return;
                    }
                    start.last_time += stop.duration;
                    start.stop_idx += 1;
                    curr_segment += 1;
                }
                TimingEvent::Warp => {
                    start.is_warping = true;
                    let warp = warps[start.warp_idx];
                    let warp_sum = warp.length + warp.beat;
                    if warp_sum > start.warp_destination {
                        start.warp_destination = warp_sum;
                    }
                    args.warp_begin_out = event_row;
                    args.warp_dest_out = start.warp_destination;
                    start.warp_idx += 1;
                    curr_segment += 1;
                }
                _ => {}
            }
            start.last_row = event_row;
        }
        if args.elapsed_time == f64::MAX {
            args.elapsed_time = start.last_time;
        }
        args.beat = note_row_to_beat(start.last_row) + (args.elapsed_time - start.last_time) * bps;
        args.bps_out = bps;
    }

    fn get_elapsed_time_internal_mut(
        &self,
        start: &mut GetBeatStarts,
        beat: f64,
        max_segment: usize,
    ) {
        let bpms = &self.beat_to_time;
        let warps = &self.warps;
        let stops = &self.stops;
        let delays = &self.delays;

        let mut curr_segment = start.bpm_idx + start.warp_idx + start.stop_idx + start.delay_idx;
        let mut bps = self.get_bpm_for_beat(note_row_to_beat(start.last_row)) / 60.0;
        let find_marker = beat < f64::MAX;

        while curr_segment < max_segment {
            let mut event_row = i32::MAX;
            let mut event_type = TimingEvent::NotFound;
            find_event(
                &mut event_row,
                &mut event_type,
                *start,
                beat,
                find_marker,
                bpms,
                warps,
                stops,
                delays,
            );
            if event_type == TimingEvent::NotFound {
                break;
            }
            let time_to_next_event = if start.is_warping {
                0.0
            } else {
                note_row_to_beat(event_row - start.last_row) / bps
            };
            start.last_time += time_to_next_event;

            match event_type {
                TimingEvent::WarpDest => start.is_warping = false,
                TimingEvent::Bpm => {
                    bps = bpms[start.bpm_idx].bpm / 60.0;
                    start.bpm_idx += 1;
                    curr_segment += 1;
                }
                TimingEvent::Stop | TimingEvent::StopDelay => {
                    start.last_time += stops[start.stop_idx].duration;
                    start.stop_idx += 1;
                    curr_segment += 1;
                }
                TimingEvent::Delay => {
                    start.last_time += delays[start.delay_idx].duration;
                    start.delay_idx += 1;
                    curr_segment += 1;
                }
                TimingEvent::Marker => return,
                TimingEvent::Warp => {
                    start.is_warping = true;
                    let warp = warps[start.warp_idx];
                    let warp_sum = warp.length + warp.beat;
                    if warp_sum > start.warp_destination {
                        start.warp_destination = warp_sum;
                    }
                    start.warp_idx += 1;
                    curr_segment += 1;
                }
                _ => {}
            }
            start.last_row = event_row;
        }
    }

    fn get_speed_segment_index_at_beat(&self, beat: f64) -> isize {
        if self.speeds.is_empty() {
            return -1;
        }
        let pos = self.speeds.partition_point(|seg| seg.beat <= beat);
        if pos == 0 { -1 } else { (pos - 1) as isize }
    }
}

fn find_event(
    event_row: &mut i32,
    event_type: &mut TimingEvent,
    start: GetBeatStarts,
    beat: f64,
    find_marker: bool,
    bpms: &[BeatTimePoint],
    warps: &[WarpSegment],
    stops: &[StopSegment],
    delays: &[DelaySegment],
) {
    if start.is_warping && beat_to_note_row(start.warp_destination) < *event_row {
        *event_row = beat_to_note_row(start.warp_destination);
        *event_type = TimingEvent::WarpDest;
    }
    if start.bpm_idx < bpms.len() && beat_to_note_row(bpms[start.bpm_idx].beat) < *event_row {
        *event_row = beat_to_note_row(bpms[start.bpm_idx].beat);
        *event_type = TimingEvent::Bpm;
    }
    if start.delay_idx < delays.len() && beat_to_note_row(delays[start.delay_idx].beat) < *event_row
    {
        *event_row = beat_to_note_row(delays[start.delay_idx].beat);
        *event_type = TimingEvent::Delay;
    }
    if find_marker && beat_to_note_row(beat) < *event_row {
        *event_row = beat_to_note_row(beat);
        *event_type = TimingEvent::Marker;
    }
    if start.stop_idx < stops.len() && beat_to_note_row(stops[start.stop_idx].beat) < *event_row {
        let tmp_row = *event_row;
        *event_row = beat_to_note_row(stops[start.stop_idx].beat);
        *event_type = if tmp_row == *event_row {
            TimingEvent::StopDelay
        } else {
            TimingEvent::Stop
        };
    }
    if start.warp_idx < warps.len() && beat_to_note_row(warps[start.warp_idx].beat) < *event_row {
        *event_row = beat_to_note_row(warps[start.warp_idx].beat);
        *event_type = TimingEvent::Warp;
    }
}

#[inline(always)]
fn parse_f64_fast(s: &str) -> Option<f64> {
    s.trim().parse::<f64>().ok()
}

fn parse_fakes(s: &str) -> Result<Vec<FakeSegment>, &'static str> {
    let mut out = Vec::new();
    if s.trim().is_empty() {
        return Ok(out);
    }
    for part in s.split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        let Some((beat_str, len_str)) = part.split_once('=') else {
            continue;
        };
        let Some(beat) = parse_f64_fast(beat_str) else {
            continue;
        };
        let Some(len) = parse_f64_fast(len_str) else {
            continue;
        };
        if beat.is_finite() && len.is_finite() && len > 0.0 {
            out.push(FakeSegment { beat, length: len });
        }
    }
    Ok(out)
}

fn parse_stops(s: &str) -> Result<Vec<StopSegment>, &'static str> {
    if s.is_empty() {
        return Ok(Vec::new());
    }
    let segments: Result<Vec<_>, _> = s
        .split(',')
        .map(|pair| -> Result<StopSegment, &'static str> {
            let mut parts = pair.split('=');
            let beat_str = parts.next().ok_or("Missing beat")?.trim();
            let duration_str = parts.next().ok_or("Missing duration")?.trim();
            let beat = beat_str.parse::<f64>().map_err(|_| "Invalid beat")?;
            let duration = duration_str
                .parse::<f64>()
                .map_err(|_| "Invalid duration")?;
            Ok(StopSegment { beat, duration })
        })
        .collect();

    Ok(segments?.into_iter().collect())
}

fn parse_delays(s: &str) -> Result<Vec<DelaySegment>, &'static str> {
    Ok(parse_stops(s)?
        .into_iter()
        .map(|s| DelaySegment {
            beat: s.beat,
            duration: s.duration,
        })
        .collect())
}

fn parse_warps(s: &str) -> Result<Vec<WarpSegment>, &'static str> {
    Ok(parse_stops(s)?
        .into_iter()
        .map(|s| WarpSegment {
            beat: s.beat,
            length: s.duration,
        })
        .collect())
}

fn parse_speeds(s: &str) -> Result<Vec<SpeedSegment>, &'static str> {
    if s.is_empty() {
        return Ok(Vec::new());
    }
    s.split(',')
        .map(|chunk| {
            let parts: Vec<_> = chunk.split('=').map(str::trim).collect();
            if parts.len() < 3 {
                return Err("Invalid speed format");
            }
            let beat = parts[0].parse::<f64>().map_err(|_| "Invalid beat")?;
            let ratio = parts[1].parse::<f64>().map_err(|_| "Invalid ratio")?;
            let delay = parts[2].parse::<f64>().map_err(|_| "Invalid delay")?;
            let unit = if parts.len() > 3 && parts[3] == "1" {
                SpeedUnit::Seconds
            } else {
                SpeedUnit::Beats
            };
            Ok(SpeedSegment {
                beat,
                ratio,
                delay,
                unit,
            })
        })
        .collect()
}

fn parse_scrolls(s: &str) -> Result<Vec<ScrollSegment>, &'static str> {
    Ok(s.split(',')
        .filter_map(|pair| {
            let mut parts = pair.split('=');
            let beat = parts.next()?.trim().parse::<f64>().ok()?;
            let ratio = parts.next()?.trim().parse::<f64>().ok()?;
            Some(ScrollSegment { beat, ratio })
        })
        .collect())
}

fn process_bpms_and_stops(
    format: TimingFormat,
    bpms: &[(f64, f64)],
    stops: &[StopSegment],
) -> (Vec<(f64, f64)>, Vec<StopSegment>, Vec<WarpSegment>, f64) {
    match format {
        TimingFormat::Sm => process_bpms_and_stops_sm(bpms, stops),
        TimingFormat::Ssc => process_bpms_and_stops_ssc(bpms, stops),
    }
}

fn tidy_bpms(mut bpms: Vec<(f64, f64)>) -> Vec<(f64, f64)> {
    if bpms.is_empty() {
        return vec![(0.0, DEFAULT_BPM)];
    }

    bpms.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(Ordering::Equal));

    let mut last_per_beat: Vec<(f64, f64)> = Vec::with_capacity(bpms.len());
    for (beat, bpm) in bpms {
        if let Some(last) = last_per_beat.last_mut() {
            if beat == last.0 {
                *last = (beat, bpm);
                continue;
            }
        }
        last_per_beat.push((beat, bpm));
    }

    if let Some(first) = last_per_beat.first_mut() {
        if first.0 != 0.0 {
            first.0 = 0.0;
        }
    }

    let mut tidied: Vec<(f64, f64)> = Vec::with_capacity(last_per_beat.len());
    let mut last_value: Option<f64> = None;
    for (beat, bpm) in last_per_beat {
        if last_value == Some(bpm) {
            continue;
        }
        last_value = Some(bpm);
        tidied.push((beat, bpm));
    }

    if tidied.is_empty() {
        tidied.push((0.0, DEFAULT_BPM));
    }
    tidied
}

fn process_bpms_and_stops_sm(
    bpms: &[(f64, f64)],
    stops: &[StopSegment],
) -> (Vec<(f64, f64)>, Vec<StopSegment>, Vec<WarpSegment>, f64) {
    let mut bpm_changes: Vec<(f64, f64)> = bpms
        .iter()
        .copied()
        .filter(|(beat, bpm)| beat.is_finite() && bpm.is_finite() && *bpm != 0.0)
        .collect();
    bpm_changes.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(Ordering::Equal));

    let mut stop_changes: Vec<(f64, f64)> = stops
        .iter()
        .filter(|s| s.beat.is_finite() && s.duration.is_finite() && s.duration != 0.0)
        .map(|s| (s.beat, s.duration))
        .collect();
    stop_changes.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(Ordering::Equal));

    let mut beat0_offset_sec = 0.0_f64;
    let mut stop_idx = 0usize;
    while stop_idx < stop_changes.len() && stop_changes[stop_idx].0 < 0.0 {
        beat0_offset_sec -= stop_changes[stop_idx].1;
        stop_idx += 1;
    }

    let mut bpm_idx = 0usize;
    let mut bpm = 0.0_f64;
    while bpm_idx < bpm_changes.len() && bpm_changes[bpm_idx].0 <= 0.0 {
        bpm = bpm_changes[bpm_idx].1;
        bpm_idx += 1;
    }

    if bpm == 0.0 {
        if bpm_idx == bpm_changes.len() {
            bpm = DEFAULT_BPM;
        } else {
            bpm = bpm_changes[bpm_idx].1;
            bpm_idx += 1;
        }
    }

    let mut out_bpms: Vec<(f64, f64)> = Vec::new();
    if bpm > 0.0 && bpm <= FAST_BPM_WARP {
        out_bpms.push((0.0, bpm));
    }

    let mut out_stops: Vec<StopSegment> = Vec::new();
    let mut out_warps: Vec<WarpSegment> = Vec::new();

    let mut prev_beat = 0.0_f64;
    let mut warp_start: Option<f64> = None;
    let mut prewarp_bpm: f64 = 0.0;
    let mut time_offset_sec = 0.0_f64;

    while bpm_idx < bpm_changes.len() || stop_idx < stop_changes.len() {
        let change_is_bpm = stop_idx == stop_changes.len()
            || (bpm_idx < bpm_changes.len() && bpm_changes[bpm_idx].0 <= stop_changes[stop_idx].0);
        let (change_beat, change_val) = if change_is_bpm {
            bpm_changes[bpm_idx]
        } else {
            stop_changes[stop_idx]
        };

        if bpm <= FAST_BPM_WARP {
            time_offset_sec += (change_beat - prev_beat) * 60.0 / bpm;
            if let Some(start) = warp_start {
                if bpm > 0.0 && time_offset_sec > 0.0 {
                    let warp_end = change_beat - (time_offset_sec * bpm / 60.0);
                    if warp_end > start {
                        out_warps.push(WarpSegment {
                            beat: start,
                            length: warp_end - start,
                        });
                    }
                    if bpm != prewarp_bpm {
                        out_bpms.push((start, bpm));
                    }
                    warp_start = None;
                }
            }
        }

        prev_beat = change_beat;

        if change_is_bpm {
            if warp_start.is_none() && (change_val < 0.0 || change_val > FAST_BPM_WARP) {
                warp_start = Some(change_beat);
                prewarp_bpm = bpm;
                time_offset_sec = 0.0;
            } else if warp_start.is_none() {
                out_bpms.push((change_beat, change_val));
            }

            bpm = change_val;
            bpm_idx += 1;
        } else {
            if warp_start.is_none() && change_val < 0.0 {
                warp_start = Some(change_beat);
                prewarp_bpm = bpm;
                time_offset_sec = change_val;
            } else if warp_start.is_none() {
                out_stops.push(StopSegment {
                    beat: change_beat,
                    duration: change_val,
                });
            } else {
                time_offset_sec += change_val;
                if change_val > 0.0 && time_offset_sec > 0.0 {
                    let warp_end = change_beat;
                    if let Some(start) = warp_start {
                        if warp_end > start {
                            out_warps.push(WarpSegment {
                                beat: start,
                                length: warp_end - start,
                            });
                        }
                        out_stops.push(StopSegment {
                            beat: change_beat,
                            duration: time_offset_sec,
                        });

                        if bpm < 0.0 || bpm > FAST_BPM_WARP {
                            warp_start = Some(change_beat);
                            time_offset_sec = 0.0;
                        } else {
                            if bpm != prewarp_bpm {
                                out_bpms.push((start, bpm));
                            }
                            warp_start = None;
                        }
                    }
                }
            }
            stop_idx += 1;
        }
    }

    if let Some(start) = warp_start {
        let warp_end = if bpm < 0.0 || bpm > FAST_BPM_WARP {
            99_999_999.0
        } else {
            prev_beat - (time_offset_sec * bpm / 60.0)
        };
        if warp_end > start {
            out_warps.push(WarpSegment {
                beat: start,
                length: warp_end - start,
            });
        }
        if bpm != prewarp_bpm {
            out_bpms.push((start, bpm));
        }
    }

    let out_bpms = tidy_bpms(out_bpms);
    out_stops.sort_by(|a, b| a.beat.partial_cmp(&b.beat).unwrap_or(Ordering::Less));
    out_warps.sort_by(|a, b| a.beat.partial_cmp(&b.beat).unwrap_or(Ordering::Less));

    (out_bpms, out_stops, out_warps, beat0_offset_sec)
}

fn process_bpms_and_stops_ssc(
    bpms: &[(f64, f64)],
    stops: &[StopSegment],
) -> (Vec<(f64, f64)>, Vec<StopSegment>, Vec<WarpSegment>, f64) {
    let mut bpm_changes: Vec<(f64, f64)> = bpms
        .iter()
        .copied()
        .filter(|(beat, bpm)| beat.is_finite() && bpm.is_finite() && *beat >= 0.0 && *bpm > 0.0)
        .collect();
    bpm_changes.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(Ordering::Equal));

    let mut out_stops: Vec<StopSegment> = stops
        .iter()
        .filter(|s| s.beat.is_finite() && s.duration.is_finite() && s.beat >= 0.0 && s.duration > 0.0)
        .map(|s| StopSegment {
            beat: s.beat,
            duration: s.duration,
        })
        .collect();
    out_stops.sort_by(|a, b| a.beat.partial_cmp(&b.beat).unwrap_or(Ordering::Less));

    let out_bpms = tidy_bpms(bpm_changes);

    (out_bpms, out_stops, Vec::new(), 0.0)
}

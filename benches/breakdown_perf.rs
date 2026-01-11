use criterion::{Criterion, criterion_group, criterion_main};
use std::hint::black_box;
use std::time::Duration;

use rssp::stats::{BreakdownMode, StreamBreakdownLevel};

const FIXTURE: &str = include_str!("fixtures/camellia_mix.ssc");
const EXTENSION: &str = "ssc";

#[derive(Clone)]
struct ChartBreakdownInput {
    chart_data: Vec<u8>,
    lanes: usize,
}

fn build_breakdown_inputs() -> Vec<ChartBreakdownInput> {
    let parsed =
        rssp::parse::extract_sections(FIXTURE.as_bytes(), EXTENSION).expect("fixture should parse");

    parsed
        .notes_list
        .into_iter()
        .filter_map(|entry| {
            if entry.field_count < 5 {
                return None;
            }

            let step_type = std::str::from_utf8(entry.fields[0]).unwrap_or("").trim();
            if step_type == "lights-cabinet" {
                return None;
            }

            Some(ChartBreakdownInput {
                chart_data: entry.note_data.to_vec(),
                lanes: rssp::step_type_lanes(step_type),
            })
        })
        .collect()
}

fn bench_breakdown_pipeline(c: &mut Criterion) {
    let fixture = FIXTURE.as_bytes();
    let options = rssp::AnalysisOptions {
        compute_tech_counts: false,
        ..rssp::AnalysisOptions::default()
    };

    let mut group = c.benchmark_group("breakdown_pipeline");
    group.sample_size(200);
    group.measurement_time(Duration::from_secs(2));
    group.bench_function("analyze_breakdowns", |b| {
        b.iter(|| {
            let summary = rssp::analyze(black_box(fixture), black_box(EXTENSION), options.clone())
                .expect("analysis should succeed");
            let mut total_len = 0usize;
            for chart in &summary.charts {
                total_len += chart.detailed_breakdown.len();
                total_len += chart.partial_breakdown.len();
                total_len += chart.simple_breakdown.len();
                total_len += chart.sn_detailed_breakdown.len();
                total_len += chart.sn_partial_breakdown.len();
                total_len += chart.sn_simple_breakdown.len();
            }
            black_box(total_len);
        })
    });
    group.finish();
}

fn bench_breakdown_inner(c: &mut Criterion) {
    let charts = build_breakdown_inputs();

    let mut group = c.benchmark_group("breakdown_inner");
    group.sample_size(200);
    group.measurement_time(Duration::from_secs(2));
    group.bench_function("compute_breakdowns", |b| {
        b.iter(|| {
            let mut totals = Vec::with_capacity(charts.len());
            for chart in &charts {
                let (_minimized, _stats, measure_densities) =
                    rssp::stats::minimize_chart_and_count_with_lanes(
                        black_box(&chart.chart_data),
                        black_box(chart.lanes),
                    );

                let detailed = rssp::stats::stream_breakdown(
                    &measure_densities,
                    StreamBreakdownLevel::Detailed,
                );
                let partial = rssp::stats::stream_breakdown(
                    &measure_densities,
                    StreamBreakdownLevel::Partial,
                );
                let simple =
                    rssp::stats::stream_breakdown(&measure_densities, StreamBreakdownLevel::Simple);

                let sn_detailed =
                    rssp::stats::generate_breakdown(&measure_densities, BreakdownMode::Detailed);
                let sn_partial =
                    rssp::stats::generate_breakdown(&measure_densities, BreakdownMode::Partial);
                let sn_simple =
                    rssp::stats::generate_breakdown(&measure_densities, BreakdownMode::Simplified);

                totals.push(
                    detailed.len()
                        + partial.len()
                        + simple.len()
                        + sn_detailed.len()
                        + sn_partial.len()
                        + sn_simple.len(),
                );
            }
            black_box(totals);
        })
    });
    group.finish();
}

fn bench_breakdown_counts(c: &mut Criterion) {
    let charts = build_breakdown_inputs();
    let densities: Vec<Vec<usize>> = charts
        .iter()
        .map(|chart| rssp::stats::measure_densities(&chart.chart_data, chart.lanes))
        .collect();

    let mut group = c.benchmark_group("breakdown_counts");
    group.sample_size(200);
    group.measurement_time(Duration::from_secs(2));
    group.bench_function("measure_densities", |b| {
        b.iter(|| {
            let mut total = 0usize;
            for chart in &charts {
                let dens = rssp::stats::measure_densities(
                    black_box(&chart.chart_data),
                    black_box(chart.lanes),
                );
                total += dens.len();
            }
            black_box(total);
        })
    });
    group.bench_function("compute_stream_counts", |b| {
        b.iter(|| {
            let mut total = 0u32;
            for dens in &densities {
                let counts = rssp::stats::compute_stream_counts(black_box(dens));
                total += counts.run16_streams
                    + counts.run20_streams
                    + counts.run24_streams
                    + counts.run32_streams
                    + counts.total_breaks
                    + counts.sn_breaks;
            }
            black_box(total);
        })
    });
    group.finish();
}

criterion_group!(
    benches,
    bench_breakdown_pipeline,
    bench_breakdown_inner,
    bench_breakdown_counts
);
criterion_main!(benches);

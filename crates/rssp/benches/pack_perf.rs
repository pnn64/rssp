use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use std::hint::black_box;
use std::path::{Path, PathBuf};
use std::time::Duration;

#[path = "support/assets.rs"]
mod assets_bench;
#[path = "support/pack.rs"]
mod pack_bench;

fn bench_pack_scan(c: &mut Criterion) {
    let fixture = pack_bench::PackFixture::new();

    let mut group = c.benchmark_group("pack_scan");
    group.sample_size(20);
    group.measurement_time(Duration::from_secs(3));
    group.throughput(Throughput::Elements(pack_bench::SONG_COUNT as u64));
    group.bench_function("scan_64_songs", |b| {
        b.iter(|| {
            let scan = rssp::pack::scan_pack_dir(
                black_box(fixture.pack_dir()),
                black_box(rssp::pack::ScanOpt::default()),
            )
            .expect("benchmark pack should scan")
            .expect("benchmark pack should contain songs");
            black_box(scan);
        });
    });
    group.finish();
}

fn bench_background_changes(c: &mut Criterion) {
    let fixture = assets_bench::AssetFixture::new();

    let mut group = c.benchmark_group("background_changes");
    group.sample_size(30);
    group.measurement_time(Duration::from_secs(3));
    group.throughput(Throughput::Elements(assets_bench::CHANGE_COUNT as u64));
    group.bench_function("resolve_256_changes", |b| {
        b.iter(|| {
            black_box(rssp::assets::resolve_background_changes_like_itg(
                black_box(fixture.song_dir()),
                black_box(fixture.simfile()),
            ))
        });
    });
    group.finish();
}

fn lowercase_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default()
}

fn legacy_first_path(paths: &[PathBuf]) -> &Path {
    let mut candidates: Vec<_> = paths.iter().map(PathBuf::as_path).collect();
    candidates.sort_by_cached_key(|path| lowercase_name(path));
    candidates[0]
}

fn legacy_match_mask_ci(name: &str, mask: &str) -> bool {
    let name = name.to_ascii_lowercase();
    let mask = mask.to_ascii_lowercase();
    let Some(first) = mask.find('*') else {
        return name == mask;
    };
    let Some(second) = mask[first + 1..].find('*').map(|index| index + first + 1) else {
        let (prefix, suffix) = (&mask[..first], &mask[first + 1..]);
        return name.starts_with(prefix)
            && name.ends_with(suffix)
            && name.len() >= prefix.len() + suffix.len();
    };
    let prefix = &mask[..first];
    let middle = &mask[first + 1..second];
    let suffix = &mask[second + 1..];
    if !name.starts_with(prefix)
        || !name.ends_with(suffix)
        || name.len() < prefix.len() + middle.len() + suffix.len()
    {
        return false;
    }
    name[prefix.len()..name.len() - suffix.len()].contains(middle)
}

fn bench_selection_algorithms(c: &mut Criterion) {
    let paths: Vec<_> = (0..256)
        .rev()
        .map(|index| PathBuf::from(format!("Song-{index:03}.SSC")))
        .collect();
    let mut selection = c.benchmark_group("pack_selection_256");
    selection.bench_function("legacy_key_sort", |b| {
        b.iter(|| black_box(legacy_first_path(black_box(&paths))));
    });
    selection.bench_function("cached_single_pass", |b| {
        b.iter(|| {
            black_box(
                rssp::profile::first_path_ci(black_box(&paths))
                    .expect("benchmark paths should not be empty"),
            )
        });
    });
    selection.finish();

    let names: Vec<_> = (0..256)
        .map(|index| format!("Background-Middle-{index:03}.JPG"))
        .collect();
    let mask = "back*middle*.jpg";
    let mut masks = c.benchmark_group("pack_mask_256");
    masks.bench_function("legacy_lowercase", |b| {
        b.iter(|| {
            let matches = names
                .iter()
                .filter(|name| legacy_match_mask_ci(black_box(name), black_box(mask)))
                .count();
            black_box(matches);
        });
    });
    masks.bench_function("allocation_free", |b| {
        b.iter(|| {
            let matches = names
                .iter()
                .filter(|name| rssp::profile::match_mask_ci(black_box(name), black_box(mask)))
                .count();
            black_box(matches);
        });
    });
    masks.finish();
}

criterion_group!(
    benches,
    bench_pack_scan,
    bench_background_changes,
    bench_selection_algorithms
);
criterion_main!(benches);

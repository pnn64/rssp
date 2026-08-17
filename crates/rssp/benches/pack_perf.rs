use criterion::{BatchSize, Criterion, Throughput, criterion_group, criterion_main};
use std::ffi::{OsStr, OsString};
use std::hint::black_box;
use std::path::{Path, PathBuf};
use std::time::Duration;

#[path = "support/assets.rs"]
mod assets_bench;
#[path = "support/pack.rs"]
mod pack_bench;
#[path = "support/path_sort.rs"]
mod path_sort_bench;

fn bench_pack_scan(c: &mut Criterion) {
    let fixture = pack_bench::PackFixture::new();
    fixture.assert_song_behavior();

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

fn bench_songs_root(c: &mut Criterion) {
    let fixture = pack_bench::PackFixture::new();
    fixture.assert_songs_behavior();
    let opt = rssp::pack::ScanOpt::default();

    let mut group = c.benchmark_group("songs_root_discovery");
    group.sample_size(20);
    group.measurement_time(Duration::from_secs(3));
    group.throughput(Throughput::Elements(
        pack_bench::SONGS_ROOT_ENTRY_COUNT as u64,
    ));
    group.bench_function("probe_every_entry", |b| {
        b.iter(|| {
            black_box(
                rssp::profile::scan_songs_dir_legacy(
                    black_box(fixture.tree_root()),
                    black_box(opt),
                )
                .expect("benchmark Songs root should scan"),
            )
        });
    });
    group.bench_function("cached_dir_types", |b| {
        b.iter(|| {
            black_box(
                rssp::pack::scan_songs_dir(black_box(fixture.tree_root()), black_box(opt))
                    .expect("benchmark Songs root should scan"),
            )
        });
    });
    group.finish();
}

fn bench_parent_img(c: &mut Criterion) {
    let fixture = pack_bench::PackFixture::new();
    fixture.assert_parent_img_behavior();

    let mut group = c.benchmark_group("pack_parent_image");
    group.sample_size(30);
    group.measurement_time(Duration::from_secs(3));
    group.throughput(Throughput::Elements(
        pack_bench::SONGS_ROOT_ENTRY_COUNT as u64,
    ));
    group.bench_function("full_path_stats", |b| {
        b.iter(|| {
            black_box(rssp::profile::pack_parent_img_legacy(
                black_box(fixture.pack_dir()),
                black_box("Performance Pack"),
            ))
        });
    });
    group.bench_function("candidate_names", |b| {
        b.iter(|| {
            black_box(rssp::profile::pack_parent_img(
                black_box(fixture.pack_dir()),
                black_box("Performance Pack"),
            ))
        });
    });
    group.finish();
}

fn bench_subdir_img(c: &mut Criterion) {
    let fixture = pack_bench::ImageHintFixture::new();
    fixture.assert_behavior();

    let mut group = c.benchmark_group("pack_subdir_image");
    group.sample_size(30);
    group.measurement_time(Duration::from_secs(3));
    group.throughput(Throughput::Elements(pack_bench::HINT_ENTRY_COUNT as u64));
    group.bench_function("full_paths", |b| {
        b.iter(|| {
            black_box(rssp::profile::pack_subdir_img_legacy(
                black_box(fixture.pack_dir()),
                black_box(pack_bench::SUBDIR_HINT),
            ))
        });
    });
    group.bench_function("candidate_names", |b| {
        b.iter(|| {
            black_box(rssp::profile::pack_subdir_img(
                black_box(fixture.pack_dir()),
                black_box(pack_bench::SUBDIR_HINT),
            ))
        });
    });
    group.finish();
}

fn bench_song_scan(c: &mut Criterion) {
    let fixture = pack_bench::PackFixture::new();
    fixture.assert_song_behavior();
    let opt = rssp::pack::ScanOpt::default();

    let mut group = c.benchmark_group("song_simfile_discovery");
    group.sample_size(30);
    group.measurement_time(Duration::from_secs(3));
    group.throughput(Throughput::Elements(pack_bench::SONG_ENTRY_COUNT as u64));
    group.bench_function("full_paths", |b| {
        b.iter(|| {
            black_box(
                rssp::profile::scan_song_dir_full_paths(
                    black_box(fixture.song_dir()),
                    black_box(opt),
                )
                .expect("benchmark song should scan"),
            )
        });
    });
    group.bench_function("candidate_names", |b| {
        b.iter(|| {
            black_box(
                rssp::pack::scan_song_dir(black_box(fixture.song_dir()), black_box(opt))
                    .expect("benchmark song should scan"),
            )
        });
    });
    group.finish();
}

fn bench_simfile_tree(c: &mut Criterion) {
    let fixture = pack_bench::PackFixture::new();
    fixture.assert_tree_behavior();
    let opt = rssp::pack::ScanOpt::default();

    let mut group = c.benchmark_group("simfile_tree_discovery");
    group.sample_size(20);
    group.measurement_time(Duration::from_secs(3));
    group.throughput(Throughput::Elements(pack_bench::TREE_ENTRY_COUNT as u64));
    group.bench_function("rescan_subdirs", |b| {
        b.iter(|| {
            black_box(rssp::profile::find_simfiles_legacy(
                black_box(fixture.tree_root()),
                black_box(opt),
            ))
        });
    });
    group.bench_function("one_snapshot", |b| {
        b.iter(|| {
            black_box(rssp::pack::find_simfiles(
                black_box(fixture.tree_root()),
                black_box(opt),
            ))
        });
    });
    group.finish();
}

fn bench_pack_root(c: &mut Criterion) {
    let fixture = pack_bench::PackFixture::new();
    fixture.assert_root_behavior();
    let opt = rssp::pack::ScanOpt::default();

    let mut group = c.benchmark_group("pack_root_discovery");
    group.sample_size(20);
    group.measurement_time(Duration::from_secs(3));
    group.throughput(Throughput::Elements(pack_bench::ROOT_ENTRY_COUNT as u64));
    group.bench_function("legacy_repeated_scans", |b| {
        b.iter(|| {
            black_box(
                rssp::profile::pack_root_legacy(
                    black_box(fixture.pack_dir()),
                    black_box(opt),
                    black_box(pack_bench::BANNER_HINT),
                    black_box(pack_bench::BACKGROUND_HINT),
                )
                .expect("benchmark pack root should scan"),
            )
        });
    });
    group.bench_function("full_path_stats", |b| {
        b.iter(|| {
            black_box(
                rssp::profile::pack_root_full_paths(
                    black_box(fixture.pack_dir()),
                    black_box(opt),
                    black_box(pack_bench::BANNER_HINT),
                    black_box(pack_bench::BACKGROUND_HINT),
                )
                .expect("benchmark pack root should scan"),
            )
        });
    });
    group.bench_function("cached_entry_types", |b| {
        b.iter(|| {
            black_box(
                rssp::profile::pack_root(
                    black_box(fixture.pack_dir()),
                    black_box(opt),
                    black_box(pack_bench::BANNER_HINT),
                    black_box(pack_bench::BACKGROUND_HINT),
                )
                .expect("benchmark pack root should scan"),
            )
        });
    });
    group.finish();
}

fn bench_background_changes(c: &mut Criterion) {
    let fixture = assets_bench::AssetFixture::with_movies(1);

    let mut group = c.benchmark_group("background_changes");
    group.sample_size(30);
    group.measurement_time(Duration::from_secs(3));
    group.throughput(Throughput::Elements(assets_bench::CHANGE_COUNT as u64));
    group.bench_function("root_rescan", |b| {
        b.iter(|| {
            black_box(rssp::profile::background_changes_legacy(
                black_box(fixture.song_dir()),
                black_box(fixture.simfile()),
            ))
        });
    });
    group.bench_function("catalog_movie", |b| {
        b.iter(|| {
            black_box(rssp::assets::resolve_background_changes_like_itg(
                black_box(fixture.song_dir()),
                black_box(fixture.simfile()),
            ))
        });
    });
    group.finish();

    let mut delimiters = c.benchmark_group("background_delimiters");
    delimiters.sample_size(30);
    delimiters.measurement_time(Duration::from_secs(3));
    delimiters.throughput(Throughput::Elements(assets_bench::CHANGE_COUNT as u64));
    delimiters.bench_function("double_find", |b| {
        b.iter(|| {
            black_box(rssp::profile::background_changes_double_find(
                black_box(fixture.song_dir()),
                black_box(fixture.simfile()),
            ))
        });
    });
    delimiters.bench_function("single_scan", |b| {
        b.iter(|| {
            black_box(rssp::assets::resolve_background_changes_like_itg(
                black_box(fixture.song_dir()),
                black_box(fixture.simfile()),
            ))
        });
    });
    delimiters.finish();
}

fn bench_delimiter_scan(c: &mut Criterion) {
    let fields = assets_bench::delimiter_fields();
    let bytes = fields.iter().map(String::len).sum::<usize>();
    let mut group = c.benchmark_group("background_delimiter_scan");
    group.throughput(Throughput::Bytes(bytes as u64));
    group.bench_function("double_find", |b| {
        b.iter(|| {
            let sum = fields
                .iter()
                .filter_map(|field| rssp::profile::bg_delimiter_legacy(black_box(field)))
                .sum::<usize>();
            black_box(sum)
        });
    });
    group.bench_function("memchr2", |b| {
        b.iter(|| {
            let sum = fields
                .iter()
                .filter_map(|field| rssp::profile::bg_delimiter(black_box(field)))
                .sum::<usize>();
            black_box(sum)
        });
    });
    group.finish();
}

fn bench_asset_fallbacks(c: &mut Criterion) {
    let fixture = assets_bench::AssetFixture::new();
    fixture.assert_music_behavior();

    let mut lookup = c.benchmark_group("asset_ci_lookup");
    lookup.sample_size(30);
    lookup.measurement_time(Duration::from_secs(3));
    lookup.throughput(Throughput::Elements(assets_bench::LOOKUP_COUNT as u64));
    lookup.bench_function("find_last_of_256", |b| {
        b.iter(|| {
            black_box(
                rssp::profile::file_ci(
                    black_box(fixture.lookup_dir()),
                    black_box(assets_bench::AssetFixture::lookup_name()),
                )
                .expect("case-insensitive benchmark asset should resolve"),
            )
        });
    });
    lookup.finish();

    let mut fallbacks = c.benchmark_group("asset_fallbacks");
    fallbacks.sample_size(30);
    fallbacks.measurement_time(Duration::from_secs(3));
    fallbacks.throughput(Throughput::Elements(assets_bench::SOUND_COUNT as u64));
    fallbacks.bench_function("music_full_paths", |b| {
        b.iter(|| {
            black_box(
                rssp::profile::music_path_legacy(black_box(fixture.song_dir()), black_box(""))
                    .expect("music fallback should resolve"),
            )
        });
    });
    fallbacks.bench_function("music_candidate_names", |b| {
        b.iter(|| {
            black_box(
                rssp::assets::resolve_music_path_like_itg(
                    black_box(fixture.song_dir()),
                    black_box(""),
                )
                .expect("music fallback should resolve"),
            )
        });
    });
    fallbacks.throughput(Throughput::Elements(assets_bench::MOVIE_COUNT as u64));
    fallbacks.bench_function("movie_128_candidates", |b| {
        b.iter(|| {
            black_box(rssp::assets::resolve_background_changes_like_itg(
                black_box(fixture.song_dir()),
                black_box(b""),
            ))
        });
    });
    fallbacks.finish();
}

fn bench_relative_asset_paths(c: &mut Criterion) {
    let fixture = assets_bench::AssetFixture::with_movies(1);
    fixture.assert_rel_path_behavior();
    let paths = assets_bench::relative_paths();
    let mut group = c.benchmark_group("asset_relative_paths");
    group.sample_size(30);
    group.measurement_time(Duration::from_secs(3));
    group.throughput(Throughput::Elements(assets_bench::REL_PATH_COUNT as u64));
    for (name, legacy) in [
        ("materialized_components", true),
        ("inline_components", false),
    ] {
        group.bench_function(name, |b| {
            b.iter(|| {
                let mut found = 0usize;
                for path in black_box(&paths) {
                    found += usize::from(
                        rssp::profile::relative_asset_path(
                            black_box(fixture.relative_dir()),
                            black_box(path),
                            legacy,
                        )
                        .is_some(),
                    );
                }
                black_box(found);
            });
        });
    }
    group.finish();
}

fn bench_relative_asset_components(c: &mut Criterion) {
    let paths = assets_bench::relative_component_paths();
    assets_bench::assert_rel_component_behavior(&paths);
    let bytes = paths.iter().map(String::len).sum::<usize>();
    let mut group = c.benchmark_group("asset_relative_components");
    group.sample_size(100);
    group.measurement_time(Duration::from_secs(3));
    group.throughput(Throughput::Bytes(bytes as u64));
    for (name, legacy) in [
        ("materialized_components", true),
        ("inline_components", false),
    ] {
        group.bench_function(name, |b| {
            b.iter(|| {
                let checksum = black_box(&paths).iter().fold(0u64, |checksum, path| {
                    checksum.rotate_left(1)
                        ^ rssp::profile::relative_asset_parts_hash(black_box(path), legacy)
                });
                black_box(checksum);
            });
        });
    }
    group.finish();
}

fn bench_song_assets(c: &mut Criterion) {
    let fixture = assets_bench::AssetFixture::new();
    fixture.assert_song_assets_behavior();

    let mut group = c.benchmark_group("song_assets");
    group.sample_size(30);
    group.measurement_time(Duration::from_secs(3));
    group.throughput(Throughput::Elements(
        (assets_bench::IMAGE_COUNT + assets_bench::NON_IMAGE_COUNT) as u64,
    ));
    group.bench_function("full_candidate_paths", |b| {
        b.iter(|| {
            black_box(rssp::profile::song_assets_legacy(
                black_box(fixture.image_dir()),
                black_box(""),
                black_box(""),
            ))
        });
    });
    group.bench_function("candidate_names", |b| {
        b.iter(|| {
            black_box(rssp::assets::resolve_song_assets(
                black_box(fixture.image_dir()),
                black_box(""),
                black_box(""),
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

fn legacy_first_two_paths(paths: &[PathBuf]) -> (&Path, &Path) {
    let mut candidates: Vec<_> = paths.iter().map(PathBuf::as_path).collect();
    candidates.sort_by_cached_key(|path| lowercase_name(path));
    (candidates[0], candidates[1])
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

fn legacy_find_name_ci<'a>(names: &'a [OsString], expected: &str) -> Option<&'a OsStr> {
    let expected = expected.to_ascii_lowercase();
    names
        .iter()
        .find(|name| name.to_string_lossy().to_ascii_lowercase() == expected)
        .map(OsString::as_os_str)
}

fn allocation_free_find_name_ci<'a>(names: &'a [OsString], expected: &str) -> Option<&'a OsStr> {
    names
        .iter()
        .find(|name| rssp::profile::name_eq_ci(name, expected))
        .map(OsString::as_os_str)
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
    selection.bench_function("allocation_free_first", |b| {
        b.iter(|| {
            black_box(
                rssp::profile::first_path_ci(black_box(&paths))
                    .expect("benchmark paths should not be empty"),
            )
        });
    });
    selection.bench_function("legacy_key_sort_first_two", |b| {
        b.iter(|| black_box(legacy_first_two_paths(black_box(&paths))));
    });
    selection.bench_function("allocation_free_first_two", |b| {
        b.iter(|| black_box(rssp::profile::first_two_paths_ci(black_box(&paths))));
    });
    selection.finish();

    let names: Vec<_> = (0..256)
        .map(|index| OsString::from(format!("Asset-{index:03}.DAT")))
        .collect();
    let expected = "asset-255.dat";
    let mut name_lookup = c.benchmark_group("asset_name_lookup_256");
    name_lookup.bench_function("legacy_lowercase", |b| {
        b.iter(|| {
            black_box(
                legacy_find_name_ci(black_box(&names), black_box(expected))
                    .expect("legacy benchmark name should resolve"),
            )
        });
    });
    name_lookup.bench_function("allocation_free", |b| {
        b.iter(|| {
            black_box(
                allocation_free_find_name_ci(black_box(&names), black_box(expected))
                    .expect("allocation-free benchmark name should resolve"),
            )
        });
    });
    name_lookup.finish();

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

fn bench_hint_normalize(c: &mut Criterion) {
    pack_bench::assert_hint_norm_behavior();
    let mut group = c.benchmark_group("pack_hint_normalize");
    group.sample_size(200);
    group.measurement_time(Duration::from_secs(2));
    group.throughput(Throughput::Elements(pack_bench::HINT_NORM_BATCH as u64));
    for (name, legacy) in [("owned", true), ("borrowed", false)] {
        group.bench_function(name, |b| {
            b.iter(|| {
                for _ in 0..pack_bench::HINT_NORM_BATCH {
                    black_box(rssp::pack::profile_normalized_img_hint(
                        black_box(pack_bench::HINT_NORM_INPUT),
                        legacy,
                    ));
                }
            });
        });
    }
    group.finish();
}

fn bench_path_sort(c: &mut Criterion) {
    path_sort_bench::assert_behavior();
    let paths = path_sort_bench::paths();
    let mut group = c.benchmark_group("path_sort_ci");
    group.sample_size(100);
    group.measurement_time(Duration::from_secs(2));
    group.throughput(Throughput::Elements(path_sort_bench::PATH_COUNT as u64));
    for (name, legacy) in [("cached_strings", true), ("contiguous_keys", false)] {
        group.bench_function(name, |b| {
            b.iter_batched(
                || paths.clone(),
                |mut paths| {
                    rssp::profile::sort_paths_ci(black_box(&mut paths), legacy);
                    black_box(paths);
                },
                BatchSize::SmallInput,
            );
        });
    }
    group.finish();
}

criterion_group!(
    benches,
    bench_pack_scan,
    bench_songs_root,
    bench_parent_img,
    bench_subdir_img,
    bench_song_scan,
    bench_simfile_tree,
    bench_pack_root,
    bench_background_changes,
    bench_delimiter_scan,
    bench_asset_fallbacks,
    bench_relative_asset_paths,
    bench_relative_asset_components,
    bench_song_assets,
    bench_selection_algorithms,
    bench_hint_normalize,
    bench_path_sort
);
criterion_main!(benches);

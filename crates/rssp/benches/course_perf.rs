use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use std::hint::black_box;
use std::time::Duration;

#[path = "support/course.rs"]
mod course_bench;

fn analyze_course(
    fixture: &course_bench::CourseFixture,
    options: rssp::AnalysisOptions,
) -> rssp::CourseSummary {
    rssp::course::analyze_crs_path(
        black_box(fixture.course_path()),
        Some(black_box(fixture.songs_dir())),
        black_box("dance-single"),
        black_box("Medium"),
        black_box(options),
    )
    .expect("benchmark course should analyze")
}

fn analyze_course_cache_all(
    fixture: &course_bench::CourseFixture,
    options: rssp::AnalysisOptions,
) -> rssp::CourseSummary {
    rssp::course::analyze_crs_path_cache_all_for_bench(
        black_box(fixture.course_path()),
        Some(black_box(fixture.songs_dir())),
        black_box("dance-single"),
        black_box("Medium"),
        black_box(options),
    )
    .expect("benchmark course should analyze")
}

fn bench_course_analysis(c: &mut Criterion) {
    let fixture = course_bench::CourseFixture::new();
    let fast_options = course_bench::fast_options();
    let clone_heavy_options = course_bench::clone_heavy_options();

    let mut group = c.benchmark_group("course_analysis");
    group.sample_size(20);
    group.measurement_time(Duration::from_secs(3));
    group.throughput(Throughput::Elements(course_bench::SONG_COUNT as u64));
    group.bench_function("analyze_64_cache_all", |b| {
        b.iter(|| {
            black_box(analyze_course_cache_all(
                &fixture,
                black_box(fast_options.clone()),
            ));
        });
    });
    group.bench_function("analyze_64_fast", |b| {
        b.iter(|| {
            black_box(analyze_course(&fixture, black_box(fast_options.clone())));
        });
    });
    group.bench_function("analyze_64_clone_heavy", |b| {
        b.iter(|| {
            black_box(analyze_course(
                &fixture,
                black_box(clone_heavy_options.clone()),
            ));
        });
    });
    group.finish();
}

fn bench_course_parse(c: &mut Criterion) {
    let fixture = course_bench::CourseFixture::new();
    let input = std::fs::read(fixture.course_path()).expect("benchmark course should be readable");
    let parsed = rssp::course::parse_crs(&input).expect("benchmark course should parse");
    assert_eq!(parsed.entries.len(), course_bench::SONG_COUNT);
    assert_eq!(
        parsed.entries.first().map(|entry| &entry.song),
        Some(&rssp::course::CourseSong::Fixed {
            group: Some("Group".to_string()),
            song: "Song000".to_string(),
        })
    );
    assert_eq!(
        parsed.entries.last().map(|entry| &entry.song),
        Some(&rssp::course::CourseSong::Fixed {
            group: Some("Group".to_string()),
            song: "Song063".to_string(),
        })
    );
    let separators = rssp::course::parse_crs(
        b"#COURSE:Separators;\n#SONG:Group\\Song:Challenge:;\n#SONG:Nested\\Group\\*:Challenge:;",
    )
    .expect("backslash course should parse");
    assert_eq!(
        separators.entries[0].song,
        rssp::course::CourseSong::Fixed {
            group: Some("Group".to_string()),
            song: "Song".to_string(),
        }
    );
    assert_eq!(
        separators.entries[1].song,
        rssp::course::CourseSong::RandomWithinGroup {
            group: "Nested/Group".to_string(),
        }
    );
    let difficulties = rssp::course::parse_crs(
        b"#COURSE:Difficulties;\n#SONG:Song: Expert :;\n#SONG:Song:LIGHT:;\n#SONG:Song:difficult:;\n#SONG:Song:12..14:;\n#SONG:Song: Custom :;\n#SONG:Song:\xA0Expert\xA0:;\n#SONG:Song:\xA0:;",
    )
    .expect("difficulty aliases should parse");
    assert_eq!(
        difficulties
            .entries
            .iter()
            .map(|entry| entry.steps.clone())
            .collect::<Vec<_>>(),
        [
            rssp::course::StepsSpec::Difficulty(rssp::course::Difficulty::Challenge),
            rssp::course::StepsSpec::Difficulty(rssp::course::Difficulty::Easy),
            rssp::course::StepsSpec::Difficulty(rssp::course::Difficulty::Medium),
            rssp::course::StepsSpec::MeterRange { low: 12, high: 14 },
            rssp::course::StepsSpec::Unknown {
                raw: "Custom".to_string(),
            },
            rssp::course::StepsSpec::Difficulty(rssp::course::Difficulty::Challenge),
            rssp::course::StepsSpec::Unknown { raw: String::new() },
        ]
    );
    let fields = rssp::course::parse_crs(
        b"#COURSE:Fields;\n#SONG:Group/Song:Hard:mod\\:value:ignored;\n#SONG:Solo;",
    )
    .expect("fixed song fields should parse");
    assert_eq!(fields.entries[0].modifiers, "mod\\:value");
    assert_eq!(
        fields.entries[0].steps,
        rssp::course::StepsSpec::Difficulty(rssp::course::Difficulty::Hard)
    );
    assert_eq!(
        fields.entries[1].song,
        rssp::course::CourseSong::Fixed {
            group: None,
            song: "Solo".to_string(),
        }
    );
    assert_eq!(
        fields.entries[1].steps,
        rssp::course::StepsSpec::Unknown { raw: String::new() }
    );
    let selected = rssp::course::parse_crs(
        b"#COURSE:Select;\n#SONGSELECT:TITLE=thank u\\, next:GROUP=A,B:ARTIST=Artist\\=Name:MODS=2x,noshowcourse,nodifficult;",
    )
    .expect("song selection should parse");
    let rssp::course::CourseSong::Select(selection) = &selected.entries[0].song else {
        panic!("benchmark selection entry should retain criteria");
    };
    assert_eq!(selection.titles, ["thank u, next"]);
    assert_eq!(selection.groups, ["A", "B"]);
    assert_eq!(selection.artists, ["Artist=Name"]);
    assert_eq!(selected.entries[0].modifiers, "2x");
    assert!(selected.entries[0].secret);
    assert!(selected.entries[0].no_difficult);

    let mut group = c.benchmark_group("course_parse");
    group.sample_size(100);
    group.measurement_time(Duration::from_secs(3));
    group.throughput(Throughput::Elements(course_bench::SONG_COUNT as u64));
    group.bench_function("parse_64", |b| {
        b.iter(|| {
            black_box(
                rssp::course::parse_crs(black_box(&input)).expect("benchmark course should parse"),
            );
        });
    });
    group.finish();
}

fn bench_song_mods(c: &mut Criterion) {
    assert_eq!(
        rssp::course::profile_song_mods(true, course_bench::MODS),
        (
            false,
            true,
            2,
            "1.5x,reverse,mirror,noholds,nomines,sudden".to_string(),
        )
    );
    assert_eq!(
        rssp::course::profile_song_mods(true, "showcourse,nodifficult,award3"),
        (false, true, 3, String::new())
    );

    let mut group = c.benchmark_group("course_song_mods");
    group.sample_size(200);
    group.measurement_time(Duration::from_secs(3));
    group.throughput(Throughput::Elements(course_bench::MOD_COUNT));
    group.bench_function("apply", |b| {
        b.iter(|| {
            black_box(rssp::course::profile_song_mods(
                black_box(true),
                black_box(course_bench::MODS),
            ));
        });
    });
    group.finish();
}

fn bench_select_mods(c: &mut Criterion) {
    assert_eq!(
        rssp::course::profile_select_mods(course_bench::SELECT_MODS),
        (
            true,
            true,
            "1.5x,reverse,mirror,noholds,nomines,sudden".to_string(),
        )
    );
    assert_eq!(
        rssp::course::profile_select_mods(b"reverse\\,invert,showcourse"),
        (false, false, "reverse,invert".to_string())
    );
    assert_eq!(
        rssp::course::profile_select_mods(b"noshowcourse,nodifficult"),
        (true, true, String::new())
    );

    let mut group = c.benchmark_group("course_select_mods");
    group.sample_size(200);
    group.measurement_time(Duration::from_secs(3));
    group.throughput(Throughput::Elements(course_bench::SELECT_MOD_COUNT));
    group.bench_function("apply", |b| {
        b.iter(|| {
            black_box(rssp::course::profile_select_mods(black_box(
                course_bench::SELECT_MODS,
            )));
        });
    });
    group.finish();
}

fn bench_select_parse(c: &mut Criterion) {
    let input = course_bench::select_input();
    let parsed = rssp::course::parse_crs(&input).expect("selection benchmark should parse");
    assert_eq!(parsed.entries.len(), course_bench::SELECT_COUNT);
    let rssp::course::CourseSong::Select(first) = &parsed.entries[0].song else {
        panic!("first benchmark entry should be a selection");
    };
    assert_eq!(first.titles, ["Song000", "Alt 000"]);
    assert_eq!(first.groups, ["Group A", "Group B"]);
    assert_eq!(first.difficulties.len(), 2);
    assert_eq!(first.meter_range, Some((8, 12)));
    assert_eq!(first.sort, Some(rssp::course::SongSort::FewestPlays));
    assert_eq!(first.index, 3);
    assert_eq!(parsed.entries[0].modifiers, "2x");
    assert!(parsed.entries[0].secret);
    assert!(parsed.entries[0].no_difficult);
    let rssp::course::CourseSong::Select(last) = &parsed.entries[63].song else {
        panic!("last benchmark entry should be a selection");
    };
    assert_eq!(last.titles, ["Song063", "Alt 063"]);
    let invalid = rssp::course::parse_crs(
        b"#COURSE:Invalid Select;\n#SONGSELECT:TITLE=A=B;\n#SONGSELECT:TITLE;\n#SONGSELECT:TITLE=Good;",
    )
    .expect("invalid selection entries should be skipped");
    assert_eq!(invalid.entries.len(), 1);
    let rssp::course::CourseSong::Select(valid) = &invalid.entries[0].song else {
        panic!("remaining benchmark entry should be a selection");
    };
    assert_eq!(valid.titles, ["Good"]);

    let mut group = c.benchmark_group("course_select_parse");
    group.sample_size(100);
    group.measurement_time(Duration::from_secs(3));
    group.throughput(Throughput::Elements(
        course_bench::SELECT_COUNT as u64 * course_bench::SELECT_PARAMS,
    ));
    group.bench_function("parse_64", |b| {
        b.iter(|| {
            black_box(
                rssp::course::parse_crs(black_box(&input))
                    .expect("selection benchmark should parse"),
            );
        });
    });
    group.finish();
}

fn bench_stepstype_match(c: &mut Criterion) {
    const CASES: [(&str, &str); 8] = [
        ("dance-single", "dance-single"),
        (" DANCE_SINGLE ", "dance-single"),
        ("dance-double", "dance-single"),
        ("DANCE-SOLO", "dance-single"),
        ("pump_single", "pump-single"),
        ("lights-cabinet", "lights-cabinet"),
        ("kb7-single", "dance-single"),
        ("非ASCII-single", "dance-single"),
    ];

    let mut group = c.benchmark_group("course_stepstype_match");
    group.sample_size(200);
    group.measurement_time(Duration::from_secs(2));
    group.throughput(Throughput::Elements(CASES.len() as u64));
    group.bench_function("allocating", |b| {
        b.iter(|| {
            for (raw, normalized) in CASES {
                black_box(rssp::course::profile_stepstype_eq_legacy(
                    black_box(raw),
                    black_box(normalized),
                ));
            }
        });
    });
    group.bench_function("bytes", |b| {
        b.iter(|| {
            for (raw, normalized) in CASES {
                black_box(rssp::course::profile_stepstype_eq(
                    black_box(raw),
                    black_box(normalized),
                ));
            }
        });
    });
    group.finish();
}

fn course_patterns(count: usize) -> Vec<rssp::patterns::CustomPatternSummary> {
    const DIRECTIONS: [u8; 4] = *b"LDUR";
    (0..count)
        .map(|mut value| {
            let mut bytes = [b'L'; 8];
            for byte in &mut bytes {
                *byte = DIRECTIONS[value & 3];
                value >>= 2;
            }
            rssp::patterns::CustomPatternSummary {
                pattern: String::from_utf8(bytes.to_vec()).expect("directions are valid UTF-8"),
                count: 1,
            }
        })
        .collect()
}

fn bench_pattern_merge(c: &mut Criterion) {
    const CHARTS: usize = 64;
    let chart = course_patterns(256);
    let mut group = c.benchmark_group("course_pattern_merge");
    group.sample_size(100);
    group.measurement_time(Duration::from_secs(2));
    group.throughput(Throughput::Elements((chart.len() * CHARTS) as u64));
    group.bench_function("linear_find_sort", |b| {
        b.iter(|| {
            let mut total = Vec::new();
            for _ in 0..CHARTS {
                rssp::profile::merge_course_patterns_legacy(
                    black_box(&mut total),
                    black_box(&chart),
                );
            }
            black_box(total);
        });
    });
    group.bench_function("binary_insert", |b| {
        b.iter(|| {
            let mut total = Vec::new();
            for _ in 0..CHARTS {
                rssp::profile::merge_course_patterns(black_box(&mut total), black_box(&chart));
            }
            black_box(total);
        });
    });
    group.finish();
}

fn bench_course_banner(c: &mut Criterion) {
    let fixture = course_bench::BannerFixture::new();
    let legacy = rssp::course::profile_course_banner(fixture.course_path(), "", true);
    let current = rssp::course::profile_course_banner(fixture.course_path(), "", false);
    assert_eq!(current, legacy, "course banner selection must not change");
    assert_eq!(current, Some(fixture.expected_banner()));

    let mut group = c.benchmark_group("course_banner_258");
    group.sample_size(20);
    group.measurement_time(Duration::from_secs(3));
    group.throughput(Throughput::Elements(
        course_bench::BANNER_ENTRY_COUNT as u64,
    ));
    group.bench_function("legacy_five_scans", |b| {
        b.iter(|| {
            black_box(rssp::course::profile_course_banner(
                black_box(fixture.course_path()),
                black_box(""),
                true,
            ))
        });
    });
    group.bench_function("one_scan_ranked", |b| {
        b.iter(|| {
            black_box(rssp::course::profile_course_banner(
                black_box(fixture.course_path()),
                black_box(""),
                false,
            ))
        });
    });
    group.finish();
}

criterion_group!(
    benches,
    bench_course_analysis,
    bench_course_parse,
    bench_song_mods,
    bench_select_mods,
    bench_select_parse,
    bench_stepstype_match,
    bench_pattern_merge,
    bench_course_banner
);
criterion_main!(benches);

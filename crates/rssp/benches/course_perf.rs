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

fn assert_same_course(left: &rssp::course::CourseFile, right: &rssp::course::CourseFile) {
    assert_eq!(left.name, right.name);
    assert_eq!(left.name_translit, right.name_translit);
    assert_eq!(left.scripter, right.scripter);
    assert_eq!(left.description, right.description);
    assert_eq!(left.banner, right.banner);
    assert_eq!(left.background, right.background);
    assert_eq!(left.repeat, right.repeat);
    assert_eq!(left.lives, right.lives);
    assert_eq!(left.meters, right.meters);
    assert_eq!(left.entries, right.entries);
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
    let legacy = rssp::course::profile_parse_crs(&input, true)
        .expect("legacy benchmark course should parse");
    assert_same_course(&parsed, &legacy);
    assert_eq!(parsed.entries.len(), course_bench::SONG_COUNT);
    assert!(parsed.repeat);
    assert_eq!(
        parsed.meters,
        [Some(3), Some(6), Some(9), Some(12), Some(15), Some(18)]
    );
    const CONTROL_CASES: [&[u8]; 4] = [
        b"#COURSE:Controls;#REPEAT:\xA0mAyBe YES\xA0;#METER: Beginner : -3 :HARD:12:Odd;",
        b"#COURSE:Single;#REPEAT:not enabled;#METER: 17 ;",
        b"#COURSE:Empty;#REPEAT:ye\xFFs;#METER:;",
        b"#COURSE:Escaped;#METER:Hard\\:Alias:99:Easy:7;",
    ];
    for control in CONTROL_CASES {
        let current = rssp::course::profile_parse_crs(control, false)
            .expect("control edge case should parse");
        let legacy = rssp::course::profile_parse_crs(control, true)
            .expect("legacy control edge case should parse");
        assert_same_course(&current, &legacy);
    }
    let controls = rssp::course::profile_parse_crs(CONTROL_CASES[0], false)
        .expect("control values should parse");
    assert!(controls.repeat);
    assert_eq!(controls.meters, [Some(0), None, None, Some(12), None, None]);
    let single = rssp::course::profile_parse_crs(CONTROL_CASES[1], false)
        .expect("single meter should parse");
    assert!(!single.repeat);
    assert_eq!(single.meters, [None, None, Some(17), None, None, None]);
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
    group.bench_function("legacy_control_allocs", |b| {
        b.iter(|| {
            black_box(
                rssp::course::profile_parse_crs(black_box(&input), true)
                    .expect("benchmark course should parse"),
            );
        });
    });
    group.bench_function("stream_control_fields", |b| {
        b.iter(|| {
            black_box(
                rssp::course::profile_parse_crs(black_box(&input), false)
                    .expect("benchmark course should parse"),
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

fn bench_stepstype_normalize(c: &mut Criterion) {
    course_bench::assert_step_norm_behavior();
    let mut group = c.benchmark_group("course_stepstype_normalize");
    group.sample_size(200);
    group.measurement_time(Duration::from_secs(2));
    group.throughput(Throughput::Elements(
        (course_bench::STEP_NORM_CASES.len() * course_bench::STEP_NORM_BATCH) as u64,
    ));
    group.bench_function("two_owned_passes", |b| {
        b.iter(|| {
            for _ in 0..course_bench::STEP_NORM_BATCH {
                for raw in course_bench::STEP_NORM_CASES {
                    black_box(rssp::course::profile_normalize_stepstype(
                        black_box(raw),
                        true,
                    ));
                }
            }
        });
    });
    group.bench_function("borrow_or_one_pass", |b| {
        b.iter(|| {
            for _ in 0..course_bench::STEP_NORM_BATCH {
                for raw in course_bench::STEP_NORM_CASES {
                    black_box(rssp::course::profile_normalize_stepstype(
                        black_box(raw),
                        false,
                    ));
                }
            }
        });
    });
    group.finish();
}

fn bench_title_match(c: &mut Criterion) {
    course_bench::assert_title_match_behavior();
    let mut group = c.benchmark_group("course_title_match");
    group.sample_size(200);
    group.measurement_time(Duration::from_secs(2));
    group.throughput(Throughput::Elements(course_bench::TITLE_MATCH_BATCH as u64));
    group.bench_function("owned_full_title", |b| {
        b.iter(|| {
            for _ in 0..course_bench::TITLE_MATCH_BATCH {
                black_box(rssp::course::profile_simfile_title_eq(
                    black_box(course_bench::TITLE_MATCH_INPUT),
                    black_box("ssc"),
                    black_box(course_bench::TITLE_MATCH_EXPECTED),
                    true,
                ));
            }
        });
    });
    group.bench_function("borrowed_parts", |b| {
        b.iter(|| {
            for _ in 0..course_bench::TITLE_MATCH_BATCH {
                black_box(rssp::course::profile_simfile_title_eq(
                    black_box(course_bench::TITLE_MATCH_INPUT),
                    black_box("ssc"),
                    black_box(course_bench::TITLE_MATCH_EXPECTED),
                    false,
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
    fixture.assert_behavior();

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
    group.bench_function("one_scan_full_path_stats", |b| {
        b.iter(|| {
            black_box(rssp::course::profile_course_banner_full_paths(
                black_box(fixture.course_path()),
                black_box(""),
            ))
        });
    });
    group.bench_function("one_scan_entry_types", |b| {
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

fn bench_song_resolve(c: &mut Criterion) {
    let fixture = course_bench::ResolveFixture::new();
    fixture.assert_behavior();

    let mut group = c.benchmark_group("course_song_resolve_384");
    group.sample_size(20);
    group.measurement_time(Duration::from_secs(3));
    group.throughput(Throughput::Elements(
        course_bench::RESOLVE_ENTRY_COUNT as u64,
    ));
    group.bench_function("full_paths_metadata_keys", |b| {
        b.iter(|| {
            black_box(rssp::course::profile_resolve_song_dir(
                black_box(fixture.songs_dir()),
                None,
                black_box(course_bench::RESOLVE_SONG),
                true,
            ))
        });
    });
    group.bench_function("entry_types_names", |b| {
        b.iter(|| {
            black_box(rssp::course::profile_resolve_song_dir(
                black_box(fixture.songs_dir()),
                None,
                black_box(course_bench::RESOLVE_SONG),
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
    bench_stepstype_normalize,
    bench_title_match,
    bench_pattern_merge,
    bench_course_banner,
    bench_song_resolve
);
criterion_main!(benches);

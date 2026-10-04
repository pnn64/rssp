use crate::perf::measure_prepared;
use std::hint::black_box;

fn names(count: usize, order: &str) -> Vec<String> {
    (0..count)
        .map(|index| {
            let key = match order {
                "reverse" => count - index,
                "cycle" => (index + count / 3) % count,
                "mixed" => index * 17 % count,
                "equal" => 0,
                "sorted" => index,
                _ => unreachable!("fixture order"),
            };
            let prefix = if index % 2 == 0 { "Pack" } else { "pACK" };
            format!("{prefix} {key:06}")
        })
        .collect()
}

fn pack(name: &str, index: usize) -> super::PackScan {
    super::PackScan {
        dir: std::path::PathBuf::new(),
        group_name: name.into(),
        display_title: String::new(),
        sort_title: String::new(),
        translit_title: String::new(),
        series: String::new(),
        year: i32::try_from(index).expect("small fixture"),
        version: 0,
        has_pack_ini: false,
        sync_pref: super::SyncPref::Default,
        banner_path: None,
        background_path: None,
        songs: Vec::new(),
    }
}

#[test]
fn pack_sort_edges() {
    for count in [0, 1, 4, 5, 32, 257] {
        for order in ["sorted", "reverse", "cycle", "mixed", "equal"] {
            let mut names = names(count, order);
            if count > 4 && order == "mixed" {
                names[1] = "歌A".into();
                names[2] = "歌a".into();
                names[3] = "é".into();
                names[4] = "É".into();
            }
            let mut expected: Vec<_> = (0..count).collect();
            expected.sort_by(|&left, &right| {
                crate::assets::cmp_ascii_ci(names[left].as_bytes(), names[right].as_bytes())
            });
            let mut packs: Vec<_> = names
                .iter()
                .enumerate()
                .map(|(index, name)| pack(name, index))
                .collect();
            super::sort_packs_ci(&mut packs);
            assert_eq!(
                packs
                    .iter()
                    .map(|pack| pack.year as usize)
                    .collect::<Vec<_>>(),
                expected,
                "{count} {order}"
            );
            for (pack, index) in packs.iter().zip(expected) {
                assert_eq!(pack.group_name, names[index]);
            }
        }
    }
}

#[test]
#[ignore = "explicit pack sorting benchmark"]
fn pack_hotpath() {
    for count in [1, 4, 5, 32, 256, 4096] {
        for order in ["sorted", "reverse", "cycle", "mixed", "equal"] {
            let names = names(count, order);
            let values: Vec<_> = names
                .iter()
                .enumerate()
                .map(|(i, s)| (s.as_str(), i))
                .collect();
            measure_prepared(
                &format!("pack_sort/{count}_{order}"),
                count,
                || values.clone(),
                |values| {
                    super::sort_compact_ci(
                        black_box(values),
                        24,
                        |value, text| {
                            text.extend(value.0.as_bytes().iter().map(u8::to_ascii_lowercase));
                        },
                        |a, b| crate::assets::cmp_ascii_ci(a.0.as_bytes(), b.0.as_bytes()),
                    );
                    black_box(values);
                },
            );
            if count <= 256 {
                let packs: Vec<_> = names.iter().enumerate().map(|(i, s)| pack(s, i)).collect();
                measure_prepared(
                    &format!("pack_list/{count}_{order}"),
                    count,
                    || packs.clone(),
                    |packs| {
                        super::sort_packs_ci(black_box(packs));
                        black_box(packs);
                    },
                );
            }
        }
    }
}

#[test]
#[ignore = "explicit pack ordering comparison"]
fn pack_trace() {
    for count in [0, 1, 4, 5, 32, 257] {
        for order in ["sorted", "reverse", "cycle", "mixed", "equal"] {
            let mut packs: Vec<_> = names(count, order)
                .iter()
                .enumerate()
                .map(|(i, s)| pack(s, i))
                .collect();
            super::sort_packs_ci(&mut packs);
            println!(
                "pack-order {count} {order} {:?}",
                packs
                    .iter()
                    .map(|pack| (&pack.group_name, pack.year))
                    .collect::<Vec<_>>()
            );
        }
    }
}

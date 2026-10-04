use crate::perf::measure;
use std::hint::black_box;
use std::path::Path;

#[test]
fn bg_decode_edges() {
    let files = super::BgFileCatalog::from_files(Vec::new());
    let calls = std::cell::Cell::new(0);
    let result = super::resolve_bgchanges_with(
        Path::new(""),
        [b"0=\\-random-=\xe9,4=\\-random-,8=\\-nosongbg-".as_slice()],
        &files,
        || {
            calls.set(calls.get() + 1);
            Some("movie.mp4".into())
        },
    );
    assert_eq!(calls.get(), 1);
    assert_eq!(result.len(), 2);
    assert_eq!(result[0].start_beat.to_bits(), 0.0f32.to_bits());
    assert_eq!(result[1].start_beat.to_bits(), 4.0f32.to_bits());
    assert!(
        result
            .iter()
            .all(|change| change.target == super::BackgroundChangeTarget::Random)
    );
}

#[test]
#[ignore = "explicit background parser benchmark"]
fn bg_hotpath() {
    let files = super::BgFileCatalog::from_files(Vec::new());
    for count in [1, 32] {
        for kind in ["plain", "escaped", "cp1252", "cp_escape"] {
            let mut data = Vec::new();
            for index in 0..count {
                if index != 0 {
                    data.push(b',');
                }
                data.extend(format!("{}=", index * 4).bytes());
                if kind == "escaped" || kind == "cp_escape" {
                    data.push(b'\\');
                }
                data.extend_from_slice(b"-random-=");
                data.extend(vec![if kind.starts_with("cp") { 0xe9 } else { b'a' }; 128]);
            }
            measure(&format!("background/{count}_{kind}"), count, || {
                black_box(super::resolve_bgchanges_with(
                    Path::new(""),
                    [black_box(data.as_slice())],
                    &files,
                    || None,
                ));
            });
        }
    }
}

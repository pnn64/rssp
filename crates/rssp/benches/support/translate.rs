pub const MARKER_COUNT: usize = 512;

pub fn unknown_input() -> String {
    let mut input = String::with_capacity(MARKER_COUNT * 24);
    for index in 0..MARKER_COUNT {
        use std::fmt::Write as _;
        write!(&mut input, "prefix&unknown{index};suffix")
            .expect("writing to a String should work");
    }
    input
}

pub fn alias_input() -> String {
    const ALIASES: [&str; 8] = [
        "&hka;",
        "&KRO;",
        "&rightarrow;",
        "&whiteheart;",
        "&kdot;",
        "&#9733;",
        "&#x266F;",
        "&omega;",
    ];

    let mut input = String::with_capacity(MARKER_COUNT * 10);
    for index in 0..MARKER_COUNT {
        input.push_str(ALIASES[index % ALIASES.len()]);
    }
    input
}

pub fn assert_behavior() {
    let generated_aliases = alias_input();
    let generated_unknown = unknown_input();
    for input in [
        "",
        "plain UTF-8 å‹•ç”»",
        "prefix&unknown;suffix",
        "&bad&hka;tail",
        "&hka;&KRO;&#9733;&#x266F;",
        "unterminated&hka",
        "malformed&#xZZ;marker",
        "invalid&#999999999999999999999999;codepoint",
        generated_aliases.as_str(),
        generated_unknown.as_str(),
    ] {
        let mut legacy = input.to_owned();
        let mut compact = input.to_owned();
        rssp::translate::profile_replace_markers(&mut legacy, true);
        rssp::translate::profile_replace_markers(&mut compact, false);
        assert_eq!(compact, legacy, "marker translation changed for {input:?}");
    }
}

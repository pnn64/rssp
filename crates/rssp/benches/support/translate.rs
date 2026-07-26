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

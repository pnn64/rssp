const SHA1_INIT: [u32; 5] = [
    0x67452301,
    0xefcdab89,
    0x98badcfe,
    0x10325476,
    0xc3d2e1f0,
];

const SHA1_K: [[u32; 4]; 4] = [
    [0x5a827999; 4],
    [0x6ed9eba1; 4],
    [0x8f1bbcdc; 4],
    [0xca62c1d6; 4],
];

#[inline(always)]
fn add4(a: [u32; 4], b: [u32; 4]) -> [u32; 4] {
    [
        a[0].wrapping_add(b[0]),
        a[1].wrapping_add(b[1]),
        a[2].wrapping_add(b[2]),
        a[3].wrapping_add(b[3]),
    ]
}

#[inline(always)]
fn xor4(a: [u32; 4], b: [u32; 4]) -> [u32; 4] {
    [a[0] ^ b[0], a[1] ^ b[1], a[2] ^ b[2], a[3] ^ b[3]]
}

#[inline(always)]
fn sha1_first_add(e: u32, w0: [u32; 4]) -> [u32; 4] {
    let [a, b, c, d] = w0;
    [e.wrapping_add(a), b, c, d]
}

#[inline(always)]
fn sha1msg1(a: [u32; 4], b: [u32; 4]) -> [u32; 4] {
    let [_, _, w2, w3] = a;
    let [w4, w5, _, _] = b;
    [a[0] ^ w2, a[1] ^ w3, a[2] ^ w4, a[3] ^ w5]
}

#[inline(always)]
fn sha1msg2(a: [u32; 4], b: [u32; 4]) -> [u32; 4] {
    let [x0, x1, x2, x3] = a;
    let [_, w13, w14, w15] = b;

    let w16 = (x0 ^ w13).rotate_left(1);
    let w17 = (x1 ^ w14).rotate_left(1);
    let w18 = (x2 ^ w15).rotate_left(1);
    let w19 = (x3 ^ w16).rotate_left(1);

    [w16, w17, w18, w19]
}

#[inline(always)]
fn sha1_first_half(abcd: [u32; 4], msg: [u32; 4]) -> [u32; 4] {
    sha1_first_add(abcd[0].rotate_left(30), msg)
}

#[inline(always)]
fn sha1rnds4c(abcd: [u32; 4], msg: [u32; 4]) -> [u32; 4] {
    let [mut a, mut b, mut c, mut d] = abcd;
    let [t, u, v, w] = msg;
    let mut e = 0u32;

    macro_rules! ch {
        ($a:expr, $b:expr, $c:expr) => {
            $c ^ ($a & ($b ^ $c))
        };
    }

    e = e
        .wrapping_add(a.rotate_left(5))
        .wrapping_add(ch!(b, c, d))
        .wrapping_add(t);
    b = b.rotate_left(30);

    d = d
        .wrapping_add(e.rotate_left(5))
        .wrapping_add(ch!(a, b, c))
        .wrapping_add(u);
    a = a.rotate_left(30);

    c = c
        .wrapping_add(d.rotate_left(5))
        .wrapping_add(ch!(e, a, b))
        .wrapping_add(v);
    e = e.rotate_left(30);

    b = b
        .wrapping_add(c.rotate_left(5))
        .wrapping_add(ch!(d, e, a))
        .wrapping_add(w);
    d = d.rotate_left(30);

    [b, c, d, e]
}

#[inline(always)]
fn sha1rnds4p(abcd: [u32; 4], msg: [u32; 4]) -> [u32; 4] {
    let [mut a, mut b, mut c, mut d] = abcd;
    let [t, u, v, w] = msg;
    let mut e = 0u32;

    macro_rules! parity {
        ($a:expr, $b:expr, $c:expr) => {
            $a ^ $b ^ $c
        };
    }

    e = e
        .wrapping_add(a.rotate_left(5))
        .wrapping_add(parity!(b, c, d))
        .wrapping_add(t);
    b = b.rotate_left(30);

    d = d
        .wrapping_add(e.rotate_left(5))
        .wrapping_add(parity!(a, b, c))
        .wrapping_add(u);
    a = a.rotate_left(30);

    c = c
        .wrapping_add(d.rotate_left(5))
        .wrapping_add(parity!(e, a, b))
        .wrapping_add(v);
    e = e.rotate_left(30);

    b = b
        .wrapping_add(c.rotate_left(5))
        .wrapping_add(parity!(d, e, a))
        .wrapping_add(w);
    d = d.rotate_left(30);

    [b, c, d, e]
}

#[inline(always)]
fn sha1rnds4m(abcd: [u32; 4], msg: [u32; 4]) -> [u32; 4] {
    let [mut a, mut b, mut c, mut d] = abcd;
    let [t, u, v, w] = msg;
    let mut e = 0u32;

    macro_rules! maj {
        ($a:expr, $b:expr, $c:expr) => {
            ($a & $b) | (($a | $b) & $c)
        };
    }

    e = e
        .wrapping_add(a.rotate_left(5))
        .wrapping_add(maj!(b, c, d))
        .wrapping_add(t);
    b = b.rotate_left(30);

    d = d
        .wrapping_add(e.rotate_left(5))
        .wrapping_add(maj!(a, b, c))
        .wrapping_add(u);
    a = a.rotate_left(30);

    c = c
        .wrapping_add(d.rotate_left(5))
        .wrapping_add(maj!(e, a, b))
        .wrapping_add(v);
    e = e.rotate_left(30);

    b = b
        .wrapping_add(c.rotate_left(5))
        .wrapping_add(maj!(d, e, a))
        .wrapping_add(w);
    d = d.rotate_left(30);

    [b, c, d, e]
}

#[inline(always)]
fn sha1_digest_round_x4<const I: usize>(abcd: [u32; 4], work: [u32; 4]) -> [u32; 4] {
    let work = add4(work, SHA1_K[I]);
    match I {
        0 => sha1rnds4c(abcd, work),
        1 | 3 => sha1rnds4p(abcd, work),
        2 => sha1rnds4m(abcd, work),
        _ => unreachable!(),
    }
}

macro_rules! rounds4 {
    ($h0:ident, $h1:ident, $wk:expr, $i:literal) => {
        sha1_digest_round_x4::<$i>($h0, sha1_first_half($h1, $wk))
    };
}

macro_rules! schedule {
    ($v0:expr, $v1:expr, $v2:expr, $v3:expr) => {
        sha1msg2(xor4(sha1msg1($v0, $v1), $v2), $v3)
    };
}

macro_rules! schedule_rounds4 {
    (
        $h0:ident, $h1:ident,
        $w0:expr, $w1:expr, $w2:expr, $w3:expr, $w4:expr,
        $i:literal
    ) => {
        $w4 = schedule!($w0, $w1, $w2, $w3);
        $h1 = rounds4!($h0, $h1, $w4, $i);
    };
}

#[inline(always)]
fn sha1_digest_block_u32(state: &mut [u32; 5], block: &[u32; 16]) {
    let mut w0 = [block[0], block[1], block[2], block[3]];
    let mut w1 = [block[4], block[5], block[6], block[7]];
    let mut w2 = [block[8], block[9], block[10], block[11]];
    let mut w3 = [block[12], block[13], block[14], block[15]];
    #[allow(clippy::needless_late_init)]
    let mut w4;

    let mut h0 = [state[0], state[1], state[2], state[3]];
    let mut h1 = sha1_first_add(state[4], w0);

    h1 = sha1_digest_round_x4::<0>(h0, h1);
    h0 = rounds4!(h1, h0, w1, 0);
    h1 = rounds4!(h0, h1, w2, 0);
    h0 = rounds4!(h1, h0, w3, 0);
    schedule_rounds4!(h0, h1, w0, w1, w2, w3, w4, 0);

    schedule_rounds4!(h1, h0, w1, w2, w3, w4, w0, 1);
    schedule_rounds4!(h0, h1, w2, w3, w4, w0, w1, 1);
    schedule_rounds4!(h1, h0, w3, w4, w0, w1, w2, 1);
    schedule_rounds4!(h0, h1, w4, w0, w1, w2, w3, 1);
    schedule_rounds4!(h1, h0, w0, w1, w2, w3, w4, 1);

    schedule_rounds4!(h0, h1, w1, w2, w3, w4, w0, 2);
    schedule_rounds4!(h1, h0, w2, w3, w4, w0, w1, 2);
    schedule_rounds4!(h0, h1, w3, w4, w0, w1, w2, 2);
    schedule_rounds4!(h1, h0, w4, w0, w1, w2, w3, 2);
    schedule_rounds4!(h0, h1, w0, w1, w2, w3, w4, 2);

    schedule_rounds4!(h1, h0, w1, w2, w3, w4, w0, 3);
    schedule_rounds4!(h0, h1, w2, w3, w4, w0, w1, 3);
    schedule_rounds4!(h1, h0, w3, w4, w0, w1, w2, 3);
    schedule_rounds4!(h0, h1, w4, w0, w1, w2, w3, 3);
    schedule_rounds4!(h1, h0, w0, w1, w2, w3, w4, 3);

    let e = h1[0].rotate_left(30);
    let [a, b, c, d] = h0;

    state[0] = state[0].wrapping_add(a);
    state[1] = state[1].wrapping_add(b);
    state[2] = state[2].wrapping_add(c);
    state[3] = state[3].wrapping_add(d);
    state[4] = state[4].wrapping_add(e);
}

#[inline(always)]
fn bytes_to_u32_be(chunk: &[u8]) -> u32 {
    u32::from_be_bytes([chunk[0], chunk[1], chunk[2], chunk[3]])
}

#[inline(always)]
fn sha1_compress_block(state: &mut [u32; 5], block: &[u8]) {
    let mut block_u32 = [0u32; 16];
    for (i, chunk) in block.chunks_exact(4).enumerate() {
        block_u32[i] = bytes_to_u32_be(chunk);
    }
    sha1_digest_block_u32(state, &block_u32);
}

#[inline(always)]
fn sha1_compress(state: &mut [u32; 5], blocks: &[[u8; 64]]) {
    for block in blocks {
        sha1_compress_block(state, block);
    }
}

#[inline(always)]
fn sha1_update(state: &mut [u32; 5], buf: &mut [u8; 64], buf_len: &mut usize, data: &[u8]) {
    let mut offset = 0usize;
    if *buf_len != 0 {
        let needed = 64 - *buf_len;
        if data.len() < needed {
            buf[*buf_len..*buf_len + data.len()].copy_from_slice(data);
            *buf_len += data.len();
            return;
        }
        buf[*buf_len..].copy_from_slice(&data[..needed]);
        sha1_compress(state, std::slice::from_ref(buf));
        *buf_len = 0;
        offset = needed;
    }

    let data = &data[offset..];
    for chunk in data.chunks_exact(64) {
        sha1_compress_block(state, chunk);
    }
    let rem = data.len() & 63;
    if rem != 0 {
        buf[..rem].copy_from_slice(&data[data.len() - rem..]);
        *buf_len = rem;
    }
}

#[inline(always)]
fn sha1_finish(
    state: &mut [u32; 5],
    buf: &mut [u8; 64],
    buf_len: usize,
    total_len: usize,
) -> [u8; 20] {
    let mut len = buf_len;
    buf[len] = 0x80;
    len += 1;

    if len > 56 {
        buf[len..].fill(0);
        sha1_compress(state, std::slice::from_ref(buf));
        len = 0;
    }

    buf[len..56].fill(0);
    buf[56..64].copy_from_slice(&((total_len as u64) << 3).to_be_bytes());
    sha1_compress(state, std::slice::from_ref(buf));

    let mut out = [0u8; 20];
    for (i, word) in state.iter().enumerate() {
        out[i * 4..][..4].copy_from_slice(&word.to_be_bytes());
    }
    out
}

#[inline(always)]
fn sha1_digest(first: &[u8], second: &[u8]) -> [u8; 20] {
    let mut state = SHA1_INIT;
    let mut buf = [0u8; 64];
    let mut buf_len = 0usize;
    sha1_update(&mut state, &mut buf, &mut buf_len, first);
    sha1_update(&mut state, &mut buf, &mut buf_len, second);
    sha1_finish(&mut state, &mut buf, buf_len, first.len() + second.len())
}

const HEX_TABLE: [[u8; 2]; 256] = {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut table = [[0u8; 2]; 256];
    let mut i = 0usize;
    while i < 256 {
        table[i][0] = HEX[i >> 4];
        table[i][1] = HEX[i & 0x0f];
        i += 1;
    }
    table
};

pub fn compute_chart_hash(chart_data: &[u8], normalized_bpms: &str) -> String {
    let digest = sha1_digest(chart_data, normalized_bpms.as_bytes());
    let mut out = [0u8; 16];
    for (i, &byte) in digest[..8].iter().enumerate() {
        let hex = HEX_TABLE[byte as usize];
        out[i * 2] = hex[0];
        out[i * 2 + 1] = hex[1];
    }
    String::from_utf8(out.to_vec()).expect("hex is always valid utf8")
}

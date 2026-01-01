const SHA1_INIT: [u32; 5] = [
    0x67452301,
    0xefcdab89,
    0x98badcfe,
    0x10325476,
    0xc3d2e1f0,
];

const SHA1_K: [u32; 4] = [0x5a827999, 0x6ed9eba1, 0x8f1bbcdc, 0xca62c1d6];

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
fn sha1_digest_round_x4(abcd: [u32; 4], work: [u32; 4], i: u8) -> [u32; 4] {
    match i {
        0 => sha1rnds4c(abcd, add4(work, [SHA1_K[0]; 4])),
        1 => sha1rnds4p(abcd, add4(work, [SHA1_K[1]; 4])),
        2 => sha1rnds4m(abcd, add4(work, [SHA1_K[2]; 4])),
        3 => sha1rnds4p(abcd, add4(work, [SHA1_K[3]; 4])),
        _ => unreachable!("unknown sha1 round"),
    }
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
            ($a & $b) ^ ($a & $c) ^ ($b & $c)
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

macro_rules! rounds4 {
    ($h0:ident, $h1:ident, $wk:expr, $i:expr) => {
        sha1_digest_round_x4($h0, sha1_first_half($h1, $wk), $i)
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
        $i:expr
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

    h1 = sha1_digest_round_x4(h0, h1, 0);
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
fn sha1_compress(state: &mut [u32; 5], blocks: &[[u8; 64]]) {
    let mut block_u32 = [0u32; 16];
    let mut state_cpy = *state;
    for block in blocks.iter() {
        for (o, chunk) in block_u32.iter_mut().zip(block.chunks_exact(4)) {
            *o = u32::from_be_bytes(chunk.try_into().unwrap());
        }
        sha1_digest_block_u32(&mut state_cpy, &block_u32);
    }
    *state = state_cpy;
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
    let blocks_len = data.len() >> 6;
    if blocks_len != 0 {
        let blocks = unsafe {
            // SAFETY: blocks_len is data.len() / 64, so the slice is in-bounds.
            std::slice::from_raw_parts(data.as_ptr() as *const [u8; 64], blocks_len)
        };
        sha1_compress(state, blocks);
    }
    let rem = data.len() & 63;
    if rem != 0 {
        let start = data.len() - rem;
        buf[..rem].copy_from_slice(&data[start..]);
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
    let bit_len = (total_len as u64) << 3;
    buf[56..64].copy_from_slice(&bit_len.to_be_bytes());
    sha1_compress(state, std::slice::from_ref(buf));

    let mut out = [0u8; 20];
    for (i, word) in state.iter().enumerate() {
        let base = i * 4;
        out[base..base + 4].copy_from_slice(&word.to_be_bytes());
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

pub fn compute_chart_hash(chart_data: &[u8], normalized_bpms: &str) -> String {
    let digest = sha1_digest(chart_data, normalized_bpms.as_bytes());
    let mut out = String::with_capacity(16);
    const HEX: &[u8; 16] = b"0123456789abcdef";
    for &byte in digest[..8].iter() {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}

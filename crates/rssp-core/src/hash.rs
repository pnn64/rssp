// SHA-1 notation conventionally names its state words a-e and message words
// w/t/u/v; retaining that notation keeps the unrolled rounds reviewable.
#![allow(clippy::many_single_char_names)]

const SHA1_INIT: [u32; 5] = [
    0x6745_2301,
    0xefcd_ab89,
    0x98ba_dcfe,
    0x1032_5476,
    0xc3d2_e1f0,
];

const SHA1_K: [[u32; 4]; 4] = [
    [0x5a82_7999; 4],
    [0x6ed9_eba1; 4],
    [0x8f1b_bcdc; 4],
    [0xca62_c1d6; 4],
];

const STREAM_BUFFER_LEN: usize = 8 * 1024;

/// Reusable measure-row and byte storage for hashing multiple charts in sequence.
///
/// Includes 8 KiB of inline staging storage, initialized once when scratch is created.
pub struct NoteHashScratch {
    rows: NoteHashRows,
    staging: [u8; STREAM_BUFFER_LEN],
}

impl Default for NoteHashScratch {
    fn default() -> Self {
        Self {
            rows: NoteHashRows::Empty,
            staging: [0; STREAM_BUFFER_LEN],
        }
    }
}

#[derive(Default)]
enum NoteHashRows {
    #[default]
    Empty,
    Rows4(Vec<[u8; 4]>),
    Rows5(Vec<[u8; 5]>),
    Rows8(Vec<[u8; 8]>),
    Rows10(Vec<[u8; 10]>),
}

struct Sha1Stream<'a> {
    state: [u32; 5],
    block: [u8; 64],
    block_len: usize,
    total_len: usize,
    staging: &'a mut [u8; STREAM_BUFFER_LEN],
    staging_len: usize,
}

impl<'a> Sha1Stream<'a> {
    fn new(staging: &'a mut [u8; STREAM_BUFFER_LEN]) -> Self {
        // Only staging[..staging_len] is read, so previous chart bytes need no clearing.
        Self {
            state: SHA1_INIT,
            block: [0; 64],
            block_len: 0,
            total_len: 0,
            staging,
            staging_len: 0,
        }
    }

    #[inline(always)]
    fn write(&mut self, bytes: &[u8]) {
        if bytes.len() > self.staging.len() {
            self.flush();
            sha1_update(&mut self.state, &mut self.block, &mut self.block_len, bytes);
            self.total_len += bytes.len();
            return;
        }
        if bytes.len() > self.staging.len() - self.staging_len {
            self.flush();
        }
        let end = self.staging_len + bytes.len();
        self.staging[self.staging_len..end].copy_from_slice(bytes);
        self.staging_len = end;
    }

    #[inline(always)]
    fn flush(&mut self) {
        if self.staging_len == 0 {
            return;
        }
        sha1_update(
            &mut self.state,
            &mut self.block,
            &mut self.block_len,
            &self.staging[..self.staging_len],
        );
        self.total_len += self.staging_len;
        self.staging_len = 0;
    }

    fn finish(mut self, suffix: &[u8]) -> [u8; 20] {
        self.flush();
        sha1_digest_suffix(
            self.state,
            self.block,
            self.block_len,
            self.total_len,
            suffix,
        )
    }
}

#[inline(always)]
const fn add4(a: [u32; 4], b: [u32; 4]) -> [u32; 4] {
    [
        a[0].wrapping_add(b[0]),
        a[1].wrapping_add(b[1]),
        a[2].wrapping_add(b[2]),
        a[3].wrapping_add(b[3]),
    ]
}

#[inline(always)]
const fn xor4(a: [u32; 4], b: [u32; 4]) -> [u32; 4] {
    [a[0] ^ b[0], a[1] ^ b[1], a[2] ^ b[2], a[3] ^ b[3]]
}

#[inline(always)]
const fn sha1_first_add(e: u32, w0: [u32; 4]) -> [u32; 4] {
    let [a, b, c, d] = w0;
    [e.wrapping_add(a), b, c, d]
}

#[inline(always)]
const fn sha1msg1(a: [u32; 4], b: [u32; 4]) -> [u32; 4] {
    let [_, _, w2, w3] = a;
    let [w4, w5, _, _] = b;
    [a[0] ^ w2, a[1] ^ w3, a[2] ^ w4, a[3] ^ w5]
}

#[inline(always)]
const fn sha1msg2(a: [u32; 4], b: [u32; 4]) -> [u32; 4] {
    let [x0, x1, x2, x3] = a;
    let [_, w13, w14, w15] = b;

    let w16 = (x0 ^ w13).rotate_left(1);
    let w17 = (x1 ^ w14).rotate_left(1);
    let w18 = (x2 ^ w15).rotate_left(1);
    let w19 = (x3 ^ w16).rotate_left(1);

    [w16, w17, w18, w19]
}

#[inline(always)]
const fn sha1_first_half(abcd: [u32; 4], msg: [u32; 4]) -> [u32; 4] {
    sha1_first_add(abcd[0].rotate_left(30), msg)
}

#[inline(always)]
const fn sha1rnds4c(abcd: [u32; 4], msg: [u32; 4]) -> [u32; 4] {
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
const fn sha1rnds4p(abcd: [u32; 4], msg: [u32; 4]) -> [u32; 4] {
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
const fn sha1rnds4m(abcd: [u32; 4], msg: [u32; 4]) -> [u32; 4] {
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
    for (i, chunk) in block.as_chunks::<4>().0.iter().enumerate() {
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
    let offset = if *buf_len != 0 {
        let needed = 64 - *buf_len;
        if data.len() < needed {
            buf[*buf_len..*buf_len + data.len()].copy_from_slice(data);
            *buf_len += data.len();
            return;
        }
        buf[*buf_len..].copy_from_slice(&data[..needed]);
        sha1_compress(state, std::slice::from_ref(buf));
        *buf_len = 0;
        needed
    } else {
        0usize
    };

    let data = &data[offset..];
    for chunk in data.as_chunks::<64>().0 {
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

#[inline(always)]
fn sha1_digest_suffix(
    mut state: [u32; 5],
    mut buf: [u8; 64],
    mut buf_len: usize,
    prefix_len: usize,
    suffix: &[u8],
) -> [u8; 20] {
    sha1_update(&mut state, &mut buf, &mut buf_len, suffix);
    sha1_finish(&mut state, &mut buf, buf_len, prefix_len + suffix.len())
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

fn short_hex(digest: &[u8; 20]) -> String {
    let mut out = String::with_capacity(16);
    for &byte in &digest[..8] {
        let hex = HEX_TABLE[byte as usize];
        out.push(hex[0] as char);
        out.push(hex[1] as char);
    }
    out
}

#[must_use]
pub fn compute_chart_hash(chart_data: &[u8], normalized_bpms: &str) -> String {
    short_hex(&sha1_digest(chart_data, normalized_bpms.as_bytes()))
}

#[must_use]
pub fn compute_chart_hash_pair(chart_data: &[u8], normalized_bpms: &str) -> (String, String) {
    let mut state = SHA1_INIT;
    let mut buf = [0u8; 64];
    let mut buf_len = 0usize;
    sha1_update(&mut state, &mut buf, &mut buf_len, chart_data);

    let hash = sha1_digest_suffix(
        state,
        buf,
        buf_len,
        chart_data.len(),
        normalized_bpms.as_bytes(),
    );
    let neutral = sha1_digest_suffix(state, buf, buf_len, chart_data.len(), b"0.000=0.000");
    (short_hex(&hash), short_hex(&neutral))
}

/// Computes a chart hash while streaming minimized note rows into SHA-1.
///
/// This is equivalent to calling [`crate::stats::minimize_chart_for_hash`],
/// trimming its trailing newline, and then calling [`compute_chart_hash`],
/// without materializing the minimized chart.
#[must_use]
pub fn compute_note_data_hash(note_data: &[u8], lanes: usize, normalized_bpms: &str) -> String {
    compute_note_data_hash_with_scratch(
        note_data,
        lanes,
        normalized_bpms,
        &mut NoteHashScratch::default(),
    )
}

/// Computes a minimized note-data hash while retaining measure storage in `scratch`.
#[must_use]
pub fn compute_note_data_hash_with_scratch(
    note_data: &[u8],
    lanes: usize,
    normalized_bpms: &str,
    scratch: &mut NoteHashScratch,
) -> String {
    fn hash_lanes<const LANES: usize>(
        note_data: &[u8],
        normalized_bpms: &str,
        rows: &mut Vec<[u8; LANES]>,
        staging: &mut [u8; STREAM_BUFFER_LEN],
    ) -> String {
        let mut stream = Sha1Stream::new(staging);
        let mut pending_newline = false;

        crate::stats::for_each_minimized_measure_in::<LANES, _>(
            note_data,
            rows,
            |_, measure, separator| {
                for row in measure {
                    if pending_newline {
                        stream.write(b"\n");
                    }
                    stream.write(row);
                    pending_newline = true;
                }
                if separator {
                    if pending_newline {
                        stream.write(b"\n");
                    }
                    stream.write(b",");
                    pending_newline = true;
                }
            },
        );

        short_hex(&stream.finish(normalized_bpms.as_bytes()))
    }

    macro_rules! hash_rows {
        ($variant:ident, $lanes:literal) => {{
            if !matches!(scratch.rows, NoteHashRows::$variant(_)) {
                scratch.rows = NoteHashRows::$variant(Vec::new());
            }
            let NoteHashRows::$variant(rows) = &mut scratch.rows else {
                unreachable!()
            };
            hash_lanes::<$lanes>(note_data, normalized_bpms, rows, &mut scratch.staging)
        }};
    }
    match lanes {
        5 => hash_rows!(Rows5, 5),
        8 => hash_rows!(Rows8, 8),
        10 => hash_rows!(Rows10, 10),
        _ => hash_rows!(Rows4, 4),
    }
}

#[cfg(test)]
mod tests {
    use super::{compute_chart_hash, compute_chart_hash_pair};

    // Independent SHA-1 vectors generated with Python hashlib over bytes
    // (i * 37 + 11) % 256. Cover padding, block, and staging boundaries.
    const SHA1_CASES: &[(usize, [u8; 20])] = &[
        (
            0,
            [
                0xda, 0x39, 0xa3, 0xee, 0x5e, 0x6b, 0x4b, 0x0d, 0x32, 0x55, 0xbf, 0xef, 0x95, 0x60,
                0x18, 0x90, 0xaf, 0xd8, 0x07, 0x09,
            ],
        ),
        (
            1,
            [
                0x06, 0x7d, 0x50, 0x96, 0xf2, 0x19, 0xc6, 0x4b, 0x53, 0xbb, 0x1c, 0x7d, 0x5e, 0x37,
                0x54, 0x28, 0x5b, 0x56, 0x5a, 0x47,
            ],
        ),
        (
            55,
            [
                0xc4, 0x62, 0x20, 0x48, 0xcf, 0xef, 0x59, 0xb7, 0x28, 0x75, 0x83, 0x9e, 0xe7, 0xae,
                0x1c, 0xbc, 0xf5, 0x5e, 0x76, 0x58,
            ],
        ),
        (
            56,
            [
                0xdd, 0xc1, 0x29, 0x42, 0x65, 0x64, 0x68, 0x47, 0x59, 0x70, 0xfa, 0x4f, 0xa4, 0x91,
                0x61, 0xf5, 0x2e, 0xd1, 0x38, 0xe4,
            ],
        ),
        (
            63,
            [
                0x7f, 0x8c, 0x3f, 0xa4, 0x9f, 0x12, 0x97, 0xbd, 0x8b, 0x9f, 0xeb, 0x96, 0x4b, 0x6b,
                0x41, 0x99, 0x87, 0xf9, 0xf0, 0xd1,
            ],
        ),
        (
            64,
            [
                0xa3, 0x34, 0xb4, 0x71, 0x80, 0xc6, 0x1f, 0xd5, 0x22, 0xf9, 0x99, 0x05, 0xec, 0x02,
                0xc3, 0x6f, 0x9e, 0x84, 0x82, 0x11,
            ],
        ),
        (
            65,
            [
                0xdd, 0x27, 0xd9, 0xeb, 0x92, 0x3d, 0x39, 0x68, 0x7e, 0x10, 0x87, 0x2c, 0x3e, 0x81,
                0x33, 0xba, 0x2f, 0x0a, 0x68, 0xa1,
            ],
        ),
        (
            119,
            [
                0xbe, 0xa9, 0x49, 0x47, 0x3b, 0x1e, 0xc3, 0x47, 0x47, 0xce, 0x12, 0x1c, 0x32, 0x93,
                0x62, 0x4b, 0x5d, 0x9d, 0x8f, 0x84,
            ],
        ),
        (
            120,
            [
                0xbf, 0x05, 0x26, 0x6a, 0xcd, 0x3e, 0xc2, 0x15, 0x92, 0xb4, 0xd4, 0x2a, 0xae, 0xa9,
                0x7f, 0xa6, 0xf3, 0xe5, 0x19, 0x26,
            ],
        ),
        (
            127,
            [
                0xb2, 0xb4, 0xbf, 0xd7, 0xb2, 0x11, 0x2a, 0x16, 0x7b, 0x77, 0xa6, 0x00, 0xcc, 0xa2,
                0x27, 0x59, 0x35, 0x23, 0xc4, 0x06,
            ],
        ),
        (
            128,
            [
                0x3b, 0x19, 0x53, 0x09, 0x18, 0x99, 0x49, 0x23, 0x77, 0xf6, 0x86, 0xc2, 0x66, 0xb8,
                0x1d, 0x84, 0xb5, 0xd4, 0x0f, 0x70,
            ],
        ),
        (
            129,
            [
                0x4f, 0xd6, 0x55, 0x8b, 0x2a, 0x93, 0x92, 0x5f, 0xb7, 0x12, 0x94, 0x47, 0xe1, 0xd1,
                0xfa, 0xc8, 0xcf, 0xf5, 0x62, 0x87,
            ],
        ),
        (
            1023,
            [
                0xe1, 0xdf, 0x23, 0xf6, 0x81, 0xf0, 0x2f, 0x79, 0xf4, 0x7a, 0x9a, 0x18, 0x3a, 0xc4,
                0x89, 0xeb, 0x17, 0x68, 0xe3, 0x23,
            ],
        ),
        (
            1024,
            [
                0x3d, 0x43, 0x69, 0x5d, 0x5e, 0x94, 0x5c, 0xea, 0x89, 0x7d, 0x48, 0x9c, 0xff, 0x9f,
                0xf4, 0x5b, 0xb0, 0x19, 0x49, 0x8b,
            ],
        ),
        (
            1025,
            [
                0x4c, 0xd9, 0x29, 0x55, 0x2c, 0xd9, 0x81, 0xca, 0x4a, 0xdb, 0x46, 0xa2, 0xed, 0x98,
                0x4f, 0xd5, 0x2d, 0xb4, 0x5f, 0x63,
            ],
        ),
        (
            8191,
            [
                0x9d, 0xc1, 0xee, 0x7f, 0x06, 0x6f, 0x90, 0xaf, 0xe5, 0xb2, 0xc7, 0x4e, 0x97, 0xe4,
                0xa4, 0x5c, 0x3a, 0x4b, 0x0e, 0x3d,
            ],
        ),
        (
            8192,
            [
                0x70, 0x6e, 0x3f, 0xc6, 0xa9, 0x3a, 0x81, 0x6c, 0x4b, 0xb4, 0x35, 0xf1, 0x66, 0xfa,
                0x60, 0xe9, 0x69, 0xa1, 0x8d, 0x35,
            ],
        ),
        (
            8193,
            [
                0x97, 0xa5, 0x88, 0x1d, 0x35, 0x34, 0x31, 0x54, 0x19, 0x1c, 0x8f, 0x5e, 0x69, 0x2c,
                0x0e, 0xeb, 0x56, 0x5e, 0xc8, 0x78,
            ],
        ),
    ];

    #[test]
    fn sha1_known_answers() {
        for &(len, expected) in SHA1_CASES {
            let data: Vec<u8> = (0..len)
                .map(|i| (i as u8).wrapping_mul(37).wrapping_add(11))
                .collect();
            for split in [
                0,
                1.min(len),
                55.min(len),
                56.min(len),
                63.min(len),
                64.min(len),
                len / 2,
                len,
            ] {
                assert_eq!(
                    super::sha1_digest(&data[..split], &data[split..]),
                    expected,
                    "len={len}, split={split}"
                );
            }
            for size in [
                5,
                64,
                super::STREAM_BUFFER_LEN,
                super::STREAM_BUFFER_LEN + 1,
            ] {
                let mut staging = [0; super::STREAM_BUFFER_LEN];
                let mut stream = super::Sha1Stream::new(&mut staging);
                for chunk in data.chunks(size) {
                    stream.write(chunk);
                }
                assert_eq!(
                    stream.finish(b""),
                    expected,
                    "stream len={len}, chunk={size}"
                );
            }
            let (hash, neutral) = compute_chart_hash_pair(&data, "0.000=150.000");
            assert_eq!(hash, compute_chart_hash(&data, "0.000=150.000"));
            assert_eq!(neutral, compute_chart_hash(&data, "0.000=0.000"));
        }
    }

    #[test]
    fn streamed_hash_matches() {
        let mut scratch = super::NoteHashScratch::default();
        for lanes in [4, 5, 8, 10, 4] {
            for data in [
                &b""[..],
                &b",\n,\n"[..],
                &b"// comment\r\n1000000000\r\n0000000000\r\n,\n0000000000\n"[..],
                &b"2000000000\n0000000000\n3000000000\nMFLK000000\n,\n"[..],
            ] {
                // Repetition crosses the staging boundary for every lane count.
                for repeats in [1, 1024] {
                    let notes = data.repeat(repeats);
                    let mut minimized = crate::stats::minimize_chart_for_hash(&notes, lanes);
                    if minimized.last() == Some(&b'\n') {
                        minimized.pop();
                    }
                    let expected = compute_chart_hash(&minimized, "0.000=150.000");
                    assert_eq!(
                        super::compute_note_data_hash(&notes, lanes, "0.000=150.000"),
                        expected
                    );
                    assert_eq!(
                        super::compute_note_data_hash_with_scratch(
                            &notes,
                            lanes,
                            "0.000=150.000",
                            &mut scratch
                        ),
                        expected
                    );
                }
            }
        }
    }

    #[test]
    fn chart_hash_pair_matches_individual_hashes() {
        let chart = b"1000\n0100\n0010\n0001\n";
        let bpms = "0.000=140.000,64.000=175.000";
        let (hash, neutral) = compute_chart_hash_pair(chart, bpms);

        assert_eq!(hash, compute_chart_hash(chart, bpms));
        assert_eq!(neutral, compute_chart_hash(chart, "0.000=0.000"));
    }
}

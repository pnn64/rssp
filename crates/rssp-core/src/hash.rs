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

/// Reusable measure-row storage for hashing multiple charts in sequence.
#[derive(Default)]
pub struct NoteHashScratch {
    rows: NoteHashRows,
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

struct Sha1Stream {
    state: [u32; 5],
    block: [u8; 64],
    block_len: usize,
    total_len: usize,
    staging: [u8; STREAM_BUFFER_LEN],
    staging_len: usize,
}

impl Sha1Stream {
    fn new() -> Self {
        Self {
            state: SHA1_INIT,
            block: [0; 64],
            block_len: 0,
            total_len: 0,
            staging: [0; STREAM_BUFFER_LEN],
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
    ) -> String {
        let mut stream = Sha1Stream::new();
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
            hash_lanes::<$lanes>(note_data, normalized_bpms, rows)
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
    use super::{NoteHashScratch, compute_note_data_hash_with_scratch};
    use super::{compute_chart_hash, compute_chart_hash_pair, compute_note_data_hash};

    #[test]
    fn chart_hash_pair_matches_individual_hashes() {
        let chart = b"1000\n0100\n0010\n0001\n";
        let bpms = "0.000=140.000,64.000=175.000";
        let (hash, neutral) = compute_chart_hash_pair(chart, bpms);

        assert_eq!(hash, compute_chart_hash(chart, bpms));
        assert_eq!(neutral, compute_chart_hash(chart, "0.000=0.000"));
    }

    #[test]
    fn streamed_note_hash_matches_materialized_minimization() {
        let cases: [(&[u8], usize); 4] = [
            (b"// comment\n1000\n0000\n0100\n,\n0010\n0001\n;\n", 4),
            (b"10000000\n00000000\n,\n00001000\n;\n", 8),
            (b"\n,\n0000\n,\n;\n", 4),
            (b" 1000 trailing\n0100\r\n", 4),
        ];
        let bpms = "0.000=120.000,64.000=180.000";

        let mut scratch = NoteHashScratch::default();
        for (note_data, lanes) in cases {
            let mut minimized = crate::stats::minimize_chart_for_hash(note_data, lanes);
            if let Some(pos) = minimized.iter().rposition(|&byte| byte != b'\n') {
                minimized.truncate(pos + 1);
            }
            assert_eq!(
                compute_note_data_hash(note_data, lanes, bpms),
                compute_chart_hash(&minimized, bpms)
            );
            assert_eq!(
                compute_note_data_hash_with_scratch(note_data, lanes, bpms, &mut scratch),
                compute_chart_hash(&minimized, bpms)
            );
        }
    }
}

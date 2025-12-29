const SHA1_INIT: [u32; 5] = [
    0x67452301,
    0xefcdab89,
    0x98badcfe,
    0x10325476,
    0xc3d2e1f0,
];

fn sha1_block(state: &mut [u32; 5], block: &[u8; 64]) {
    let mut words = [0u32; 80];
    for (i, word) in words[..16].iter_mut().enumerate() {
        let base = i * 4;
        *word = u32::from_be_bytes([
            block[base],
            block[base + 1],
            block[base + 2],
            block[base + 3],
        ]);
    }
    for i in 16..80 {
        words[i] = (words[i - 3] ^ words[i - 8] ^ words[i - 14] ^ words[i - 16]).rotate_left(1);
    }

    let mut a = state[0];
    let mut b = state[1];
    let mut c = state[2];
    let mut d = state[3];
    let mut e = state[4];

    for i in 0..80 {
        let (f, k) = if i < 20 {
            ((b & c) | ((!b) & d), 0x5a827999)
        } else if i < 40 {
            (b ^ c ^ d, 0x6ed9eba1)
        } else if i < 60 {
            ((b & c) | (b & d) | (c & d), 0x8f1bbcdc)
        } else {
            (b ^ c ^ d, 0xca62c1d6)
        };

        let temp = a
            .rotate_left(5)
            .wrapping_add(f)
            .wrapping_add(e)
            .wrapping_add(k)
            .wrapping_add(words[i]);
        e = d;
        d = c;
        c = b.rotate_left(30);
        b = a;
        a = temp;
    }

    state[0] = state[0].wrapping_add(a);
    state[1] = state[1].wrapping_add(b);
    state[2] = state[2].wrapping_add(c);
    state[3] = state[3].wrapping_add(d);
    state[4] = state[4].wrapping_add(e);
}

fn sha1_update(state: &mut [u32; 5], buf: &mut [u8; 64], buf_len: &mut usize, data: &[u8]) {
    let mut offset = 0usize;
    if *buf_len != 0 {
        let needed = 64 - *buf_len;
        if data.len() < needed {
            buf[*buf_len..*buf_len + data.len()].copy_from_slice(data);
            *buf_len += data.len();
            return;
        }
        buf[*buf_len..64].copy_from_slice(&data[..needed]);
        sha1_block(state, buf);
        *buf_len = 0;
        offset = needed;
    }

    let chunks = data[offset..].chunks_exact(64);
    let remainder = chunks.remainder();
    for chunk in chunks {
        let block: &[u8; 64] = chunk.try_into().expect("chunked to 64 bytes");
        sha1_block(state, block);
    }
    if !remainder.is_empty() {
        buf[..remainder.len()].copy_from_slice(remainder);
        *buf_len = remainder.len();
    }
}

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
        sha1_block(state, buf);
        len = 0;
    }

    buf[len..56].fill(0);
    let bit_len = (total_len as u64) << 3;
    buf[56..64].copy_from_slice(&bit_len.to_be_bytes());
    sha1_block(state, buf);

    let mut out = [0u8; 20];
    for (i, word) in state.iter().enumerate() {
        let base = i * 4;
        out[base..base + 4].copy_from_slice(&word.to_be_bytes());
    }
    out
}

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

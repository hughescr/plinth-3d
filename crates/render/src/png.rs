//! A small PNG writer: 8-bit RGB, Sub filter, zlib with fixed Huffman codes and a greedy
//! LZ77 match finder. No dependencies. Flat colour areas, which the review pictures are made
//! of, compress to a few percent of their raw size.

const LEN_BASE: [u16; 29] = [
    3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131,
    163, 195, 227, 258,
];
const LEN_EXTRA: [u8; 29] = [
    0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0,
];
const DIST_BASE: [u16; 30] = [
    1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537,
    2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577,
];
const DIST_EXTRA: [u8; 30] = [
    0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13,
    13,
];

struct Bits {
    out: Vec<u8>,
    acc: u64,
    n: u32,
}

impl Bits {
    /// Append `n` low bits of `v`, least significant first.
    fn put(&mut self, v: u32, n: u32) {
        self.acc |= u64::from(v) << self.n;
        self.n += n;
        while self.n >= 8 {
            self.out.push((self.acc & 0xFF) as u8);
            self.acc >>= 8;
            self.n -= 8;
        }
    }

    /// Append a Huffman code of `n` bits, most significant bit first.
    fn code(&mut self, c: u32, n: u32) {
        let mut r = 0;
        for i in 0..n {
            r |= ((c >> i) & 1) << (n - 1 - i);
        }
        self.put(r, n);
    }

    fn lit(&mut self, sym: u32) {
        match sym {
            0..=143 => self.code(0x30 + sym, 8),
            144..=255 => self.code(0x190 + sym - 144, 9),
            256..=279 => self.code(sym - 256, 7),
            _ => self.code(0xC0 + sym - 280, 8),
        }
    }

    fn finish(mut self) -> Vec<u8> {
        if self.n > 0 {
            self.out.push((self.acc & 0xFF) as u8);
        }
        self.out
    }
}

fn length_code(len: usize) -> (u32, u32, u32) {
    let i = LEN_BASE.partition_point(|&b| usize::from(b) <= len) - 1;
    (
        257 + i as u32,
        (len - usize::from(LEN_BASE[i])) as u32,
        u32::from(LEN_EXTRA[i]),
    )
}

fn dist_code(d: usize) -> (u32, u32, u32) {
    let i = DIST_BASE.partition_point(|&b| usize::from(b) <= d) - 1;
    (
        i as u32,
        (d - usize::from(DIST_BASE[i])) as u32,
        u32::from(DIST_EXTRA[i]),
    )
}

/// Deflate `data` as one block with fixed Huffman codes.
fn deflate(data: &[u8]) -> Vec<u8> {
    let mut b = Bits {
        out: Vec::with_capacity(data.len() / 8 + 16),
        acc: 0,
        n: 0,
    };
    b.put(1, 1);
    b.put(1, 2);
    const HASH: usize = 1 << 15;
    let mut head = vec![usize::MAX; HASH];
    let mut prev = vec![usize::MAX; data.len()];
    let hash = |i: usize| -> usize {
        ((usize::from(data[i]) << 10) ^ (usize::from(data[i + 1]) << 5) ^ usize::from(data[i + 2]))
            & (HASH - 1)
    };
    let mut i = 0;
    while i < data.len() {
        let (mut best_len, mut best_dist) = (0, 0);
        if i + 3 <= data.len() {
            let h = hash(i);
            let mut cand = head[h];
            let mut tries = 0;
            while cand != usize::MAX && i - cand <= 32768 && tries < 24 {
                let max = (data.len() - i).min(258);
                let mut l = 0;
                while l < max && data[cand + l] == data[i + l] {
                    l += 1;
                }
                if l > best_len {
                    best_len = l;
                    best_dist = i - cand;
                    if l == max {
                        break;
                    }
                }
                cand = prev[cand];
                tries += 1;
            }
        }
        let step = if best_len >= 3 {
            let (sym, extra, nbits) = length_code(best_len);
            b.lit(sym);
            b.put(extra, nbits);
            let (dsym, dextra, dbits) = dist_code(best_dist);
            b.code(dsym, 5);
            b.put(dextra, dbits);
            best_len
        } else {
            b.lit(u32::from(data[i]));
            1
        };
        let end = (i + step).min(data.len().saturating_sub(2));
        for (k, p) in prev.iter_mut().enumerate().take(end).skip(i) {
            let h = hash(k);
            *p = head[h];
            head[h] = k;
        }
        i += step;
    }
    b.lit(256);
    b.finish()
}

/// CRC-32 (ISO 3309) as PNG uses it.
#[must_use]
pub fn crc32(bytes: &[u8]) -> u32 {
    let mut table = [0_u32; 256];
    for (n, t) in table.iter_mut().enumerate() {
        let mut c = n as u32;
        for _ in 0..8 {
            c = if c & 1 == 1 {
                0xEDB8_8320 ^ (c >> 1)
            } else {
                c >> 1
            };
        }
        *t = c;
    }
    !bytes.iter().fold(!0_u32, |c, &b| {
        table[((c ^ u32::from(b)) & 0xFF) as usize] ^ (c >> 8)
    })
}

/// Adler-32 as zlib uses it.
#[must_use]
pub fn adler32(bytes: &[u8]) -> u32 {
    let (mut a, mut b) = (1_u32, 0_u32);
    for &x in bytes {
        a = (a + u32::from(x)) % 65521;
        b = (b + a) % 65521;
    }
    (b << 16) | a
}

fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], body: &[u8]) {
    out.extend_from_slice(&(body.len() as u32).to_be_bytes());
    let mut c = kind.to_vec();
    c.extend_from_slice(body);
    out.extend_from_slice(&c);
    out.extend_from_slice(&crc32(&c).to_be_bytes());
}

/// Encode `rgb` (3 bytes per pixel, row by row) as a PNG file.
///
/// # Panics
/// Panics when `rgb` is not `w * h * 3` bytes long.
#[must_use]
pub fn encode_rgb(w: u32, h: u32, rgb: &[u8]) -> Vec<u8> {
    let stride = (w as usize) * 3;
    assert_eq!(rgb.len(), stride * h as usize, "pixel data size");
    let mut raw = Vec::with_capacity((stride + 1) * h as usize);
    for row in rgb.chunks_exact(stride.max(1)) {
        raw.push(1);
        for (i, &v) in row.iter().enumerate() {
            let left = if i >= 3 { row[i - 3] } else { 0 };
            raw.push(v.wrapping_sub(left));
        }
    }
    let mut z = vec![0x78, 0x01];
    z.extend_from_slice(&deflate(&raw));
    z.extend_from_slice(&adler32(&raw).to_be_bytes());
    let mut out = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&w.to_be_bytes());
    ihdr.extend_from_slice(&h.to_be_bytes());
    ihdr.extend_from_slice(&[8, 2, 0, 0, 0]);
    chunk(&mut out, b"IHDR", &ihdr);
    chunk(&mut out, b"IDAT", &z);
    chunk(&mut out, b"IEND", &[]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_check_values_match_the_known_answers() {
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
        assert_eq!(adler32(b"Wikipedia"), 0x11E6_0398);
    }

    /// Inflate a fixed-Huffman block back (the decoder side of what `deflate` writes), so the
    /// encoder is checked against an independent reading of the format.
    fn inflate_fixed(z: &[u8]) -> Vec<u8> {
        let (mut pos, mut out) = (0_usize, Vec::<u8>::new());
        let bit = |p: &mut usize| -> u32 {
            let v = (z[*p / 8] >> (*p % 8)) & 1;
            *p += 1;
            u32::from(v)
        };
        let bits = |p: &mut usize, n: u32| -> u32 { (0..n).fold(0, |a, i| a | (bit(p) << i)) };
        assert_eq!(bits(&mut pos, 1), 1);
        assert_eq!(bits(&mut pos, 2), 1);
        loop {
            // Read a literal/length symbol: 7, 8 or 9 bit codes, msb first.
            let mut c = 0;
            let mut len = 0;
            let sym = loop {
                c = (c << 1) | bit(&mut pos);
                len += 1;
                match (len, c) {
                    (7, 0..=0x17) => break 256 + c,
                    (8, 0x30..=0xBF) => break c - 0x30,
                    (8, 0xC0..=0xC7) => break 280 + c - 0xC0,
                    (9, 0x190..=0x1FF) => break 144 + c - 0x190,
                    _ => {}
                }
            };
            match sym {
                256 => break,
                0..=255 => out.push(sym as u8),
                _ => {
                    let i = (sym - 257) as usize;
                    let l =
                        usize::from(LEN_BASE[i]) + bits(&mut pos, u32::from(LEN_EXTRA[i])) as usize;
                    let mut dc = 0;
                    for _ in 0..5 {
                        dc = (dc << 1) | bit(&mut pos);
                    }
                    let d = usize::from(DIST_BASE[dc as usize])
                        + bits(&mut pos, u32::from(DIST_EXTRA[dc as usize])) as usize;
                    for _ in 0..l {
                        out.push(out[out.len() - d]);
                    }
                }
            }
        }
        out
    }

    #[test]
    fn deflate_round_trips_through_an_independent_inflate() {
        let mut data = Vec::new();
        for i in 0..5000_u32 {
            data.push((i % 7) as u8);
            data.push(if i % 100 < 60 {
                200
            } else {
                (i * 31 % 251) as u8
            });
        }
        data.extend(std::iter::repeat_n(9_u8, 3000));
        let z = deflate(&data);
        assert!(z.len() < data.len() / 2, "{} {}", z.len(), data.len());
        assert_eq!(inflate_fixed(&z), data);
    }

    #[test]
    fn a_png_has_the_signature_the_chunks_and_a_small_size_for_flat_colour() {
        let rgb = vec![40_u8; 300 * 200 * 3];
        let p = encode_rgb(300, 200, &rgb);
        assert_eq!(&p[..8], &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]);
        assert_eq!(&p[12..16], b"IHDR");
        assert_eq!(&p[p.len() - 8..p.len() - 4], b"IEND");
        assert!(p.len() < 6000, "{} bytes for {} raw", p.len(), rgb.len());
    }
}

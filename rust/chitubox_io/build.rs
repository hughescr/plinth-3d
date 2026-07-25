// build.rs — bakes SOURCE_DIGEST: a SHA-256 over the sorted src/*.rs contents
// plus the build-configuration files (Cargo.toml, Cargo.lock, build.rs,
// pyproject.toml). The Python conftest staleness gate recomputes the same
// digest over the same file set and FAILS chitubox-marked tests on a mismatch,
// so a stale extension can never certify itself green.
//
// Byte-for-byte the same algorithm as rust/geom_kernel/build.rs (only the
// emitted env-var name differs); scripts/tests/conftest.py drives both through
// one shared `_crate_source_digest(crate_dir)`.
//
// A minimal, dependency-free SHA-256 is implemented here so build.rs needs no
// build-dependencies (keeps the crate graph tiny and reproducible).

use std::fs;
use std::path::{Path, PathBuf};

fn main() {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());

    // Files that feed the digest, in a FIXED order (sorted src/*.rs then the
    // named config files). Matches conftest._kernel_source_digest exactly.
    let mut files: Vec<PathBuf> = Vec::new();

    let src_dir = manifest.join("src");
    let mut src_files: Vec<PathBuf> = fs::read_dir(&src_dir)
        .expect("src/ must exist")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().map(|x| x == "rs").unwrap_or(false))
        .collect();
    src_files.sort();
    files.extend(src_files);

    for name in ["Cargo.toml", "Cargo.lock", "build.rs", "pyproject.toml"] {
        let p = manifest.join(name);
        if p.exists() {
            files.push(p);
        }
    }

    let mut hasher = Sha256::new();
    for f in &files {
        // Hash the file's basename then its bytes, so a rename is detected.
        let name = f.file_name().unwrap().to_string_lossy();
        hasher.update(name.as_bytes());
        hasher.update(b"\0");
        let bytes = fs::read(f).unwrap_or_default();
        hasher.update(&(bytes.len() as u64).to_le_bytes());
        hasher.update(&bytes);
        println!("cargo:rerun-if-changed={}", f.display());
    }
    let digest = hasher.finalize_hex();
    println!("cargo:rustc-env=CHITUBOX_IO_SOURCE_DIGEST={}", digest);
    println!("cargo:rerun-if-changed=build.rs");
    let _ = Path::new(&manifest); // silence unused in some cfgs
}

// ---- minimal SHA-256 (public-domain style reference implementation) --------

struct Sha256 {
    state: [u32; 8],
    buf: Vec<u8>,
    len: u64,
}

const K: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

impl Sha256 {
    fn new() -> Self {
        Sha256 {
            state: [
                0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
                0x5be0cd19,
            ],
            buf: Vec::with_capacity(64),
            len: 0,
        }
    }
    fn update(&mut self, data: &[u8]) {
        self.len = self.len.wrapping_add(data.len() as u64);
        self.buf.extend_from_slice(data);
        while self.buf.len() >= 64 {
            let block: [u8; 64] = self.buf[0..64].try_into().unwrap();
            self.process(&block);
            self.buf.drain(0..64);
        }
    }
    fn process(&mut self, block: &[u8; 64]) {
        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes(block[i * 4..i * 4 + 4].try_into().unwrap());
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }
        let mut h = self.state;
        for i in 0..64 {
            let s1 = h[4].rotate_right(6) ^ h[4].rotate_right(11) ^ h[4].rotate_right(25);
            let ch = (h[4] & h[5]) ^ ((!h[4]) & h[6]);
            let t1 = h[7]
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = h[0].rotate_right(2) ^ h[0].rotate_right(13) ^ h[0].rotate_right(22);
            let maj = (h[0] & h[1]) ^ (h[0] & h[2]) ^ (h[1] & h[2]);
            let t2 = s0.wrapping_add(maj);
            h[7] = h[6];
            h[6] = h[5];
            h[5] = h[4];
            h[4] = h[3].wrapping_add(t1);
            h[3] = h[2];
            h[2] = h[1];
            h[1] = h[0];
            h[0] = t1.wrapping_add(t2);
        }
        for (s, hv) in self.state.iter_mut().zip(h.iter()) {
            *s = s.wrapping_add(*hv);
        }
    }
    fn finalize_hex(mut self) -> String {
        // Save the true message bit length BEFORE appending padding.
        let bit_len = self.len.wrapping_mul(8);
        // Append 0x80, then 0x00 until the buffer length is 56 mod 64, then the
        // 8-byte big-endian bit length — operating directly on buf so the len
        // counter is not disturbed.
        self.buf.push(0x80);
        while self.buf.len() % 64 != 56 {
            self.buf.push(0x00);
        }
        self.buf.extend_from_slice(&bit_len.to_be_bytes());
        // buf is now a whole number of 64-byte blocks.
        let mut off = 0;
        while off < self.buf.len() {
            let block: [u8; 64] = self.buf[off..off + 64].try_into().unwrap();
            self.process(&block);
            off += 64;
        }
        self.buf.clear();
        let mut out = String::with_capacity(64);
        for word in &self.state {
            out.push_str(&format!("{:08x}", word));
        }
        out
    }
}

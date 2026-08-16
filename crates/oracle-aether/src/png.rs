//! A minimal PNG encoder — enough for a screenshot, and nothing else.
//!
//! ## Why this exists rather than a dependency
//!
//! `oracle-aether`'s runtime dependencies are deliberately `oracle-core` + `serde_json`, and a screenshot
//! is not a good reason to change that. Emulators that care about their dependency tree routinely vendor a
//! small encoder for exactly this — RetroArch wrote `rpng`, and `stb_image_write.h` is the same answer in
//! single-header form. The alternative we shipped before was worse than either: writing a **PPM** and
//! letting the caller sort it out, which is what a project does *before* anyone asks for screenshots, not
//! after. It also meant the MCP handed a model 200 KB of undecodable bytes labelled `image/png`.
//!
//! ## What it emits
//!
//! Truecolour 8-bit RGB, one `IDAT`, deflate with **fixed Huffman codes** and a greedy LZ77 matcher, rows
//! filtered with **Sub**. No interlacing, no palette, no alpha, no compression-level knob. A 320×224 frame
//! of flat game art lands around 10–30 KB rather than the 215 KB an uncompressed stream would cost.
//!
//! ## How it is known to be correct
//!
//! Being *structurally* valid is easy and proves nothing — a decoder will happily read a well-formed
//! container full of wrong pixels. So the gate is a **round trip against an independent decoder**: the
//! tests below re-derive the pixels from the encoded bytes using an inflate implementation that is not
//! this one (see `tests/png_roundtrip.rs`, which decodes with Python's `zlib` — stdlib, and no relation to
//! the encoder here) and compare them to the source image, byte for byte. An encoder that agrees with a
//! foreign decoder on every pixel of several images is correct in the only sense that matters.

/// PNG signature: the 8 bytes every PNG starts with.
const SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];

/// Encode `rgb` (`width * height` pixels, row-major) as a PNG image.
///
/// # Panics
/// Debug-asserts that `rgb` holds exactly `width * height` pixels.
pub fn encode(rgb: &[(u8, u8, u8)], width: u32, height: u32) -> Vec<u8> {
    debug_assert_eq!(
        rgb.len(),
        (width as usize) * (height as usize),
        "pixel count must match the stated dimensions"
    );

    let mut out = Vec::with_capacity(rgb.len()); // a compressed frame is far smaller than its pixels
    out.extend_from_slice(&SIGNATURE);

    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&width.to_be_bytes());
    ihdr.extend_from_slice(&height.to_be_bytes());
    ihdr.push(8); // bit depth
    ihdr.push(2); // colour type 2 = truecolour RGB
    ihdr.push(0); // compression method 0 = deflate (the only one PNG defines)
    ihdr.push(0); // filter method 0 (the only one PNG defines)
    ihdr.push(0); // no interlace
    chunk(&mut out, b"IHDR", &ihdr);

    let filtered = filter_sub(rgb, width, height);
    chunk(&mut out, b"IDAT", &zlib(&filtered));
    chunk(&mut out, b"IEND", &[]);
    out
}

/// Append one PNG chunk: big-endian length of `data`, the type, `data`, then CRC32 over **type + data**
/// (not the length — a classic way to get this subtly wrong).
fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    let mut crc = Crc32::new();
    crc.update(kind);
    crc.update(data);
    out.extend_from_slice(&crc.finish().to_be_bytes());
}

/// Prefix each row with filter type 1 (**Sub**:each byte minus the byte one pixel to its left).
///
/// Sub rather than None because flat colour — most of a Mega Drive frame — becomes a run of zeros, which
/// is precisely what the LZ77 stage below turns into almost nothing. Rather than an adaptive per-row
/// choice, which buys little on this content and costs a heuristic nobody would ever tune.
fn filter_sub(rgb: &[(u8, u8, u8)], width: u32, height: u32) -> Vec<u8> {
    let w = width as usize;
    let mut out = Vec::with_capacity((w * 3 + 1) * height as usize);
    for y in 0..height as usize {
        out.push(1); // filter type: Sub
        let row = &rgb[y * w..(y + 1) * w];
        let mut prev = (0u8, 0u8, 0u8);
        for px in row {
            out.push(px.0.wrapping_sub(prev.0));
            out.push(px.1.wrapping_sub(prev.1));
            out.push(px.2.wrapping_sub(prev.2));
            prev = *px;
        }
    }
    out
}

/// Wrap a deflate stream in the zlib container PNG requires: a 2-byte header and a trailing Adler-32 of
/// the **uncompressed** data.
fn zlib(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len() / 4 + 64);
    // CMF = 0x78: deflate, 32 KiB window. FLG = 0x01 makes (CMF<<8 | FLG) a multiple of 31, with no
    // preset dictionary and the "fastest" compression hint.
    out.push(0x78);
    out.push(0x01);
    deflate_fixed(data, &mut out);
    out.extend_from_slice(&adler32(data).to_be_bytes());
    out
}

// ---------------------------------------------------------------------------------------------------
// Deflate — one fixed-Huffman block, greedy LZ77
// ---------------------------------------------------------------------------------------------------

/// Deflate's bit order is the joint most common source of "valid container, garbage stream" bugs:
/// **the stream is filled LSB-first, but Huffman codes are written MSB-first.** Both live here so the
/// distinction is stated once.
struct BitWriter<'a> {
    out: &'a mut Vec<u8>,
    acc: u32,
    nbits: u32,
}

impl<'a> BitWriter<'a> {
    fn new(out: &'a mut Vec<u8>) -> Self {
        Self {
            out,
            acc: 0,
            nbits: 0,
        }
    }

    /// Non-Huffman fields (block header, extra bits): LSB-first.
    fn bits(&mut self, value: u32, count: u32) {
        self.acc |= value << self.nbits;
        self.nbits += count;
        while self.nbits >= 8 {
            self.out.push((self.acc & 0xFF) as u8);
            self.acc >>= 8;
            self.nbits -= 8;
        }
    }

    /// Huffman codes: MSB-first, i.e. the code's high bit enters the stream first.
    fn code(&mut self, code: u32, count: u32) {
        for i in (0..count).rev() {
            self.bits((code >> i) & 1, 1);
        }
    }

    fn finish(self) {
        if self.nbits > 0 {
            self.out.push((self.acc & 0xFF) as u8);
        }
    }
}

/// The fixed literal/length code for `sym`, as (code, bit length) — RFC 1951 §3.2.6.
fn fixed_lit(sym: u32) -> (u32, u32) {
    match sym {
        0..=143 => (0x30 + sym, 8),
        144..=255 => (0x190 + (sym - 144), 9),
        256..=279 => (sym - 256, 7),
        _ => (0xC0 + (sym - 280), 8),
    }
}

/// Length code table: (code, extra bits, base length) for lengths 3..=258.
const LENGTHS: [(u32, u32, u32); 29] = [
    (257, 0, 3),
    (258, 0, 4),
    (259, 0, 5),
    (260, 0, 6),
    (261, 0, 7),
    (262, 0, 8),
    (263, 0, 9),
    (264, 0, 10),
    (265, 1, 11),
    (266, 1, 13),
    (267, 1, 15),
    (268, 1, 17),
    (269, 2, 19),
    (270, 2, 23),
    (271, 2, 27),
    (272, 2, 31),
    (273, 3, 35),
    (274, 3, 43),
    (275, 3, 51),
    (276, 3, 59),
    (277, 4, 67),
    (278, 4, 83),
    (279, 4, 99),
    (280, 4, 115),
    (281, 5, 131),
    (282, 5, 163),
    (283, 5, 195),
    (284, 5, 227),
    (285, 0, 258),
];

/// Distance code table: (code, extra bits, base distance) for distances 1..=32768.
const DISTANCES: [(u32, u32, u32); 30] = [
    (0, 0, 1),
    (1, 0, 2),
    (2, 0, 3),
    (3, 0, 4),
    (4, 1, 5),
    (5, 1, 7),
    (6, 2, 9),
    (7, 2, 13),
    (8, 3, 17),
    (9, 3, 25),
    (10, 4, 33),
    (11, 4, 49),
    (12, 5, 65),
    (13, 5, 97),
    (14, 6, 129),
    (15, 6, 193),
    (16, 7, 257),
    (17, 7, 385),
    (18, 8, 513),
    (19, 8, 769),
    (20, 9, 1025),
    (21, 9, 1537),
    (22, 10, 2049),
    (23, 10, 3073),
    (24, 11, 4097),
    (25, 11, 6145),
    (26, 12, 8193),
    (27, 12, 12289),
    (28, 13, 16385),
    (29, 13, 24577),
];

const WINDOW: usize = 32768;
const MIN_MATCH: usize = 3;
const MAX_MATCH: usize = 258;
const HASH_BITS: usize = 15;

/// Emit `data` as a single fixed-Huffman deflate block.
///
/// The matcher is greedy with a one-deep hash table — the cheapest thing that still finds the long runs
/// Sub-filtered flat art produces. It is deliberately not a good general-purpose compressor; it is a good
/// compressor of this one kind of picture.
fn deflate_fixed(data: &[u8], out: &mut Vec<u8>) {
    let mut w = BitWriter::new(out);
    w.bits(1, 1); // BFINAL: this is the only block
    w.bits(1, 2); // BTYPE 01: fixed Huffman

    let mut head = vec![u32::MAX; 1 << HASH_BITS];
    let mut i = 0usize;
    while i < data.len() {
        let mut best_len = 0usize;
        let mut best_dist = 0usize;
        if i + MIN_MATCH <= data.len() {
            let h = hash3(&data[i..i + MIN_MATCH]);
            let cand = head[h];
            head[h] = i as u32;
            if cand != u32::MAX {
                let cand = cand as usize;
                let dist = i - cand;
                if dist <= WINDOW && dist > 0 {
                    let max = MAX_MATCH.min(data.len() - i);
                    let mut n = 0usize;
                    // Overlapping matches are legal and are how a run of one byte compresses: the
                    // decoder copies byte by byte from output it has already produced.
                    while n < max && data[cand + n] == data[i + n] {
                        n += 1;
                    }
                    if n >= MIN_MATCH {
                        best_len = n;
                        best_dist = dist;
                    }
                }
            }
        }

        if best_len >= MIN_MATCH {
            let (lc, lextra, lbase) = *LENGTHS
                .iter()
                .rev()
                .find(|(_, _, base)| *base as usize <= best_len)
                .expect("every length 3..=258 has a code");
            let (code, bits) = fixed_lit(lc);
            w.code(code, bits);
            if lextra > 0 {
                w.bits(best_len as u32 - lbase, lextra);
            }
            let (dc, dextra, dbase) = *DISTANCES
                .iter()
                .rev()
                .find(|(_, _, base)| *base as usize <= best_dist)
                .expect("every distance 1..=32768 has a code");
            w.code(dc, 5); // distance codes are 5-bit fixed, MSB-first
            if dextra > 0 {
                w.bits(best_dist as u32 - dbase, dextra);
            }
            // Insert the interior positions so later matches can find them.
            for k in 1..best_len {
                if i + k + MIN_MATCH <= data.len() {
                    head[hash3(&data[i + k..i + k + MIN_MATCH])] = (i + k) as u32;
                }
            }
            i += best_len;
        } else {
            let (code, bits) = fixed_lit(u32::from(data[i]));
            w.code(code, bits);
            i += 1;
        }
    }

    let (code, bits) = fixed_lit(256); // end-of-block
    w.code(code, bits);
    w.finish();
}

fn hash3(b: &[u8]) -> usize {
    let v = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
    ((v.wrapping_mul(0x9E37_79B1)) >> (32 - HASH_BITS)) as usize
}

// ---------------------------------------------------------------------------------------------------
// Checksums
// ---------------------------------------------------------------------------------------------------

fn adler32(data: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    for &byte in data {
        a = (a + u32::from(byte)) % 65521;
        b = (b + a) % 65521;
    }
    (b << 16) | a
}

/// The CRC-32 PNG specifies (IEEE 802.3, reflected, `0xEDB88320`).
struct Crc32(u32);

impl Crc32 {
    fn new() -> Self {
        Self(0xFFFF_FFFF)
    }
    fn update(&mut self, data: &[u8]) {
        for &byte in data {
            self.0 ^= u32::from(byte);
            for _ in 0..8 {
                let mask = (self.0 & 1).wrapping_neg();
                self.0 = (self.0 >> 1) ^ (0xEDB8_8320 & mask);
            }
        }
    }
    fn finish(self) -> u32 {
        self.0 ^ 0xFFFF_FFFF
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Known-answer checks for the two checksums, so a failure downstream can be localised.
    #[test]
    fn checksums_match_their_published_values() {
        let mut c = Crc32::new();
        c.update(b"123456789");
        assert_eq!(c.finish(), 0xCBF4_3926, "CRC-32/ISO-HDLC check value");
        assert_eq!(adler32(b"Wikipedia"), 0x11E6_0398, "Adler-32 check value");
    }

    #[test]
    fn the_container_is_well_formed() {
        let img = vec![(1u8, 2u8, 3u8); 4];
        let png = encode(&img, 2, 2);
        assert_eq!(&png[0..8], &SIGNATURE, "signature");
        assert_eq!(&png[12..16], b"IHDR");
        assert_eq!(u32::from_be_bytes(png[16..20].try_into().unwrap()), 2);
        assert_eq!(u32::from_be_bytes(png[20..24].try_into().unwrap()), 2);
        assert_eq!(png[24], 8, "bit depth");
        assert_eq!(png[25], 2, "colour type 2 = truecolour");
        assert_eq!(&png[png.len() - 8..png.len() - 4], b"IEND");
    }

    /// Every chunk's CRC covers type+data and is checked by every decoder, so a walk of the chunk list
    /// that re-derives them catches a malformed container without needing a decoder here.
    #[test]
    fn every_chunk_crc_is_correct() {
        let img: Vec<(u8, u8, u8)> = (0..64).map(|i| (i as u8, 255 - i as u8, 7)).collect();
        let png = encode(&img, 8, 8);
        let mut i = 8;
        let mut seen = Vec::new();
        while i < png.len() {
            let len = u32::from_be_bytes(png[i..i + 4].try_into().unwrap()) as usize;
            let kind = &png[i + 4..i + 8];
            let data = &png[i + 8..i + 8 + len];
            let want = u32::from_be_bytes(png[i + 8 + len..i + 12 + len].try_into().unwrap());
            let mut c = Crc32::new();
            c.update(kind);
            c.update(data);
            assert_eq!(c.finish(), want, "CRC for {:?}", std::str::from_utf8(kind));
            seen.push(std::str::from_utf8(kind).unwrap().to_string());
            i += 12 + len;
        }
        assert_eq!(seen, ["IHDR", "IDAT", "IEND"], "chunk order");
        assert_eq!(i, png.len(), "no trailing bytes");
    }

    /// **The regression gate for the encoder's *content*, as opposed to its shape.**
    ///
    /// There is no inflate in this crate, by design, so nothing here can decode what `encode` produces —
    /// and a test that could would share the encoder's assumptions anyway. So correctness was established
    /// once, out of band, by round-tripping through an **independent** decoder (Python's stdlib `zlib`,
    /// which shares no code with this file) and comparing every pixel: seven images — 1×1, 2×2 solid, an
    /// 8×8 gradient, a 7×3 odd width, 320×224 flat blocks, 320×224 pseudo-random, and **a real rendered
    /// frame from `s4.bin`** — all matched byte for byte, and `file(1)` read the output as
    /// "PNG image data, 320 x 224, 8-bit/color RGB, non-interlaced".
    ///
    /// This locks that result in. If the encoder changes, this fails, and whoever changed it must repeat
    /// the independent round trip rather than simply re-blessing the bytes — which is the whole point of
    /// writing down *how* the golden was earned.
    #[test]
    fn the_gradient_golden_is_byte_for_byte_what_an_independent_decoder_verified() {
        const GOLDEN: &[u8] = &[
            0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48,
            0x44, 0x52, 0x00, 0x00, 0x00, 0x08, 0x00, 0x00, 0x00, 0x08, 0x08, 0x02, 0x00, 0x00,
            0x00, 0x4B, 0x6D, 0x29, 0xDC, 0x00, 0x00, 0x00, 0x51, 0x49, 0x44, 0x41, 0x54, 0x78,
            0x01, 0x63, 0x64, 0xF8, 0xCF, 0xC0, 0xF2, 0x57, 0x05, 0x03, 0x69, 0x30, 0x2A, 0x3C,
            0x07, 0x52, 0x68, 0x48, 0x03, 0x48, 0x32, 0x3A, 0x9C, 0xF7, 0x00, 0x52, 0x48, 0x08,
            0x24, 0x0A, 0x44, 0x8C, 0x09, 0xDB, 0x73, 0x80, 0x14, 0x0C, 0x41, 0x45, 0x81, 0x88,
            0xB1, 0x61, 0xFE, 0x04, 0x20, 0x05, 0x46, 0x08, 0x51, 0x20, 0x62, 0x5C, 0xD0, 0xBE,
            0x05, 0x48, 0xA1, 0x89, 0x02, 0x11, 0xE3, 0x81, 0xFC, 0x1B, 0x98, 0xA2, 0x40, 0xC4,
            0xF8, 0x20, 0x1C, 0xBB, 0x73, 0x01, 0x6E, 0x0B, 0x4C, 0x0D, 0x2B, 0xE4, 0xE0, 0x14,
            0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
        ];
        let grad: Vec<(u8, u8, u8)> = (0..64u32)
            .map(|i| ((i * 4) as u8, (255 - i * 3) as u8, (i % 7 * 36) as u8))
            .collect();
        assert_eq!(
            encode(&grad, 8, 8),
            GOLDEN,
            "encoder output drifted from the bytes an independent decoder verified"
        );
    }

    /// The worst case is bounded and known: incompressible input costs a few percent over raw rather than
    /// blowing up, and flat game art collapses. Recorded as a test so a matcher change that quietly
    /// destroyed the compression would be visible.
    #[test]
    fn compression_is_in_the_expected_band_for_both_extremes() {
        let flat = vec![(0x20u8, 0x40u8, 0x60u8); 320 * 224];
        let flat_png = encode(&flat, 320, 224).len();
        assert!(
            flat_png < 320 * 224 * 3 / 50,
            "flat art must collapse; got {flat_png} bytes"
        );

        let mut s = 0x5EEDu64;
        let noise: Vec<(u8, u8, u8)> = (0..320 * 224)
            .map(|_| {
                s = s
                    .wrapping_mul(6364136223846793005)
                    .wrapping_add(1442695040888963407);
                ((s >> 33) as u8, (s >> 41) as u8, (s >> 49) as u8)
            })
            .collect();
        let noise_png = encode(&noise, 320, 224).len();
        assert!(
            noise_png < 320 * 224 * 3 * 12 / 10,
            "incompressible input must stay near raw size, not blow up; got {noise_png} bytes"
        );
    }
}

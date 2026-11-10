//! Read-only Elegoo GOO V3.0 reader. See docs/format/goo.md.
//! Decode into sparse row spans: no resolution-sized image per layer.
use crate::Result;
use serde::Serialize;
use std::collections::BTreeMap;

const MAGIC: [u8; 8] = [7, 0, 0, 0, 0x44, 0x4c, 0x50, 0];
const END: [u8; 11] = [0, 0, 0, 7, 0, 0, 0, 0x44, 0x4c, 0x50, 0];
struct Cursor<'a> {
    bytes: &'a [u8],
    at: usize,
}
impl<'a> Cursor<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        let end = self.at.checked_add(n).ok_or("offset overflow")?;
        let s = self.bytes.get(self.at..end).ok_or("truncated GOO")?;
        self.at = end;
        Ok(s)
    }
    fn u8(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }
    fn u16(&mut self) -> Result<u16> {
        Ok(u16::from_be_bytes(self.take(2)?.try_into()?))
    }
    fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_be_bytes(self.take(4)?.try_into()?))
    }
    fn f32(&mut self) -> Result<f32> {
        let f = f32::from_bits(self.u32()?);
        if !f.is_finite() {
            return Err("nonfinite GOO field".into());
        }
        Ok(f)
    }
    fn text(&mut self, n: usize) -> Result<String> {
        Ok(String::from_utf8_lossy(self.take(n)?)
            .trim_end_matches('\0')
            .to_string())
    }
    fn delimiter(&mut self) -> Result<()> {
        if self.take(2)? != b"\r\n" {
            return Err("bad GOO delimiter".into());
        }
        Ok(())
    }
}
#[derive(Debug, Serialize)]
pub struct Header {
    pub version: String,
    pub software: String,
    pub software_version: String,
    pub file_time: String,
    pub printer: String,
    pub printer_type: String,
    pub profile: String,
    pub aa: u16,
    pub grey: u16,
    pub blur: u16,
    pub layers: u32,
    pub resolution: [u16; 2],
    pub mirror: [u8; 2],
    pub size_mm: [f32; 3],
    pub layer_mm: f32,
    pub exposure_s: f32,
    pub static_delay: u8,
    pub settings: BTreeMap<String, f32>,
    pub bottom_layers: u32,
    pub pwm: [u16; 2],
    pub advanced: u8,
    pub print_seconds: u32,
    pub volume_mm3: f32,
    pub weight_g: f32,
    pub price: f32,
    pub price_unit: String,
    pub layer_offset: u32,
    pub grey_scale: u8,
    pub transition_layers: u16,
}
impl Header {
    #[must_use]
    pub fn pitch(&self) -> [f64; 2] {
        std::array::from_fn(|i| f64::from(self.size_mm[i]) / f64::from(self.resolution[i]))
    }
}
#[derive(Debug)]
pub struct Layer {
    pub z: f32,
    pub exposure: f32,
    pub definition: Vec<u8>,
    pub encoded: Vec<u8>,
}
#[derive(Debug)]
pub struct Goo {
    pub header: Header,
    raw_header: Vec<u8>,
    pub layers: Vec<Layer>,
}
impl Goo {
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        let mut c = Cursor { bytes, at: 0 };
        let version = c.text(4)?;
        if version != "V3.0" || c.take(8)? != MAGIC {
            return Err("not GOO V3.0".into());
        }
        let software = c.text(32)?;
        let software_version = c.text(24)?;
        let file_time = c.text(24)?;
        let printer = c.text(32)?;
        let printer_type = c.text(32)?;
        let profile = c.text(32)?;
        let aa = c.u16()?;
        let grey = c.u16()?;
        let blur = c.u16()?;
        c.take(116 * 116 * 2)?;
        c.delimiter()?;
        c.take(290 * 290 * 2)?;
        c.delimiter()?;
        let layers = c.u32()?;
        let resolution = [c.u16()?, c.u16()?];
        let mirror = [c.u8()?, c.u8()?];
        let size_mm = [c.f32()?, c.f32()?, c.f32()?];
        let layer_mm = c.f32()?;
        let exposure_s = c.f32()?;
        let static_delay = c.u8()?;
        let mut settings = BTreeMap::new();
        for key in [
            "off_s",
            "bottom_before_lift_s",
            "bottom_after_lift_s",
            "bottom_after_retract_s",
            "before_lift_s",
            "after_lift_s",
            "after_retract_s",
            "bottom_exposure_s",
        ] {
            settings.insert(key.to_string(), c.f32()?);
        }
        let bottom_layers = c.u32()?;
        for key in [
            "bottom_lift_mm",
            "bottom_lift_mm_min",
            "lift_mm",
            "lift_mm_min",
            "bottom_retract_mm",
            "bottom_retract_mm_min",
            "retract_mm",
            "retract_mm_min",
            "bottom_lift2_mm",
            "bottom_lift2_mm_min",
            "lift2_mm",
            "lift2_mm_min",
            "bottom_retract2_mm",
            "bottom_retract2_mm_min",
            "retract2_mm",
            "retract2_mm_min",
        ] {
            settings.insert(key.to_string(), c.f32()?);
        }
        let pwm = [c.u16()?, c.u16()?];
        let advanced = c.u8()?;
        let print_seconds = c.u32()?;
        let volume_mm3 = c.f32()?;
        let weight_g = c.f32()?;
        let price = c.f32()?;
        let price_unit = c.text(8)?;
        let layer_offset = c.u32()?;
        let grey_scale = c.u8()?;
        let transition_layers = c.u16()?;
        if layer_offset as usize != c.at
            || grey_scale > 1
            || resolution.contains(&0)
            || size_mm.iter().any(|x| *x <= 0.0)
            || layer_mm <= 0.0
            || layers as usize > bytes.len() / 78
        {
            return Err("invalid GOO header bounds".into());
        }
        let header = Header {
            version,
            software,
            software_version,
            file_time,
            printer,
            printer_type,
            profile,
            aa,
            grey,
            blur,
            layers,
            resolution,
            mirror,
            size_mm,
            layer_mm,
            exposure_s,
            static_delay,
            settings,
            bottom_layers,
            pwm,
            advanced,
            print_seconds,
            volume_mm3,
            weight_g,
            price,
            price_unit,
            layer_offset,
            grey_scale,
            transition_layers,
        };
        let raw_header = bytes[..c.at].to_vec();
        let result = read_layers(&mut c, layers)?;
        if c.take(END.len())? != END || c.at != bytes.len() {
            return Err("bad GOO ending or trailing bytes".into());
        }
        Ok(Self {
            header,
            raw_header,
            layers: result,
        })
    }
    /// Lossless envelope round-trip; no regenerated settings, previews or pixels.
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        let mut out = self.raw_header.clone();
        for l in &self.layers {
            out.extend_from_slice(&l.definition);
            out.extend_from_slice(&u32::try_from(l.encoded.len())?.to_be_bytes());
            out.extend_from_slice(&l.encoded);
            out.extend_from_slice(b"\r\n");
        }
        out.extend_from_slice(&END);
        Ok(out)
    }
    pub fn spans(&self, index: usize) -> Result<Vec<Span>> {
        decode(
            &self.layers.get(index).ok_or("layer index")?.encoded,
            self.header.resolution,
            self.header.grey_scale,
        )
    }
}
fn read_layers(c: &mut Cursor<'_>, layers: u32) -> Result<Vec<Layer>> {
    let mut result = Vec::new();
    for _ in 0..layers {
        let start = c.at;
        c.u16()?;
        c.f32()?;
        let z = c.f32()?;
        let exposure = c.f32()?;
        for _ in 0..12 {
            c.f32()?;
        }
        c.u16()?;
        c.delimiter()?;
        let definition = c.bytes[start..c.at].to_vec();
        let n = c.u32()? as usize;
        let encoded = c.take(n)?.to_vec();
        validate_encoded(&encoded)?;
        c.delimiter()?;
        result.push(Layer {
            z,
            exposure,
            definition,
            encoded,
        });
    }
    Ok(result)
}
fn validate_encoded(b: &[u8]) -> Result<()> {
    if b.len() < 2 || b[0] != 0x55 || b[1..].iter().fold(0u8, |a, x| a.wrapping_add(*x)) != 0xff {
        return Err("bad GOO image magic/checksum".into());
    }
    Ok(())
}
#[derive(Debug, Clone, Copy)]
pub struct Span {
    pub y: u32,
    pub x0: u32,
    pub x1: u32,
    pub value: u8,
}
/// Sparse, nonzero spans in raster order. Length has its low nibble in the tag;
/// the remaining bytes form a big-endian integer shifted left four bits.
pub fn decode(encoded: &[u8], resolution: [u16; 2], scale: u8) -> Result<Vec<Span>> {
    validate_encoded(encoded)?;
    let mut c = Cursor {
        bytes: &encoded[1..encoded.len() - 1],
        at: 0,
    };
    let width = u32::from(resolution[0]);
    let total = width * u32::from(resolution[1]);
    let mut offset = 0u32;
    let mut color = 0u8;
    let mut spans = Vec::new();
    while c.at < c.bytes.len() {
        let tag = c.u8()?;
        let kind = tag >> 6;
        let low = u32::from(tag & 15);
        let len;
        if kind == 2 {
            color = if tag & 32 == 0 {
                color.checked_add(low as u8)
            } else {
                color.checked_sub(low as u8)
            }
            .ok_or("grey delta overflow")?;
            len = if tag & 16 == 0 { 1 } else { u32::from(c.u8()?) };
        } else {
            color = match kind {
                0 => 0,
                1 => c.u8()?,
                _ => 255,
            };
            let mut upper = 0u32;
            for _ in 0..((tag >> 4) & 3) {
                upper = (upper << 8) | u32::from(c.u8()?);
            }
            len = low | (upper << 4);
        }
        let end = offset.checked_add(len).ok_or("RLE overflow")?;
        if len == 0 || end > total {
            return Err("invalid GOO run length".into());
        }
        let value = if scale == 0 && color != 255 {
            if color > 15 {
                return Err("4-bit grey out of range".into());
            }
            color * 17
        } else {
            color
        };
        if value > 0 {
            while offset < end {
                let row_end = ((offset / width) + 1) * width;
                let stop = end.min(row_end);
                spans.push(Span {
                    y: offset / width,
                    x0: offset % width,
                    x1: (stop - 1) % width + 1,
                    value,
                });
                offset = stop;
            }
        }
        offset = end;
    }
    if offset != total {
        return Err(format!("GOO image has {offset} pixels, expected {total}").into());
    }
    Ok(spans)
}
#[derive(Debug, Clone, Serialize)]
pub struct Blob {
    pub pixels: u64,
    pub weighted_pixels: f64,
    pub centroid: [f64; 2],
    pub bounds: [u32; 4],
    pub overlap_below_pixels: u64,
    #[serde(skip)]
    pub span_ids: Vec<usize>,
}
fn root(parent: &mut [usize], mut i: usize) -> usize {
    while parent[i] != i {
        parent[i] = parent[parent[i]];
        i = parent[i];
    }
    i
}
/// 8-connected within a layer, exact XY pixel overlap with the previous layer.
/// Plate-touching first-layer blobs are not islands. AA pixels count as lit.
#[must_use]
pub fn blobs(spans: &[Span], below: &[Span]) -> Vec<Blob> {
    let mut parent: Vec<_> = (0..spans.len()).collect();
    let mut rows: BTreeMap<u32, Vec<usize>> = BTreeMap::new();
    for (i, s) in spans.iter().enumerate() {
        rows.entry(s.y).or_default().push(i);
    }
    for (&y, ids) in &rows {
        for pair in ids.windows(2) {
            if spans[pair[0]].x1 == spans[pair[1]].x0 {
                let a = root(&mut parent, pair[0]);
                let b = root(&mut parent, pair[1]);
                parent[a] = b;
            }
        }
        if let Some(prev) = y.checked_sub(1).and_then(|p| rows.get(&p)) {
            let mut first = 0;
            for &i in ids {
                while first < prev.len() && spans[prev[first]].x1 < spans[i].x0 {
                    first += 1;
                }
                for &j in &prev[first..] {
                    if spans[j].x0 > spans[i].x1 {
                        break;
                    }
                    let a = root(&mut parent, i);
                    let b = root(&mut parent, j);
                    parent[a] = b;
                }
            }
        }
    }
    let mut below_rows: BTreeMap<u32, Vec<&Span>> = BTreeMap::new();
    for s in below {
        below_rows.entry(s.y).or_default().push(s);
    }
    let mut groups: BTreeMap<usize, Blob> = BTreeMap::new();
    for (i, s) in spans.iter().enumerate() {
        let b = groups.entry(root(&mut parent, i)).or_insert(Blob {
            pixels: 0,
            weighted_pixels: 0.0,
            centroid: [0.0; 2],
            bounds: [u32::MAX, u32::MAX, 0, 0],
            overlap_below_pixels: 0,
            span_ids: Vec::new(),
        });
        let n = u64::from(s.x1 - s.x0);
        b.pixels += n;
        b.weighted_pixels += n as f64 * f64::from(s.value) / 255.0;
        b.centroid[0] += f64::from(s.x0 + s.x1) / 2.0 * n as f64;
        b.centroid[1] += (f64::from(s.y) + 0.5) * n as f64;
        b.bounds[0] = b.bounds[0].min(s.x0);
        b.bounds[1] = b.bounds[1].min(s.y);
        b.bounds[2] = b.bounds[2].max(s.x1);
        b.bounds[3] = b.bounds[3].max(s.y + 1);
        b.span_ids.push(i);
        if let Some(prev) = below_rows.get(&s.y) {
            for p in prev {
                if p.x1 <= s.x0 {
                    continue;
                }
                if p.x0 >= s.x1 {
                    break;
                }
                b.overlap_below_pixels += u64::from(p.x1.min(s.x1) - p.x0.max(s.x0));
            }
        }
    }
    groups
        .into_values()
        .map(|mut b| {
            b.centroid = b.centroid.map(|v| v / b.pixels as f64);
            b
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn wrap(mut payload: Vec<u8>) -> Vec<u8> {
        let sum = payload.iter().fold(0u8, |a, x| a.wrapping_add(*x));
        payload.insert(0, 0x55);
        payload.push(!sum);
        payload
    }
    #[test]
    fn known_rle_and_grey_delta() {
        // 2 black, 3 grey 128, delta +1, delta -2, 3 white: ten pixels.
        let spans = decode(&wrap(vec![2, 0x43, 128, 0x81, 0xa2, 0xc3]), [10, 1], 1).unwrap();
        assert_eq!(spans.iter().map(|s| s.x1 - s.x0).sum::<u32>(), 8);
        assert_eq!(
            spans.iter().map(|s| s.value).collect::<Vec<_>>(),
            [128, 129, 127, 255]
        );
        // Spec example: low nibble is least significant, not most significant.
        let spans = decode(&wrap(vec![0xd1, 2]), [33, 1], 1).unwrap();
        assert_eq!(spans[0].x1, 33);
    }
    #[test]
    fn sabotage_checksum_and_count() {
        let mut bytes = wrap(vec![0xc8]);
        bytes[1] ^= 1;
        assert!(decode(&bytes, [8, 1], 1).is_err());
        assert!(decode(&wrap(vec![0xc7]), [8, 1], 1).is_err());
    }
    #[test]
    fn detects_island_but_not_diagonal_component_split() {
        let s = [
            Span {
                y: 0,
                x0: 0,
                x1: 1,
                value: 255,
            },
            Span {
                y: 1,
                x0: 1,
                x1: 2,
                value: 1,
            },
        ];
        let b = blobs(&s, &[]);
        assert_eq!(b.len(), 1);
        assert_eq!(b[0].overlap_below_pixels, 0);
        assert_eq!(blobs(&s, &s[..1])[0].overlap_below_pixels, 1);
        let separated = [
            s[0],
            Span {
                y: 1,
                x0: 3,
                x1: 4,
                value: 255,
            },
        ];
        assert_eq!(blobs(&separated, &[]).len(), 2);
    }
    #[test]
    fn synthetic_header_known_offsets_roundtrip_and_header_sabotage() {
        let mut b = vec![0u8; 195477];
        b[..4].copy_from_slice(b"V3.0");
        b[4..12].copy_from_slice(&MAGIC);
        let name = b"ELEGOO Saturn 4 Ultra 16K";
        b[92..92 + name.len()].copy_from_slice(name);
        for (at, v) in [
            (188, 4u16),
            (190, 3),
            (192, 2),
            (195314, 15120),
            (195316, 6230),
            (195475, 5),
        ] {
            b[at..at + 2].copy_from_slice(&v.to_be_bytes());
        }
        for at in [27106, 195308] {
            b[at..at + 2].copy_from_slice(b"\r\n");
        }
        b[195310..195314].copy_from_slice(&1u32.to_be_bytes());
        b[195318] = 1;
        for (at, v) in [
            (195320, 211.68f32),
            (195324, 118.37),
            (195328, 220.0),
            (195332, 0.05),
            (195336, 2.35),
        ] {
            b[at..at + 4].copy_from_slice(&v.to_be_bytes());
        }
        b[195470..195474].copy_from_slice(&195477u32.to_be_bytes());
        b[195474] = 1;
        let mut definition = vec![0; 66];
        definition[6..10].copy_from_slice(&0.05f32.to_be_bytes());
        definition[64..66].copy_from_slice(b"\r\n");
        b.extend(definition);
        let n = 15120u32 * 6230;
        // Length bytes contain bits 27..4, not bits 31..8.
        let payload = wrap(vec![
            0xf0 | (n & 15) as u8,
            (n >> 20) as u8,
            (n >> 12) as u8,
            (n >> 4) as u8,
        ]);
        b.extend_from_slice(&(payload.len() as u32).to_be_bytes());
        b.extend(payload);
        b.extend_from_slice(b"\r\n");
        b.extend_from_slice(&END);
        let file = Goo::parse(&b).unwrap();
        assert_eq!(file.header.printer, "ELEGOO Saturn 4 Ultra 16K");
        assert_eq!(
            [file.header.aa, file.header.grey, file.header.blur],
            [4, 3, 2]
        );
        assert!((file.header.pitch()[0] - 0.014).abs() < 1e-8);
        assert!((file.header.pitch()[1] - 0.019).abs() < 1e-8);
        assert_eq!(file.spans(0).unwrap().len(), 6230);
        assert_eq!(file.to_bytes().unwrap(), b);
        b[195470] ^= 1;
        assert!(Goo::parse(&b).is_err());
    }
    #[test]
    #[allow(clippy::disallowed_methods)] // Optional external test fixture, not an application setting.
    fn owner_header_and_lossless_roundtrip_when_supplied() {
        // External read-only fixture opt-in; clean checkouts still run synthetic tests.
        let Ok(dir) = std::env::var("CAL1_GOO_INPUT") else {
            return;
        };
        for name in [
            "a0",
            "a1",
            "a1-resave",
            "b100",
            "s100",
            "s324",
            "k15",
            "k60",
            "w",
        ] {
            let bytes =
                std::fs::read(std::path::Path::new(&dir).join(format!("{name}.goo"))).unwrap();
            let file = Goo::parse(&bytes).unwrap();
            assert_eq!(file.header.resolution, [15120, 6230]);
            assert!((file.header.layer_mm - 0.05).abs() < 1e-7);
            assert_eq!(file.to_bytes().unwrap(), bytes);
            for i in 0..file.layers.len() {
                file.spans(i).unwrap();
            }
        }
    }
}

//! Removing GPS location from EXIF without re-encoding.
//!
//! EXIF is a TIFF structure. Its first directory (IFD0) may point to a GPS
//! directory (tag 0x8825). Scrubbing zeroes every GPS entry and every value
//! those entries point to, then marks the directory empty. Other metadata
//! (capture date, camera, lens) is untouched, and so is the image data.

use std::path::Path;

#[derive(Clone, Copy)]
enum Endian {
    Little,
    Big,
}

struct Tiff<'a> {
    b: &'a mut [u8],
    e: Endian,
}

impl Tiff<'_> {
    fn u16(&self, at: usize) -> Option<u16> {
        let s = self.b.get(at..at + 2)?;
        Some(match self.e {
            Endian::Little => u16::from_le_bytes([s[0], s[1]]),
            Endian::Big => u16::from_be_bytes([s[0], s[1]]),
        })
    }

    fn u32(&self, at: usize) -> Option<u32> {
        let s = self.b.get(at..at + 4)?;
        Some(match self.e {
            Endian::Little => u32::from_le_bytes([s[0], s[1], s[2], s[3]]),
            Endian::Big => u32::from_be_bytes([s[0], s[1], s[2], s[3]]),
        })
    }

    fn zero(&mut self, at: usize, len: usize) {
        if let Some(s) = self.b.get_mut(at..at.saturating_add(len)) {
            s.fill(0);
        }
    }
}

fn type_size(t: u16) -> usize {
    match t {
        1 | 2 | 6 | 7 => 1,
        3 | 8 => 2,
        4 | 9 | 11 | 13 => 4,
        5 | 10 | 12 => 8,
        _ => 0,
    }
}

fn open(b: &mut [u8]) -> Option<Tiff<'_>> {
    let e = match b.get(0..4)? {
        [b'I', b'I', 42, 0] => Endian::Little,
        [b'M', b'M', 0, 42] => Endian::Big,
        _ => return None,
    };
    Some(Tiff { b, e })
}

/// Offset of the GPS directory, if IFD0 points to one.
fn gps_ifd(t: &Tiff) -> Option<usize> {
    let ifd0 = t.u32(4)? as usize;
    let n = t.u16(ifd0)? as usize;
    (0..n).map(|i| ifd0 + 2 + 12 * i).find(|&e| t.u16(e) == Some(0x8825)).and_then(|e| t.u32(e + 8)).map(|o| o as usize)
}

/// Scrub GPS from raw EXIF (TIFF) bytes in place. Returns whether any was there.
pub fn strip_gps(exif: &mut [u8]) -> bool {
    let Some(mut t) = open(exif) else { return false };
    let Some(gps) = gps_ifd(&t) else { return false };
    let Some(n) = t.u16(gps) else { return false };
    let n = n as usize;
    if n == 0 {
        return false;
    }
    for i in 0..n {
        let e = gps + 2 + 12 * i;
        let (Some(typ), Some(count)) = (t.u16(e + 2), t.u32(e + 4)) else { break };
        let len = type_size(typ).saturating_mul(count as usize);
        if len > 4 {
            if let Some(off) = t.u32(e + 8) {
                t.zero(off as usize, len);
            }
        }
        t.zero(e, 12);
    }
    // Empty directory, no next directory.
    t.zero(gps, 2);
    t.zero(gps + 2 + 12 * n, 4);
    true
}

/// Whether raw EXIF bytes still hold GPS entries.
#[cfg_attr(not(test), allow(dead_code))]
pub fn has_gps(exif: &[u8]) -> bool {
    let mut copy = exif.to_vec();
    let Some(t) = open(&mut copy) else { return false };
    gps_ifd(&t).and_then(|g| t.u16(g)).is_some_and(|n| n > 0)
}

fn crc32(data: &[u8]) -> u32 {
    let mut c: u32 = 0xffff_ffff;
    for &b in data {
        c ^= b as u32;
        for _ in 0..8 {
            c = if c & 1 != 0 { 0xedb8_8320 ^ (c >> 1) } else { c >> 1 };
        }
    }
    c ^ 0xffff_ffff
}

/// The EXIF (TIFF) byte range inside a JPEG, TIFF, or PNG file.
fn exif_ranges(b: &[u8]) -> Option<Vec<(usize, usize)>> {
    if b.starts_with(&[0xff, 0xd8]) {
        let mut out = Vec::new();
        let mut i = 2;
        while i + 4 <= b.len() && b[i] == 0xff {
            let marker = b[i + 1];
            if marker == 0xda || marker == 0xd9 {
                break;
            }
            let len = u16::from_be_bytes([b[i + 2], b[i + 3]]) as usize;
            let body = i + 4;
            if marker == 0xe1 && b.get(body..body + 6) == Some(b"Exif\0\0") {
                out.push((body + 6, (i + 2 + len).min(b.len())));
            }
            i += 2 + len;
        }
        return Some(out);
    }
    if b.starts_with(b"II*\0") || b.starts_with(b"MM\0*") {
        return Some(vec![(0, b.len())]);
    }
    if b.starts_with(&[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]) {
        let mut out = Vec::new();
        let mut i = 8;
        while i + 8 <= b.len() {
            let len = u32::from_be_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]]) as usize;
            if &b[i + 4..i + 8] == b"eXIf" {
                out.push((i + 8, (i + 8 + len).min(b.len())));
            }
            i += 12 + len;
        }
        return Some(out);
    }
    None
}

/// Scrub GPS from a JPEG, TIFF (including TIFF-based RAW), or PNG file in place.
/// Returns Ok(false) for formats it can't read, so the caller can choose another path.
pub fn strip_gps_file(path: &Path) -> Result<bool, String> {
    let mut b = std::fs::read(path).map_err(|e| e.to_string())?;
    let Some(ranges) = exif_ranges(&b) else { return Ok(false) };
    let png = b.starts_with(&[0x89, b'P', b'N', b'G']);
    let mut changed = false;
    for (start, end) in ranges {
        if strip_gps(&mut b[start..end]) {
            changed = true;
            if png {
                // PNG chunks carry a CRC over type and data.
                let crc = crc32(&b[start - 4..end]);
                b[end..end + 4].copy_from_slice(&crc.to_be_bytes());
            }
        }
    }
    if changed {
        std::fs::write(path, b).map_err(|e| e.to_string())?;
    }
    Ok(true)
}

/// GPS still present in a file (JPEG, TIFF, PNG)?
#[cfg_attr(not(test), allow(dead_code))]
pub fn file_has_gps(path: &Path) -> bool {
    let Ok(b) = std::fs::read(path) else { return false };
    exif_ranges(&b).unwrap_or_default().into_iter().any(|(s, e)| has_gps(&b[s..e]))
}

#[cfg(test)]
pub mod tests {
    /// A minimal little-endian EXIF block: Make, an Exif IFD with DateTimeOriginal,
    /// and a GPS IFD with latitude.
    pub fn sample_exif() -> Vec<u8> {
        let mut b: Vec<u8> = b"II*\0".to_vec();
        b.extend(8u32.to_le_bytes());
        // IFD0 at 8: 3 entries.
        let ifd0 = 8usize;
        let exif_ifd = ifd0 + 2 + 3 * 12 + 4; // 50
        let gps_ifd = exif_ifd + 2 + 12 + 4; // 68
        let gps_data = gps_ifd + 2 + 2 * 12 + 4; // 98
        let date_off = gps_data + 24; // 122
        let make_off = date_off + 20; // 142
        let entry = |tag: u16, typ: u16, count: u32, val: u32| {
            let mut e = tag.to_le_bytes().to_vec();
            e.extend(typ.to_le_bytes());
            e.extend(count.to_le_bytes());
            e.extend(val.to_le_bytes());
            e
        };
        b.extend(3u16.to_le_bytes());
        b.extend(entry(0x010f, 2, 5, make_off as u32)); // Make
        b.extend(entry(0x8769, 4, 1, exif_ifd as u32)); // Exif IFD
        b.extend(entry(0x8825, 4, 1, gps_ifd as u32)); // GPS IFD
        b.extend(0u32.to_le_bytes());
        b.extend(1u16.to_le_bytes());
        b.extend(entry(0x9003, 2, 20, date_off as u32)); // DateTimeOriginal
        b.extend(0u32.to_le_bytes());
        b.extend(2u16.to_le_bytes());
        b.extend(entry(0x0001, 2, 2, u32::from_le_bytes([b'N', 0, 0, 0]))); // GPSLatitudeRef
        b.extend(entry(0x0002, 5, 3, gps_data as u32)); // GPSLatitude
        b.extend(0u32.to_le_bytes());
        for (n, d) in [(37u32, 1u32), (46, 1), (2988, 100)] {
            b.extend(n.to_le_bytes());
            b.extend(d.to_le_bytes());
        }
        b.extend(b"2026:11:04 21:00:00\0");
        b.extend(b"Test\0");
        b
    }

    #[test]
    fn strips_gps_and_keeps_the_rest() {
        let mut e = sample_exif();
        assert!(super::has_gps(&e));
        assert!(super::strip_gps(&mut e));
        assert!(!super::has_gps(&e));
        let s = String::from_utf8_lossy(&e);
        assert!(s.contains("2026:11:04 21:00:00") && s.contains("Test"));
        // The latitude values themselves are gone, not just unlinked.
        assert!(!e.windows(4).any(|w| w == 2988u32.to_le_bytes()));
    }
}

//! Container detection from file content.
//!
//! Obfuscated Usenet posts often carry random names without an extension, so
//! the only reliable way to know what a download actually is, is to look at the
//! first bytes. RAR5 also stores a volume number in its main header, which lets
//! a set be renamed into `name.partNNN.rar` so UnRAR can chain the volumes.

/// A container format recognised from the first bytes of a payload.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Container {
    /// RAR 5 archive. `volume` is `Some(n)` for a volume of a set, `None` for a
    /// single-volume archive.
    Rar5 { volume: Option<u32> },
    /// RAR 1.5 – 4.x archive.
    Rar4,
    Zip,
    SevenZip,
    Par2,
}

impl Container {
    pub fn extension(self) -> &'static str {
        match self {
            Self::Rar5 { .. } | Self::Rar4 => "rar",
            Self::Zip => "zip",
            Self::SevenZip => "7z",
            Self::Par2 => "par2",
        }
    }
}

const RAR5_SIGNATURE: &[u8] = b"Rar!\x1a\x07\x01\x00";
const RAR4_SIGNATURE: &[u8] = b"Rar!\x1a\x07\x00";
const SEVENZIP_SIGNATURE: &[u8] = b"7z\xbc\xaf\x27\x1c";
const ZIP_SIGNATURE: &[u8] = b"PK\x03\x04";
const PAR2_SIGNATURE: &[u8] = b"PAR2\0PKT";

/// How many leading bytes [`sniff`] needs to classify every supported format.
pub const SNIFF_LEN: usize = 64;

pub fn sniff(data: &[u8]) -> Option<Container> {
    if data.starts_with(RAR5_SIGNATURE) {
        return Some(Container::Rar5 {
            volume: rar5_volume_number(data),
        });
    }
    if data.starts_with(RAR4_SIGNATURE) {
        return Some(Container::Rar4);
    }
    if data.starts_with(SEVENZIP_SIGNATURE) {
        return Some(Container::SevenZip);
    }
    if data.starts_with(ZIP_SIGNATURE) {
        return Some(Container::Zip);
    }
    if data.starts_with(PAR2_SIGNATURE) {
        return Some(Container::Par2);
    }
    None
}

/// Reads the volume number from a RAR 5 main archive header.
///
/// Returns `Some(0)` for the first volume of a set, `Some(n)` for later volumes
/// and `None` when the archive is not a volume or the header cannot be read.
pub fn rar5_volume_number(data: &[u8]) -> Option<u32> {
    if !data.starts_with(RAR5_SIGNATURE) {
        return None;
    }
    // signature (8) + block CRC32 (4)
    let mut index = 12;
    vint(data, &mut index)?; // header size
    let header_type = vint(data, &mut index)?;
    if header_type != 1 {
        // Not the main archive header.
        return None;
    }
    let header_flags = vint(data, &mut index)?;
    if header_flags & 0x0001 != 0 {
        vint(data, &mut index)?; // extra area size
    }
    if header_flags & 0x0002 != 0 {
        vint(data, &mut index)?; // data size
    }
    let archive_flags = vint(data, &mut index)?;
    if archive_flags & 0x0001 == 0 {
        // Plain archive, not part of a volume set.
        return None;
    }
    if archive_flags & 0x0002 == 0 {
        // Volume flag set but no number field: this is the first volume.
        return Some(0);
    }
    vint(data, &mut index)
}

/// The name a multi-volume RAR should get so UnRAR can follow it.
pub fn rar_volume_name(base: &str, volume: u32) -> String {
    format!("{base}.part{:03}.rar", volume.saturating_add(1))
}

/// The name a single-file download of this container should get.
pub fn single_name(base: &str, container: Container) -> Option<String> {
    match container {
        Container::Rar5 {
            volume: Some(volume),
        } => Some(rar_volume_name(base, volume)),
        _ => Some(format!("{base}.{}", container.extension())),
    }
}

/// Reads a RAR5 variable-length integer.
fn vint(data: &[u8], index: &mut usize) -> Option<u32> {
    let mut value = 0u64;
    let mut shift = 0u32;
    loop {
        let byte = *data.get(*index)?;
        *index += 1;
        value |= u64::from(byte & 0x7f) << shift;
        if byte & 0x80 == 0 {
            return u32::try_from(value).ok();
        }
        shift += 7;
        if shift > 63 {
            return None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a RAR5 main header with the given archive flags and volume number.
    fn rar5_header(archive_flags: u8, volume: Option<u8>) -> Vec<u8> {
        let mut out = RAR5_SIGNATURE.to_vec();
        out.extend_from_slice(&[0xaa, 0xbb, 0xcc, 0xdd]); // block CRC32
        out.push(16); // header size
        out.push(1); // header type: main archive header
        out.push(5); // header flags
        out.push(12); // extra area size
        out.push(archive_flags);
        if let Some(volume) = volume {
            out.push(volume);
        }
        out.extend_from_slice(&[0x0b, 0x01, 0x01]);
        out.resize(64, 0);
        out
    }

    #[test]
    fn reads_rar5_volume_numbers() {
        let first = rar5_header(1, None);
        assert_eq!(rar5_volume_number(&first), Some(0));
        let later = rar5_header(3, Some(54));
        assert_eq!(rar5_volume_number(&later), Some(54));
        let plain = rar5_header(0, None);
        assert_eq!(rar5_volume_number(&plain), None);
        assert_eq!(sniff(&plain), Some(Container::Rar5 { volume: None }));
    }

    #[test]
    fn recognises_the_common_containers() {
        assert_eq!(
            sniff(&rar5_header(3, Some(7))),
            Some(Container::Rar5 { volume: Some(7) })
        );
        assert_eq!(sniff(RAR4_SIGNATURE), Some(Container::Rar4));
        assert_eq!(sniff(SEVENZIP_SIGNATURE), Some(Container::SevenZip));
        assert_eq!(sniff(b"PK\x03\x04rest"), Some(Container::Zip));
        assert_eq!(sniff(PAR2_SIGNATURE), Some(Container::Par2));
        assert_eq!(sniff(b"\x00\x01\x02\x03"), None);
    }

    #[test]
    fn names_volumes_for_unrar() {
        assert_eq!(
            single_name("jada-39768c", Container::Rar5 { volume: Some(0) }),
            Some("jada-39768c.part001.rar".into())
        );
        assert_eq!(
            single_name("jada-39768c", Container::Rar5 { volume: Some(54) }),
            Some("jada-39768c.part055.rar".into())
        );
        assert_eq!(
            single_name("single", Container::Zip),
            Some("single.zip".into())
        );
    }
}

use std::io::Error;

use crate::rpm::IndexEntryKind;

#[derive(Debug)]
struct Header<Tag> {
    entries: Vec<IndexEntry<Tag>>,
    version: u8,
}

impl<Tag: From<u32>> Header<Tag> {
    fn read(input: &[u8]) -> Result<(Self, usize), Error> {
        if input.len() < MIN_HEADER_LEN {
            return Err(Error::other("rpm header is too small"));
        }
        if input[0..HEADER_MAGIC.len()] != HEADER_MAGIC[..] {
            return Err(not_a_header());
        }
        let version = input[3];
        let num_entries: usize = get_u32(&input[8..12]) as usize;
        eprintln!("num entries {}", num_entries);
        let index_len = num_entries
            .checked_mul(INDEX_ENTRY_LEN)
            .ok_or_else(|| Error::other("bogus no. of index entries"))?;
        eprintln!("index len {}", index_len);
        if input.len() - MIN_HEADER_LEN < index_len {
            return Err(Error::other("rpm header is too small"));
        }
        let store_len = get_u32(&input[12..16]) as usize;
        eprintln!("store len {}", store_len);
        if input.len() - MIN_HEADER_LEN - index_len < store_len {
            return Err(Error::other("rpm header is too small"));
        }
        let mut entries = Vec::with_capacity(num_entries);
        let mut i = MIN_HEADER_LEN;
        for _ in 0..num_entries {
            let tag = get_u32(&input[i..(i + 4)]).into();
            i += 4;
            let kind: IndexEntryKind = get_u32(&input[i..(i + 4)]).try_into()?;
            i += 4;
            let offset = get_u32(&input[i..(i + 4)]);
            i += 4;
            // TODO take into account type size
            if offset as usize >= store_len {
                return Err(Error::other(format!("invalid offset: {}", offset)));
            }
            let count = get_u32(&input[i..(i + 4)]);
            i += 4;
            kind.validate_count(count)?;
            entries.push(IndexEntry {
                tag,
                kind,
                offset,
                count,
            });
        }
        // TODO read store
        Ok((Self { version, entries }, i + store_len))
    }
}

#[derive(Debug)]
struct IndexEntry<Tag> {
    tag: Tag,
    kind: IndexEntryKind,
    offset: u32,
    count: u32,
}

#[derive(Debug)]
struct Lead {
    name: String,
    kind: PackageKind,
    archnum: u16,
    osnum: u16,
    signature_kind: u16,
    major: u8,
    minor: u8,
}

impl Lead {
    fn read(input: &[u8]) -> Result<Self, Error> {
        if input.len() < LEAD_LEN {
            return Err(Error::other("rpm lead is too small"));
        }
        if input[0..LEAD_MAGIC.len()] != LEAD_MAGIC[..] {
            return Err(not_an_rpm_file());
        }
        let major: u8 = input[4];
        let minor: u8 = input[5];
        let kind: PackageKind = get_u16(&input[6..8]).try_into()?;
        let archnum: u16 = get_u16(&input[8..10]);
        let name: [u8; NAME_LEN] = input[10..(10 + NAME_LEN)]
            .try_into()
            .map_err(|_| other_error())?;
        let name_end = name
            .iter()
            .position(|ch| *ch == 0)
            .ok_or_else(|| Error::other("invalid package name"))?;
        let name = String::from_utf8(name[..name_end].to_vec())
            .map_err(|_| Error::other("invalid package name"))?;
        let offset = 10 + NAME_LEN;
        let osnum: u16 = get_u16(&input[offset..(offset + 2)]);
        let signature_kind: u16 = get_u16(&input[(offset + 4)..(offset + 6)]);
        Ok(Self {
            major,
            minor,
            kind,
            name,
            archnum,
            osnum,
            signature_kind,
        })
    }
}

#[derive(Debug)]
#[repr(u16)]
enum PackageKind {
    Binary,
    Source,
}

impl TryFrom<u16> for PackageKind {
    type Error = Error;
    fn try_from(other: u16) -> Result<Self, Self::Error> {
        match other {
            0 => Ok(Self::Binary),
            1 => Ok(Self::Source),
            _ => Err(Error::other(format!("unknown rpm kind: {}", other))),
        }
    }
}

fn get_u16(input: &[u8]) -> u16 {
    assert_eq!(2, input.len());
    u16::from_be_bytes([input[0], input[1]])
}

fn get_u32(input: &[u8]) -> u32 {
    assert_eq!(4, input.len());
    u32::from_be_bytes([input[0], input[1], input[2], input[3]])
}

fn not_an_rpm_file() -> Error {
    Error::other("not an rpm file")
}

fn not_a_header() -> Error {
    Error::other("not an rpm header")
}

fn other_error() -> Error {
    Error::other("i/o error")
}

const LEAD_MAGIC: [u8; 4] = [0xed, 0xab, 0xee, 0xdb];
const HEADER_MAGIC: [u8; 3] = [0x8e, 0xad, 0xe8];
const NAME_LEN: usize = 66;
const LEAD_LEN: usize = 96;
const MIN_HEADER_LEN: usize = 16;
const INDEX_ENTRY_LEN: usize = 16;

#[cfg(test)]
mod tests {
    use cpio::newc::Reader as CpioReader;

    use super::*;
    use crate::compress::AnyDecoder;
    use crate::rpm::SignatureTag;
    use crate::rpm::Tag;

    #[test]
    fn lead_read() {
        let rpm = std::fs::read("wireguard-tools-1.0.20210914-7.fc41.x86_64.rpm").unwrap();
        let lead = Lead::read(&rpm[..]).unwrap();
        eprintln!("lead {:?}", lead);
        let (header, offset1) = Header::<SignatureTag>::read(&rpm[LEAD_LEN..]).unwrap();
        eprintln!("header {:?}", header);
        let (header, offset2) = Header::<Tag>::read(&rpm[(LEAD_LEN + offset1)..]).unwrap();
        eprintln!("header {:?}", header);
        let archive = &rpm[(LEAD_LEN + offset1 + offset2)..];
        eprintln!("archive {:02x?}", &archive[..10]);
        let mut reader = AnyDecoder::new(&archive[..]);
        loop {
            let cpio = CpioReader::new(reader).unwrap();
            if cpio.entry().is_trailer() {
                break;
            }
            eprintln!(
                "{} ({} bytes)",
                cpio.entry().name(),
                cpio.entry().file_size()
            );
            reader = cpio.finish().unwrap();
        }
    }
}

use std::collections::HashMap;
use std::fmt::Debug;
use std::hash::Hash;
use std::io::Error;
use std::io::Write;

use crate::rpm::write_u32;
use crate::rpm::EntryIo;
use crate::rpm::ENTRY_LEN;

#[derive(Debug)]
pub struct Header<E>
where
    E: EntryIo + Debug,
    <E as EntryIo>::Tag: Hash + Debug,
{
    entries: HashMap<<E as EntryIo>::Tag, E>,
    version: u8,
}

impl<E> Header<E>
where
    E: EntryIo + Debug,
    <E as EntryIo>::Tag: Hash + Debug + Eq,
{
    pub fn new(entries: HashMap<<E as EntryIo>::Tag, E>) -> Self {
        Self {
            entries,
            version: DEFAULT_HEADER_VERSION,
        }
    }

    pub fn to_vec(&self) -> Result<Vec<u8>, Error> {
        let mut buf = Vec::new();
        self.write(&mut buf)?;
        Ok(buf)
    }

    pub fn write<W: Write>(&self, mut writer: W) -> Result<(), Error> {
        let mut index = Vec::new();
        let mut store = Vec::new();
        // TODO leader/trailer
        for (_tag, entry) in self.entries.iter() {
            let offset = store.len();
            if offset > u32::MAX as usize {
                return Err(Error::other("too large store"));
            }
            entry.write(&mut index, &mut store, offset as u32)?;
        }
        {
            let mut leader_index = Vec::new();
            let leader = E::leader_entry((index.len() + 16) as u32);
            let offset = store.len();
            if offset > u32::MAX as usize {
                return Err(Error::other("too large store"));
            }
            leader.write(&mut leader_index, &mut store, offset as u32)?;
            index.splice(0..0, leader_index);
        }
        let index_len = self.entries.len() + 1;
        if index_len > u32::MAX as usize {
            return Err(Error::other(format!("too many entries: {}", index_len)));
        }
        let store_len = store.len();
        if store_len > u32::MAX as usize {
            return Err(Error::other(format!("too large store: {}", store_len)));
        }
        let index_len = index_len as u32;
        let store_len = store_len as u32;
        assert_eq!(0, (index_len * ENTRY_LEN as u32) % ALIGN);
        eprintln!("write {:?}", self.entries);
        eprintln!("write index len {}", index_len);
        eprintln!("write store len {}", store_len);
        writer.write_all(&HEADER_MAGIC[..])?;
        write_u32(writer.by_ref(), &index_len)?;
        write_u32(writer.by_ref(), &store_len)?;
        writer.write_all(&index)?;
        writer.write_all(&store)?;
        Ok(())
    }

    pub fn read(input: &[u8]) -> Result<(Self, usize), Error> {
        let offset = input
            .windows(HEADER_MAGIC.len())
            .position(|bytes| bytes == &HEADER_MAGIC[..])
            .ok_or_else(|| Error::other("unable to find header magic"))?;
        let input = &input[offset..];
        if input.len() < MIN_HEADER_LEN {
            return Err(Error::other(format!(
                "header is too small: {} < {}",
                input.len(),
                MIN_HEADER_LEN
            )));
        }
        let version = input[3];
        let num_entries: usize = get_u32(&input[8..12]) as usize;
        eprintln!("read index len {}", num_entries);
        let index_len = num_entries
            .checked_mul(ENTRY_LEN)
            .ok_or_else(|| Error::other("bogus no. of index entries"))?;
        if input.len() - MIN_HEADER_LEN < index_len {
            return Err(Error::other(format!(
                "header is too small: {} < {}",
                input.len(),
                index_len + MIN_HEADER_LEN
            )));
        }
        let store_len = get_u32(&input[12..16]) as usize;
        eprintln!("read store len {}", store_len);
        if input.len() - MIN_HEADER_LEN - index_len < store_len {
            return Err(Error::other("header is too small"));
        }
        let store_offset = MIN_HEADER_LEN + index_len;
        let store = &input[store_offset..(store_offset + store_len)];
        let mut entries = HashMap::with_capacity(num_entries);
        let mut i = MIN_HEADER_LEN;
        for _ in 0..num_entries {
            let entry = E::read(&input[i..store_offset], store)?;
            if let Some(entry) = entry {
                entries.insert(entry.tag(), entry);
            }
            i += ENTRY_LEN;
        }
        assert_eq!(i, store_offset);
        eprintln!("store offset = {}", store_offset);
        eprintln!("store len = {}", store_len);
        eprintln!("name offset global = {}", 11016 + store_offset);
        Ok((Self { version, entries }, i + store_len))
    }

    pub(crate) fn insert(&mut self, entry: E) {
        self.entries.insert(entry.tag(), entry);
    }
}

#[derive(Debug)]
#[cfg_attr(test, derive(PartialEq, Eq))]
pub struct Lead {
    pub name: String,
    pub kind: PackageKind,
    pub archnum: u16,
    pub osnum: u16,
    pub signature_kind: u16,
    pub major: u8,
    pub minor: u8,
}

impl Lead {
    pub fn new(name: String) -> Self {
        Self {
            name,
            kind: PackageKind::Binary,
            archnum: 1,
            osnum: 1,
            signature_kind: 0,
            major: 3,
            minor: 0,
        }
    }

    pub fn read(input: &[u8]) -> Result<Self, Error> {
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
        let name: [u8; MAX_NAME_LEN] = input[10..(10 + MAX_NAME_LEN)]
            .try_into()
            .map_err(|_| other_error())?;
        let name_end = name
            .iter()
            .position(|ch| *ch == 0)
            .ok_or_else(|| Error::other("invalid package name"))?;
        let name = String::from_utf8(name[..name_end].to_vec())
            .map_err(|_| Error::other("invalid package name"))?;
        let offset = 10 + MAX_NAME_LEN;
        let osnum: u16 = get_u16(&input[offset..(offset + 2)]);
        let signature_kind: u16 = get_u16(&input[(offset + 2)..(offset + 4)]);
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

    pub fn write<W: Write>(&self, mut writer: W) -> Result<(), Error> {
        // -1 because of the zero byte
        let name_len = self.name.len();
        if name_len > MAX_NAME_LEN - 1 {
            return Err(Error::other(format!(
                "package name is too long: {} {}",
                self.name,
                self.name.len()
            )));
        }
        writer.write_all(&LEAD_MAGIC[..])?;
        writer.write_all(&[self.major, self.minor])?;
        writer.write_all(&(self.kind as u16).to_be_bytes()[..])?;
        writer.write_all(&self.archnum.to_be_bytes()[..])?;
        let mut name = [0_u8; MAX_NAME_LEN];
        name[..name_len].copy_from_slice(self.name.as_bytes());
        writer.write_all(&name[..])?;
        writer.write_all(&self.osnum.to_be_bytes()[..])?;
        writer.write_all(&self.signature_kind.to_be_bytes()[..])?;
        // reserved bytes
        writer.write_all(&[0_u8; 16])?;
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(test, derive(arbitrary::Arbitrary))]
#[repr(u16)]
pub enum PackageKind {
    Binary = 0,
    Source = 1,
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
    Error::other("not an header")
}

fn other_error() -> Error {
    Error::other("i/o error")
}

const LEAD_MAGIC: [u8; 4] = [0xed, 0xab, 0xee, 0xdb];
const HEADER_MAGIC: [u8; 8] = [0x8e, 0xad, 0xe8, 0x01, 0x00, 0x00, 0x00, 0x00];
const MAX_NAME_LEN: usize = 66;
const LEAD_LEN: usize = 96;
const MIN_HEADER_LEN: usize = 16;
const DEFAULT_HEADER_VERSION: u8 = 1;
pub(crate) const ALIGN: u32 = 8;

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use arbitrary::Arbitrary;
    use arbitrary::Unstructured;
    use arbtest::arbtest;
    use cpio::newc::Reader as CpioReader;

    use super::*;
    use crate::compress::AnyDecoder;
    use crate::rpm::Entry;
    use crate::rpm::SignatureEntry;
    use crate::test::Chars;
    use crate::test::CONTROL;
    use crate::test::UNICODE;

    #[test]
    fn lead_write_read() {
        arbtest(|u| {
            let expected: Lead = u.arbitrary()?;
            let mut buf = Vec::new();
            expected.write(&mut buf).unwrap();
            assert_eq!(LEAD_LEN, buf.len());
            let actual = Lead::read(&buf).unwrap();
            assert_eq!(expected, actual);
            Ok(())
        });
    }

    #[test]
    fn header_write_read() {
        arbtest(|u| {
            let expected: Header<Entry> = u.arbitrary()?;
            let mut buf = Vec::new();
            expected.write(&mut buf).unwrap();
            let (actual, _offset) = Header::<Entry>::read(&buf).unwrap();
            assert_eq!(expected.entries, actual.entries);
            Ok(())
        });
    }

    #[test]
    fn signature_header_write_read() {
        arbtest(|u| {
            let expected: Header<SignatureEntry> = u.arbitrary()?;
            let mut buf = Vec::new();
            expected.write(&mut buf).unwrap();
            let (actual, _offset) = Header::<SignatureEntry>::read(&buf).unwrap();
            assert_eq!(expected.entries, actual.entries);
            Ok(())
        });
    }

    #[test]
    fn lead_read() {
        let rpm = std::fs::read("wg.rpm").unwrap();
        let lead = Lead::read(&rpm[..]).unwrap();
        eprintln!("lead {:?}", lead);
        let (header, offset1) = Header::<SignatureEntry>::read(&rpm[LEAD_LEN..]).unwrap();
        eprintln!("header {:?}", header);
        eprintln!("store2 plus offset = {}", offset1);
        let (header, offset2) = Header::<Entry>::read(&rpm[(LEAD_LEN + offset1)..]).unwrap();
        eprintln!("header {:?}", header);
        let archive = &rpm[(LEAD_LEN + offset1 + offset2)..];
        eprintln!("archive {:02x?}", &archive[..10]);
        let mut reader = AnyDecoder::new(&archive[..]);
        loop {
            let cpio = CpioReader::new(reader).unwrap();
            if cpio.entry().is_trailer() {
                break;
            }
            //eprintln!(
            //    "{} ({} bytes)",
            //    cpio.entry().name(),
            //    cpio.entry().file_size()
            //);
            reader = cpio.finish().unwrap();
        }
    }

    impl<'a> Arbitrary<'a> for Lead {
        fn arbitrary(u: &mut Unstructured<'a>) -> arbitrary::Result<Self> {
            let valid_chars = Chars::from(UNICODE).difference(CONTROL);
            let len: usize = u.int_in_range(0..=(MAX_NAME_LEN - 1))?;
            let name: String = valid_chars.arbitrary_byte_string(u, len)?;
            Ok(Self {
                name,
                kind: u.arbitrary()?,
                archnum: u.arbitrary()?,
                osnum: u.arbitrary()?,
                signature_kind: u.arbitrary()?,
                major: u.arbitrary()?,
                minor: u.arbitrary()?,
            })
        }
    }

    impl<'a> Arbitrary<'a> for Header<Entry> {
        fn arbitrary(u: &mut Unstructured<'a>) -> arbitrary::Result<Self> {
            // TODO fix zero bytes in strings
            Ok(Self {
                entries: u
                    .arbitrary::<HashSet<Entry>>()?
                    .into_iter()
                    .map(Into::into)
                    .collect(),
                version: 1,
            })
        }
    }

    impl<'a> Arbitrary<'a> for Header<SignatureEntry> {
        fn arbitrary(u: &mut Unstructured<'a>) -> arbitrary::Result<Self> {
            // TODO fix zero bytes in strings
            Ok(Self {
                entries: u
                    .arbitrary::<HashSet<SignatureEntry>>()?
                    .into_iter()
                    .map(Into::into)
                    .collect(),
                version: 1,
            })
        }
    }
}

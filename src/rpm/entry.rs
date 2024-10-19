use std::fmt::Debug;
use std::io::Error;
use std::str::from_utf8;

use crate::hash::Md5Hash;
use crate::hash::Sha1Hash;
use crate::hash::Sha256Hash;
use crate::rpm::IndexEntryKind;
use crate::rpm::SignatureTag;
use crate::rpm::Tag;

pub trait EntryRead {
    fn read(input: &[u8], store: &[u8]) -> Result<Option<Self>, Error>
    where
        Self: Sized;
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub enum Entry {
    Name(String),
    Version(String),
    Release(String),
    Summary(String),
    Description(String),
    License(String),
    Url(String),
    Size(u32),
    Arch(String),
    Vendor(String),
    Os(String),
}

impl EntryRead for Entry {
    fn read(input: &[u8], store: &[u8]) -> Result<Option<Self>, Error> {
        if input.len() < INDEX_ENTRY_LEN {
            return Err(Error::other("rpm index entry is too small"));
        }
        let tag: Tag = get_u32(&input[0..4]).into();
        let kind: IndexEntryKind = get_u32(&input[4..8]).try_into()?;
        let offset = get_u32(&input[8..12]) as usize;
        if offset >= store.len() {
            return Err(Error::other("invalid offset in index entry"));
        }
        let count: u32 = get_u32(&input[12..16]);
        eprintln!("tag {:?} {:?} {}", tag, kind, count);
        let entry = do_read(tag, &store[offset..])?;
        Ok(entry)
    }
}

fn do_read(tag: Tag, input: &[u8]) -> Result<Option<Entry>, Error> {
    match tag {
        Tag::Name => Ok(Some(Entry::Name(read_string(input)?))),
        Tag::Version => Ok(Some(Entry::Version(read_string(input)?))),
        Tag::Release => Ok(Some(Entry::Release(read_string(input)?))),
        Tag::Summary => Ok(Some(Entry::Summary(read_string(input)?))),
        Tag::Description => Ok(Some(Entry::Description(read_string(input)?))),
        Tag::License => Ok(Some(Entry::License(read_string(input)?))),
        Tag::Url => Ok(Some(Entry::Url(read_string(input)?))),
        Tag::Size => Ok(Some(Entry::Size(get_u32(input)))),
        Tag::Arch => Ok(Some(Entry::Arch(read_string(input)?))),
        Tag::Vendor => Ok(Some(Entry::Vendor(read_string(input)?))),
        Tag::Os => Ok(Some(Entry::Os(read_string(input)?))),
        _ => {
            eprintln!("unsupported tag: {:?}", tag);
            Ok(None)
        }
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub enum SignatureEntry {
    Size(u32),
    PayloadSize(u32),
    Sha1(Sha1Hash),
    Sha256(Sha256Hash),
    Dsa(Vec<u8>),
    Rsa(Vec<u8>),
    Md5(Md5Hash),
}

impl EntryRead for SignatureEntry {
    fn read(input: &[u8], store: &[u8]) -> Result<Option<Self>, Error> {
        if input.len() < INDEX_ENTRY_LEN {
            return Err(Error::other("rpm index entry is too small"));
        }
        let tag: SignatureTag = get_u32(&input[0..4]).into();
        let kind: IndexEntryKind = get_u32(&input[4..8]).try_into()?;
        let offset = get_u32(&input[8..12]) as usize;
        if offset >= store.len() {
            return Err(Error::other("invalid offset in index entry"));
        }
        let count: u32 = get_u32(&input[12..16]);
        if matches!(tag, SignatureTag::Dsa) {
            std::fs::write("/tmp/sig", &store[offset..(offset + count as usize)]).unwrap();
        }
        eprintln!("signature {:?} {:?} {}", tag, kind, count);
        let entry = read_signature_entry(tag, &store[offset..], count as usize)?;
        Ok(entry)
    }
}

fn read_signature_entry(
    tag: SignatureTag,
    input: &[u8],
    n: usize,
) -> Result<Option<SignatureEntry>, Error> {
    use SignatureEntry as E;
    use SignatureTag::*;
    match tag {
        Size => Ok(Some(E::Size(get_u32(input)))),
        PayloadSize => Ok(Some(E::PayloadSize(get_u32(input)))),
        Sha1 => Ok(Some(E::Sha1(
            read_string(input)?
                .parse()
                .map_err(|_| Error::other("invalid sha1"))?,
        ))),
        Sha256 => Ok(Some(E::Sha256(
            read_string(input)?
                .parse()
                .map_err(|_| Error::other("invalid sha256"))?,
        ))),
        Md5 => Ok(Some(E::Md5(
            input[..n]
                .try_into()
                .map_err(|_| Error::other("invalid md5 size"))?,
        ))),
        Dsa => Ok(Some(E::Dsa(input[..n].into()))),
        Rsa => Ok(Some(E::Rsa(input[..n].into()))),
        Signatures | ReservedSpace => Ok(None),
        _ => {
            eprintln!("unsupported signature tag: {:?}", tag);
            Ok(None)
        }
    }
}

fn get_u32(input: &[u8]) -> u32 {
    assert!(4 <= input.len());
    u32::from_be_bytes([input[0], input[1], input[2], input[3]])
}

fn read_string(input: &[u8]) -> Result<String, Error> {
    let n = input
        .iter()
        .position(|ch| *ch == 0)
        .ok_or_else(|| Error::other("string is not terminated"))?;
    let s =
        from_utf8(&input[..n]).map_err(|e| Error::other(format!("invalid utf-8 string: {}", e)))?;
    Ok(s.into())
}

const INDEX_ENTRY_LEN: usize = 16;

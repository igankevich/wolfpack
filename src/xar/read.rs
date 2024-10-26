use std::fmt::Display;
use std::fmt::Formatter;
use std::io::Error;
use std::io::Read;
use std::io::Seek;
use std::io::SeekFrom;
use std::io::Write;
use std::iter::FusedIterator;
use std::path::PathBuf;
use std::str::FromStr;

use flate2::read::ZlibDecoder;
use serde::Deserialize;
use serde::Serialize;

use crate::compress::AnyDecoder;
use crate::hash::Hasher;
use crate::hash::Sha1;
use crate::hash::Sha1Hash;
use crate::hash::Sha256;
use crate::hash::Sha256Hash;
use crate::hash::Sha512;
use crate::hash::Sha512Hash;

pub struct XarArchive<R: Read + Seek> {
    files: Vec<xml::File>,
    reader: R,
    heap_offset: u64,
}

impl<R: Read + Seek> XarArchive<R> {
    pub fn new(mut reader: R) -> Result<Self, Error> {
        let header = Header::read(&mut reader)?;
        eprintln!("header {:?}", header);
        eprintln!("header len {:?}", HEADER_LEN);
        let mut toc_bytes = vec![0_u8; header.toc_len_compressed as usize];
        reader.read_exact(&mut toc_bytes[..])?;
        let toc = xml::Xar::read(&toc_bytes[..])?.toc;
        eprintln!("toc {:?}", toc);
        let heap_offset = reader.stream_position()?;
        reader.seek(SeekFrom::Start(heap_offset + toc.checksum.offset))?;
        let mut checksum = vec![0_u8; toc.checksum.size as usize];
        reader.read_exact(&mut checksum[..])?;
        let checksum = Checksum::new(toc.checksum.algo, &checksum[..])?;
        eprintln!("checksum {:?}", checksum);
        let actual_checksum = checksum.compute(&toc_bytes[..]);
        eprintln!("checksum actual {:?}", actual_checksum);
        if checksum != actual_checksum {
            return Err(Error::other("toc checksum mismatch"));
        }
        Ok(Self {
            files: toc.files,
            reader,
            heap_offset,
        })
    }

    pub fn files(&mut self) -> Iter<R> {
        Iter::new(self)
    }

    fn seek_to_file(&mut self, i: usize) -> Result<(), Error> {
        let offset = self.heap_offset + self.files[i].data.offset;
        let mut file_bytes = vec![0_u8; self.files[i].data.length as usize];
        self.reader.seek(SeekFrom::Start(offset))?;
        self.reader.read_exact(&mut file_bytes[..])?;
        let actual_checksum = self.files[i]
            .data
            .archived_checksum
            .value
            .compute(&file_bytes[..]);
        if self.files[i].data.archived_checksum.value != actual_checksum {
            return Err(Error::other("file checksum mismatch"));
        }
        self.reader.seek(SeekFrom::Start(offset))?;
        Ok(())
    }
}

pub struct Entry<'a, R: Read + Seek> {
    archive: &'a mut XarArchive<R>,
    i: usize,
}

impl<'a, R: Read + Seek> Entry<'a, R> {
    pub fn reader(&mut self) -> Result<AnyDecoder<&mut R>, Error> {
        self.archive.seek_to_file(self.i)?;
        Ok(AnyDecoder::new(self.archive.reader.by_ref()))
    }

    pub fn file(&self) -> &xml::File {
        &self.archive.files[self.i]
    }
}

pub struct Iter<'a, R: Read + Seek> {
    archive: &'a mut XarArchive<R>,
    first: usize,
    last: usize,
}

impl<'a, R: Read + Seek> Iter<'a, R> {
    fn new(archive: &'a mut XarArchive<R>) -> Self {
        let last = archive.files.len();
        Self {
            archive,
            first: 0,
            last,
        }
    }

    fn entry(&mut self, i: usize) -> Entry<'a, R> {
        // TODO safe?
        let archive = unsafe {
            std::mem::transmute::<&mut XarArchive<R>, &'a mut XarArchive<R>>(self.archive)
        };
        Entry { archive, i }
    }
}

impl<'a, R: Read + Seek> Iterator for Iter<'a, R> {
    type Item = Entry<'a, R>;
    fn next(&mut self) -> Option<Self::Item> {
        if self.first == self.last {
            return None;
        }
        let entry = self.entry(self.first);
        self.first += 1;
        Some(entry)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = self.len();
        (len, Some(len))
    }
}

impl<'a, R: Read + Seek> DoubleEndedIterator for Iter<'a, R> {
    fn next_back(&mut self) -> Option<Self::Item> {
        if self.first == self.last {
            return None;
        }
        self.last -= 1;
        let entry = self.entry(self.last);
        Some(entry)
    }
}

impl<'a, R: Read + Seek> ExactSizeIterator for Iter<'a, R> {
    fn len(&self) -> usize {
        self.last - self.first
    }
}

impl<'a, R: Read + Seek> FusedIterator for Iter<'a, R> {}

#[derive(Debug)]
#[cfg_attr(test, derive(arbitrary::Arbitrary, PartialEq, Eq))]
pub struct Header {
    toc_len_compressed: u64,
    toc_len_uncompressed: u64,
    checksum_algo: ChecksumAlgorithm,
}

impl Header {
    pub fn read<R: Read>(mut reader: R) -> Result<Self, Error> {
        let mut header = [0_u8; HEADER_LEN];
        reader.read_exact(&mut header[..])?;
        if header[0..MAGIC.len()] != MAGIC[..] {
            return Err(Error::other("not a xar file"));
        }
        let header_len = u16_read(&header[4..6]) as usize;
        if header_len > HEADER_LEN {
            // consume the rest of the header
            let mut remaining = header_len - HEADER_LEN;
            let mut buf = [0_u8; 64];
            while remaining != 0 {
                let m = remaining.min(buf.len());
                reader.read_exact(&mut buf[..m])?;
                remaining -= m;
            }
        }
        let _version = u16_read(&header[6..8]);
        let toc_len_compressed = u64_read(&header[8..16]);
        let toc_len_uncompressed = u64_read(&header[16..24]);
        let checksum_algo = u32_read(&header[24..28]).try_into()?;
        Ok(Self {
            toc_len_compressed,
            toc_len_uncompressed,
            checksum_algo,
        })
    }

    pub fn write<W: Write>(&self, mut writer: W) -> Result<(), Error> {
        writer.write_all(&MAGIC[..])?;
        writer.write_all(&(HEADER_LEN as u16).to_be_bytes()[..])?;
        writer.write_all(&1_u16.to_be_bytes()[..])?;
        writer.write_all(&self.toc_len_compressed.to_be_bytes()[..])?;
        writer.write_all(&self.toc_len_uncompressed.to_be_bytes()[..])?;
        writer.write_all(&(self.checksum_algo as u32).to_be_bytes()[..])?;
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[cfg_attr(test, derive(arbitrary::Arbitrary, PartialEq, Eq))]
#[serde(rename_all = "lowercase")]
#[repr(u32)]
pub enum ChecksumAlgorithm {
    Sha1 = 1,
    Sha256 = 3,
    Sha512 = 4,
}

impl TryFrom<u32> for ChecksumAlgorithm {
    type Error = Error;
    fn try_from(other: u32) -> Result<Self, Self::Error> {
        match other {
            0 => return Err(Error::other("no hashing algorithm")),
            1 => Ok(Self::Sha1),
            2 => return Err(Error::other("unsafe md5 hashing algorithm")),
            3 => Ok(Self::Sha256),
            4 => Ok(Self::Sha512),
            other => return Err(Error::other(format!("unknown hashing algorithm {}", other))),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(test, derive(arbitrary::Arbitrary))]
#[serde(into = "String", try_from = "String")]
pub enum Checksum {
    Sha1(Sha1Hash),
    Sha256(Sha256Hash),
    Sha512(Sha512Hash),
}

impl Checksum {
    pub fn new(algo: ChecksumAlgorithm, data: &[u8]) -> Result<Self, Error> {
        use ChecksumAlgorithm::*;
        Ok(match algo {
            Sha1 => Self::Sha1(
                data.try_into()
                    .map_err(|_| Error::other("invalid sha1 length"))?,
            ),
            Sha256 => Self::Sha256(
                data.try_into()
                    .map_err(|_| Error::other("invalid sha256 length"))?,
            ),
            Sha512 => Self::Sha512(
                data.try_into()
                    .map_err(|_| Error::other("invalid sha512 length"))?,
            ),
        })
    }

    pub fn compute(&self, data: &[u8]) -> Self {
        match self {
            Self::Sha1(..) => Self::Sha1(Sha1::compute(data)),
            Self::Sha256(..) => Self::Sha256(Sha256::compute(data)),
            Self::Sha512(..) => Self::Sha512(Sha512::compute(data)),
        }
    }
}

impl TryFrom<String> for Checksum {
    type Error = Error;
    fn try_from(other: String) -> Result<Self, Self::Error> {
        match other.len() {
            Sha1Hash::HEX_LEN => Ok(Self::Sha1(
                other
                    .parse()
                    .map_err(|_| Error::other("invalid sha1 string"))?,
            )),
            Sha256Hash::HEX_LEN => Ok(Self::Sha256(
                other
                    .parse()
                    .map_err(|_| Error::other("invalid sha256 string"))?,
            )),
            Sha512Hash::HEX_LEN => Ok(Self::Sha512(
                other
                    .parse()
                    .map_err(|_| Error::other("invalid sha512 string"))?,
            )),
            _ => Err(Error::other("invalid hash length")),
        }
    }
}

impl From<Checksum> for String {
    fn from(other: Checksum) -> String {
        use Checksum::*;
        match other {
            Sha1(hash) => hash.to_string(),
            Sha256(hash) => hash.to_string(),
            Sha512(hash) => hash.to_string(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
#[serde(into = "String", try_from = "String")]
pub struct FileMode(u32);

impl Default for FileMode {
    fn default() -> Self {
        FileMode(0o644)
    }
}

impl FromStr for FileMode {
    type Err = Error;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Ok(Self(
            u32::from_str_radix(value, 8).map_err(|_| Error::other("invalid file mode"))?,
        ))
    }
}

impl Display for FileMode {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        write!(f, "{:o}", self.0)
    }
}

impl TryFrom<String> for FileMode {
    type Error = Error;
    fn try_from(other: String) -> Result<Self, Self::Error> {
        other.parse()
    }
}

impl From<FileMode> for String {
    fn from(other: FileMode) -> String {
        other.to_string()
    }
}

pub mod xml {
    use std::io::BufReader;

    use quick_xml::de::from_reader;

    use super::*;

    #[derive(Serialize, Deserialize, Debug)]
    #[serde(rename = "xar")]
    pub struct Xar {
        pub toc: Toc,
    }

    impl Xar {
        pub fn read<R: Read>(reader: R) -> Result<Self, Error> {
            let reader = ZlibDecoder::new(reader);
            let reader = BufReader::new(reader);
            from_reader(reader).map_err(Error::other)
        }
    }

    #[derive(Serialize, Deserialize, Debug)]
    #[serde(rename = "toc", rename_all = "kebab-case")]
    pub struct Toc {
        pub checksum: TocChecksum,
        pub creation_time: Option<String>,
        #[serde(rename = "file", default)]
        pub files: Vec<File>,
        #[serde(rename = "signature", default)]
        pub signatures: Vec<Signature>,
        #[serde(rename = "x-signature", default)]
        pub x_signatures: Vec<Signature>,
    }

    #[derive(Serialize, Deserialize, Debug)]
    #[serde(rename = "checksum")]
    pub struct TocChecksum {
        #[serde(rename = "@style")]
        pub algo: ChecksumAlgorithm,
        pub offset: u64,
        pub size: u64,
    }

    #[derive(Serialize, Deserialize, Debug)]
    #[serde(rename = "file")]
    pub struct File {
        #[serde(rename = "@id")]
        pub id: u64,
        pub name: PathBuf,
        #[serde(rename = "type", default)]
        pub kind: String,
        #[serde(default)]
        pub inode: u64,
        #[serde(default)]
        pub deviceno: u64,
        #[serde(default)]
        pub mode: FileMode,
        #[serde(default)]
        pub uid: u32,
        #[serde(default)]
        pub gid: u32,
        #[serde(default)]
        pub atime: String,
        #[serde(default)]
        pub mtime: String,
        #[serde(default)]
        pub ctime: String,
        pub data: Data,
    }

    #[derive(Serialize, Deserialize, Debug)]
    #[serde(rename = "data", rename_all = "kebab-case")]
    pub struct Data {
        // ignore <contents>
        pub archived_checksum: FileChecksum,
        pub extracted_checksum: FileChecksum,
        pub encoding: Encoding,
        pub offset: u64,
        pub size: u64,
        pub length: u64,
    }

    #[derive(Serialize, Deserialize, Debug)]
    #[serde(rename = "encoding")]
    pub struct Encoding {
        #[serde(rename = "@style")]
        pub style: String,
    }

    #[derive(Serialize, Deserialize, Debug)]
    pub struct FileChecksum {
        #[serde(rename = "@style")]
        pub algo: ChecksumAlgorithm,
        #[serde(rename = "$value")]
        pub value: Checksum,
    }

    #[derive(Serialize, Deserialize, Debug)]
    #[serde(rename = "signature")]
    pub struct Signature {
        #[serde(rename = "@style")]
        pub style: String,
        pub offset: u64,
        pub size: u64,
        #[serde(rename = "KeyInfo")]
        pub key_info: KeyInfo,
    }

    #[derive(Deserialize, Debug)]
    #[serde(rename = "KeyInfo")]
    pub struct KeyInfo {
        #[serde(rename = "X509Data")]
        pub data: X509Data,
    }

    impl Serialize for KeyInfo {
        fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: Serializer,
        {
            let mut state = serializer.serialize_struct("KeyInfo", 2)?;
            state.serialize_field("@xmlns", "http://www.w3.org/2000/09/xmldsig#")?;
            state.serialize_field("X509Data", &self.data)?;
            state.end()
        }
    }

    #[derive(Serialize, Deserialize, Debug)]
    #[serde(rename = "X509Data")]
    pub struct X509Data {
        #[serde(rename = "X509Certificate", default)]
        pub certificates: Vec<X509Certificate>,
    }

    #[derive(Serialize, Deserialize, Debug)]
    #[serde(rename = "X509Certificate")]
    pub struct X509Certificate {
        #[serde(rename = "$value")]
        pub data: String,
    }
}

fn u16_read(data: &[u8]) -> u16 {
    u16::from_be_bytes([data[0], data[1]])
}

fn u32_read(data: &[u8]) -> u32 {
    u32::from_be_bytes([data[0], data[1], data[2], data[3]])
}

fn u64_read(data: &[u8]) -> u64 {
    u64::from_be_bytes([
        data[0], data[1], data[2], data[3], data[4], data[5], data[6], data[7],
    ])
}

const HEADER_LEN: usize = 4 + 2 + 2 + 8 + 8 + 4;
const MAGIC: [u8; 4] = *b"xar!";

#[cfg(test)]
mod tests {
    use std::fs::File;

    use super::*;

    #[test]
    fn xar_read() {
        let reader = File::open("tmp.sh.xar").unwrap();
        let mut xar_archive = XarArchive::new(reader).unwrap();
        for mut entry in xar_archive.files() {
            eprintln!("file {:?}", entry.file());
            eprintln!(
                "{}",
                std::io::read_to_string(entry.reader().unwrap()).unwrap()
            );
        }
    }
}

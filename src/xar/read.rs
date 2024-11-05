use std::fmt::Display;
use std::fmt::Formatter;
use std::fs::FileType;
use std::fs::Metadata;
use std::io::Error;
use std::io::Read;
use std::io::Seek;
use std::io::SeekFrom;
use std::io::Write;
use std::iter::FusedIterator;
use std::path::Path;
use std::path::PathBuf;
use std::str::FromStr;
use std::time::Duration;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

use bzip2::write::BzEncoder;
use chrono::format::SecondsFormat;
use chrono::DateTime;
use chrono::Utc;
use flate2::read::ZlibDecoder;
use flate2::write::ZlibEncoder;
use serde::ser::SerializeStruct;
use serde::Deserialize;
use serde::Serialize;
use serde::Serializer;

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
        let heap_offset = reader.stream_position()?;
        reader.seek(SeekFrom::Start(heap_offset + toc.checksum.offset))?;
        let mut checksum = vec![0_u8; toc.checksum.size as usize];
        reader.read_exact(&mut checksum[..])?;
        let checksum = Checksum::new(toc.checksum.algo, &checksum[..])?;
        let actual_checksum = checksum.compute(&toc_bytes[..]);
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

pub struct XarBuilder<W: Write> {
    writer: W,
    checksum_algo: ChecksumAlgorithm,
    files: Vec<xml::File>,
    contents: Vec<Vec<u8>>,
    offset: u64,
}

impl<W: Write> XarBuilder<W> {
    pub fn new(writer: W) -> Self {
        Self::do_new::<NoSigner>(writer, None)
    }

    pub fn new_signed<S: XarSigner>(writer: W, signer: &S) -> Self {
        Self::do_new(writer, Some(signer))
    }

    fn do_new<S: XarSigner>(writer: W, signer: Option<&S>) -> Self {
        let checksum_algo = ChecksumAlgorithm::Sha256;
        Self {
            offset: (checksum_algo.size() + signer.map(|s| s.signature_len()).unwrap_or(0)) as u64,
            writer,
            checksum_algo,
            files: Default::default(),
            contents: Default::default(),
        }
    }

    pub fn files(&self) -> &[xml::File] {
        &self.files[..]
    }

    pub fn add_file_by_path<P: AsRef<Path>>(
        &mut self,
        archive_path: PathBuf,
        path: P,
        compression: XarCompression,
    ) -> Result<(), Error> {
        let path = path.as_ref();
        let metadata = path.metadata()?;
        let contents = if metadata.is_dir() {
            Vec::new()
        } else {
            std::fs::read(path)?
        };
        let mut status: FileStatus = metadata.into();
        status.name = archive_path;
        self.add_file(status, &contents, compression)
    }

    pub fn add_file<C: AsRef<[u8]>>(
        &mut self,
        status: FileStatus,
        contents: C,
        compression: XarCompression,
    ) -> Result<(), Error> {
        let contents = contents.as_ref();
        let extracted_checksum = Checksum::new_from_data(self.checksum_algo, contents);
        let mut encoder = compression.encoder(Vec::new());
        encoder.write_all(contents)?;
        let archived = encoder.finish()?;
        let archived_checksum = Checksum::new_from_data(self.checksum_algo, &archived);
        let file = xml::File::new(
            self.files.len() as u64,
            status,
            xml::Data {
                archived_checksum: archived_checksum.into(),
                extracted_checksum: extracted_checksum.into(),
                encoding: xml::Encoding {
                    style: compression.as_str().into(),
                },
                size: contents.len() as u64,
                length: archived.len() as u64,
                offset: self.offset,
            },
        );
        self.offset += file.data.length;
        self.files.push(file);
        self.contents.push(archived);
        Ok(())
    }

    pub fn finish(self) -> Result<W, Error> {
        self.do_finish::<NoSigner>(None)
    }

    pub fn sign<S: XarSigner>(self, signer: &S) -> Result<W, Error> {
        self.do_finish(Some(signer))
    }

    fn do_finish<S: XarSigner>(mut self, signer: Option<&S>) -> Result<W, Error> {
        let checksum_len = self.checksum_algo.size() as u64;
        let xar = xml::Xar {
            toc: xml::Toc {
                checksum: xml::TocChecksum {
                    algo: self.checksum_algo,
                    offset: 0,
                    size: checksum_len,
                },
                files: self.files,
                // http://users.wfu.edu/cottrell/productsign/productsign_linux.html
                signatures: signer.map(|signer| {
                    xml::Signature {
                        style: signer.signature_style().into(),
                        offset: checksum_len,
                        size: signer.signature_len() as u64,
                        key_info: xml::KeyInfo {
                            data: xml::X509Data {
                                // TODO certs
                                certificates: Default::default(),
                            },
                        },
                    }
                }),
                creation_time: xml::Timestamp(SystemTime::now()),
            },
        };
        // write header and toc
        xar.write(self.writer.by_ref(), self.checksum_algo, signer)?;
        for content in self.contents.into_iter() {
            self.writer.write_all(&content)?;
        }
        Ok(self.writer)
    }

    pub fn get_mut(&mut self) -> &mut W {
        self.writer.by_ref()
    }

    pub fn get(&self) -> &W {
        &self.writer
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

impl ChecksumAlgorithm {
    pub fn size(self) -> usize {
        use ChecksumAlgorithm::*;
        match self {
            Sha1 => 20,
            Sha256 => 32,
            Sha512 => 64,
        }
    }
}

impl TryFrom<u32> for ChecksumAlgorithm {
    type Error = Error;
    fn try_from(other: u32) -> Result<Self, Self::Error> {
        match other {
            0 => Err(Error::other("no hashing algorithm")),
            1 => Ok(Self::Sha1),
            2 => Err(Error::other("unsafe md5 hashing algorithm")),
            3 => Ok(Self::Sha256),
            4 => Ok(Self::Sha512),
            other => Err(Error::other(format!("unknown hashing algorithm {}", other))),
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

    pub fn new_from_data(algo: ChecksumAlgorithm, data: &[u8]) -> Self {
        match algo {
            ChecksumAlgorithm::Sha1 => Self::Sha1(Sha1::compute(data)),
            ChecksumAlgorithm::Sha256 => Self::Sha256(Sha256::compute(data)),
            ChecksumAlgorithm::Sha512 => Self::Sha512(Sha512::compute(data)),
        }
    }

    pub fn compute(&self, data: &[u8]) -> Self {
        match self {
            Self::Sha1(..) => Self::Sha1(Sha1::compute(data)),
            Self::Sha256(..) => Self::Sha256(Sha256::compute(data)),
            Self::Sha512(..) => Self::Sha512(Sha512::compute(data)),
        }
    }

    pub fn algo(&self) -> ChecksumAlgorithm {
        match self {
            Self::Sha1(..) => ChecksumAlgorithm::Sha1,
            Self::Sha256(..) => ChecksumAlgorithm::Sha256,
            Self::Sha512(..) => ChecksumAlgorithm::Sha512,
        }
    }
}

impl TryFrom<String> for Checksum {
    type Error = Error;
    fn try_from(other: String) -> Result<Self, Self::Error> {
        let other = other.trim();
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

impl AsRef<[u8]> for Checksum {
    fn as_ref(&self) -> &[u8] {
        match self {
            Self::Sha1(h) => h.as_ref(),
            Self::Sha256(h) => h.as_ref(),
            Self::Sha512(h) => h.as_ref(),
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

impl From<u32> for FileMode {
    fn from(other: u32) -> Self {
        Self(other & 0o7777)
    }
}

#[derive(Serialize, Deserialize, Debug, Default)]
pub struct FileStatus {
    pub name: PathBuf,
    #[serde(rename = "type", default)]
    pub kind: FileKind,
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
    pub atime: xml::Timestamp,
    #[serde(default)]
    pub mtime: xml::Timestamp,
    #[serde(default)]
    pub ctime: xml::Timestamp,
}

impl From<Metadata> for FileStatus {
    fn from(other: Metadata) -> Self {
        use std::os::unix::fs::MetadataExt;
        Self {
            kind: other.file_type().into(),
            inode: other.ino(),
            deviceno: other.rdev(),
            mode: other.mode().into(),
            uid: other.uid(),
            gid: other.gid(),
            atime: (other.atime() as u64).try_into().unwrap_or_default(),
            mtime: (other.mtime() as u64).try_into().unwrap_or_default(),
            ctime: (other.ctime() as u64).try_into().unwrap_or_default(),
            ..Default::default()
        }
    }
}

#[derive(
    Serialize, Deserialize, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default,
)]
pub enum FileKind {
    #[default]
    #[serde(rename = "file")]
    File,
    #[serde(rename = "hardlink")]
    Hardlink,
    #[serde(rename = "directory")]
    Directory,
    #[serde(rename = "symlink")]
    Symlink,
    #[serde(rename = "fifo")]
    Fifo,
    #[serde(rename = "character special")]
    CharacterSpecial,
    #[serde(rename = "block special")]
    BlockSpecial,
    #[serde(rename = "socket")]
    Socket,
    #[serde(rename = "whiteout")]
    Whiteout,
}

impl From<FileType> for FileKind {
    fn from(other: FileType) -> Self {
        use std::os::unix::fs::FileTypeExt;
        if other.is_dir() {
            Self::Directory
        } else if other.is_symlink() {
            Self::Symlink
        } else if other.is_block_device() {
            Self::BlockSpecial
        } else if other.is_char_device() {
            Self::CharacterSpecial
        } else if other.is_fifo() {
            Self::Fifo
        } else if other.is_socket() {
            Self::Socket
        } else if other.is_file() {
            Self::File
        } else {
            Default::default()
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
pub enum XarCompression {
    None,
    #[default]
    Gzip,
    Bzip2,
}

impl XarCompression {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "application/octet-stream",
            Self::Gzip => "application/x-gzip",
            Self::Bzip2 => "application/x-bzip2",
        }
    }

    fn encoder<W: Write + 'static>(self, writer: W) -> XarEncoder<W> {
        match self {
            Self::None => XarEncoder::OctetStream(writer),
            Self::Gzip => XarEncoder::Gzip(ZlibEncoder::new(writer, flate2::Compression::best())),
            Self::Bzip2 => XarEncoder::Bzip2(BzEncoder::new(writer, bzip2::Compression::best())),
        }
    }
}

enum XarEncoder<W: Write> {
    OctetStream(W),
    Gzip(ZlibEncoder<W>),
    Bzip2(BzEncoder<W>),
}

impl<W: Write> XarEncoder<W> {
    fn finish(self) -> Result<W, Error> {
        match self {
            Self::OctetStream(w) => Ok(w),
            Self::Gzip(w) => w.finish(),
            Self::Bzip2(w) => w.finish(),
        }
    }
}

impl<W: Write> Write for XarEncoder<W> {
    fn write(&mut self, buf: &[u8]) -> Result<usize, Error> {
        match self {
            Self::OctetStream(w) => w.write(buf),
            Self::Gzip(w) => w.write(buf),
            Self::Bzip2(w) => w.write(buf),
        }
    }

    fn flush(&mut self) -> Result<(), Error> {
        match self {
            Self::OctetStream(w) => w.flush(),
            Self::Gzip(w) => w.flush(),
            Self::Bzip2(w) => w.flush(),
        }
    }
}

pub trait XarSigner {
    fn sign(&self, data: &[u8]) -> Result<Vec<u8>, Error>;
    fn signature_style(&self) -> &str;
    fn signature_len(&self) -> usize;
}

struct NoSigner;

impl XarSigner for NoSigner {
    fn sign(&self, _data: &[u8]) -> Result<Vec<u8>, Error> {
        Ok(Vec::new())
    }
    fn signature_style(&self) -> &str {
        ""
    }
    fn signature_len(&self) -> usize {
        0
    }
}

pub mod xml {
    use std::io::BufReader;

    use quick_xml::de::from_reader;
    use quick_xml::se::to_writer;

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

        pub fn write<W: Write, S: XarSigner>(
            &self,
            mut writer: W,
            checksum_algo: ChecksumAlgorithm,
            signer: Option<&S>,
        ) -> Result<(), Error> {
            let mut toc_uncompressed = String::new();
            toc_uncompressed.push_str(XML_DECLARATION);
            to_writer(&mut toc_uncompressed, self).map_err(Error::other)?;
            let toc_len_uncompressed = toc_uncompressed.as_bytes().len();
            eprintln!("toc {}", toc_uncompressed);
            let mut encoder = ZlibEncoder::new(Vec::new(), flate2::Compression::best());
            encoder.write_all(toc_uncompressed.as_bytes())?;
            let toc_compressed = encoder.finish()?;
            let header = Header {
                toc_len_compressed: toc_compressed.len() as u64,
                toc_len_uncompressed: toc_len_uncompressed as u64,
                checksum_algo,
            };
            eprintln!("write header {:?}", header);
            header.write(writer.by_ref())?;
            writer.write_all(&toc_compressed)?;
            let checksum = Checksum::new_from_data(checksum_algo, &toc_compressed);
            // heap starts
            debug_assert!(checksum.as_ref().len() == checksum_algo.size());
            writer.write_all(checksum.as_ref())?;
            if let Some(signer) = signer {
                let signature = signer
                    .sign(checksum.as_ref())
                    .map_err(|_| Error::other("failed to sign"))?;
                writer.write_all(&signature)?;
            }
            Ok(())
        }
    }

    #[derive(Serialize, Deserialize, Debug)]
    #[serde(rename = "toc", rename_all = "kebab-case")]
    pub struct Toc {
        pub checksum: TocChecksum,
        #[serde(default)]
        pub creation_time: Timestamp,
        #[serde(rename = "file", default)]
        pub files: Vec<File>,
        #[serde(rename = "signature", default)]
        pub signatures: Option<Signature>,
    }

    #[derive(Serialize, Deserialize, Debug)]
    #[serde(rename = "checksum")]
    pub struct TocChecksum {
        #[serde(rename = "@style")]
        pub algo: ChecksumAlgorithm,
        pub offset: u64,
        pub size: u64,
    }

    #[derive(Serialize, Deserialize, Debug, Clone)]
    #[cfg_attr(test, derive(PartialEq, Eq))]
    #[serde(rename = "file")]
    pub struct File {
        #[serde(rename = "@id")]
        pub id: u64,
        pub name: PathBuf,
        #[serde(rename = "type", default)]
        pub kind: FileKind,
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
        pub atime: xml::Timestamp,
        #[serde(default)]
        pub mtime: xml::Timestamp,
        #[serde(default)]
        pub ctime: xml::Timestamp,
        pub data: Data,
    }

    impl File {
        pub fn new(id: u64, status: FileStatus, data: Data) -> Self {
            Self {
                id,
                name: status.name,
                kind: status.kind,
                inode: status.inode,
                deviceno: status.deviceno,
                mode: status.mode,
                uid: status.uid,
                gid: status.gid,
                atime: status.atime,
                mtime: status.mtime,
                ctime: status.ctime,
                data,
            }
        }
    }

    #[derive(Serialize, Deserialize, Debug, Clone)]
    #[cfg_attr(test, derive(PartialEq, Eq))]
    #[serde(rename = "data", rename_all = "kebab-case")]
    pub struct Data {
        // TODO add custom properties here
        // ignore <contents>
        pub archived_checksum: FileChecksum,
        pub extracted_checksum: FileChecksum,
        pub encoding: Encoding,
        pub offset: u64,
        pub size: u64,
        pub length: u64,
    }

    #[derive(Serialize, Deserialize, Debug, Clone)]
    #[cfg_attr(test, derive(PartialEq, Eq))]
    #[serde(rename = "encoding")]
    pub struct Encoding {
        #[serde(rename = "@style")]
        pub style: String,
    }

    #[derive(Serialize, Deserialize, Debug, Clone)]
    #[cfg_attr(test, derive(PartialEq, Eq))]
    pub struct FileChecksum {
        #[serde(rename = "@style")]
        pub algo: ChecksumAlgorithm,
        #[serde(rename = "$value")]
        pub value: Checksum,
    }

    impl From<Checksum> for FileChecksum {
        fn from(other: Checksum) -> Self {
            Self {
                algo: other.algo(),
                value: other,
            }
        }
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

    #[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
    #[serde(try_from = "String", into = "String")]
    pub struct Timestamp(pub SystemTime);

    impl From<Timestamp> for String {
        fn from(other: Timestamp) -> String {
            let date_time: DateTime<Utc> = other.0.into();
            date_time.to_rfc3339_opts(SecondsFormat::Secs, true)
        }
    }

    impl TryFrom<String> for Timestamp {
        type Error = Error;
        fn try_from(other: String) -> Result<Self, Self::Error> {
            let Ok(t) = DateTime::parse_from_rfc3339(&other) else {
                return Ok(Default::default());
            };
            Ok(Self(t.to_utc().into()))
        }
    }

    impl TryFrom<u64> for Timestamp {
        type Error = Error;
        fn try_from(other: u64) -> Result<Self, Self::Error> {
            let t = UNIX_EPOCH
                .checked_add(Duration::from_secs(other))
                .ok_or_else(|| Error::other("invalid timestamp"))?;
            Ok(Self(t))
        }
    }

    impl Default for Timestamp {
        fn default() -> Self {
            Self(UNIX_EPOCH)
        }
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
const XML_DECLARATION: &str = r#"<?xml version="1.0" encoding="UTF-8"?>"#;

#[cfg(test)]
mod tests {
    use std::fs::File;

    use arbtest::arbtest;
    use normalize_path::NormalizePath;
    use tempfile::TempDir;
    use walkdir::WalkDir;

    use super::*;
    use crate::test::DirectoryOfFiles;

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

    #[test]
    fn xar_write_read() {
        let workdir = TempDir::new().unwrap();
        arbtest(|u| {
            let directory: DirectoryOfFiles = u.arbitrary()?;
            let xar_path = workdir.path().join("test.xar");
            let mut xar = XarBuilder::new(File::create(&xar_path).unwrap());
            for entry in WalkDir::new(directory.path()).into_iter() {
                let entry = entry.unwrap();
                let entry_path = entry
                    .path()
                    .strip_prefix(directory.path())
                    .unwrap()
                    .normalize();
                if entry_path == Path::new("") {
                    continue;
                }
                xar.add_file_by_path(entry_path, entry.path(), XarCompression::Gzip)
                    .unwrap();
            }
            let expected_files = xar.files().to_vec();
            xar.finish().unwrap();
            let reader = File::open(&xar_path).unwrap();
            let mut xar_archive = XarArchive::new(reader).unwrap();
            let mut actual_files = Vec::new();
            for mut entry in xar_archive.files() {
                actual_files.push(entry.file().clone());
                let mut buf = Vec::new();
                entry.reader().unwrap().read_to_end(&mut buf).unwrap();
                let actual_checksum = entry.file().data.extracted_checksum.value.compute(&buf);
                assert_eq!(
                    entry.file().data.extracted_checksum.value,
                    actual_checksum,
                    "file = {:?}",
                    entry.file()
                );
            }
            assert_eq!(expected_files, actual_files);
            Ok(())
        });
    }
}

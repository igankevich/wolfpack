use std::ffi::CString;
use std::io::Error;
use std::io::ErrorKind;
use std::io::Write;

use crate::hash::Md5Hash;
use crate::hash::Sha1Hash;
use crate::hash::Sha256Hash;
use crate::rpm::NonEmptyVec;
use crate::rpm::ValueIo;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(test, derive(arbitrary::Arbitrary))]
#[repr(u32)]
pub enum EntryKind {
    Char = 1,
    Int8 = 2,
    Int16 = 3,
    Int32 = 4,
    Int64 = 5,
    String = 6,
    Bin = 7,
    StringArray = 8,
    I18nString = 9,
}

impl EntryKind {
    pub fn validate_count(self, count: u32) -> Result<(), Error> {
        use EntryKind::*;
        if count == 0 || matches!(self, String | I18nString if count != 1) {
            return Err(Error::other(format!("{:?}: invalid count", self)));
        }
        Ok(())
    }

    pub fn align(self) -> usize {
        use EntryKind::*;
        match self {
            Char => 1,
            Int8 => 1,
            Int16 => 2,
            Int32 => 4,
            Int64 => 8,
            String => 1,
            Bin => 1,
            StringArray => 1,
            I18nString => 1,
        }
    }
}

impl TryFrom<u32> for EntryKind {
    type Error = Error;
    fn try_from(other: u32) -> Result<Self, Error> {
        use EntryKind::*;
        match other {
            1 => Ok(Char),
            2 => Ok(Int8),
            3 => Ok(Int16),
            4 => Ok(Int32),
            5 => Ok(Int64),
            6 => Ok(String),
            7 => Ok(Bin),
            8 => Ok(StringArray),
            9 => Ok(I18nString),
            other => Err(Error::other(format!("invalid index entry kind: {}", other))),
        }
    }
}

impl ValueIo for EntryKind {
    fn read(input: &[u8], count: usize) -> Result<Self, Error> {
        u32::read(input, count)?.try_into()
    }

    fn write<W: Write>(&self, writer: W) -> Result<(), Error> {
        (*self as u32).write(writer)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(test, derive(arbitrary::Arbitrary))]
#[repr(u32)]
pub enum HashAlgorithm {
    Md5 = 1,
    Sha1 = 2,
    RipeMd160 = 3,
    Md2 = 5,
    Tiger192 = 6,
    Haval5_160 = 7,
    Sha256 = 8,
    Sha384 = 9,
    Sha512 = 10,
    Sha224 = 11,
}

impl ValueIo for HashAlgorithm {
    fn read(input: &[u8], count: usize) -> Result<Self, Error> {
        use HashAlgorithm::*;
        match u32::read(input, count)? {
            1 => Ok(Md5),
            2 => Ok(Sha1),
            3 => Ok(RipeMd160),
            5 => Ok(Md2),
            6 => Ok(Tiger192),
            7 => Ok(Haval5_160),
            8 => Ok(Sha256),
            9 => Ok(Sha384),
            10 => Ok(Sha512),
            11 => Ok(Sha224),
            other => Err(Error::other(format!("invalid hash algorithm: {}", other))),
        }
    }

    fn write<W: Write>(&self, writer: W) -> Result<(), Error> {
        (*self as u32).write(writer)
    }
}

pub struct RawEntry {
    pub tag: u32,
    pub kind: EntryKind,
    pub offset: u32,
    pub count: u32,
}

impl RawEntry {
    pub fn read(index: &[u8], store_len: usize) -> Result<Self, Error> {
        if index.len() < ENTRY_LEN {
            return Err(Error::other("index entry is too small"));
        }
        let tag = u32::read(&index[0..4], 1)?;
        let kind = EntryKind::read(&index[4..8], 1)?;
        let offset = u32::read(&index[8..12], 1)?;
        if offset as usize >= store_len {
            return Err(Error::other("invalid offset in index entry"));
        }
        let count: u32 = u32::read(&index[12..16], 1)?;
        kind.validate_count(count)?;
        Ok(Self {
            tag,
            kind,
            offset,
            count,
        })
    }

    pub fn write<W: Write>(&self, mut index: W) -> Result<(), Error> {
        debug_assert!(self.count != 0, "zero count is illegal in rpm");
        self.tag.write(index.by_ref())?;
        self.kind.write(index.by_ref())?;
        self.offset.write(index.by_ref())?;
        self.count.write(index.by_ref())?;
        Ok(())
    }
}

pub trait EntryIo {
    type Tag;

    fn read(tag: u32, kind: EntryKind, count: u32, store: &[u8]) -> Result<Self, Error>
    where
        Self: Sized;

    fn write<W1: Write, W2: Write>(&self, index: W1, store: W2, offset: u32) -> Result<(), Error>;

    fn tag(&self) -> Self::Tag;

    fn leader_entry(index_len: u32) -> Self
    where
        Self: Sized;

    fn leader_tag() -> Self::Tag;
}

define_entry_enums! {
    Tag,
    Entry,
    Immutable,
    Immutable = (63, Bin, NonEmptyVec<u8>),
    //I18nTable = 100,
    Name = (1000, String, CString),
    Version = (1001, String, CString),
    Release = (1002, String, CString),
    //Epoch = 1003,
    Summary = (1004, I18nString, CString),
    Description = (1005, I18nString, CString),
    //BuildTime = 1006,
    //BuildHost = 1007,
    //InstallTime = 1008,
    Size = (1009, Int32, u32),
    //Distribution = 1010,
    Vendor = (1011, String, CString),
    //Gif = 1012,
    //Xpm = 1013,
    License = (1014, String, CString),
    //Packager = 1015,
    //Group = 1016,
    //Changelog = 1017,
    //Source = 1018,
    //Patch = 1019,
    Url = (1020, String, CString),
    Os = (1021, String, CString),
    Arch = (1022, String, CString),
    PreIn = (1023, String, CString),
    PostIn = (1024, String, CString),
    PreUn = (1025, String, CString),
    PostUn = (1026, String, CString),
    //OldFileNames = 1027,
    FileSizes = (1028, Int32, NonEmptyVec<u32>),
    //FileStates = 1029,
    FileModes = (1030, Int16, NonEmptyVec<u16>),
    //FileUids = 1031,
    //FileGids = 1032,
    FileRdevs = (1033, Int16, NonEmptyVec<u16>),
    FileMtimes = (1034, Int32, NonEmptyVec<u32>),
    FileDigests = (1035, StringArray, NonEmptyVec<CString>),
    FileLinkToS = (1036, StringArray, NonEmptyVec<CString>),
    FileFlags = (1037, Int32, NonEmptyVec<u32>),
    //Root = 1038,
    FileUserName = (1039, StringArray, NonEmptyVec<CString>),
    FileGroupName = (1040, StringArray, NonEmptyVec<CString>),
    //Exclude = 1041,
    //Exclusive = 1042,
    //Icon = 1043,
    //SourceRpm = 1044,
    FileVerifyFlags = (1045, Int32, NonEmptyVec<u32>),
    //ArchiveSize = 1046,
    //ProvideName = 1047,
    //RequireFlags = 1048,
    //RequireName = 1049,
    //RequireVersion = 1050,
    //NoSource = 1051,
    //NoPatch = 1052,
    //ConflictFlags = 1053,
    //ConflictName = 1054,
    //ConflictVersion = 1055,
    //DefaultPrefix = 1056,
    //BuildRoot = 1057,
    //InstallPrefix = 1058,
    //ExcludeArch = 1059,
    //ExcludeOs = 1060,
    //ExclusiveArch = 1061,
    //ExclusiveOs = 1062,
    //AutoreqProv = 1063,
    //RpmVersion = 1064,
    //TriggerScripts = 1065,
    //TriggerName = 1066,
    //TriggerVersion = 1067,
    //TriggerFlags = 1068,
    //TriggerIndex = 1069,
    //VerifyScript = 1079,
    //ChangelogTime = 1080,
    //ChangelogName = 1081,
    //ChangelogText = 1082,
    //BrokenMd5 = 1083,
    //Prereq = 1084,
    //PreInProg = 1085,
    //PostInProg = 1086,
    //PreUnProg = 1087,
    //PostUnProg = 1088,
    //BuildArchs = 1089,
    //ObsoleteName = 1090,
    //VerifyScriptProg = 1091,
    //TriggerScriptProg = 1092,
    //DocDir = 1093,
    //Cookie = 1094,
    FileDevices = (1095, Int32, NonEmptyVec<u32>),
    FileInodes = (1096, Int32, NonEmptyVec<u32>),
    FileLangs = (1097, StringArray, NonEmptyVec<CString>),
    //Prefixes = 1098,
    //InstPrefixes = 1099,
    //TriggerIn = 1100,
    //TriggerUn = 1101,
    //TriggerPostUn = 1102,
    //AutoReq = 1103,
    //AutoProv = 1104,
    //Capability = 1105,
    //SourcePackage = 1106,
    //OldOrigFileNames = 1107,
    //BuildPrereq = 1108,
    //BuildRequires = 1109,
    //BuildConflicts = 1110,
    //BuildMacros = 1111,
    //ProvideFlags = 1112,
    //ProvideVersion = 1113,
    //ObsoleteFlags = 1114,
    //ObsoleteVersion = 1115,
    DirIndexes = (1116, Int32, NonEmptyVec<u32>),
    BaseNames = (1117, StringArray, NonEmptyVec<CString>),
    DirNames = (1118, StringArray, NonEmptyVec<CString>),
    //OrigDirIndexes = 1119,
    //OrigBaseNames = 1120,
    //OrigDirNames = 1121,
    //OptFlags = 1122,
    //DistUrl = 1123,
    PayloadFormat = (1124, String, CString),
    PayloadCompressor = (1125, String, CString),
    //PayloadFlags = 1126,
    //InstallColor = 1127,
    //InstallTid = 1128,
    //RemoveTid = 1129,
    //Sha1Rhn = 1130,
    //RhnPlatform = 1131,
    //Platform = 1132,
    //PatchesName = 1133,
    //PatchesFlags = 1134,
    //PatchesVersion = 1135,
    //CacheCtime = 1136,
    //CachePkgPath = 1137,
    //CachePkgSize = 1138,
    //CachePkgMtime = 1139,
    FileColors = (1140, Int32, NonEmptyVec<u32>),
    FileClass = (1141, Int32, NonEmptyVec<u32>),
    //ClassDict = 1142,
    FileDependsX = (1143, Int32, NonEmptyVec<u32>),
    FileDependsN = (1144, Int32, NonEmptyVec<u32>),
    DependsDict = (1145, Int32, NonEmptyVec<u32>),
    //SourcePkgId = 1146,
    //FileContexts = 1147,
    //FsContexts = 1148,
    //ReContexts = 1149,
    //Policies = 1150,
    //PreTrans = 1151,
    //PostTrans = 1152,
    //PreTransProg = 1153,
    //PostTransProg = 1154,
    //DistTag = 1155,
    //OldSuggestsName = 1156,
    //OldSuggestsVersion = 1157,
    //OldSuggestsFlags = 1158,
    //OldEnhancesName = 1159,
    //OldEnhancesVersion = 1160,
    //OldEnhancesFlags = 1161,
    //Priority = 1162,
    //CvsId = 1163,
    //BlinkPkgId = 1164,
    //BlinkHdrId = 1165,
    //BlinkNevra = 1166,
    //FlinkPkgId = 1167,
    //FlinkHdrId = 1168,
    //FlinkNevra = 1169,
    //PackageOrigin = 1170,
    //TriggerPreIn = 1171,
    //BuildSuggests = 1172,
    //BuildEnhances = 1173,
    //ScriptStates = 1174,
    //ScriptMetrics = 1175,
    //BuildCpuClock = 1176,
    //FileDigestAlgos = 1177,
    //Variants = 1178,
    //Xmajor = 1179,
    //Xminor = 1180,
    //RepoTag = 1181,
    //Keywords = 1182,
    //BuildPlatforms = 1183,
    //PackageColor = 1184,
    //PackagePrefColor = 1185,
    //XattrsDict = 1186,
    //FileXattrsx = 1187,
    //DepAttrsDict = 1188,
    //ConflictAttrsX = 1189,
    //ObsoleteAttrsX = 1190,
    //ProvideAttrsX = 1191,
    //RequireAttrsX = 1192,
    //BuildProvides = 1193,
    //BuildObsoletes = 1194,
    //DbInstance = 1195,
    //Nvra = 1196,
    //FileNames = 5000,
    //FileProvide = 5001,
    //FileRequire = 5002,
    //FsNames = 5003,
    //FsSizes = 5004,
    //TriggerConds = 5005,
    //TriggerType = 5006,
    //OrigFileNames = 5007,
    LongFileSizes = (5008, Int64, NonEmptyVec<u64>),
    LongSize = (5009, Int64, u64),
    //FileCaps = 5010,
    FileDigestAlgo = (5011, Int32, HashAlgorithm),
    //BugUrl = 5012,
    //Evr = 5013,
    //Nvr = 5014,
    //Nevr = 5015,
    //Nevra = 5016,
    //HeaderColor = 5017,
    //Verbose = 5018,
    //EpochNum = 5019,
    //PreinFlags = 5020,
    //PostinFlags = 5021,
    //PreunFlags = 5022,
    //PostunFlags = 5023,
    //PreTransFlags = 5024,
    //PostTransFlags = 5025,
    //VerifyScriptFlags = 5026,
    //TriggerScriptFlags = 5027,
    //Collections = 5029,
    //PolicyNames = 5030,
    //PolicyTypes = 5031,
    //PolicyTypesIndexes = 5032,
    //PolicyFlags = 5033,
    //Vcs = 5034,
    //OrderName = 5035,
    //OrderVersion = 5036,
    //OrderFlags = 5037,
    //MssfManifest = 5038,
    //MssfDomain = 5039,
    //InstFileNames = 5040,
    //RequireNevrs = 5041,
    //ProvideNevrs = 5042,
    //ObsoleteNevrs = 5043,
    //ConflictNevrs = 5044,
    //FilenLinks = 5045,
    //RecommendName = 5046,
    //RecommendVersion = 5047,
    //RecommendFlags = 5048,
    //SuggestName = 5049,
    //SuggestVersion = 5050,
    //SuggestFlags = 5051,
    //SupplementName = 5052,
    //SupplementVersion = 5053,
    //SupplementFlags = 5054,
    //EnhanceName = 5055,
    //EnhanceVersion = 5056,
    //EnhanceFlags = 5057,
    //RecommendNevrs = 5058,
    //SuggestNevrs = 5059,
    //SupplementNevrs = 5060,
    //EnhanceNevrs = 5061,
    //Encoding = 5062,
    //FileTriggerIn = 5063,
    //FileTriggerUn = 5064,
    //FileTriggerPostUn = 5065,
    //FileTriggerScripts = 5066,
    //FileTriggerScriptProg = 5067,
    //FileTriggerScriptFlags = 5068,
    //FileTriggerName = 5069,
    //FileTriggerIndex = 5070,
    //FileTriggerVersion = 5071,
    //FileTriggerFlags = 5072,
    //TransFileTriggerIn = 5073,
    //TransFileTriggerUn = 5074,
    //TransFileTriggerPostUn = 5075,
    //TransFileTriggerScripts = 5076,
    //TransFileTriggerScriptProg = 5077,
    //TransFileTriggerScriptFlags = 5078,
    //TransFileTriggerName = 5079,
    //TransFileTriggerIndex = 5080,
    //TransFileTriggerVersion = 5081,
    //TransFileTriggerFlags = 5082,
    //RemovePathPostfixes = 5083,
    //FileTriggerPriorities = 5084,
    //TransFileTriggerPriorities = 5085,
    //FileTriggerConds = 5086,
    //FileTriggerType = 5087,
    //TransFileTriggerConds = 5088,
    //TransFileTriggerType = 5089,
    //FileSignatures = 5090,
    //FileSignatureLength = 5091,
    PayloadDigest = (5092, StringArray, Sha256Hash),
    PayloadDigestAlgo = (5093, Int32, HashAlgorithm),
    //AutoInstalled = 5094,
    //Identity = 5095,
    //ModularityLabel = 5096,
    PayloadDigestAlt = (5097, StringArray, Sha256Hash),
    //ArchSuffix = 5098,
    //Spec = 5099,
    //TranslationUrl = 5100,
    //UpstreamReleases = 5101,
    //SourceLicense = 5102,
    //PreunTrans = 5103,
    //PostunTrans = 5104,
    //PreunTransProg = 5105,
    //PostunTransProg = 5106,
    //PreunTransFlags = 5107,
    //PostunTransFlags = 5108,
    //SysUsers = 5109,
    //BuildSystem = 5110,
    //BuildOption = 5111,
    //PayloadSize = 5112,
    //PayloadSizeAlt = 5113,
    //RpmFormat = 5114,
    //FileMimeIndex = 5115,
    //MimeDict = 5116,
    //FileMimes = 5117,
}

define_entry_enums! {
    SignatureTag,
    SignatureEntry,
    Signatures,
    Signatures = (62, Bin, NonEmptyVec<u8>),
    Size = (1000, Int32, u32),
    //LeMd5_1 = 1001,
    //Pgp = 1002,
    //LeMd5_2 = 1003,
    Md5 = (1004, Bin, Md5Hash),
    Gpg = (1005, Bin, NonEmptyVec<u8>),
    //Pgp5 = 1006,
    PayloadSize = (1007, Int32, u32),
    //ReservedSpace = 1008,
    //BadSha1_1 = 264,
    //BadSha1_2 = 265,
    Dsa = (267, Bin, NonEmptyVec<u8>),
    Rsa = (268, Bin, NonEmptyVec<u8>),
    Sha1 = (269, String, Sha1Hash),
    LongSize = (270, Int64, u64),
    LongArchiveSize = (271, Int64, u64),
    Sha256 = (273, String, Sha256Hash),
    //FileSignatures = 274,
    //FileSignatureLength = 275,
    //VeritySignatures = 276,
    //VeritySignatureAlgo = 277,
}

pub(crate) fn pad(offset: u32, align: u32) -> u32 {
    let remaining = offset % align;
    if remaining == 0 {
        return 0;
    }
    align - remaining
}

pub(crate) const ENTRY_LEN: usize = 16;

macro_rules! define_entry_enums {
    {
        $tag_enum:ident,
        $entry_enum:ident,
        $leader_tag:ident,
        $($name:ident = (
            $value:literal,
            $entry_kind:ident,
            $entry_type:ty
        ),)*
    } => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
        #[cfg_attr(test, derive(arbitrary::Arbitrary))]
        #[repr(u32)]
        pub enum $tag_enum {
            $( $name = $value, )*
            Other(u32),
        }

        impl $tag_enum {
            pub fn as_u32(self) -> u32 {
                self.into()
            }
        }

        impl From<u32> for $tag_enum {
            fn from(other: u32) -> Self {
                match other {
                    $( $value => $tag_enum::$name, )*
                    other => $tag_enum::Other(other),
                }
            }
        }

        impl From<$tag_enum> for u32 {
            fn from(other: $tag_enum) -> Self {
                match other {
                    $( $tag_enum::$name => $value, )*
                    $tag_enum::Other(other) => other,
                }
            }
        }

        impl ValueIo for $tag_enum {
            fn read(input: &[u8], count: usize) -> Result<Self, Error> {
                Ok(u32::read(input, count)?.into())
            }

            fn write<W: Write>(&self, writer: W) -> Result<(), Error> {
                let i: u32 = (*self).into();
                i.write(writer)
            }
        }

        #[derive(Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
        #[cfg_attr(test, derive(arbitrary::Arbitrary))]
        pub enum $entry_enum {
            $( $name($entry_type), )*
        }

        impl $entry_enum {
            pub fn kind(&self) -> EntryKind {
                match self {
                    $( $entry_enum::$name(..) => EntryKind::$entry_kind, )*
                }
            }

            pub fn count(&self) -> usize {
                match self {
                    $( $entry_enum::$name(v) => ValueIo::count(v), )*
                }
            }

            fn raw_entry(&self, mut offset: u32) -> Result<(RawEntry, u32), Error> {
                let (tag, kind, count) = match self {
                    $( $entry_enum::$name(v) => ($tag_enum::$name, EntryKind::$entry_kind, ValueIo::count(v)), )*
                };
                if count > u32::MAX as usize {
                    return Err(Error::other("rpm index entry is too big"));
                }
                let padding = pad(offset, kind.align() as u32);
                offset += padding;
                let raw = RawEntry {tag: tag.into(), kind, offset, count: count as u32};
                Ok((raw, padding))
            }

            fn do_write<W: Write>(&self, store: W) -> Result<(), Error> {
                match self {
                    $( $entry_enum::$name(value) => ValueIo::write(value, store), )*
                }
            }
        }

        impl EntryIo for $entry_enum {
            type Tag = $tag_enum;

            fn tag(&self) -> $tag_enum {
                match self {
                    $( $entry_enum::$name(..) => $tag_enum::$name, )*
                }
            }

            fn leader_tag() -> Self::Tag {
                $tag_enum::$leader_tag
            }

            fn leader_entry(index_len: u32) -> Self where Self: Sized {
                let tag: u32 = $tag_enum::$leader_tag.into();
                let offset: i32 = -(index_len as i32);
                let mut data = Vec::new();
                data.extend(tag.to_be_bytes());
                data.extend((EntryKind::Bin as u32).to_be_bytes());
                data.extend((offset as u32).to_be_bytes());
                data.extend(16_u32.to_be_bytes());
                $entry_enum::$leader_tag(data.try_into().expect("always non-empty"))
            }

            fn read(
                tag: u32,
                kind: EntryKind,
                count: u32,
                store: &[u8]
            ) -> Result<$entry_enum, Error> {
                let tag: $tag_enum = tag.into();
                match tag {
                    $( $tag_enum::$name => {
                        debug_assert!(
                            EntryKind::$entry_kind == kind,
                            "{:?}: invalid entry type: expected {:?}, actual {:?}",
                            tag,
                            EntryKind::$entry_kind,
                            kind
                        );
                        let value = ValueIo::read(store, count as usize)?;
                        Ok($entry_enum::$name(value))
                    }, )*
                    $tag_enum::Other(_tag) => Err(Error::new(ErrorKind::InvalidData, "unsupported tag")),
                }
            }

            fn write<W1: Write, W2: Write>(
                &self,
                index: W1,
                mut store: W2,
                offset: u32,
            ) -> Result<(), Error> {
                let (raw, padding) = self.raw_entry(offset)?;
                raw.write(index)?;
                if padding != 0 {
                    store.write_all(get_zeroes(padding as usize))?;
                }
                self.do_write(store)?;
                Ok(())
            }
        }

        impl From<$entry_enum> for ($tag_enum, $entry_enum) {
            fn from(other: $entry_enum) -> ($tag_enum, $entry_enum) {
                (other.tag(), other)
            }
        }
    };
}

use define_entry_enums;

pub(crate) fn get_zeroes(len: usize) -> &'static [u8] {
    debug_assert!(len <= PADDING.len());
    &PADDING[..len]
}

const PADDING: [u8; 7] = [0_u8; 7];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rpm::test::write_read_entry_symmetry;
    use crate::rpm::test::write_read_symmetry;

    #[test]
    fn symmetry() {
        write_read_symmetry::<HashAlgorithm>();
        write_read_symmetry::<EntryKind>();
        write_read_symmetry::<Tag>();
        write_read_symmetry::<SignatureTag>();
        write_read_entry_symmetry::<SignatureEntry>();
        write_read_entry_symmetry::<Entry>();
    }
}

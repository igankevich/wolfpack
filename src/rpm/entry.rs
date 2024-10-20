use std::io::Error;
use std::io::Write;
use std::str::from_utf8;

use crate::hash::Md5Hash;
use crate::hash::Sha1Hash;
use crate::hash::Sha256Hash;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u32)]
pub enum EntryKind {
    Null = 0,
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
}

impl TryFrom<u32> for EntryKind {
    type Error = Error;
    fn try_from(other: u32) -> Result<Self, Error> {
        use EntryKind::*;
        match other {
            0 => Ok(Null),
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

pub trait EntryRead {
    fn read(input: &[u8], store: &[u8]) -> Result<Option<Self>, Error>
    where
        Self: Sized;

    fn write<W1: Write, W2: Write>(
        &self,
        writer: &mut W1,
        store: &mut W2,
        offset: u32,
    ) -> Result<(), Error>;
}

define_entry_enums! {
    //Size = (1000, Int32, u32, one, read_u32, write_u32),
    Tag,
    Entry,
    //I18nTable = 100,
    Name = (1000, String, String, one, read_string, write_string),
    Version = (1001, String, String, one, read_string, write_string),
    //Release = 1002,
    //Epoch = 1003,
    Summary = (1004, I18nString, String, one, read_string, write_string),
    Description = (1005, I18nString, String, one, read_string, write_string),
    //BuildTime = 1006,
    //BuildHost = 1007,
    //InstallTime = 1008,
    Size = (1009, Int32, u32, one, read_u32, write_u32),
    //Distribution = 1010,
    Vendor = (1011, String, String, one, read_string, write_string),
    //Gif = 1012,
    //Xpm = 1013,
    License = (1014, String, String, one, read_string, write_string),
    //Packager = 1015,
    //Group = 1016,
    //Changelog = 1017,
    //Source = 1018,
    //Patch = 1019,
    Url = (1020, String, String, one, read_string, write_string),
    //Os = 1021,
    Arch = (1022, String, String, one, read_string, write_string),
    //PreIn = 1023,
    //PostIn = 1024,
    //PreUn = 1025,
    //PostUn = 1026,
    //OldFileNames = 1027,
    //FileSizes = 1028,
    //FileStates = 1029,
    //FileModes = 1030,
    //FileUids = 1031,
    //FileGids = 1032,
    //FileRdevs = 1033,
    //FileMtimes = 1034,
    //FileDigests = 1035,
    //FileLinkTos = 1036,
    //FileFlags = 1037,
    //Root = 1038,
    //FileUserName = 1039,
    //FileGroupName = 1040,
    //Exclude = 1041,
    //Exclusive = 1042,
    //Icon = 1043,
    //SourceRpm = 1044,
    //FileVerifyFlags = 1045,
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
    //FileDevices = 1095,
    //FileInodes = 1096,
    //FileLangs = 1097,
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
    //DirIndexes = 1116,
    //BaseNames = 1117,
    //DirNames = 1118,
    //OrigDirIndexes = 1119,
    //OrigBaseNames = 1120,
    //OrigDirNames = 1121,
    //OptFlags = 1122,
    //DistUrl = 1123,
    //PayloadFormat = 1124,
    //PayloadCompressor = 1125,
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
    //FileColors = 1140,
    //FileClass = 1141,
    //ClassDict = 1142,
    //FileDependsX = 1143,
    //FileDependsN = 1144,
    //DependsDict = 1145,
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
    //LongFileSizes = 5008,
    //LongSize = 5009,
    //FileCaps = 5010,
    //FileDigestAlgo = 5011,
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
    //PayloadDigest = 5092,
    //PayloadDigestAlgo = 5093,
    //AutoInstalled = 5094,
    //Identity = 5095,
    //ModularityLabel = 5096,
    //PayloadDigestAlt = 5097,
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
    Signatures = (62, Bin, Vec<u8>, Vec::len, read_vec, write_vec),
    Size = (1000, Int32, u32, one, read_u32, write_u32),
    //LeMd5_1 = 1001,
    //Pgp = 1002,
    //LeMd5_2 = 1003,
    Md5 = (1004, Bin, Md5Hash, Md5Hash::len, read_md5, write_md5),
    //Gpg = 1005,
    //Pgp5 = 1006,
    PayloadSize = (1007, Int32, u32, one, read_u32, write_u32),
    //ReservedSpace = 1008,
    //BadSha1_1 = 264,
    //BadSha1_2 = 265,
    Dsa = (267, Bin, Vec<u8>, Vec::len, read_vec, write_vec),
    Rsa = (268, Bin, Vec<u8>, Vec::len, read_vec, write_vec),
    Sha1 = (269, String, Sha1Hash, Sha1Hash::len, read_sha1, write_sha1),
    //LongSize = 270,
    //LongArchiveSize = 271,
    Sha256 = (273, String, Sha256Hash, Sha256Hash::len, read_sha256, write_sha256),
    //FileSignatures = 274,
    //FileSignatureLength = 275,
    //VeritySignatures = 276,
    //VeritySignatureAlgo = 277,
}

fn one<T>(_: T) -> usize {
    1
}

fn read_u32(input: &[u8], _count: usize) -> Result<u32, Error> {
    assert!(4 <= input.len());
    Ok(u32::from_be_bytes([input[0], input[1], input[2], input[3]]))
}

fn write_u32<W: Write>(writer: &mut W, value: &u32) -> Result<(), Error> {
    writer.write_all(value.to_be_bytes().as_slice())
}

fn read_vec(input: &[u8], count: usize) -> Result<Vec<u8>, Error> {
    assert!(count <= input.len());
    Ok(input[..count].into())
}

fn write_vec<W: Write>(writer: &mut W, value: &[u8]) -> Result<(), Error> {
    writer.write_all(value)
}

fn read_string(input: &[u8], _count: usize) -> Result<String, Error> {
    let n = input
        .iter()
        .position(|ch| *ch == 0)
        .ok_or_else(|| Error::other("string is not terminated"))?;
    let s =
        from_utf8(&input[..n]).map_err(|e| Error::other(format!("invalid utf-8 string: {}", e)))?;
    Ok(s.into())
}

fn write_string<W: Write>(writer: &mut W, value: &str) -> Result<(), Error> {
    writer.write_all(value.as_bytes())?;
    writer.write_all(&[0_u8])?;
    Ok(())
}

fn read_md5(input: &[u8], count: usize) -> Result<Md5Hash, Error> {
    input
        .get(..count)
        .ok_or_else(|| Error::other("invalid md5 size"))?
        .try_into()
        .map_err(|_| Error::other("invalid md5 size"))
}

fn write_md5<W: Write>(writer: &mut W, value: &Md5Hash) -> Result<(), Error> {
    write_vec(writer, value.as_slice())
}

fn read_sha1(input: &[u8], count: usize) -> Result<Sha1Hash, Error> {
    read_string(input, count)?
        .parse()
        .map_err(|_| Error::other("invalid sha1"))
}

fn write_sha1<W: Write>(writer: &mut W, value: &Sha1Hash) -> Result<(), Error> {
    write_vec(writer, value.as_slice())
}

fn read_sha256(input: &[u8], count: usize) -> Result<Sha256Hash, Error> {
    read_string(input, count)?
        .parse()
        .map_err(|_| Error::other("invalid sha256"))
}

fn write_sha256<W: Write>(writer: &mut W, value: &Sha256Hash) -> Result<(), Error> {
    write_vec(writer, value.as_slice())
}

const INDEX_ENTRY_LEN: usize = 16;

macro_rules! define_entry_enums {
    {
        $tag_enum:ident,
        $entry_enum:ident,
        $($name:ident = (
            $value:literal,
            $entry_kind:ident,
            $entry_type:ty,
            $entry_count:expr,
            $entry_read:expr,
            $entry_write:expr
        ),)*
    } => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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

        #[derive(Debug, PartialEq, Eq, Hash)]
        pub enum $entry_enum {
            $( $name($entry_type), )*
        }

        impl $entry_enum {
            pub fn tag(&self) -> $tag_enum {
                match self {
                    $( $entry_enum::$name(..) => $tag_enum::$name, )*
                }
            }

            pub fn kind(&self) -> EntryKind {
                match self {
                    $( $entry_enum::$name(..) => EntryKind::$entry_kind, )*
                }
            }

            pub fn count(&self) -> usize {
                match self {
                    $( $entry_enum::$name(v) => $entry_count(v), )*
                }
            }

            fn tag_kind_count(&self) -> ($tag_enum, EntryKind, usize) {
                match self {
                    $( $entry_enum::$name(v) => (
                        $tag_enum::$name,
                        EntryKind::$entry_kind,
                        $entry_count(v)
                    ), )*
                }
            }

            fn do_read(
                tag: $tag_enum,
                kind: EntryKind,
                count: usize,
                input: &[u8]
            ) -> Result<Option<$entry_enum>, Error> {
                match tag {
                    $( $tag_enum::$name => {
                        if EntryKind::$entry_kind != kind {
                            return Err(Error::other(format!(
                                "{:?}: invalid entry type: expected {:?}, actual {:?}",
                                tag,
                                EntryKind::$entry_kind,
                                kind,
                            )));
                        }
                        let value = $entry_read(input, count)?;
                        Ok(Some($entry_enum::$name(value)))
                    },)*
                    $tag_enum::Other(tag) => {
                        eprintln!("unsupported tag: {}", tag);
                        Ok(None)
                    }
                }
            }

            fn do_write<W: Write>(&self, store: &mut W) -> Result<(), Error> {
                match self {
                    $( $entry_enum::$name(value) => $entry_write(store, value), )*
                }
            }
        }

        impl EntryRead for $entry_enum {
            fn read(input: &[u8], store: &[u8]) -> Result<Option<Self>, Error> {
                if input.len() < INDEX_ENTRY_LEN {
                    return Err(Error::other("rpm index entry is too small"));
                }
                let tag: $tag_enum = read_u32(&input[0..4], 1)?.into();
                let kind: EntryKind = read_u32(&input[4..8], 1)?.try_into()?;
                let offset = read_u32(&input[8..12], 1)? as usize;
                if offset >= store.len() {
                    return Err(Error::other("invalid offset in index entry"));
                }
                let count: u32 = read_u32(&input[12..16], 1)?;
                kind.validate_count(count)?;
                eprintln!("tag {:?} {:?} {}", tag, kind, count);
                $entry_enum::do_read(tag, kind, count as usize, &store[offset..])
            }

            fn write<W1: Write, W2: Write>(
                &self,
                index: &mut W1,
                store: &mut W2,
                offset: u32,
            ) -> Result<(), Error> {
                let (tag, kind, count) = self.tag_kind_count();
                write_u32(index, &tag.as_u32())?;
                write_u32(index, &(kind as u32))?;
                write_u32(index, &offset)?;
                if count > u32::MAX as usize {
                    return Err(Error::other("rpm index entry is too big"));
                }
                write_u32(index, &(count as u32))?;
                self.do_write(store)?;
                Ok(())
            }
        }
    };
}

pub(crate) use define_entry_enums;

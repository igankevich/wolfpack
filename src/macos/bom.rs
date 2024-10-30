use std::collections::HashMap;
use std::collections::HashSet;
use std::collections::VecDeque;
use std::ffi::CStr;
use std::ffi::CString;
use std::ffi::OsStr;
use std::ffi::OsString;
use std::fs::FileType;
use std::io::Error;
use std::io::Read;
use std::io::Seek;
use std::io::SeekFrom;
use std::io::Write;
use std::ops::Deref;
use std::ops::DerefMut;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;
use std::path::PathBuf;

use normalize_path::NormalizePath;
use walkdir::WalkDir;

struct Bom {
    nodes: Nodes,
}

impl Bom {
    fn write<W: Write + Seek>(&self, mut writer: W) -> Result<(), Error> {
        // skip the header
        writer.seek(SeekFrom::Start(HEADER_LEN as u64))?;
        let bom_info = BomInfo {
            num_paths: 0,
            entries: Default::default(),
        };
        let mut blocks = Blocks::new();
        let mut vars = Vars::new();
        // bom info
        {
            let i = blocks.write_block(writer.by_ref(), |writer| bom_info.write(writer))?;
            vars.insert(BOM_INFO.into(), i);
        }
        // v index
        {
            let paths = Paths::null();
            let i = blocks.write_block(writer.by_ref(), |writer| paths.write(writer))?;
            let tree = Tree::new_v_index(i);
            let i = blocks.write_block(writer.by_ref(), |writer| tree.write(writer))?;
            vars.insert(V_INDEX.into(), i);
        }
        // hl index
        {
            let paths = Paths::null();
            let i = blocks.write_block(writer.by_ref(), |writer| paths.write(writer))?;
            let tree = Tree::null(i);
            let i = blocks.write_block(writer.by_ref(), |writer| tree.write(writer))?;
            vars.insert(HL_INDEX.into(), i);
        }
        // size 64
        {
            let paths = Paths::null();
            let i = blocks.write_block(writer.by_ref(), |writer| paths.write(writer))?;
            let tree = Tree::null(i);
            let i = blocks.write_block(writer.by_ref(), |writer| tree.write(writer))?;
            vars.insert(SIZE_64.into(), i);
        }
        // paths
        {
        }
        // TODO vars
        // TODO blocks
        // write the header
        writer.seek(SeekFrom::Start(0))?;
        let header = Header {
            num_non_null_blocks: 0,
            index_offset: HEADER_LEN as u32,
            index_len: 0,
            vars_offset: 0,
            vars_len: 0,
        };
        header.write(writer.by_ref())?;
        Ok(())
    }

    fn read<R: Read>(mut reader: R) -> Result<Self, Error> {
        let mut file = Vec::new();
        reader.read_to_end(&mut file)?;
        let header = Header::read(&file[..HEADER_LEN])?;
        eprintln!("header {header:?}");
        let index_offset = header.index_offset as usize;
        let index_len = header.index_len as usize;
        let vars_offset = header.vars_offset as usize;
        let vars_len = header.vars_len as usize;
        let mut vars = Vars::read(&file[vars_offset..(vars_offset + vars_len)])?;
        let blocks = Blocks::read(&file[index_offset..(index_offset + index_len)])?;
        {
            let name = BOM_INFO;
            let index = vars
                .remove(name)
                .ok_or_else(|| Error::other(format!("{:?} is missing", name)))?;
            let bom_info = BomInfo::read(blocks.slice(index, &file)?)?;
            eprintln!("bom info {:?}", bom_info);
        }
        let mut trees = VecDeque::new();
        {
            let name = V_INDEX;
            let index = vars
                .remove(name)
                .ok_or_else(|| Error::other(format!("{:?} is missing", name)))?;
            let v_index = VIndex::read(blocks.slice(index, &file)?)?;
            eprintln!("v index {:?}", v_index);
            let name: CString = c"VIndex.index".into();
            trees.push_back((name, v_index.index));
        }
        let mut paths = VecDeque::new();
        let vars = vars.vars;
        eprintln!("vars {:?}", vars);
        for (name, index) in vars.into_iter() {
            trees.push_back((name, index));
        }
        while let Some((name, index)) = trees.pop_front() {
            let tree = match Tree::read(blocks.slice(index, &file)?) {
                Ok(tree) => tree,
                Err(e) => {
                    eprintln!("failed to parse {:?} as tree: {}", name, e);
                    continue;
                }
            };
            eprintln!("tree {:?} {:?}", name.to_str(), tree);
            paths.push_back(tree.child);
        }
        // id -> data
        let mut nodes = HashMap::new();
        let mut visited = HashSet::new();
        while let Some(index) = paths.pop_front() {
            if !visited.insert(index) {
                //eprintln!("loop {}", index);
                continue;
                //return Err(Error::other("loop"));
            }
            let path = Paths::read(blocks.slice(index, &file)?)?;
            //eprintln!("paths {:?}", path);
            // is_leaf == 0 means count == 1?
            for (index0, index1) in path.indices.into_iter() {
                let child = if !path.is_leaf {
                    paths.push_back(index0);
                    // TODO ???
                    None
                } else {
                    let block_bytes = blocks.slice(index0, &file)?;
                    let id = u32_read(&block_bytes[0..4]);
                    let index = u32_read(&block_bytes[4..8]);
                    let block_bytes = blocks.slice(index, &file)?;
                    let kind: NodeKind = block_bytes[0].try_into()?;
                    let _x0 = block_bytes[1];
                    let _arch = u16_read(&block_bytes[2..4]);
                    let mode = u16_read(&block_bytes[4..6]);
                    let uid = u32_read(&block_bytes[6..10]);
                    let gid = u32_read(&block_bytes[10..14]);
                    let mtime = u32_read(&block_bytes[14..18]);
                    let size = u32_read(&block_bytes[18..22]);
                    let node = Node {
                        id,
                        metadata: Metadata {
                            kind,
                            mode: mode & 0o7777,
                            uid,
                            gid,
                            mtime,
                            size,
                        },
                        parent: 0,
                        name: Default::default(),
                    };
                    Some(node)
                };
                //eprintln!("path {} {}", index0, index1);
                {
                    let block_bytes = blocks.slice(index1, &file)?;
                    let parent = u32_read(&block_bytes[0..4]);
                    let name =
                        CStr::from_bytes_with_nul(&block_bytes[4..]).map_err(Error::other)?;
                    let name = OsStr::from_bytes(name.to_bytes());
                    //eprintln!("file parent {} name {}", parent, name,);
                    if let Some(mut child) = child {
                        child.name = name.into();
                        child.parent = parent;
                        nodes.insert(child.id, child);
                    }
                }
            }
            if path.forward != 0 {
                paths.push_back(path.forward);
            }
            if path.backward != 0 {
                paths.push_back(path.backward);
            }
        }
        let nodes = Nodes { nodes };
        let paths = nodes.to_paths()?;
        for (path, metadata) in paths.iter() {
            eprintln!("{:?} {:?}", path, metadata);
        }
        Ok(Self { nodes })
    }
}

#[derive(Debug)]
struct Header {
    num_non_null_blocks: u32,
    index_offset: u32,
    index_len: u32,
    vars_offset: u32,
    vars_len: u32,
}

impl Header {
    fn read<R: Read>(mut reader: R) -> Result<Self, Error> {
        let mut file = [0_u8; HEADER_LEN];
        reader.read_exact(&mut file[..])?;
        if file[..MAGIC.len()] != MAGIC[..] {
            return Err(Error::other("not a bom store"));
        }
        let version = u32_read(&file[8..12]);
        if version != 1 {
            return Err(Error::other(format!(
                "unsupported BOM store version: {}",
                version
            )));
        }
        let num_non_null_blocks = u32_read(&file[12..16]);
        let index_offset = u32_read(&file[16..20]);
        let index_len = u32_read(&file[20..24]);
        let vars_offset = u32_read(&file[24..28]);
        let vars_len = u32_read(&file[28..32]);
        eprintln!("vars offset {} len {}", vars_offset, vars_len);
        eprintln!("index offset {} len {}", index_offset, index_len);
        eprintln!("num non null blocks {}", num_non_null_blocks);
        Ok(Self {
            num_non_null_blocks,
            index_offset,
            index_len,
            vars_offset,
            vars_len,
        })
    }

    fn write<W: Write>(&self, mut writer: W) -> Result<(), Error> {
        u32_write(writer.by_ref(), VERSION)?;
        u32_write(writer.by_ref(), self.num_non_null_blocks)?;
        u32_write(writer.by_ref(), self.index_offset)?;
        u32_write(writer.by_ref(), self.index_len)?;
        u32_write(writer.by_ref(), self.vars_offset)?;
        u32_write(writer.by_ref(), self.vars_len)?;
        Ok(())
    }
}

struct Vars {
    /// Variable name -> block index.
    vars: HashMap<CString, u32>,
}

impl Vars {
    fn new() -> Self {
        Self {
            vars: Default::default(),
        }
    }

    fn read<R: Read>(mut reader: R) -> Result<Self, Error> {
        let num_vars = u32_read_v2(reader.by_ref())? as usize;
        let mut vars = HashMap::with_capacity(num_vars);
        for _ in 0..num_vars {
            let index = u32_read_v2(reader.by_ref())?;
            let len = u8_read(reader.by_ref())? as usize;
            let mut name = vec![0_u8; len];
            reader.read_exact(&mut name[..])?;
            let name = CString::new(name).map_err(|_| Error::other("invalid variable name"))?;
            vars.insert(name, index);
        }
        eprintln!("vars {:?}", vars);
        Ok(Self { vars })
    }

    fn write<W: Write>(&self, mut writer: W) -> Result<(), Error> {
        let num_vars = self.vars.len() as u32;
        u32_write(writer.by_ref(), num_vars)?;
        for (name, index) in self.vars.iter() {
            let name = name.to_bytes();
            let len = name.len();
            if len > u8::MAX as usize {
                return Err(Error::other("variable name is too long"));
            }
            writer.write_all(&[len as u8])?;
            writer.write_all(name)?;
            u32_write(writer.by_ref(), *index)?;
        }
        Ok(())
    }
}

impl Deref for Vars {
    type Target = HashMap<CString, u32>;
    fn deref(&self) -> &Self::Target {
        &self.vars
    }
}

impl DerefMut for Vars {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.vars
    }
}

#[derive(Debug)]
struct Blocks {
    blocks: Vec<Block>,
    free_blocks: Vec<Block>,
}

impl Blocks {
    fn new() -> Self {
        Self {
            // start with the null block
            blocks: vec![Block::null()],
            free_blocks: Default::default(),
        }
    }

    fn slice<'a>(&self, index: u32, file: &'a [u8]) -> Result<&'a [u8], Error> {
        let block = self
            .blocks
            .get(index as usize)
            .ok_or_else(|| Error::other("invalid block index"))?;
        Ok(block.slice(file))
    }

    fn read<R: Read>(mut reader: R) -> Result<Self, Error> {
        let num_blocks = u32_read_v2(reader.by_ref())? as usize;
        let mut blocks = Vec::with_capacity(num_blocks);
        for _ in 0..num_blocks {
            let block = Block::read(reader.by_ref())?;
            blocks.push(block);
        }
        let num_free_blocks = u32_read_v2(reader.by_ref())? as usize;
        let mut free_blocks = Vec::with_capacity(num_free_blocks);
        for _ in 0..num_free_blocks {
            let block = Block::read(reader.by_ref())?;
            free_blocks.push(block);
        }
        Ok(Self {
            blocks,
            free_blocks,
        })
    }

    fn write<W: Write>(&self, mut writer: W) -> Result<(), Error> {
        let num_blocks = self.blocks.len() as u32;
        u32_write(writer.by_ref(), num_blocks + 1)?;
        for block in self.blocks.iter() {
            block.write(writer.by_ref())?;
        }
        let num_free_blocks = self.free_blocks.len() as u32;
        // write two empty blocks at the end
        u32_write(writer.by_ref(), num_free_blocks + 2)?;
        for block in self.free_blocks.iter() {
            block.write(writer.by_ref())?;
        }
        Block::null().write(writer.by_ref())?;
        Block::null().write(writer.by_ref())?;
        Ok(())
    }

    fn push(&mut self, block: Block) {
        self.blocks.push(block);
    }

    fn write_block<W: Write + Seek, F: FnOnce(&mut W) -> Result<(), Error>>(
        &mut self,
        writer: W,
        f: F,
    ) -> Result<u32, Error> {
        let index = self.blocks.len();
        self.blocks.push(Block::from_write(writer, f)?);
        Ok(index as u32)
    }
}

#[derive(Debug)]
struct Block {
    // Byte offset from the start of the file.
    offset: u32,
    // Size in bytes.
    len: u32,
}

impl Block {
    fn slice<'a>(&self, file: &'a [u8]) -> &'a [u8] {
        let i = self.offset as usize;
        let j = i + self.len as usize;
        &file[i..j]
    }

    fn read<R: Read>(mut reader: R) -> Result<Self, Error> {
        let offset = u32_read_v2(reader.by_ref())?;
        let len = u32_read_v2(reader.by_ref())?;
        Ok(Self { offset, len })
    }

    fn write<W: Write>(&self, mut writer: W) -> Result<(), Error> {
        u32_write(writer.by_ref(), self.offset)?;
        u32_write(writer.by_ref(), self.len)?;
        Ok(())
    }

    fn is_null(&self) -> bool {
        self.offset == 0 && self.len == 0
    }

    fn null() -> Self {
        Self { offset: 0, len: 0 }
    }

    fn from_write<W: Write + Seek, F: FnOnce(&mut W) -> Result<(), Error>>(
        mut writer: W,
        f: F,
    ) -> Result<Self, Error> {
        let offset = writer.stream_position()?;
        // TODO align to block size??
        {
            let remainder = (offset % ALIGN as u64) as usize;
            if remainder != 0 {
                let n = ALIGN - remainder;
                writer.write_all(&PADDING[..n])?;
            }
        }
        f(writer.by_ref())?;
        let len = writer.stream_position()? - offset;
        if offset > u32::MAX as u64 {
            return Err(Error::other("the file is too large"));
        }
        if len > u32::MAX as u64 {
            return Err(Error::other("the block is too large"));
        }
        Ok(Self {
            offset: offset as u32,
            len: len as u32,
        })
    }
}

#[derive(Debug)]
struct BomInfo {
    num_paths: u32,
    entries: Vec<BomInfoEntry>,
}

impl BomInfo {
    fn read(data: &[u8]) -> Result<Self, Error> {
        let version = u32_read(&data[0..4]);
        if version != VERSION {
            return Err(Error::other(format!(
                "unsupported BOMInfo version: {}",
                version
            )));
        }
        let num_paths = u32_read(&data[4..8]);
        let num_entries = u32_read(&data[8..12]);
        eprintln!("num paths {}", num_paths);
        eprintln!("num entries {}", num_entries);
        let mut entries = Vec::new();
        let mut data = &data[12..];
        for _ in 0..num_entries {
            entries.push(BomInfoEntry::read(&data[..16])?);
            data = &data[16..];
        }
        Ok(Self { num_paths, entries })
    }

    fn write<W: Write>(&self, mut writer: W) -> Result<(), Error> {
        u32_write(writer.by_ref(), VERSION)?;
        u32_write(writer.by_ref(), self.num_paths)?;
        u32_write(writer.by_ref(), self.entries.len() as u32)?;
        for entry in self.entries.iter() {
            entry.write(writer.by_ref())?;
        }
        Ok(())
    }
}

#[derive(Debug)]
struct BomInfoEntry {
    x: [u32; 4],
}

impl BomInfoEntry {
    fn read<R: Read>(mut reader: R) -> Result<Self, Error> {
        let mut data = [0_u8; 16];
        reader.read_exact(&mut data[..])?;
        Ok(BomInfoEntry {
            x: [
                u32_read(&data[0..4]),
                u32_read(&data[4..8]),
                u32_read(&data[8..12]),
                u32_read(&data[12..16]),
            ],
        })
    }

    fn write<W: Write>(&self, mut writer: W) -> Result<(), Error> {
        u32_write(writer.by_ref(), self.x[0])?;
        u32_write(writer.by_ref(), self.x[1])?;
        u32_write(writer.by_ref(), self.x[2])?;
        u32_write(writer.by_ref(), self.x[3])?;
        Ok(())
    }
}

#[derive(Debug)]
struct Node {
    id: u32,
    parent: u32,
    metadata: Metadata,
    name: OsString,
}

#[derive(Debug)]
struct VIndex {
    index: u32,
}

impl VIndex {
    fn new(index: u32) -> Self {
        Self { index }
    }

    fn read<R: Read>(mut reader: R) -> Result<Self, Error> {
        let version = u32_read_v2(reader.by_ref())?;
        if version != VERSION {
            return Err(Error::other(format!(
                "unsupported VIndex version: {}",
                version
            )));
        }
        let index = u32_read_v2(reader.by_ref())?;
        let _x0 = u32_read_v2(reader.by_ref())?;
        let _x1 = u8_read(reader.by_ref())?;
        Ok(Self { index })
    }

    fn write<W: Write>(&self, mut writer: W) -> Result<(), Error> {
        u32_write(writer.by_ref(), VERSION)?;
        u32_write(writer.by_ref(), self.index)?;
        u32_write(writer.by_ref(), 0_u32)?;
        u8_write(writer.by_ref(), 0_u8)?;
        Ok(())
    }
}

#[derive(Debug)]
struct Tree {
    child: u32,
    block_size: u32,
    num_paths: u32,
}

impl Tree {
    fn new_v_index(child: u32) -> Self {
        Self {
            child,
            block_size: 128,
            num_paths: 0,
        }
    }

    fn null(child: u32) -> Self {
        Self {
            child,
            block_size: 4096,
            num_paths: 0,
        }
    }

    fn read<R: Read>(mut reader: R) -> Result<Self, Error> {
        let mut magic = [0_u8; 4];
        reader.read_exact(&mut magic[..])?;
        if TREE_MAGIC[..] != magic[..] {
            return Err(Error::other("invalid tree magic"));
        }
        let version = u32_read_v2(reader.by_ref())?;
        if version != VERSION {
            return Err(Error::other(format!(
                "unsupported tree version: {}",
                version
            )));
        }
        let child = u32_read_v2(reader.by_ref())?;
        let block_size = u32_read_v2(reader.by_ref())?;
        let num_paths = u32_read_v2(reader.by_ref())?;
        let _x = u8_read(reader.by_ref())?;
        Ok(Self {
            child,
            block_size,
            num_paths,
        })
    }

    fn write<W: Write>(&self, mut writer: W) -> Result<(), Error> {
        writer.write_all(&TREE_MAGIC[..])?;
        u32_write(writer.by_ref(), VERSION)?;
        u32_write(writer.by_ref(), self.child)?;
        u32_write(writer.by_ref(), self.block_size)?;
        u32_write(writer.by_ref(), self.num_paths)?;
        u8_write(writer.by_ref(), 0_u8)?;
        Ok(())
    }
}

#[derive(Debug)]
struct Paths {
    forward: u32,
    backward: u32,
    indices: Vec<(u32, u32)>,
    is_leaf: bool,
}

impl Paths {
    fn null() -> Self {
        Self {
            forward: 0,
            backward: 0,
            indices: Default::default(),
            is_leaf: true,
        }
    }

    fn read<R: Read>(mut reader: R) -> Result<Self, Error> {
        let is_leaf = u16_read_v2(reader.by_ref())? != 0;
        let count = u16_read_v2(reader.by_ref())?;
        let forward = u32_read_v2(reader.by_ref())?;
        let backward = u32_read_v2(reader.by_ref())?;
        let mut indices = Vec::new();
        for _ in 0..count {
            let index0 = u32_read_v2(reader.by_ref())?;
            let index1 = u32_read_v2(reader.by_ref())?;
            indices.push((index0, index1));
        }
        Ok(Self {
            forward,
            backward,
            indices,
            is_leaf,
        })
    }

    fn write<W: Write>(&self, mut writer: W) -> Result<(), Error> {
        let is_leaf: u16 = if self.is_leaf { 1 } else { 0 };
        u16_write(writer.by_ref(), is_leaf)?;
        let count = self.indices.len();
        if count > u16::MAX as usize {
            return Err(Error::other("too many path indices"));
        }
        u16_write(writer.by_ref(), count as u16)?;
        u32_write(writer.by_ref(), self.forward)?;
        u32_write(writer.by_ref(), self.backward)?;
        for (index0, index1) in self.indices.iter() {
            u32_write(writer.by_ref(), *index0)?;
            u32_write(writer.by_ref(), *index1)?;
        }
        Ok(())
    }
}

#[derive(Debug)]
struct Nodes {
    nodes: HashMap<u32, Node>,
}

impl Nodes {
    fn path(&self, mut id: u32) -> Result<PathBuf, Error> {
        let mut visited = HashSet::new();
        let mut components = Vec::new();
        loop {
            if !visited.insert(id) {
                return Err(Error::other("loop"));
            }
            let Some(node) = self.nodes.get(&id) else {
                break;
            };
            components.push(node.name.as_os_str());
            id = node.parent;
        }
        let mut path = PathBuf::new();
        path.extend(components.into_iter().rev());
        Ok(path)
    }

    fn to_paths(&self) -> Result<HashMap<PathBuf, Metadata>, Error> {
        let mut paths = HashMap::new();
        for (id, node) in self.nodes.iter() {
            let mut path = self.path(*id)?;
            path.push(&node.name);
            paths.insert(path, node.metadata.clone());
        }
        Ok(paths)
    }

    fn from_directory<P: AsRef<Path>>(directory: P) -> Result<Self, Error> {
        let directory = directory.as_ref();
        let mut nodes: HashMap<PathBuf, Node> = HashMap::new();
        let mut id: u32 = 1;
        for entry in WalkDir::new(directory).into_iter() {
            let entry = entry?;
            let entry_path = entry
                .path()
                .strip_prefix(directory)
                .map_err(Error::other)?
                .normalize();
            if entry_path == Path::new("") {
                continue;
            }
            let relative_path = Path::new(".").join(entry_path);
            let dirname = relative_path.parent();
            let basename = relative_path.file_name();
            let metadata = std::fs::metadata(entry.path())?;
            let node = Node {
                id,
                parent: match dirname {
                    Some(d) => nodes.get(d).map(|node| node.id).unwrap_or(0),
                    None => 0,
                },
                name: match basename {
                    Some(s) => s.into(),
                    None => relative_path.clone().into(),
                },
                metadata: metadata.try_into()?,
            };
            nodes.insert(relative_path, node);
            id += 1;
        }
        let nodes = nodes.into_iter().map(|(_, node)| (node.id, node)).collect();
        Ok(Self { nodes })
    }
}

#[derive(Debug, Clone)]
struct Metadata {
    kind: NodeKind,
    mode: u16,
    uid: u32,
    gid: u32,
    mtime: u32,
    size: u32,
}

impl TryFrom<std::fs::Metadata> for Metadata {
    type Error = Error;
    fn try_from(other: std::fs::Metadata) -> Result<Self, Self::Error> {
        use std::os::unix::fs::MetadataExt;
        Ok(Self {
            kind: other.file_type().try_into()?,
            mode: (other.mode() & 0o7777) as u16,
            uid: other.uid(),
            gid: other.gid(),
            mtime: other.mtime().try_into().unwrap_or(0),
            size: other
                .size()
                .try_into()
                .map_err(|_| Error::other("files larger than 4 GiB are not supported"))?,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
#[repr(u8)]
enum NodeKind {
    #[default]
    File = 1,
    Directory = 2,
    Symlink = 3,
    Device = 4,
}

impl TryFrom<u8> for NodeKind {
    type Error = Error;
    fn try_from(other: u8) -> Result<Self, Self::Error> {
        use NodeKind::*;
        match other {
            1 => Ok(File),
            2 => Ok(Directory),
            3 => Ok(Symlink),
            4 => Ok(Device),
            _ => return Err(Error::other("invalid node kind")),
        }
    }
}

impl TryFrom<FileType> for NodeKind {
    type Error = Error;
    fn try_from(other: FileType) -> Result<Self, Self::Error> {
        use std::os::unix::fs::FileTypeExt;
        if other.is_dir() {
            Ok(Self::Directory)
        } else if other.is_symlink() {
            Ok(Self::Symlink)
        } else if other.is_block_device() || other.is_char_device() {
            Ok(Self::Device)
        } else if other.is_file() {
            Ok(Self::File)
        } else {
            Err(Error::other(format!("unsupported file type {:?}", other)))
        }
    }
}

fn u8_read<R: Read>(mut reader: R) -> Result<u8, Error> {
    let mut data = [0_u8; 1];
    reader.read_exact(&mut data[..])?;
    Ok(data[0])
}

fn u16_read(data: &[u8]) -> u16 {
    u16::from_be_bytes([data[0], data[1]])
}

fn u16_read_v2<R: Read>(mut reader: R) -> Result<u16, Error> {
    let mut data = [0_u8; 2];
    reader.read_exact(&mut data[..])?;
    Ok(u16::from_be_bytes([data[0], data[1]]))
}

fn u32_read(data: &[u8]) -> u32 {
    u32::from_be_bytes([data[0], data[1], data[2], data[3]])
}

fn u32_read_v2<R: Read>(mut reader: R) -> Result<u32, Error> {
    let mut data = [0_u8; 4];
    reader.read_exact(&mut data[..])?;
    Ok(u32::from_be_bytes([data[0], data[1], data[2], data[3]]))
}

fn u8_write<W: Write>(mut writer: W, value: u8) -> Result<(), Error> {
    writer.write_all(&[value])
}

fn u16_write<W: Write>(mut writer: W, value: u16) -> Result<(), Error> {
    writer.write_all(value.to_be_bytes().as_slice())
}

fn u32_write<W: Write>(mut writer: W, value: u32) -> Result<(), Error> {
    writer.write_all(value.to_be_bytes().as_slice())
}

const MAGIC: [u8; 8] = *b"BOMStore";
const TREE_MAGIC: [u8; 4] = *b"tree";
const V_INDEX: &CStr = c"VIndex";
const HL_INDEX: &CStr = c"HLIndex";
const SIZE_64: &CStr = c"Size64";
const BOM_INFO: &CStr = c"BomInfo";
// TODO why 512?
const HEADER_LEN: usize = 32;
const VERSION: u32 = 1;
const ALIGN: usize = 4;
const PADDING: [u8; ALIGN] = [0_u8; ALIGN];

#[cfg(test)]
mod tests {
    use std::fs::File;

    use super::*;

    #[test]
    fn bom_read() {
        Bom::read(File::open("macos/Bom").unwrap()).unwrap();
        //Header::read(File::open("macos/src.bom").unwrap()).unwrap();
    }
}

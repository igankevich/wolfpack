use std::collections::HashMap;
use std::collections::HashSet;
use std::collections::VecDeque;
use std::ffi::CStr;
use std::ffi::OsStr;
use std::ffi::OsString;
use std::fs::FileType;
use std::io::Error;
use std::io::Read;
use std::io::Seek;
use std::io::SeekFrom;
use std::io::Write;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;
use std::path::PathBuf;
use std::str::from_utf8;

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
        bom_info.write(writer.by_ref())?;
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
        let mut vars = read_variables(&file[vars_offset..(vars_offset + vars_len)])?;
        let blocks = Blocks::read(&file[index_offset..(index_offset + index_len)])?;
        {
            let index = vars
                .remove("BomInfo")
                .ok_or_else(|| Error::other("\"BomInfo\" is missing"))?;
            let bom_info = BomInfo::read(blocks.slice(index, &file)?)?;
            eprintln!("bom info {:?}", bom_info);
        }
        let mut trees = VecDeque::new();
        {
            let name = "VIndex";
            let index = vars
                .remove(name)
                .ok_or_else(|| Error::other(format!("{:?} is missing", name)))?;
            let v_index = VIndex::read(blocks.slice(index, &file)?)?;
            eprintln!("v index {:?}", v_index);
            trees.push_back(("VIndex.index".to_string(), v_index.index));
        }
        let mut paths = VecDeque::new();
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
            eprintln!("tree {:?}", tree);
            paths.push_back(tree.child);
        }
        // id -> data
        let mut nodes = HashMap::new();
        let mut visited = HashSet::new();
        while let Some(index) = paths.pop_front() {
            if !visited.insert(index) {
                eprintln!("loop {}", index);
                continue;
                //return Err(Error::other("loop"));
            }
            let path = Paths::read(blocks.slice(index, &file)?)?;
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

fn read_variables(data: &[u8]) -> Result<HashMap<String, u32>, Error> {
    let num_vars = u32_read(&data[0..4]) as usize;
    let mut vars = HashMap::with_capacity(num_vars);
    let mut data = &data[4..];
    for _ in 0..num_vars {
        let index = u32_read(&data[0..4]);
        let len = data[4] as usize;
        let name =
            from_utf8(&data[5..(5 + len)]).map_err(|_| Error::other("invalid variable name"))?;
        vars.insert(name.to_string(), index);
        data = &data[(5 + len)..];
    }
    eprintln!("vars {:?}", vars);
    Ok(vars)
}

#[derive(Debug)]
struct Blocks {
    blocks: Vec<Block>,
    free_blocks: Vec<Block>,
}

impl Blocks {
    fn slice<'a>(&self, index: u32, file: &'a [u8]) -> Result<&'a [u8], Error> {
        let block = self
            .blocks
            .get(index as usize)
            .ok_or_else(|| Error::other("invalid block index"))?;
        Ok(block.slice(file))
    }

    fn read(data: &[u8]) -> Result<Self, Error> {
        let num_blocks = u32_read(&data[0..4]) as usize;
        let mut blocks = Vec::with_capacity(num_blocks);
        let mut offset = 4;
        for _ in 0..num_blocks {
            let address = u32_read(&data[offset..(offset + 4)]);
            let len = u32_read(&data[(offset + 4)..(offset + 8)]);
            blocks.push(Block { address, len });
            offset += 8;
        }
        for b in blocks.iter() {
            eprintln!("block {:?}", b);
        }
        let free_blocks_bytes = &data[offset..];
        let num_free_blocks = u32_read(&free_blocks_bytes[0..4]) as usize;
        let mut free_blocks = Vec::with_capacity(num_free_blocks);
        let mut offset = 4;
        for _ in 0..num_free_blocks {
            let address = u32_read(&free_blocks_bytes[offset..(offset + 4)]);
            let len = u32_read(&free_blocks_bytes[(offset + 4)..(offset + 8)]);
            free_blocks.push(Block { address, len });
            offset += 8;
        }
        for b in free_blocks.iter() {
            eprintln!("free block {:?}", b);
        }
        Ok(Self {
            blocks,
            free_blocks,
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
    x0: u32,
    x1: u8,
}

impl VIndex {
    fn read(data: &[u8]) -> Result<Self, Error> {
        let version = u32_read(&data[0..4]);
        if version != 1 {
            return Err(Error::other(format!(
                "unsupported VIndex version: {}",
                version
            )));
        }
        let index = u32_read(&data[4..8]);
        let x0 = u32_read(&data[8..12]);
        let x1 = data[12];
        Ok(Self { index, x0, x1 })
    }
}

#[derive(Debug)]
struct Tree {
    child: u32,
    block_size: u32,
    num_paths: u32,
    x0: u8,
}

impl Tree {
    fn read(data: &[u8]) -> Result<Self, Error> {
        if TREE_MAGIC[..] != data[..TREE_MAGIC.len()] {
            return Err(Error::other("invalid tree magic"));
        }
        let version = u32_read(&data[4..8]);
        if version != 1 {
            return Err(Error::other(format!(
                "unsupported tree version: {}",
                version
            )));
        }
        let child = u32_read(&data[8..12]);
        let block_size = u32_read(&data[12..16]);
        let num_paths = u32_read(&data[16..20]);
        let x0 = data[20];
        Ok(Self {
            child,
            block_size,
            num_paths,
            x0,
        })
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
    fn read(data: &[u8]) -> Result<Self, Error> {
        let is_leaf = u16_read(&data[0..2]) != 0;
        let count = u16_read(&data[2..4]);
        let forward = u32_read(&data[4..8]);
        let backward = u32_read(&data[8..12]);
        let mut indices = Vec::new();
        let mut data = &data[12..];
        for _ in 0..count {
            let index0 = u32_read(&data[0..4]);
            let index1 = u32_read(&data[4..8]);
            indices.push((index0, index1));
            data = &data[8..];
        }
        eprintln!("remaining len {}", data.len());
        Ok(Self {
            forward,
            backward,
            indices,
            is_leaf,
        })
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

#[derive(Debug)]
struct Block {
    address: u32,
    len: u32,
}

impl Block {
    fn slice<'a>(&self, file: &'a [u8]) -> &'a [u8] {
        let i = self.address as usize;
        let j = i + self.len as usize;
        &file[i..j]
    }
}

fn u16_read(data: &[u8]) -> u16 {
    u16::from_be_bytes([data[0], data[1]])
}

fn u32_read(data: &[u8]) -> u32 {
    u32::from_be_bytes([data[0], data[1], data[2], data[3]])
}

fn u32_write<W: Write>(mut writer: W, value: u32) -> Result<(), Error> {
    writer.write_all(value.to_be_bytes().as_slice())
}

const MAGIC: [u8; 8] = *b"BOMStore";
const TREE_MAGIC: [u8; 4] = *b"tree";
// TODO why 512?
const HEADER_LEN: usize = 32;
const VERSION: u32 = 1;

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

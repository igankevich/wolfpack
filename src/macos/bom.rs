use std::collections::HashMap;
use std::collections::VecDeque;
use std::ffi::CStr;
use std::io::Error;
use std::io::Read;
use std::str::from_utf8;

struct Header {}

impl Header {
    fn read<R: Read>(mut reader: R) -> Result<Self, Error> {
        let mut file = Vec::new();
        reader.read_to_end(&mut file)?;
        if file.len() < HEADER_LEN || file[..MAGIC.len()] != MAGIC[..] {
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
        let index_offset = u32_read(&file[16..20]) as usize;
        let index_len = u32_read(&file[20..24]) as usize;
        let vars_offset = u32_read(&file[24..28]) as usize;
        let vars_len = u32_read(&file[28..32]) as usize;
        eprintln!("vars offset {} len {}", vars_offset, vars_len);
        eprintln!("index offset {} len {}", index_offset, index_len);
        eprintln!("num non null blocks {}", num_non_null_blocks);
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
        while let Some(index) = paths.pop_front() {
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
                    let node = Node {
                        id,
                        kind,
                        parent: 0,
                        name: String::new(),
                    };
                    Some(node)
                };
                eprintln!("path {} {}", index0, index1);
                {
                    let block_bytes = blocks.slice(index1, &file)?;
                    let parent = u32_read(&block_bytes[0..4]);
                    let name = CStr::from_bytes_with_nul(&block_bytes[4..])
                        .map_err(Error::other)?
                        .to_str()
                        .map_err(Error::other)?;
                    eprintln!("file parent {} name {}", parent, name,);
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
            // TODO backward ? infinite loop
            //if path.backward != 0 {
            //    paths.push_back(path.backward);
            //}
        }
        eprintln!("nodes {:?}", nodes);
        Ok(Self {})
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
    num_entries: u32,
    entries: Vec<BomInfoEntry>,
}

impl BomInfo {
    fn read(data: &[u8]) -> Result<Self, Error> {
        let version = u32_read(&data[0..4]);
        if version != 1 {
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
            entries.push(BomInfoEntry {
                x: [
                    u32_read(&data[0..4]),
                    u32_read(&data[4..8]),
                    u32_read(&data[8..12]),
                    u32_read(&data[12..16]),
                ],
            });
            data = &data[16..];
        }
        Ok(Self {
            num_paths,
            num_entries,
            entries,
        })
    }
}

#[derive(Debug)]
struct BomInfoEntry {
    x: [u32; 4],
}

#[derive(Debug)]
struct Node {
    id: u32,
    parent: u32,
    kind: NodeKind,
    name: String,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
enum NodeKind {
    File = 1,
    Directory = 2,
    Link = 3,
    Device = 4,
}

impl TryFrom<u8> for NodeKind {
    type Error = Error;
    fn try_from(other: u8) -> Result<Self, Self::Error> {
        use NodeKind::*;
        match other {
            1 => Ok(File),
            2 => Ok(Directory),
            3 => Ok(Link),
            4 => Ok(Device),
            _ => return Err(Error::other("invalid node kind")),
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

const MAGIC: [u8; 8] = *b"BOMStore";
const TREE_MAGIC: [u8; 4] = *b"tree";
const HEADER_LEN: usize = 512;

#[cfg(test)]
mod tests {
    use std::fs::File;

    use super::*;

    #[test]
    fn bom_read() {
        Header::read(File::open("macos/Bom").unwrap()).unwrap();
        //Header::read(File::open("macos/src.bom").unwrap()).unwrap();
    }
}

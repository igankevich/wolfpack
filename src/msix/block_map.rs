use std::ffi::OsString;
use std::io::Error;
use std::io::Write;

use quick_xml::se::to_writer;
use serde::ser::SerializeStruct;
use serde::Deserialize;
use serde::Serialize;
use serde::Serializer;

/// https://learn.microsoft.com/en-us/uwp/schemas/blockmapschema/app-package-block-map
#[derive(Deserialize, Debug)]
#[serde(rename = "BlockMap")]
pub struct BlockMap {
    #[serde(rename = "@HashMethod")]
    pub hash_method: String,
    #[serde(rename = "File", default)]
    pub files: Vec<File>,
}

impl BlockMap {
    pub fn write<W: Write>(&self, mut writer: W) -> Result<(), Error> {
        let mut s = String::new();
        to_writer(&mut s, self).map_err(Error::other)?;
        writer.write_all(r#"<?xml version="1.0" encoding="UTF-8" standalone="no"?>"#.as_bytes())?;
        writer.write_all(s.as_bytes())?;
        Ok(())
    }
}

impl Serialize for BlockMap {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_struct("BlockMap", 3)?;
        state.serialize_field("@xmlns", "http://schemas.microsoft.com/appx/2010/blockmap")?;
        state.serialize_field("@HashMethod", &self.hash_method)?;
        state.serialize_field("File", &self.files)?;
        state.end()
    }
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename = "File")]
pub struct File {
    #[serde(rename = "@Name")]
    pub name: OsString,
    #[serde(rename = "@Size")]
    pub size: u64,
    #[serde(rename = "@LfhSize")]
    pub lfh_size: u64,
    #[serde(rename = "Block", default)]
    pub blocks: Vec<Block>,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename = "Block")]
pub struct Block {
    #[serde(rename = "@Hash")]
    pub hash: String,
    #[serde(rename = "@Size")]
    pub size: u64,
}

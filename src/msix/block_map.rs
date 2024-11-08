use std::ffi::OsString;
use std::io::Error;
use std::io::Write;

use quick_xml::se::to_writer;
use serde::Deserialize;
use serde::Serialize;

pub mod xml {
    use super::*;

    /// https://learn.microsoft.com/en-us/uwp/schemas/blockmapschema/app-package-block-map
    #[derive(Serialize, Deserialize, Debug)]
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
            writer.write_all(s.as_bytes())?;
            Ok(())
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
}

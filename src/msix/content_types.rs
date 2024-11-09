use std::io::Error;
use std::io::Write;

use quick_xml::se::to_writer;
use serde::ser::SerializeStruct;
use serde::Deserialize;
use serde::Serialize;
use serde::Serializer;

/// https://learn.microsoft.com/en-us/uwp/schemas/blockmapschema/app-package-block-map
#[derive(Deserialize, Debug)]
#[serde(rename = "Types")]
pub struct Types {
    #[serde(rename = "Override", default)]
    pub overrides: Vec<Override>,
    #[serde(rename = "Default", default)]
    pub defaults: Vec<DefaultType>,
}

impl Types {
    pub fn write<W: Write>(&self, mut writer: W) -> Result<(), Error> {
        let mut s = String::new();
        to_writer(&mut s, self).map_err(Error::other)?;
        writer.write_all(r#"<?xml version="1.0" encoding="UTF-8" standalone="no"?>"#.as_bytes())?;
        writer.write_all(s.as_bytes())?;
        Ok(())
    }
}

impl Serialize for Types {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_struct("Types", 3)?;
        state.serialize_field(
            "@xmlns",
            "http://schemas.openxmlformats.org/package/2006/content-types",
        )?;
        state.serialize_field("Override", &self.overrides)?;
        state.serialize_field("Default", &self.defaults)?;
        state.end()
    }
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename = "Override")]
pub struct Override {
    #[serde(rename = "@ContentType")]
    pub content_type: String,
    #[serde(rename = "@Partname")]
    pub part_name: String,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename = "Default")]
pub struct DefaultType {
    #[serde(rename = "@ContentType")]
    pub content_type: String,
    #[serde(rename = "@Extension")]
    pub extension: String,
}

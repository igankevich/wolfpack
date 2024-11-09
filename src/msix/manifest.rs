use std::io::Error;
use std::io::Write;

use quick_xml::se::to_writer;
use serde::ser::SerializeStruct;
use serde::Deserialize;
use serde::Serialize;
use serde::Serializer;

/// https://learn.microsoft.com/en-us/uwp/schemas/blockmapschema/app-package-block-map
#[derive(Deserialize, Debug)]
#[serde(rename = "Package")]
pub struct Package {
    #[serde(rename = "Identity")]
    pub identity: Identity,
    #[serde(rename = "Properties")]
    pub properties: Properties,
    #[serde(rename = "Resources")]
    pub resources: Resources,
    #[serde(rename = "Dependencies")]
    pub dependencies: Dependencies,
    #[serde(rename = "Applications")]
    pub applications: Applications,
}

impl Package {
    pub fn write<W: Write>(&self, mut writer: W) -> Result<(), Error> {
        let mut s = String::new();
        to_writer(&mut s, self).map_err(Error::other)?;
        writer.write_all(r#"<?xml version="1.0" encoding="UTF-8" standalone="no"?>"#.as_bytes())?;
        writer.write_all(s.as_bytes())?;
        Ok(())
    }
}

impl Serialize for Package {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_struct("Package", 3)?;
        state.serialize_field(
            "@xmlns",
            "http://schemas.microsoft.com/appx/manifest/foundation/windows10",
        )?;
        state.serialize_field(
            "@xmlns:mp",
            "http://schemas.microsoft.com/appx/2014/phone/manifest",
        )?;
        state.serialize_field(
            "@xmlns:uap",
            "http://schemas.microsoft.com/appx/manifest/uap/windows10",
        )?;
        state.serialize_field(
            "@xmlns:uap3",
            "http://schemas.microsoft.com/appx/manifest/uap/windows10/3",
        )?;
        state.serialize_field("@IgnorableNamespaces", "mp uap uap3")?;
        state.end()
    }
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename = "Identity")]
pub struct Identity {
    #[serde(rename = "@Name")]
    pub name: String,
    #[serde(rename = "@Publisher")]
    pub publisher: String,
    #[serde(rename = "@Version")]
    pub version: String,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename = "Properties")]
pub struct Properties {
    #[serde(rename = "DisplayName")]
    pub display_name: String,
    #[serde(rename = "PublisherDisplayName")]
    pub publisher_display_name: String,
    #[serde(rename = "Decsription")]
    pub description: String,
    #[serde(rename = "Logo")]
    pub logo: String,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename = "Resources")]
pub struct Resources {
    #[serde(rename = "Resource")]
    pub resources: Vec<Resource>,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename = "Resource")]
pub struct Resource {
    #[serde(rename = "Language")]
    pub language: String,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename = "Dependencies")]
pub struct Dependencies {
    #[serde(rename = "TargetDeviceFamily")]
    pub target_device_families: Vec<TargetDeviceFamily>,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename = "TargetDeviceFamily")]
pub struct TargetDeviceFamily {
    #[serde(rename = "@Name")]
    pub name: String,
    #[serde(rename = "@MinVersion")]
    pub min_version: String,
    #[serde(rename = "@MaxVersionTested")]
    pub max_version_tested: String,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename = "Applications")]
pub struct Applications {
    #[serde(rename = "Application")]
    pub applications: Vec<Application>,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename = "Application")]
pub struct Application {
    #[serde(rename = "@Id")]
    pub id: String,
    #[serde(rename = "@Executable")]
    pub executable: String,
    #[serde(rename = "uap:VisualElements")]
    pub visual_elements: VisualElements,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename = "uap::VisualElements")]
pub struct VisualElements {
    #[serde(rename = "@DisplayName")]
    pub display_name: String,
    #[serde(rename = "@Decsription")]
    pub description: String,
    #[serde(rename = "@BackgroundColor")]
    pub background_color: String,
    #[serde(rename = "@Square150x150Logo")]
    pub square150x150_logo: String,
    #[serde(rename = "@Square44x44Logo")]
    pub square44x44_logo: String,
    #[serde(rename = "@AppListEntry")]
    pub app_list_entry: String,
}

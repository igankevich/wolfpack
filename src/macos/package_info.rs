use std::io::Error;
use std::io::Write;
use std::path::PathBuf;

use quick_xml::se::to_writer;
use serde::Deserialize;
use serde::Serialize;

// http://s.sudre.free.fr/Stuff/Ivanhoe/FLAT.html
pub mod xml {
    use super::*;

    #[derive(Serialize, Deserialize, Debug)]
    #[serde(rename = "pkg-info")]
    pub struct PackageInfo {
        #[serde(rename = "@format-version")]
        pub format_version: u64,
        #[serde(rename = "@install-location")]
        pub install_location: Option<PathBuf>,
        #[serde(rename = "@identifier")]
        pub identifier: String,
        #[serde(rename = "@version")]
        pub version: String,
        #[serde(rename = "@generator_version")]
        pub generator_version: Option<String>,
        #[serde(rename = "@auth")]
        pub auth: Auth,
        #[serde(rename = "@relocatable")]
        pub relocatable: Option<bool>,
        pub payload: Payload,
        #[serde(rename = "bundle", default)]
        pub bundles: Vec<Bundle>,
        #[serde(rename = "bundle-version", default)]
        pub bundle_version: BundleVersion,
        #[serde(rename = "upgrade-bundle", default)]
        pub upgrade_bundle: UpgradeBundle,
        #[serde(rename = "update-bundle", default)]
        pub update_bundle: UpdateBundle,
        #[serde(rename = "atomic-update-bundle", default)]
        pub atomic_update_bundle: AtomicUpdateBundle,
        #[serde(rename = "strict-identifier", default)]
        pub strict_identifier: StrictIdentifier,
        #[serde(rename = "relocate", default)]
        pub relocate: Relocate,
        #[serde(rename = "scripts", default)]
        pub scripts: Scripts,
    }

    impl PackageInfo {
        pub fn write<W: Write>(&self, mut writer: W) -> Result<(), Error> {
            let mut s = String::new();
            to_writer(&mut s, self).map_err(Error::other)?;
            writer.write_all(s.as_bytes())?;
            Ok(())
        }
    }

    #[derive(Serialize, Deserialize, Debug)]
    #[serde(rename = "payload")]
    pub struct Payload {
        #[serde(rename = "@numberOfFiles")]
        pub number_of_files: u64,
        #[serde(rename = "@installKBytes")]
        pub install_kb: u64,
    }

    #[derive(Serialize, Deserialize, Debug)]
    #[serde(rename = "bundle", rename_all = "camelCase")]
    pub struct Bundle {
        #[serde(rename = "@path")]
        pub path: PathBuf,
        #[serde(rename = "@id")]
        pub id: String,
        #[serde(rename = "@CFBundleIdentifier")]
        pub identifier: String,
        #[serde(rename = "@CFBundleShortVersionString")]
        pub short_version_string: String,
        #[serde(rename = "@CFBundleVersion")]
        pub version: String,
    }

    #[derive(Serialize, Deserialize, Debug, Default)]
    #[serde(rename = "bundle-version")]
    pub struct BundleVersion {
        #[serde(rename = "bundle")]
        pub bundles: Vec<BundleRef>,
    }

    #[derive(Serialize, Deserialize, Debug, Default)]
    #[serde(rename = "upgrade-bundle")]
    pub struct UpgradeBundle {
        #[serde(rename = "bundle")]
        pub bundles: Vec<BundleRef>,
    }

    #[derive(Serialize, Deserialize, Debug, Default)]
    #[serde(rename = "update-bundle")]
    pub struct UpdateBundle {
        #[serde(rename = "bundle")]
        pub bundles: Vec<BundleRef>,
    }

    #[derive(Serialize, Deserialize, Debug, Default)]
    #[serde(rename = "atomic-update-bundle")]
    pub struct AtomicUpdateBundle {
        #[serde(rename = "bundle")]
        pub bundles: Vec<BundleRef>,
    }

    #[derive(Serialize, Deserialize, Debug, Default)]
    #[serde(rename = "strict-identifier")]
    pub struct StrictIdentifier {
        #[serde(rename = "bundle")]
        pub bundles: Vec<BundleRef>,
    }

    #[derive(Serialize, Deserialize, Debug, Default)]
    #[serde(rename = "relocate")]
    pub struct Relocate {
        #[serde(rename = "bundle")]
        pub bundles: Vec<BundleRef>,
    }

    #[derive(Serialize, Deserialize, Debug, Default)]
    #[serde(rename = "scripts")]
    pub struct Scripts {
        #[serde(rename = "preinstall")]
        pub pre_install: Vec<PreInstall>,
        #[serde(rename = "postinstall")]
        pub post_install: Vec<PostInstall>,
    }

    #[derive(Serialize, Deserialize, Debug, Default)]
    #[serde(rename = "scripts")]
    pub struct PreInstall {
        #[serde(rename = "@file")]
        pub file: PathBuf,
    }

    #[derive(Serialize, Deserialize, Debug, Default)]
    #[serde(rename = "scripts")]
    pub struct PostInstall {
        #[serde(rename = "@file")]
        pub file: PathBuf,
    }

    #[derive(Serialize, Deserialize, Debug)]
    #[serde(rename = "bundle")]
    pub struct BundleRef {
        #[serde(rename = "@id")]
        pub id: String,
    }

    #[derive(Serialize, Deserialize, Debug)]
    #[serde(rename = "auth", rename_all = "kebab-case")]
    pub enum Auth {
        // TODO ignore case
        None,
        Root,
    }

    // https://developer.apple.com/library/archive/documentation/DeveloperTools/Reference/DistributionDefinitionRef/Chapters/Introduction.html
    #[derive(Serialize, Deserialize, Debug)]
    #[serde(rename = "installer-gui-script")]
    pub struct Distribution {
        #[serde(rename = "minSpecVersion")]
        pub min_spec_version: u64,
        #[serde(rename = "title")]
        pub title: String,
        #[serde(rename = "domains")]
        pub domains: Domains,
        #[serde(rename = "options")]
        pub options: Options,
        #[serde(rename = "background")]
        pub background: Option<Background>,
        #[serde(rename = "license")]
        pub license: Option<License>,
        #[serde(rename = "welcome")]
        pub welcome: Option<Welcome>,
        #[serde(rename = "conclusion")]
        pub conclusion: Option<Conclusion>,
        #[serde(rename = "choices-outline")]
        pub choices_outline: ChoicesOutline,
        #[serde(rename = "choice")]
        pub choices: Vec<Choice>,
    }

    #[derive(Serialize, Deserialize, Debug)]
    #[serde(rename = "domains")]
    pub struct Domains {
        #[serde(rename = "@enable_anywhere")]
        pub enable_anywhere: bool,
        #[serde(rename = "@enable_currentUserHome")]
        pub enable_current_user_home: bool,
        #[serde(rename = "@enable_localSystem")]
        pub enable_local_system: bool,
    }

    #[derive(Serialize, Deserialize, Debug)]
    #[serde(rename = "options")]
    pub struct Options {
        #[serde(rename = "@customize")]
        pub customize: String,
        #[serde(rename = "@hostArchitectures")]
        pub host_architectures: String,
    }

    #[derive(Serialize, Deserialize, Debug)]
    #[serde(rename = "background")]
    pub struct Background {
        #[serde(rename = "@file")]
        pub file: PathBuf,
        #[serde(rename = "@scaling")]
        pub scaling: String,
        #[serde(rename = "@alignment")]
        pub alignment: String,
    }

    #[derive(Serialize, Deserialize, Debug)]
    #[serde(rename = "license")]
    pub struct License {
        #[serde(rename = "@file")]
        pub file: PathBuf,
        #[serde(rename = "@mime-type")]
        pub mime_type: String,
    }

    #[derive(Serialize, Deserialize, Debug)]
    #[serde(rename = "welcome")]
    pub struct Welcome {
        #[serde(rename = "@file")]
        pub file: PathBuf,
        #[serde(rename = "@mime-type")]
        pub mime_type: String,
    }

    #[derive(Serialize, Deserialize, Debug)]
    #[serde(rename = "conclusion")]
    pub struct Conclusion {
        #[serde(rename = "@file")]
        pub file: PathBuf,
        #[serde(rename = "@mime-type")]
        pub mime_type: String,
    }

    #[derive(Serialize, Deserialize, Debug)]
    #[serde(rename = "choices-outline")]
    pub struct ChoicesOutline {
        #[serde(rename = "line")]
        pub lines: Vec<Line>,
    }

    #[derive(Serialize, Deserialize, Debug)]
    #[serde(rename = "line")]
    pub struct Line {
        #[serde(rename = "@choice")]
        pub line: String,
    }

    #[derive(Serialize, Deserialize, Debug)]
    #[serde(rename = "choice")]
    pub struct Choice {
        #[serde(rename = "@id")]
        pub id: String,
        #[serde(rename = "@title")]
        pub title: String,
        #[serde(rename = "@description")]
        pub description: String,
        #[serde(rename = "@start_selected")]
        pub start_selected: bool,
        #[serde(rename = "@start_enabled")]
        pub start_enabled: bool,
        #[serde(rename = "@start_visible")]
        pub start_visible: bool,
        #[serde(rename = "pkg-ref")]
        pub pkg_ref: PkgRef,
    }

    #[derive(Serialize, Deserialize, Debug)]
    #[serde(rename = "pkg-ref")]
    pub struct PkgRef {
        #[serde(rename = "@id")]
        pub id: String,
        #[serde(rename = "@auth")]
        pub auth: Auth,
        #[serde(rename = "@version")]
        pub version: String,
        #[serde(rename = "@installKBytes")]
        pub install_kb: u64,
        #[serde(rename = "$value")]
        pub value: String,
    }
}

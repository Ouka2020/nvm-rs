use crate::PROJECT_DIR;

use super::Result;
use anyhow::bail;
use derive_more::Deref;
use num_enum::FromPrimitive;
use serde::Deserializer;
use serde::{Deserialize, Serialize};
use strum::Display;

use clap::{Parser, Subcommand};
use std::path::Path;
use std::path::PathBuf;
use url::Url;
use windows_sys::Win32::System::SystemInformation::{
  IMAGE_FILE_MACHINE, IMAGE_FILE_MACHINE_AMD64, IMAGE_FILE_MACHINE_ARM64,
};
use windows_sys::Win32::System::Threading::{
  GetCurrentProcess, IsWow64Process2,
};

#[derive(Parser, Debug)]
#[command(version, about)]
pub struct CliArgs {
  #[command(subcommand)]
  pub command: Commands,
}

#[derive(Clone, Debug)]
pub enum VersionSpec {
  /// Latest LTS version
  Lts,
  /// Latest version
  Latest,
  /// Exact semver version number.(eg: v1.1.0)
  Exact(String),
}

// Clap automatically derives a value_parser for it using the FromStr trait.
impl std::str::FromStr for VersionSpec {
  type Err = String;

  fn from_str(s: &str) -> core::result::Result<Self, Self::Err> {
    match s.to_lowercase().as_str() {
      "lts" => Ok(Self::Lts),
      "latest" => Ok(Self::Latest),
      v => {
        // Check if the version is already prefixed with 'v'
        if v.starts_with('v') {
          Ok(Self::Exact(v.to_string()))
        } else {
          Ok(Self::Exact(format!("v{}", v)))
        }
      }
    }
  }
}

#[derive(Subcommand, Debug)]
pub enum Commands {
  /// Install the target node version.
  #[command(visible_alias = "i")]
  Install {
    /// The version can be a specific version, "latest" for the latest current version, or "lts" for the
    /// most recent LTS version. [possible values: <semver>(eg: 1.1.0), lts, latest]
    version: VersionSpec,
    /// Skip validation of the downloaded file.
    #[arg(short, long, default_value_t = false)]
    skip: bool,
  },
  /// The version must be a specific version.
  #[command(visible_alias = "un")]
  Uninstall {
    /// The version to uninstall.
    version: VersionSpec,
  },
  /// List the node.js installations.
  #[command(visible_alias = "ls")]
  List {
    /// Show online available versions.
    #[arg(short, long, default_value_t = false)]
    remote: bool,
  },
  /// Enable node.js version management.
  On,
  /// Disable node.js version management.
  Off,
  /// Set the directory where nvm should store different versions of node.js.
  /// If <path> is not set, the current root will be displayed.
  Root {
    /// The path to set as the root directory.
    path: Option<String>,
  },
  /// Display active version.
  Current,
  /// Set the node mirror. Defaults to https://nodejs.org/dist/. Leave [url] blank to use default url.
  NodeMirror {
    /// The node mirror to use. Leave [url] blank to use default url.
    url: Option<Url>,
  },
  /// Set the npm mirror. Defaults to https://github.com/npm/cli/archive/. Leave [url] blank to use default url.
  NpmMirror {
    /// The npm mirror to use. Leave [url] blank to use default url.
    url: Option<Url>,
  },
  /// Switch to use the specified version. Optionally use "latest", "lts", or "newest".
  /// "newest" is the latest installed version. Optionally specify 32/64bit architecture.
  /// nvm use <arch> will continue using the selected version, but switch to 32/64 bit mode.
  Use {
    /// The version to use.
    version: VersionSpec,
    // /// The architecture to use.
    // /// # deprecated
    // /// automatically by system arch.
    // #[deprecated(note = "automatically system arch")]
    // arch: Option<ArchSpec>,
  },
  /// Set up the application.
  Setup,
}

#[derive(Clone, Debug, Display, PartialEq, FromPrimitive)]
#[repr(u16)]
pub enum ArchSpec {
  #[num_enum(default)]
  Unknown = 0,
  /// 64 bit<br>
  // Since v0.6.13, supported on Windows.<br>
  /// it is suggested to use 64 bit version.
  #[strum(to_string = "x64")]
  X64 = IMAGE_FILE_MACHINE_AMD64,
  /// 64 bit ARM<br>
  // Since v19.9.0, supported on Windows.
  #[strum(to_string = "arm64")]
  Arm64 = IMAGE_FILE_MACHINE_ARM64,
}

impl ArchSpec {
  pub fn get_from_machine() -> Self {
    // Prefer IsWow64Process2 (available on Win10+): it reports the native
    // machine type even when this process runs under WOW64.
    // And faster than GetNativeSystemInfo.
    unsafe {
      let mut process_machine: IMAGE_FILE_MACHINE = 0;
      let mut native_machine: IMAGE_FILE_MACHINE = 0;

      IsWow64Process2(
        GetCurrentProcess(),
        &mut process_machine,
        &mut native_machine,
      );

      ArchSpec::from(native_machine)
    }
  }
}

#[derive(Debug, Clone)]
pub enum LtsSpec {
  Codename(String),
  NotLts,
}

impl<'de> Deserialize<'de> for LtsSpec {
  fn deserialize<D>(deserializer: D) -> core::result::Result<Self, D::Error>
  where
    D: Deserializer<'de>,
  {
    // Use untagged helper to accept both string and bool
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Raw {
      Str(String),
      Bool(bool),
    }

    match Raw::deserialize(deserializer)? {
      Raw::Str(s) => Ok(LtsSpec::Codename(s)),
      Raw::Bool(false) => Ok(LtsSpec::NotLts),
      Raw::Bool(true) => Err(serde::de::Error::custom(
        "expected a codename string or `false`, found `true`",
      )),
    }
  }
}

/// Node.js release info
#[derive(Debug, Clone, Deserialize)]
pub struct NodeReleaseInfo {
  /// semver, e.g. "v26.8.1"
  pub version: String,

  /// release date, format YYYY-MM-DD
  // #[serde(deserialize_with = "deserialize_jiff_date")]
  #[allow(dead_code)]
  // #[tabled(skip)]
  pub date: String,

  // /// Available build artifacts / platform list
  // #[tabled(skip)]
  // pub files: Vec<String>,

  // /// Bundled npm version
  // #[tabled(skip)]
  // pub npm: Option<String>,

  // /// V8 engine version
  // #[tabled(skip)]
  // pub v8: String,

  // /// libuv version
  // #[tabled(skip)]
  // pub uv: Option<String>,

  // /// zlib version
  // #[tabled(skip)]
  // pub zlib: Option<String>,

  // /// OpenSSL version
  // #[tabled(skip)]
  // pub openssl: Option<String>,

  // /// Node-API (ABI) module version
  // #[tabled(skip)]
  // #[serde(rename = "modules")]
  // pub abi_version: Option<String>,

  // /// Whether this is an LTS (Long Term Support) version
  // #[tabled(skip)]
  // #[serde(deserialize_with = "deserialize_lts")]
  pub lts: LtsSpec,
  // /// Whether this is a security release
  // #[tabled(skip)]
  // pub security: bool,
}

#[derive(Debug, Clone, Deserialize, Deref)]
#[serde(transparent)]
pub struct ReleaseDatabase {
  #[deref]
  inner: Vec<NodeReleaseInfo>,
}

impl ReleaseDatabase {
  pub fn load_from_url(agent: &ureq::Agent, target_url: Url) -> Result<Self> {
    // let target_url: Url = target_url.parse()?;
    log::debug!("target url: {}", target_url);

    let json_data: ReleaseDatabase = agent
      .get(target_url.as_str())
      .call()?
      .body_mut()
      .read_json()?;

    log::debug!("node_release_info count: {}", json_data.len());

    Ok(json_data)
  }

  /// query latest version
  pub fn latest(&self) -> Option<String> {
    self.inner.first().map(|f| f.version.clone())
  }

  /// query latest LTS version
  pub fn latest_lts(&self) -> Option<String> {
    self
      .inner
      .iter()
      .find(|r| matches!(r.lts, LtsSpec::Codename(_)))
      .map(|r| r.version.clone())
  }

  /// query version exists
  pub fn version_exists(&self, version: &str) -> bool {
    log::debug!("version_exists: {:?}", version);

    let version = if version.starts_with('v') {
      version.to_string()
    } else {
      format!("v{}", version)
    };

    self.inner.iter().any(|f| f.version == version)
  }

  /// query all releases under a major version
  pub fn by_major(&self, major: u64, len: usize) -> Vec<String> {
    let major = format!("v{}", major);

    let list: Vec<_> = self
      .inner
      .iter()
      .filter(|f| f.version.starts_with(&major))
      .map(|f| f.version.clone())
      .collect();

    fill_len(list, len)
  }

  /// query latest N versions
  pub fn latest_list(&self, count: usize) -> Vec<String> {
    let list: Vec<_> = self
      .inner
      .iter()
      .filter(|r| matches!(r.lts, LtsSpec::NotLts))
      .take(count)
      .map(|r| r.version.clone())
      .collect();

    fill_len(list, count)
  }

  /// query latest N LTS versions
  pub fn lts_list(&self, count: usize) -> Vec<String> {
    let list: Vec<_> = self
      .inner
      .iter()
      .filter(|r| matches!(r.lts, LtsSpec::Codename(_)))
      .take(count)
      .map(|r| r.version.clone())
      .collect();

    fill_len(list, count)
  }
}

fn fill_len<T>(mut list: Vec<T>, len: usize) -> Vec<T>
where
  T: Default,
{
  if list.len() < len {
    for _ in 0..(len - list.len()) {
      list.push(T::default());
    }
  }

  list
}

#[cfg(any(feature = "toml", feature = "yaml"))]
const CONFIG_FILE_NAME: &str = "settings";

#[derive(Debug, Default, Clone, Deserialize, Serialize)]
pub enum ConfigFileType {
  #[default]
  Toml,
  Yaml,
}

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct Config {
  /// Node.js storage root
  pub root: Option<PathBuf>,
  /// Node.js mirror
  #[serde(deserialize_with = "deserialize_mirror")]
  pub node_mirror: Option<Url>,
  /// npm mirror
  #[serde(deserialize_with = "deserialize_mirror")]
  pub npm_mirror: Option<Url>,
  #[serde(skip)]
  ty: ConfigFileType,
}

fn deserialize_mirror<'de, D>(
  deserializer: D,
) -> core::result::Result<Option<Url>, D::Error>
where
  D: Deserializer<'de>,
{
  let s = String::deserialize(deserializer)?;
  if s.is_empty()
    || s.eq_ignore_ascii_case("null")
    || s.eq_ignore_ascii_case("none")
  {
    return Ok(None);
  }

  match Url::parse(&s) {
    Ok(url) => Ok(Some(url)),
    Err(e) => {
      log::warn!("failed to parse {}: {}", s, e);
      Ok(None)
    }
  }
}

impl Config {
  /// Load config from file. Returns None if no config file exists.<br>
  /// If both toml and yaml features are enabled, it will try to load from toml first.
  /// If toml is not enabled, it will try to load from yaml.
  /// If yaml is not enabled, it will return None.
  pub fn load() -> Result<Config> {
    log::debug!("load config");

    let current_dir = PROJECT_DIR.get().unwrap().preference_dir();

    #[cfg(feature = "toml")]
    {
      if let Some(config) = read_from_toml(current_dir) {
        return Ok(config);
      }
    }

    #[cfg(feature = "yaml")]
    {
      if let Some(config) = read_from_txt(current_dir) {
        return Ok(config);
      }
    }

    Ok(Config::default())
  }

  pub fn save(&self) -> Result {
    log::debug!("save config");

    let path = PROJECT_DIR
      .get()
      .unwrap()
      .preference_dir()
      .join(CONFIG_FILE_NAME);

    #[cfg(feature = "toml")]
    {
      let config_str = toml::to_string(self)?;
      std::fs::write(path.with_extension("toml"), config_str)?;
    }

    #[cfg(all(not(feature = "toml"), feature = "yaml"))]
    {
      let config_str = noyalib::to_string(self)?;
      std::fs::write(path.with_extension("txt"), config_str)?;
    }

    #[cfg(not(any(feature = "toml", feature = "yaml")))]
    compile_error!(
      "At least one of `toml` or `yaml` features must be enabled for `save()`"
    );

    Ok(())
  }

  pub fn is_valid(&self) -> Result {
    match &self.root {
      Some(root) => {
        if !root.is_dir() {
          bail!(
            "Config item `root` is not a valid directory.\nTips: Use `nvm setup` to initialize it."
          );
        }
      }
      None => {
        bail!(
          "Config item `root` is not set.\nTips: Use `nvm setup` to initialize it."
        );
      }
    }

    Ok(())
  }

  pub fn get_node_url(&self, paths: &[&str]) -> Url {
    let mut base_url = if let Some(mirror) = &self.node_mirror {
      mirror.clone()
    } else {
      Url::parse("https://nodejs.org/dist").unwrap()
    };

    {
      let mut path = base_url.path_segments_mut().unwrap();
      path.extend(paths);
    }

    base_url
  }

  pub fn get_npm_url(&self, paths: &[&str]) -> Url {
    let mut base_url = if let Some(mirror) = &self.npm_mirror {
      mirror.clone()
    } else {
      Url::parse("https://npmjs.org/dist").unwrap()
    };

    {
      let mut path = base_url.path_segments_mut().unwrap();
      path.extend(paths);
    }

    base_url
  }
}

#[cfg(feature = "toml")]
fn read_from_toml<T>(current_dir: T) -> Option<Config>
where
  T: AsRef<Path>,
{
  let path = current_dir.as_ref();
  let file_path = path.join(CONFIG_FILE_NAME).with_extension("toml");
  let Ok(data) = std::fs::read_to_string(&file_path) else {
    log::warn!("failed to read {}", file_path.display());
    return None;
  };

  let Ok(config) = toml::from_str::<Config>(&data) else {
    log::warn!("failed to parse {}:", file_path.display());
    return None;
  };

  Some(config)
}

#[cfg(feature = "yaml")]
fn read_from_txt<T>(current_dir: T) -> Option<Config>
where
  T: AsRef<Path>,
{
  let path = current_dir.as_ref();
  let file_path = path.join(CONFIG_FILE_NAME).with_extension("txt");
  let Ok(data) = std::fs::read_to_string(&file_path) else {
    log::warn!("failed to read {}", file_path.display());
    return None;
  };

  match noyalib::from_str::<Config>(&data) {
    Ok(mut config) => {
      config.ty = ConfigFileType::Yaml;
      Some(config)
    }
    Err(e) => {
      log::warn!("failed to parse {}: {}", file_path.display(), e);
      None
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use pretty_assertions::assert_eq;
  use rstest::{fixture, rstest};

  /// Build a ReleaseDatabase with 4 versions:
  /// v22.0.0(non-LTS) / v20.0.0(LTS Iron) / v18.0.0(LTS Hydrogen) / v21.0.0(non-LTS)
  #[fixture]
  fn sample_db() -> ReleaseDatabase {
    let json = r#"[
      {"version": "v22.0.0", "date": "2024-04-01", "lts": false},
      {"version": "v20.0.0", "date": "2023-04-01", "lts": "Iron"},
      {"version": "v18.0.0", "date": "2022-04-01", "lts": "Hydrogen"},
      {"version": "v21.0.0", "date": "2023-10-01", "lts": false}
    ]"#;
    serde_json::from_str(json).unwrap()
  }

  // ---------- VersionSpec ----------

  #[rstest]
  fn version_spec_parses_keywords() {
    assert!(matches!(
      "lts".parse::<VersionSpec>().unwrap(),
      VersionSpec::Lts
    ));
    assert!(matches!(
      "LTS".parse::<VersionSpec>().unwrap(),
      VersionSpec::Lts
    ));
    assert!(matches!(
      "latest".parse::<VersionSpec>().unwrap(),
      VersionSpec::Latest
    ));
  }

  #[rstest]
  fn version_spec_parses_exact_version() {
    match "22.5.1".parse::<VersionSpec>().unwrap() {
      VersionSpec::Exact(v) => assert_eq!(v, "v22.5.1"),
      other => panic!("expected Exact, got {other:?}"),
    }
  }

  // ---------- ArchSpec ----------

  #[rstest]
  fn arch_spec_display() {
    // assert_eq!(ArchSpec::X86.to_string(), "x86");
    assert_eq!(ArchSpec::X64.to_string(), "x64");
    assert_eq!(ArchSpec::Arm64.to_string(), "arm64");
  }

  // ---------- LtsSpec / NodeReleaseInfo ----------

  #[rstest]
  fn lts_spec_codename_deserializes() {
    let info: NodeReleaseInfo = serde_json::from_str(
      r#"{"version":"v20.0.0","date":"2023-04-01","lts":"Iron"}"#,
    )
    .unwrap();
    assert!(matches!(info.lts, LtsSpec::Codename(name) if name == "Iron"));
  }

  #[rstest]
  fn lts_spec_false_deserializes() {
    let info: NodeReleaseInfo = serde_json::from_str(
      r#"{"version":"v22.0.0","date":"2024-04-01","lts":false}"#,
    )
    .unwrap();
    assert!(matches!(info.lts, LtsSpec::NotLts));
  }

  // ---------- fill_len ----------

  #[rstest]
  fn fill_len_pads_short_list() {
    assert_eq!(fill_len(vec![1u8, 2], 4), vec![1, 2, 0, 0]);
  }

  #[rstest]
  fn fill_len_keeps_longer_list() {
    assert_eq!(fill_len(vec![1, 2, 3], 2), vec![1, 2, 3]);
  }

  // ---------- ReleaseDatabase ----------

  #[rstest]
  fn release_db_latest(sample_db: ReleaseDatabase) {
    assert_eq!(sample_db.latest().as_deref(), Some("v22.0.0"));
  }

  #[rstest]
  fn release_db_latest_lts(sample_db: ReleaseDatabase) {
    assert_eq!(sample_db.latest_lts().as_deref(), Some("v20.0.0"));
  }

  #[rstest]
  fn release_db_version_exists(sample_db: ReleaseDatabase) {
    assert!(sample_db.version_exists("v22.0.0"));
    // Should also match without the 'v' prefix
    assert!(sample_db.version_exists("18.0.0"));
    assert!(!sample_db.version_exists("v99.0.0"));
  }

  #[rstest]
  fn release_db_by_major(sample_db: ReleaseDatabase) {
    assert_eq!(sample_db.by_major(20, 1), vec!["v20.0.0".to_string()]);
    // Pad with empty strings when fewer than requested
    assert_eq!(
      sample_db.by_major(18, 3),
      vec!["v18.0.0".to_string(), String::new(), String::new()]
    );
    assert_eq!(
      sample_db.by_major(99, 2),
      vec![String::new(), String::new()]
    );
  }

  #[rstest]
  fn release_db_latest_list(sample_db: ReleaseDatabase) -> () {
    // Non-LTS versions first, pad with empty strings if fewer
    assert_eq!(
      sample_db.latest_list(3),
      vec!["v22.0.0".to_string(), "v21.0.0".to_string(), String::new()]
    );
  }

  #[rstest]
  fn release_db_lts_list(sample_db: ReleaseDatabase) -> () {
    assert_eq!(
      sample_db.lts_list(3),
      vec!["v20.0.0".to_string(), "v18.0.0".to_string(), String::new()]
    );
  }

  #[rstest]
  fn release_db_empty() {
    let db: ReleaseDatabase = serde_json::from_str("[]").unwrap();
    assert_eq!(db.len(), 0);
    assert_eq!(db.latest(), None);
    assert_eq!(db.latest_lts(), None);
    assert!(!db.version_exists("v1.0.0"));
    assert_eq!(db.by_major(1, 2), vec![String::new(), String::new()]);
    assert_eq!(db.latest_list(2), vec![String::new(), String::new()]);
    assert_eq!(db.lts_list(2), vec![String::new(), String::new()]);
  }

  // ---------- Config ----------

  // #[rstest]
  // fn config_default_is_valid() {
  //   assert!(Config::default().is_valid().is_ok());
  // }

  #[rstest]
  fn config_valid_with_existing_root() {
    let mut config = Config::default();
    config.root = Some(std::env::temp_dir());
    assert!(config.is_valid().is_ok());
  }

  #[rstest]
  fn config_invalid_with_missing_root() {
    let mut config = Config::default();
    config.root = Some(PathBuf::from("definitely_not_exists_xyz_123"));
    assert!(config.is_valid().is_err());
  }

  // #[rstest]
  // fn config_proxy_deserializes() {
  //   let cases = [
  //     (r#"{"proxy": ""}"#, None),
  //     (r#"{"proxy": "none"}"#, None),
  //     (r#"{"proxy": "NULL"}"#, None),
  //     (
  //       r#"{"proxy": "http://127.0.0.1:8080"}"#,
  //       Some("http://127.0.0.1:8080"),
  //     ),
  //   ];

  //   for (json, expected) in cases {
  //     let config: Config = serde_json::from_str(json).unwrap();
  //     assert_eq!(config.proxy.as_deref(), expected, "case: {json}");
  //   }
  // }
}

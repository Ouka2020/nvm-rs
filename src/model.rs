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
    #[arg(short, long)]
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
    #[arg(short, long)]
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

  /// Whether this is an LTS (Long Term Support) version
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
  // #[default]
  Toml,
  #[default]
  Yaml,
}

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Deserialize, Serialize)]
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
  pub ty: ConfigFileType,
  #[serde(skip)]
  pub dir: directories::ProjectDirs,
}

// ProjectDirs does not implement Default, so provide a manual Default that
// fills the dir field with the same function used by serde.
impl Default for Config {
  fn default() -> Self {
    let dir = directories::ProjectDirs::from("", "", "nvm")
      .expect("fail to load app root.");
    std::fs::create_dir_all(dir.preference_dir())
      .expect("fail to create preference dir.");
    std::fs::create_dir_all(dir.cache_dir())
      .expect("fail to create cache dir.");

    Self {
      root: None,
      node_mirror: None,
      npm_mirror: None,
      ty: ConfigFileType::default(),
      dir,
    }
  }
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
  pub fn load() -> Result<Self> {
    log::debug!("load config");

    let _self = Self::default();
    let current_dir = _self.dir.preference_dir();

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

    Ok(_self)
  }

  pub fn save(&self) -> Result {
    log::debug!("save config");

    let path = self.dir.preference_dir().join(CONFIG_FILE_NAME);

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

  #[rstest]
  fn config_valid_with_existing_root() {
    let config = Config {
      root: Some(std::env::temp_dir()),
      ..Default::default()
    };
    assert!(config.is_valid().is_ok());
  }

  #[rstest]
  fn config_invalid_with_missing_root() {
    let config = Config {
      root: Some(PathBuf::from("definitely_not_exists_xyz_123")),
      ..Default::default()
    };
    assert!(config.is_valid().is_err());
  }

  #[rstest]
  fn config_invalid_with_root_not_set() {
    let config = Config::default();
    assert_eq!(config.root, None);
    let result = config.is_valid();
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("not set"));
  }

  // ---------- VersionSpec additional tests ----------

  #[rstest]
  fn version_spec_parses_with_v_prefix() {
    match "v22.5.1".parse::<VersionSpec>().unwrap() {
      VersionSpec::Exact(v) => assert_eq!(v, "v22.5.1"),
      other => panic!("expected Exact, got {other:?}"),
    }
  }

  #[rstest]
  fn version_spec_parses_latest_case_insensitive() {
    assert!(matches!(
      "Latest".parse::<VersionSpec>().unwrap(),
      VersionSpec::Latest
    ));
    assert!(matches!(
      "LATEST".parse::<VersionSpec>().unwrap(),
      VersionSpec::Latest
    ));
  }

  // ---------- ArchSpec additional tests ----------

  #[rstest]
  fn arch_spec_unknown_from_zero() {
    let arch = ArchSpec::from(0u16);
    assert_eq!(arch, ArchSpec::Unknown);
  }

  #[rstest]
  fn arch_spec_from_valid_machine_type() {
    let arch = ArchSpec::from(IMAGE_FILE_MACHINE_AMD64);
    assert_eq!(arch, ArchSpec::X64);

    let arch = ArchSpec::from(IMAGE_FILE_MACHINE_ARM64);
    assert_eq!(arch, ArchSpec::Arm64);
  }

  // ---------- LtsSpec additional tests ----------

  #[rstest]
  fn lts_spec_true_is_error() {
    let result = serde_json::from_str::<NodeReleaseInfo>(
      r#"{"version":"v20.0.0","date":"2023-04-01","lts":true}"#,
    );
    assert!(result.is_err());
  }

  #[rstest]
  fn lts_spec_empty_codename() {
    let info: NodeReleaseInfo = serde_json::from_str(
      r#"{"version":"v20.0.0","date":"2023-04-01","lts":""}"#,
    )
    .unwrap();
    assert!(matches!(info.lts, LtsSpec::Codename(name) if name.is_empty()));
  }

  // ---------- fill_len additional tests ----------

  #[rstest]
  fn fill_len_exact_length() {
    assert_eq!(fill_len(vec![1, 2, 3], 3), vec![1, 2, 3]);
  }

  #[rstest]
  fn fill_len_zero_length() {
    let empty: Vec<i32> = vec![];
    assert_eq!(fill_len(empty, 0), Vec::<i32>::new());
  }

  #[rstest]
  fn fill_len_empty_to_n() {
    let empty: Vec<u8> = vec![];
    assert_eq!(fill_len(empty, 3), vec![0, 0, 0]);
  }

  // ---------- Config::get_node_url ----------

  #[rstest]
  fn config_get_node_url_with_mirror() {
    let config = Config {
      node_mirror: Some(Url::parse("https://npmmirror.com/mirrors/node").unwrap()),
      ..Default::default()
    };

    let url = config.get_node_url(&["index.json"]);
    assert_eq!(
      url.as_str(),
      "https://npmmirror.com/mirrors/node/index.json"
    );
  }

  #[rstest]
  fn config_get_node_url_without_mirror() {
    let config = Config::default();
    assert_eq!(config.node_mirror, None);

    let url = config.get_node_url(&["index.json"]);
    assert_eq!(url.as_str(), "https://nodejs.org/dist/index.json");
  }

  #[rstest]
  fn config_get_node_url_multiple_paths() {
    let config = Config::default();
    let url = config.get_node_url(&["v18.0.0", "node-v18.0.0-win-x64.zip"]);
    assert_eq!(
      url.as_str(),
      "https://nodejs.org/dist/v18.0.0/node-v18.0.0-win-x64.zip"
    );
  }

  // ---------- Config::get_npm_url ----------

  #[rstest]
  fn config_get_npm_url_with_mirror() {
    let config = Config {
      npm_mirror: Some(Url::parse("https://npmmirror.com/mirrors/npm").unwrap()),
      ..Default::default()
    };

    let url = config.get_npm_url(&["v10.0.0", "npm-10.0.0.zip"]);
    assert_eq!(
      url.as_str(),
      "https://npmmirror.com/mirrors/npm/v10.0.0/npm-10.0.0.zip"
    );
  }

  #[rstest]
  fn config_get_npm_url_without_mirror() {
    let config = Config::default();
    assert_eq!(config.npm_mirror, None);

    let url = config.get_npm_url(&["v10.0.0"]);
    assert_eq!(url.as_str(), "https://npmjs.org/dist/v10.0.0");
  }

  // ---------- ConfigFileType ----------

  #[rstest]
  fn config_file_type_default_is_yaml() {
    let ty = ConfigFileType::default();
    assert!(matches!(ty, ConfigFileType::Yaml));
  }

  // ---------- deserialize_mirror ----------

  #[rstest]
  fn deserialize_mirror_empty_string() {
    #[derive(Deserialize)]
    struct TestConfig {
      #[serde(deserialize_with = "deserialize_mirror")]
      mirror: Option<Url>,
    }

    let json = r#"{"mirror": ""}"#;
    let config: TestConfig = serde_json::from_str(json).unwrap();
    assert_eq!(config.mirror, None);
  }

  #[rstest]
  fn deserialize_mirror_null_string() {
    #[derive(Deserialize)]
    struct TestConfig {
      #[serde(deserialize_with = "deserialize_mirror")]
      mirror: Option<Url>,
    }

    let json = r#"{"mirror": "null"}"#;
    let config: TestConfig = serde_json::from_str(json).unwrap();
    assert_eq!(config.mirror, None);
  }

  #[rstest]
  fn deserialize_mirror_none_string() {
    #[derive(Deserialize)]
    struct TestConfig {
      #[serde(deserialize_with = "deserialize_mirror")]
      mirror: Option<Url>,
    }

    let json = r#"{"mirror": "none"}"#;
    let config: TestConfig = serde_json::from_str(json).unwrap();
    assert_eq!(config.mirror, None);

    let json = r#"{"mirror": "NONE"}"#;
    let config: TestConfig = serde_json::from_str(json).unwrap();
    assert_eq!(config.mirror, None);
  }

  #[rstest]
  fn deserialize_mirror_valid_url() {
    #[derive(Deserialize)]
    struct TestConfig {
      #[serde(deserialize_with = "deserialize_mirror")]
      mirror: Option<Url>,
    }

    let json = r#"{"mirror": "https://example.com/mirror/"}"#;
    let config: TestConfig = serde_json::from_str(json).unwrap();
    assert!(config.mirror.is_some());
    assert_eq!(
      config.mirror.unwrap().as_str(),
      "https://example.com/mirror/"
    );
  }

  #[rstest]
  fn deserialize_mirror_invalid_url_returns_none() {
    #[derive(Deserialize)]
    struct TestConfig {
      #[serde(deserialize_with = "deserialize_mirror")]
      mirror: Option<Url>,
    }

    let json = r#"{"mirror": "not-a-valid-url"}"#;
    let config: TestConfig = serde_json::from_str(json).unwrap();
    assert_eq!(config.mirror, None);
  }

  // ---------- ReleaseDatabase additional tests ----------

  #[rstest]
  fn release_db_version_exists_without_v_prefix(sample_db: ReleaseDatabase) {
    assert!(sample_db.version_exists("22.0.0"));
    assert!(sample_db.version_exists("20.0.0"));
  }

  #[rstest]
  fn release_db_by_major_with_multiple_versions() {
    let json = r#"[
      {"version": "v20.3.0", "date": "2024-01-01", "lts": false},
      {"version": "v20.2.0", "date": "2023-12-01", "lts": false},
      {"version": "v20.1.0", "date": "2023-11-01", "lts": false},
      {"version": "v19.0.0", "date": "2023-10-01", "lts": false}
    ]"#;
    let db: ReleaseDatabase = serde_json::from_str(json).unwrap();

    let result = db.by_major(20, 3);
    assert_eq!(result.len(), 3);
    assert_eq!(result[0], "v20.3.0");
    assert_eq!(result[1], "v20.2.0");
    assert_eq!(result[2], "v20.1.0");
  }

  #[rstest]
  fn release_db_latest_list_filters_lts(sample_db: ReleaseDatabase) {
    let result = sample_db.latest_list(10);
    assert!(!result.contains(&"v20.0.0".to_string()));
    assert!(!result.contains(&"v18.0.0".to_string()));
    assert!(result.contains(&"v22.0.0".to_string()));
    assert!(result.contains(&"v21.0.0".to_string()));
  }

  #[rstest]
  fn release_db_lts_list_filters_non_lts(sample_db: ReleaseDatabase) {
    let result = sample_db.lts_list(10);
    assert!(!result.contains(&"v22.0.0".to_string()));
    assert!(!result.contains(&"v21.0.0".to_string()));
    assert!(result.contains(&"v20.0.0".to_string()));
    assert!(result.contains(&"v18.0.0".to_string()));
  }

  // ---------- Config serialization ----------

  #[rstest]
  fn config_serialization_roundtrip() {
    let config = Config {
      root: Some(PathBuf::from("/test/root")),
      node_mirror: Some(Url::parse("https://example.com/node/").unwrap()),
      npm_mirror: Some(Url::parse("https://example.com/npm/").unwrap()),
      ..Default::default()
    };

    let serialized = serde_json::to_string(&config).unwrap();
    let deserialized: Config = serde_json::from_str(&serialized).unwrap();

    assert_eq!(deserialized.root, config.root);
    assert_eq!(deserialized.node_mirror, config.node_mirror);
    assert_eq!(deserialized.npm_mirror, config.npm_mirror);
  }

  #[rstest]
  fn config_serialization_with_none_values() {
    let config = Config::default();
    assert_eq!(config.root, None);
    assert_eq!(config.node_mirror, None);
    assert_eq!(config.npm_mirror, None);

    let serialized = serde_json::to_string(&config).unwrap();
    assert!(!serialized.contains("root"));
    assert!(!serialized.contains("node_mirror"));
    assert!(!serialized.contains("npm_mirror"));
  }
}

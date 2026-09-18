use super::Result;
use anyhow::bail;
use derive_more::Deref;
use serde::{Deserialize, Deserializer, Serialize};
use strum::Display;

use clap::{Parser, Subcommand, ValueEnum};
use std::path::Path;
use std::path::PathBuf;

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
  /// Exact semver version number.(eg: 1.1.0)
  Exact(String),
}

// Clap 通过 FromStr 自动将其作为 value_parser
impl std::str::FromStr for VersionSpec {
  type Err = String;

  fn from_str(s: &str) -> core::result::Result<Self, Self::Err> {
    match s.to_lowercase().as_str() {
      "lts" => Ok(Self::Lts),
      "latest" => Ok(Self::Latest),
      v => Ok(Self::Exact(format!("v{}", v))),
    }
  }
}

#[derive(Subcommand, Debug)]
pub enum Commands {
  /// The version can be a specific version, "latest" for the latest current version, or "lts" for the
  /// most recent LTS version. Optionally specify whether to install the 32 or 64 bit version (defaults
  /// to system arch). Set [arch] to "all" to install 32 AND 64 bit versions.
  /// Add --insecure to the end of this command to bypass SSL validation of the remote download server.
  #[command(visible_alias = "i")]
  Install {
    /// The version can be a specific version, "latest" for the latest current version, or "lts" for the
    /// most recent LTS version. [possible values: <semver>(eg: 1.1.0), lts, latest]
    version: VersionSpec,
    /// Specify whether to install the 32 or 64 bit version (defaults to system arch).
    /// # deprecated
    /// automatically by system arch.
    #[deprecated(note = "automatically system arch")]
    arch: Option<ArchSpec>,
    /// Pass SSL validation of the remote download server.
    #[arg(short, long, default_value_t = false)]
    insecure: bool,
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
    available: bool,
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
  /// Show if node is running in 32 or 64 bit mode.
  /// # deprecated
  /// automatically by system arch.
  #[deprecated(note = "automatically system arch")]
  Arch,
  /// Set a proxy to use for downloads.
  /// # deprecated
  /// automatically by system detect.
  #[deprecated(note = "automatically system detect")]
  Proxy {
    /// Leave [url] blank to see the current proxy.
    /// Set [url] to "none" to remove the proxy.
    url: Option<String>,
  },
  /// Display active version.
  Current,
  /// Set the node mirror. Defaults to https://nodejs.org/dist/. Leave [url] blank to use default url.
  NodeMirror {
    /// The node mirror to use. Leave [url] blank to use default url.
    url: Option<String>,
  },
  /// Set the npm mirror. Defaults to https://github.com/npm/cli/archive/. Leave [url] blank to use default url.
  NpmMirror {
    /// The npm mirror to use. Leave [url] blank to use default url.
    url: Option<String>,
  },
  /// Switch to use the specified version. Optionally use "latest", "lts", or "newest".
  /// "newest" is the latest installed version. Optionally specify 32/64bit architecture.
  /// nvm use <arch> will continue using the selected version, but switch to 32/64 bit mode.
  Use {
    /// The version to use.
    version: VersionSpec,
    /// The architecture to use.
    /// # deprecated
    /// automatically by system arch.
    #[deprecated(note = "automatically system arch")]
    arch: Option<ArchSpec>,
  },
}

#[derive(Clone, Debug, ValueEnum, Display, Deserialize, PartialEq)]
// #[repr(u8)]
pub enum ArchSpec {
  /// 32 bit<br>
  /// Since v23.0.0, not supported on Windows.
  // #[value(name = "32")]
  // #[strum(to_string = "32")]
  X86,
  /// 64 bit<br>
  /// it is suggested to use 64 bit version.
  // #[value(name = "64")]
  // #[strum(to_string = "64")]
  X64,
  /// 64 bit ARM<br>
  /// Since v19.9.0, supported on Windows.
  // #[value(name = "arm64")]
  // #[strum(to_string = "arm64")]
  Arm64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
// 字段负载仅用于反序列化时区分变体，业务上只判断变体形状
#[allow(dead_code)]
pub enum LtsSpec {
  Codename(String),
  NotLts(bool), // 只接受 false；如果 JSON 里出现 true 也会匹配进来
}

/// Node.js 版本发布信息
#[derive(Debug, Clone, Deserialize)]
pub struct NodeReleaseInfo {
  /// 版本号，如 "v26.8.1"
  pub version: String,

  /// 发布日期，格式 YYYY-MM-DD
  // #[serde(deserialize_with = "deserialize_jiff_date")]
  #[allow(dead_code)]
  // #[tabled(skip)]
  pub date: String,

  // /// 可用的构建产物/平台列表
  // #[tabled(skip)]
  // pub files: Vec<String>,

  // /// 捆绑的 npm 版本
  // #[tabled(skip)]
  // pub npm: Option<String>,

  // /// V8 引擎版本
  // #[tabled(skip)]
  // pub v8: String,

  // /// libuv 版本
  // #[tabled(skip)]
  // pub uv: Option<String>,

  // /// zlib 版本
  // #[tabled(skip)]
  // pub zlib: Option<String>,

  // /// OpenSSL 版本
  // #[tabled(skip)]
  // pub openssl: Option<String>,

  // /// Node-API (ABI) 模块版本号
  // #[tabled(skip)]
  // #[serde(rename = "modules")]
  // pub abi_version: Option<String>,

  // /// 是否为 LTS（长期支持）版本
  // #[tabled(skip)]
  // #[serde(deserialize_with = "deserialize_lts")]
  pub lts: LtsSpec,
  // /// 是否为安全修复版本
  // #[tabled(skip)]
  // pub security: bool,
}

/// 核心查询缓存（启动时构建一次）
#[derive(Debug, Clone, Deserialize, Deref)]
#[serde(transparent)]
pub struct ReleaseDatabase {
  #[deref]
  inner: Vec<NodeReleaseInfo>,
}

impl ReleaseDatabase {
  /// ✅ 获取最新版本
  pub fn latest(&self) -> Option<String> {
    self.inner.first().map(|f| f.version.clone())
  }

  /// ✅ 获取最新 LTS 版本
  pub fn latest_lts(&self) -> Option<String> {
    self
      .inner
      .iter()
      .find(|r| matches!(r.lts, LtsSpec::Codename(_)))
      .map(|r| r.version.clone())
  }

  // /// ✅ 按条件组合查询（示例：最新 LTS + 指定平台）
  // pub fn latest_lts_with_platform(
  //   &self,
  //   platform: &str,
  // ) -> Option<&NodeReleaseInfo> {
  //   self
  //     .lts_indices
  //     .iter()
  //     .map(|&i| &self.sorted_releases[i])
  //     .find(|r| r.files.iter().any(|f| f == platform))
  // }

  /// 版本查询
  pub fn version_exists(&self, version: &str) -> bool {
    log::debug!("version_exists: {:?}", version);

    let version = if version.starts_with('v') {
      version.to_string()
    } else {
      format!("v{}", version)
    };

    self.inner.iter().any(|f| f.version == version)
  }

  /// 获取某主版本下所有发布
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

  /// 最新的 N 个版本
  pub fn latest_list(&self, count: usize) -> Vec<String> {
    let list: Vec<_> = self
      .inner
      .iter()
      .filter(|r| matches!(r.lts, LtsSpec::NotLts(false)))
      .take(count)
      .map(|r| r.version.clone())
      .collect();

    fill_len(list, count)
  }

  /// 最新 n 个 LTS 版本
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

#[serde_with::skip_serializing_none]
#[derive(Debug, Default, Clone, Deserialize, Serialize)]
pub struct Config {
  /// Node.js storage root
  pub root: Option<PathBuf>,
  /// Node.js proxy
  #[serde(deserialize_with = "deserialize_proxy")]
  #[deprecated(
    since = "0.1.3",
    note = "proxy is deprecated, can auto detect it"
  )]
  pub proxy: Option<String>,
  /// Node.js mirror
  pub node_mirror: Option<String>,
  /// npm mirror
  pub npm_mirror: Option<String>,
  // #[deprecated(
  //   since = "0.1.3",
  //   note = "arch is deprecated, can auto detect it"
  // )]
  // pub arch: Option<ArchSpec>,
  #[deprecated(
    since = "0.1.3",
    note = "originalpath is deprecated, maybe deleted in future"
  )]
  pub originalpath: Option<PathBuf>,
  #[deprecated(
    since = "0.1.3",
    note = "originalversion is deprecated, maybe deleted in future"
  )]
  pub originalversion: Option<String>,
  // pub symlink: Option<String>,
  #[serde(skip)]
  config_path: PathBuf,
}

fn deserialize_proxy<'de, D>(
  deserializer: D,
) -> core::result::Result<Option<String>, D::Error>
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

  Ok(Some(s))
}

impl Config {
  /// Load config from file. Returns None if no config file exists.<br>
  /// If both toml and yaml features are enabled, it will try to load from toml first.
  /// If toml is not enabled, it will try to load from yaml.
  /// If yaml is not enabled, it will return None.
  pub fn load() -> Result<Config> {
    log::debug!("load config");

    // // 从当前目录加载配置文件
    // let current_dir = std::env::current_dir()?;
    let Ok(current_dir) = get_exec_path() else {
      bail!("failed to get current exe path");
    };

    #[cfg(feature = "yaml")]
    {
      if let Some(config) = read_from_txt(&current_dir) {
        return Ok(config_path_patch(config, &current_dir));
      }
    }

    #[cfg(feature = "toml")]
    {
      if let Some(config) = read_from_toml(&current_dir) {
        return Ok(config_path_patch(config, &current_dir));
      }
    }

    Ok(config_path_patch(Config::default(), &current_dir))
  }

  pub fn save(&self) -> Result {
    log::debug!("save config");

    #[cfg(feature = "toml")]
    {
      let config_str = toml::to_string(self)?;
      let path_str = self
        .config_path
        .join(CONFIG_FILE_NAME)
        .with_extension("toml");
      std::fs::write(&path_str, config_str)?;
    }

    #[cfg(feature = "yaml")]
    {
      let config_str = noyalib::to_string(self)?;
      let path_str = self
        .config_path
        .join(CONFIG_FILE_NAME)
        .with_extension("txt");
      std::fs::write(&path_str, config_str)?;
    }

    Ok(())
  }

  pub fn is_valid(&self) -> Result {
    if let Some(root) = &self.root
      && !root.is_dir()
    {
      bail!("root is not a directory");
    }

    if let Some(originalpath) = &self.originalpath
      && !originalpath.is_dir()
    {
      bail!("originalpath is not a directory");
    }

    Ok(())
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
  let Ok(config) = noyalib::from_str::<Config>(&data) else {
    log::warn!("failed to parse {}:", file_path.display());
    return None;
  };

  Some(config)
}

fn config_path_patch(mut config: Config, path: &Path) -> Config {
  config.config_path = path.to_path_buf();
  config
}

fn get_exec_path() -> Result<PathBuf> {
  let Ok(exe_path) = std::env::current_exe() else {
    bail!("failed to get current exe path");
  };

  let Some(current_dir) = exe_path.parent() else {
    bail!("failed to get current exe path");
  };

  Ok(current_dir.to_path_buf())
}

#[cfg(test)]
mod tests {
  use super::*;
  use pretty_assertions::assert_eq;
  use rstest::{fixture, rstest};

  /// 构造一个包含 4 个版本的 ReleaseDatabase：
  /// v22.0.0(非LTS) / v20.0.0(LTS Iron) / v18.0.0(LTS Hydrogen) / v21.0.0(非LTS)
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
    assert_eq!(ArchSpec::X86.to_string(), "X86");
    assert_eq!(ArchSpec::X64.to_string(), "X64");
    assert_eq!(ArchSpec::Arm64.to_string(), "Arm64");
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
    assert!(matches!(info.lts, LtsSpec::NotLts(false)));
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
    // 不带 v 前缀也应能匹配
    assert!(sample_db.version_exists("18.0.0"));
    assert!(!sample_db.version_exists("v99.0.0"));
  }

  #[rstest]
  fn release_db_by_major(sample_db: ReleaseDatabase) {
    assert_eq!(sample_db.by_major(20, 1), vec!["v20.0.0".to_string()]);
    // 数量不足时用空字符串填充
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
    // 非 LTS 版本在前，不足部分填充空字符串
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
  fn config_default_is_valid() {
    assert!(Config::default().is_valid().is_ok());
  }

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

  #[rstest]
  fn config_proxy_deserializes() {
    let cases = [
      (r#"{"proxy": ""}"#, None),
      (r#"{"proxy": "none"}"#, None),
      (r#"{"proxy": "NULL"}"#, None),
      (
        r#"{"proxy": "http://127.0.0.1:8080"}"#,
        Some("http://127.0.0.1:8080"),
      ),
    ];

    for (json, expected) in cases {
      let config: Config = serde_json::from_str(json).unwrap();
      assert_eq!(config.proxy.as_deref(), expected, "case: {json}");
    }
  }
}

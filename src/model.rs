use super::Result;
use anyhow::bail;
use derive_more::Deref;
use serde::{Deserialize, Deserializer, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};
use strum::Display;

use clap::{Parser, Subcommand, ValueEnum};
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
  Arch,
  /// Set a proxy to use for downloads.
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
    arch: Option<ArchSpec>,
  },
}

#[derive(
  Clone, Debug, ValueEnum, Display, Deserialize_repr, Serialize_repr, PartialEq,
)]
#[repr(u8)]
pub enum ArchSpec {
  /// 32 bit
  #[value(name = "32")]
  #[strum(to_string = "32")]
  X86 = 32,
  /// 64 bit
  #[value(name = "64")]
  #[strum(to_string = "64")]
  X64 = 64,
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
  pub proxy: Option<String>,
  /// Node.js mirror
  pub node_mirror: Option<String>,
  /// npm mirror
  pub npm_mirror: Option<String>,
  pub arch: Option<ArchSpec>,
  pub originalpath: Option<PathBuf>,
  pub originalversion: Option<String>,
  // pub symlink: Option<String>,
  #[serde(skip)]
  inner: PathBuf,
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
  pub fn load() -> Config {
    log::debug!("load config");

    #[cfg(feature = "toml")]
    {
      if let Some(config) = read_from_toml() {
        return config;
      }
    }

    #[cfg(feature = "yaml")]
    {
      if let Some(config) = read_from_txt() {
        return config;
      }
    }

    Config::default()
  }

  pub fn save(&self) -> Result {
    log::debug!("save config");

    // toml 优先；仅在未启用 toml 时才使用 txt，与 load() 的优先级保持一致
    #[cfg(feature = "toml")]
    let result = write_to_toml(self);

    #[cfg(all(not(feature = "toml"), feature = "yaml"))]
    let result = write_to_txt(self);

    #[cfg(all(not(feature = "toml"), not(feature = "yaml")))]
    let result: Result = Ok(());

    result
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
fn read_from_toml() -> Option<Config> {
  use std::fs;

  let Ok(current_dir) = get_exec_path() else {
    log::warn!("failed to get current exe path");
    return None;
  };

  let path = current_dir.join(CONFIG_FILE_NAME);
  let Ok(data) = fs::read_to_string(&path) else {
    log::warn!("failed to read {}", path.display());
    return None;
  };

  let Ok(mut config) = toml::from_str::<Config>(&data) else {
    log::warn!("failed to parse {}", path.display());
    return None;
  };

  config.inner = current_dir.to_path_buf();

  Some(config)
}

#[cfg(feature = "yaml")]
fn read_from_txt() -> Option<Config> {
  use std::fs;

  let Ok(current_dir) = get_exec_path() else {
    log::warn!("failed to get current exe path");
    return None;
  };

  let path = current_dir.join(CONFIG_FILE_NAME).with_extension("txt");
  let Ok(data) = fs::read_to_string(&path) else {
    log::warn!("failed to read {}", path.display());
    return None;
  };
  let Ok(mut config) = noyalib::from_str::<Config>(&data) else {
    log::warn!("failed to parse {}:", path.display());
    return None;
  };

  config.inner = current_dir.to_path_buf();

  Some(config)
}

#[cfg(feature = "toml")]
fn write_to_toml(config: &Config) -> Result {
  use std::fs;

  let config_str = toml::to_string(config)?;
  let path_str = if cfg!(debug_assertions) {
    format!("{}.toml.toml", CONFIG_FILE_NAME)
  } else {
    format!("{}.toml", CONFIG_FILE_NAME)
  };
  fs::write(&path_str, config_str)?;
  Ok(())
}

#[cfg(feature = "yaml")]
fn write_to_txt(config: &Config) -> Result {
  use std::fs;

  let config_str = noyalib::to_string(config)?;
  let path_str = if cfg!(debug_assertions) {
    format!("{}.txt.txt", CONFIG_FILE_NAME)
  } else {
    format!("{}.txt", CONFIG_FILE_NAME)
  };
  fs::write(&path_str, config_str)?;
  Ok(())
}

fn get_exec_path() -> Result<PathBuf> {
  use std::env;

  let Ok(exe_path) = env::current_exe() else {
    bail!("failed to get current exe path");
  };

  let Some(current_dir) = exe_path.parent() else {
    bail!("failed to get current exe path");
  };

  Ok(current_dir.to_path_buf())
}

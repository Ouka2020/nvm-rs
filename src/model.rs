use super::Result;
use anyhow::bail;
use clap::ValueEnum;
use jiff::civil::Date;
use serde::{Deserialize, Deserializer, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};
use strum::Display;

use clap::{Parser, Subcommand};

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
  Exact(semver::Version),
}

// Clap 通过 FromStr 自动将其作为 value_parser
impl std::str::FromStr for VersionSpec {
  type Err = String;

  fn from_str(s: &str) -> core::result::Result<Self, Self::Err> {
    match s.to_lowercase().as_str() {
      "lts" => Ok(Self::Lts),
      "latest" => Ok(Self::Latest),
      v => semver::Version::parse(v).map(Self::Exact).map_err(|_| {
        format!(
          "'{s}' invalid. Possible values: <semver>(eg: 1.1.0), lts, latest."
        )
      }),
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
  /// List the node.js installations. Type "available" at the end to see what can be installed.
  #[command(visible_alias = "ls")]
  List {
    /// Show available versions.
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
  #[serde(deserialize_with = "deserialize_jiff_date")]
  // #[tabled(skip)]
  pub date: Date,

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

/// 自定义反序列化函数：将 "YYYY-MM-DD" 字符串解析为 jiff::civil::Date
fn deserialize_jiff_date<'de, D>(
  deserializer: D,
) -> core::result::Result<Date, D::Error>
where
  D: Deserializer<'de>,
{
  let s = String::deserialize(deserializer)?;
  // jiff 的 Date 原生支持 ISO 8601 日期格式解析，无需指定格式化字符串
  s.parse::<Date>().map_err(serde::de::Error::custom)
}

use std::{
  collections::{BTreeMap, HashMap},
  path::PathBuf,
};

/// 核心查询缓存（启动时构建一次）
#[derive(Debug, Clone)]
pub struct ReleaseDatabase {
  /// 按版本号降序排列的完整列表（最新版 = index[0]）
  sorted_releases: Vec<NodeReleaseInfo>,

  /// LTS 版本子集引用（避免重复过滤）
  lts_indices: Vec<usize>,

  /// 按主版本号分组索引，如 "v26" -> [0, 1, 2]
  major_version_map: BTreeMap<u64, Vec<usize>>,

  /// 精确版本号 → 索引位置 O(1) 点查
  version_lookup: HashMap<String, usize>,
}

impl ReleaseDatabase {
  /// 启动时调用一次，O(n log n) 构建
  pub fn build(mut releases: Vec<NodeReleaseInfo>) -> Self {
    // 1. 按版本号降序排序（语义化版本比较）
    releases.sort_by(|a, b| {
      semver::Version::parse(b.version.trim_start_matches('v'))
        .unwrap()
        .cmp(
          &semver::Version::parse(a.version.trim_start_matches('v')).unwrap(),
        )
    });

    // 2. 构建辅助索引
    let mut lts_indices = Vec::new();
    let mut major_version_map: BTreeMap<u64, Vec<usize>> = BTreeMap::new();
    let mut version_lookup = HashMap::with_capacity(releases.len());

    for (idx, release) in releases.iter().enumerate() {
      if let LtsSpec::Codename(_) = &release.lts {
        lts_indices.push(idx);
      }
      let major =
        semver::Version::parse(release.version.trim_start_matches('v'))
          .unwrap()
          .major;
      major_version_map.entry(major).or_default().push(idx);
      version_lookup.insert(release.version.clone(), idx);
    }

    Self {
      sorted_releases: releases,
      lts_indices,
      major_version_map,
      version_lookup,
    }
  }

  // ========== 查询接口 ==========

  /// ✅ 获取最新版本 → O(1)
  pub fn latest(&self) -> Option<&NodeReleaseInfo> {
    self.sorted_releases.first()
  }

  /// ✅ 获取最新 LTS 版本 → O(1)
  pub fn latest_lts(&self) -> Option<&NodeReleaseInfo> {
    self.lts_indices.first().map(|&i| &self.sorted_releases[i])
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

  /// ✅ 精确版本查询 → O(1)
  pub fn get_by_version(&self, version: &str) -> Option<&NodeReleaseInfo> {
    log::debug!("get_by_version: {:?}", version);

    let version = if version.starts_with('v') {
      version.to_string()
    } else {
      format!("v{}", version)
    };
    self
      .version_lookup
      .get(&version)
      .map(|&i| &self.sorted_releases[i])
  }

  // /// ✅ 获取某主版本下所有发布 → O(log n + k)
  // pub fn by_major(&self, major: u64) -> impl Iterator<Item = &NodeReleaseInfo> {
  //   self
  //     .major_version_map
  //     .get(&major)
  //     .into_iter()
  //     .flat_map(move |indices| {
  //       indices.iter().map(move |&i| &self.sorted_releases[i])
  //     })
  // }

  /// ✅ 获取最新的 N 个版本 → O(1) 切片，零拷贝
  pub fn latest_list(&self, n: usize) -> &[NodeReleaseInfo] {
    let end = n.min(self.sorted_releases.len());
    &self.sorted_releases[..end]
  }

  // /// ✅ 获取所有 LTS 版本（已按版本降序排列）→ O(1) 间接引用
  // pub fn lts_releases(&self) -> impl Iterator<Item = &NodeReleaseInfo> {
  //   self
  //     .lts_indices
  //     .iter()
  //     .map(move |&i| &self.sorted_releases[i])
  // }

  /// ✅ 组合查询：最新10个版本中的 LTS 版本
  pub fn lts_list(&self, n: usize) -> Vec<&NodeReleaseInfo> {
    self
      .lts_indices
      .iter()
      .take(n)
      .map(|&i| &self.sorted_releases[i])
      .collect()
  }
}

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
}

fn deserialize_proxy<'de, D>(
  deserializer: D,
) -> core::result::Result<Option<String>, D::Error>
where
  D: Deserializer<'de>,
{
  let s = String::deserialize(deserializer)?;
  if s.is_empty()
    || s.to_ascii_lowercase() == "null"
    || s.to_ascii_lowercase() == "none"
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

  pub fn save(&self) {
    log::debug!("save config");

    #[cfg(feature = "toml")]
    {
      write_to_toml(self);
      return;
    }

    #[cfg(feature = "yaml")]
    {
      write_to_txt(self);
    }
  }

  pub fn is_valid(&self) -> Result {
    if self.root.is_some() {
      let root = self.root.as_ref().unwrap();
      if !root.is_dir() {
        bail!("root is not a directory");
      }
    }

    if self.originalpath.is_some() {
      let originalpath = self.originalpath.as_ref().unwrap();
      if !originalpath.is_dir() {
        bail!("originalpath is not a directory");
      }
    }

    Ok(())
  }
}

#[cfg(feature = "toml")]
fn read_from_toml() -> Option<Config> {
  use std::fs;

  let path_str = format!("{}.toml", CONFIG_FILE_NAME);
  if let Ok(v) = fs::exists(&path_str)
    && v == true
  {
    let data = fs::read_to_string(&path_str).unwrap();
    let config: Config = toml::from_str(&data).unwrap();
    return Some(config);
  }
  None
}

#[cfg(feature = "yaml")]
fn read_from_txt() -> Option<Config> {
  use std::fs;

  let path_str = format!("{}.txt", CONFIG_FILE_NAME);
  if let Ok(v) = fs::exists(&path_str)
    && v == true
  {
    let data = fs::read_to_string(&path_str).unwrap();
    let config: Config = noyalib::from_str(&data).unwrap();

    return Some(config);
  }

  None
}

#[cfg(feature = "toml")]
fn write_to_toml(config: &Config) {
  use std::fs;

  let config_str = toml::to_string(config).unwrap();
  let path_str = if cfg!(debug_assertions) {
    format!("{}.toml.toml", CONFIG_FILE_NAME)
  } else {
    format!("{}.toml", CONFIG_FILE_NAME)
  };
  fs::write(&path_str, config_str).unwrap();
}

#[cfg(feature = "yaml")]
fn write_to_txt(config: &Config) {
  use std::fs;

  let config_str = noyalib::to_string(config).unwrap();
  let path_str = if cfg!(debug_assertions) {
    format!("{}.txt.txt", CONFIG_FILE_NAME)
  } else {
    format!("{}.txt", CONFIG_FILE_NAME)
  };
  fs::write(&path_str, config_str).unwrap();
}

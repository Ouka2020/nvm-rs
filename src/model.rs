use clap::ValueEnum;
use jiff::civil::Date;
use serde::{Deserialize, Deserializer};
use serde_repr::{Deserialize_repr, Serialize_repr};

#[derive(Clone, Debug, ValueEnum, Deserialize_repr, Serialize_repr)]
#[repr(u8)]
pub enum ArchSpec {
  /// 32 bit
  #[value(name = "32")]
  X86 = 32,
  /// 64 bit
  #[value(name = "64")]
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

  /// 可用的构建产物/平台列表
  // #[tabled(skip)]
  pub files: Vec<String>,

  /// 捆绑的 npm 版本
  // #[tabled(skip)]
  pub npm: Option<String>,

  /// V8 引擎版本
  // #[tabled(skip)]
  pub v8: String,

  /// libuv 版本
  // #[tabled(skip)]
  pub uv: Option<String>,

  /// zlib 版本
  // #[tabled(skip)]
  pub zlib: Option<String>,

  /// OpenSSL 版本
  // #[tabled(skip)]
  pub openssl: Option<String>,

  /// Node-API (ABI) 模块版本号
  // #[tabled(skip)]
  #[serde(rename = "modules")]
  pub abi_version: Option<String>,

  /// 是否为 LTS（长期支持）版本
  // #[tabled(skip)]
  // #[serde(deserialize_with = "deserialize_lts")]
  pub lts: LtsSpec,

  /// 是否为安全修复版本
  // #[tabled(skip)]
  pub security: bool,
}

/// 自定义反序列化函数：将 "YYYY-MM-DD" 字符串解析为 jiff::civil::Date
fn deserialize_jiff_date<'de, D>(deserializer: D) -> Result<Date, D::Error>
where
  D: Deserializer<'de>,
{
  let s = String::deserialize(deserializer)?;
  // jiff 的 Date 原生支持 ISO 8601 日期格式解析，无需指定格式化字符串
  s.parse::<Date>().map_err(serde::de::Error::custom)
}

use std::collections::{BTreeMap, HashMap};

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

  /// ✅ 按条件组合查询（示例：最新 LTS + 指定平台）
  pub fn latest_lts_with_platform(
    &self,
    platform: &str,
  ) -> Option<&NodeReleaseInfo> {
    self
      .lts_indices
      .iter()
      .map(|&i| &self.sorted_releases[i])
      .find(|r| r.files.iter().any(|f| f == platform))
  }

  /// ✅ 精确版本查询 → O(1)
  pub fn get_by_version(&self, version: &str) -> Option<&NodeReleaseInfo> {
    self
      .version_lookup
      .get(version)
      .map(|&i| &self.sorted_releases[i])
  }

  /// ✅ 获取某主版本下所有发布 → O(log n + k)
  pub fn by_major(&self, major: u64) -> impl Iterator<Item = &NodeReleaseInfo> {
    self
      .major_version_map
      .get(&major)
      .into_iter()
      .flat_map(move |indices| {
        indices.iter().map(move |&i| &self.sorted_releases[i])
      })
  }

  /// ✅ 获取最新的 N 个版本 → O(1) 切片，零拷贝
  pub fn latest_list(&self, n: usize) -> &[NodeReleaseInfo] {
    let end = n.min(self.sorted_releases.len());
    &self.sorted_releases[..end]
  }

  /// ✅ 获取所有 LTS 版本（已按版本降序排列）→ O(1) 间接引用
  pub fn lts_releases(&self) -> impl Iterator<Item = &NodeReleaseInfo> {
    self
      .lts_indices
      .iter()
      .map(move |&i| &self.sorted_releases[i])
  }

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

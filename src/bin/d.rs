use derive_more::{Deref, IntoIterator};
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
#[allow(dead_code)]
pub enum LtsSpec {
  Codename(String),
  NotLts(bool),
}

#[derive(Debug, Clone, Deserialize)]
pub struct NodeReleaseInfo {
  /// 版本号，如 "v26.8.1"
  pub version: String,

  /// 发布日期，格式 YYYY-MM-DD
  // #[serde(deserialize_with = "deserialize_jiff_date")]
  // #[allow(dead_code)]
  // #[tabled(skip)]
  // pub date: String,

  /// 可用的构建产物/平台列表
  // #[tabled(skip)]
  pub files: Vec<String>,

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
  pub lts: LtsSpec,
  // /// 是否为安全修复版本
  // #[tabled(skip)]
  // pub security: bool,
}

#[derive(Debug, Clone, Deserialize, Deref, IntoIterator)]
#[serde(transparent)]
struct Releases {
  #[deref]
  inner: Vec<NodeReleaseInfo>,
}

impl Releases {
  pub fn latest(&self) -> String {
    self.inner[0].version.clone()
  }

  pub fn latest_lts(&self) -> String {
    self
      .inner
      .iter()
      .find(|r| matches!(r.lts, LtsSpec::Codename(_)))
      .map(|r| r.version.clone())
      .unwrap_or_default()
  }

  pub fn latest_list(&self, count: usize) -> Vec<String> {
    let mut list: Vec<_> = self
      .inner
      .iter()
      .filter(|r| matches!(r.lts, LtsSpec::NotLts(false)))
      .take(count)
      .map(|r| r.version.clone())
      .collect();

    if list.len() < count {
      for _ in 0..(count - list.len()) {
        list.push("".to_string());
      }
    }

    list
  }

  pub fn lts_list(&self, count: usize) -> Vec<String> {
    let mut list: Vec<_> = self
      .inner
      .iter()
      .filter(|r| matches!(r.lts, LtsSpec::Codename(_)))
      .take(count)
      .map(|r| r.version.clone())
      .collect();

    if list.len() < count {
      for _ in 0..(count - list.len()) {
        list.push("".to_string());
      }
    }

    list
  }
}

fn main() -> anyhow::Result<()> {
  use_ureq()
}

// fn symlink() -> anyhow::Result<()> {
//   let key = std::env::var("NVM_SYMLINK")?;
//   println!("key: {:?}", key);
//   let s_meta = std::fs::symlink_metadata(&key)?;
//   let d_meta = std::fs::metadata(&key)?;
//   let link = std::fs::read_link(key)?;
//   println!("是符号链接: {}, {}", s_meta.is_dir(), s_meta.is_symlink());
//   println!("链接大小: {} bytes", s_meta.len());
//   println!("是目录: {}", d_meta.is_dir());
//   println!("目录大小: {} bytes", d_meta.len());
//   println!("链接目标: {:?}", link.file_name());

//   Ok(())
// }

fn use_ureq() -> anyhow::Result<()> {
  let resp: Releases =
    ureq::get("https://npmmirror.com/mirrors/node/index.json")
      .call()?
      .body_mut()
      .read_json()?;
  // let data = resp.to_string();
  println!("latest lts: {:?}", resp.latest_lts());
  println!("latest: {:?}", resp.latest());
  println!("latest list: {:?}", resp.latest_list(10));
  println!("lts list: {:?}", resp.lts_list(10));

  Ok(())
}

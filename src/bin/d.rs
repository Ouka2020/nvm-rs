use nvm_rs::Result;
#[cfg(feature = "debug")]
use nvm_rs::log_init;
use std::path::PathBuf;
use url::Url;

fn main() -> Result {
  #[cfg(feature = "debug")]
  log_init();

  download_file()
}

fn download_file() -> Result {
  let url: Url =
    r"https://npmmirror.com//mirrors/node/v20.12.2//node-v20.12.2-win-x64.zip"
      .parse()?;
  let url_str = url.as_str();
  log::debug!("下载文件: {}", url_str);

  let dest: PathBuf = r"g:\nvm\node-v20.12.2-win-x64.zip".parse()?;
  nvm_rs::download_file(url, dest)?;
  println!("下载完成");

  Ok(())
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

// fn use_ureq() -> anyhow::Result<()> {
//   let resp: ReleaseDatabase =
//     ureq::get("https://npmmirror.com/mirrors/node/index.json")
//       .call()?
//       .body_mut()
//       .read_json()?;
//   // let data = resp.to_string();
//   println!("latest lts: {:?}", resp.latest_lts());
//   println!("latest: {:?}", resp.latest());
//   println!("latest list: {:?}", resp.latest_list(10));
//   println!("lts list: {:?}", resp.lts_list(10));

//   Ok(())
// }

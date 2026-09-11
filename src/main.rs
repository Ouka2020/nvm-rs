mod model;
mod stream;

#[cfg(feature = "debug")]
use std::sync::OnceLock;
use std::{
  env, fs,
  io::{BufReader, Read},
  path::{Path, PathBuf},
};

use anyhow::bail;
use clap::Parser;
use comfy_table::{Cell, CellAlignment, Table, presets};
use indicatif::{ProgressBar, ProgressStyle};
use path_clean::PathClean;
#[cfg(feature = "debug")]
use tracing_appender::{
  non_blocking::WorkerGuard,
  rolling::{RollingFileAppender, Rotation},
};
#[cfg(feature = "debug")]
use tracing_subscriber::{
  EnvFilter, Layer, fmt, layer::SubscriberExt, util::SubscriberInitExt,
};
use zip::ZipArchive;

use crate::{
  model::{ArchSpec, CliArgs, Commands, VersionSpec},
  stream::ProgressStream,
};

pub type Result<T = ()> = anyhow::Result<T>;

#[cfg(feature = "debug")]
static LOG_GUARD: OnceLock<WorkerGuard> = OnceLock::new();

#[cfg(feature = "debug")]
const LOG_FILTER: &str = "info,nvm=debug";

const LIST_COUNT: usize = 20;

#[cfg(feature = "debug")]
fn log_init() {
  // 1. 滚动文件 Appender（按天轮转，保留30天）
  let file_appender = RollingFileAppender::builder()
    .rotation(Rotation::DAILY)
    .filename_prefix("app")
    .filename_suffix("log")
    .max_log_files(30)
    .build("./logs")
    .expect("failed to create rolling file appender");

  // 非阻塞写入，避免日志I/O阻塞主线程
  let (non_blocking_file, guard) =
    tracing_appender::non_blocking(file_appender);

  // ✅ 将 guard 存入全局静态变量，确保活到程序结束
  LOG_GUARD.set(guard).expect("logging already initialized");

  // 2. 构建 Layer
  // 控制台：彩色、人类可读、INFO 级别以上
  let console_layer = fmt::layer()
    .with_writer(std::io::stderr)
    .pretty()
    .with_filter(EnvFilter::new(LOG_FILTER));

  // 文件：无颜色、带时间戳、DEBUG 级别以上（记录更详细）
  let file_layer = fmt::layer()
    .with_writer(non_blocking_file)
    .with_ansi(false)
    .with_target(true)
    .with_line_number(true)
    .with_filter(EnvFilter::new(LOG_FILTER));

  // 3. 组合并初始化
  tracing_subscriber::registry()
    .with(console_layer)
    .with(file_layer)
    .init();
}

#[tokio::main]
async fn main() -> Result<()> {
  #[cfg(feature = "debug")]
  log_init();

  let args = CliArgs::parse();
  log::debug!("args: {:?}", args);
  let config = model::Config::load();
  log::debug!("config: {:?}", config);
  config.is_valid()?;

  match args.command {
    Commands::Install {
      version,
      arch,
      insecure,
    } => install_version(config, version, arch, insecure).await?,
    Commands::Uninstall { version } => uninstall_version(config, version)?,
    Commands::List { available } => list_versions(config, available).await?,
    Commands::On => activate_version(config)?,
    Commands::Off => todo!("deactivate_version"),
    Commands::Root { path } => display_or_update_root(config, path)?,
    Commands::Arch => todo!("get_architecture"),
    Commands::Proxy { url } => display_or_update_proxy(config, url)?,
    Commands::Current => display_current()?,
    Commands::NodeMirror { url } => display_or_update_node_mirror(config, url)?,
    Commands::NpmMirror { url } => display_or_update_npm_mirror(config, url)?,
    Commands::Use { version, arch } => {
      switch_version(config, version, arch).await?
    }
  }

  Ok(())
}

async fn list_remote_versions(config: model::Config) -> Result<()> {
  // get_release_db appends "index.json" to the mirror base url itself.
  let base_url = get_node_mirror(&config);
  log::debug!("url: {}", base_url);

  let release_database = get_release_db(&base_url).await?;

  let latest_version = release_database.latest_list(LIST_COUNT);

  let lts_version = release_database.lts_list(LIST_COUNT);

  log::debug!("latest_version: {:?}", latest_version);
  log::debug!("lts_version: {:?}", lts_version);

  let mut table = Table::new();
  table.load_style(presets::UTF8_FULL_CONDENSED).set_header([
    Cell::new("current").set_alignment(CellAlignment::Center),
    Cell::new("lts").set_alignment(CellAlignment::Center),
  ]);

  for (c, l) in latest_version.iter().zip(lts_version.iter()) {
    table.add_row(vec![
      Cell::new(c).set_alignment(CellAlignment::Center),
      Cell::new(l).set_alignment(CellAlignment::Center),
    ]);
  }

  println!("{}", table);

  //  You can use --full to show all versions.
  println!(
    "\n * Note: The list only shows the latest {} versions. Visit https://nodejs.org/en/ for more info.",
    LIST_COUNT
  );

  Ok(())
}

fn list_local_versions(config: model::Config) -> Result<()> {
  let (current_version, current_arch) = get_current_version_and_arch();
  log::debug!("current version: {}({}bit)", current_version, current_arch);

  let path = get_root(&config)?;
  let versions = get_local_versions(path)?;

  println!();
  for version in versions {
    // let version = version.strip_prefix('v').unwrap_or(version);
    log::debug!("found version: {version}");

    if version == current_version {
      println!(
        "  * {} (Currently using {}-bit executable)",
        version, current_arch
      );
    } else {
      println!("    {}", version);
    }
  }

  println!();

  Ok(())
}

/// Get all local installed versions.
/// # Returns
/// A vector of strings, each string is a version number. (e.g. "v24.2.2")
fn get_local_versions<T>(root: T) -> Result<Vec<String>>
where
  T: AsRef<Path>,
{
  let mut versions = Vec::new();

  for entry in fs::read_dir(root.as_ref())? {
    let entry = entry?;
    if !entry.file_type()?.is_dir() {
      continue;
    }

    match entry.file_name().to_str() {
      Some(name) => versions.push(name.to_string()),
      None => {
        log::debug!("skipping non-UTF8 directory entry: {:?}", entry.path())
      }
    }
  }

  Ok(versions)
}

/// Get the current version and architecture of Node.js.
/// # Returns
/// A tuple of strings, the first string is the version number, the second string is the architecture.
/// (e.g. "v24.2.2", "64")
fn get_current_version_and_arch() -> (String, String) {
  let Ok(output) = std::process::Command::new("node")
    .arg("-p")
    .arg("`${process.version},${process.arch}`") // print v24.2.2,x64
    .output()
  else {
    return (String::new(), String::new());
  };

  let stdout = String::from_utf8_lossy(&output.stdout);
  let mut parts = stdout.trim_end().split(',');

  let Some(version) = parts.next() else {
    return (String::new(), String::new());
  };
  let arch = match parts.next() {
    Some("x64") => "64",
    Some(_) => "32",
    None => return (String::new(), String::new()),
  };
  log::debug!("version: {}", version);

  (version.to_string(), arch.to_string())
}

fn get_processor_architecture() -> String {
  // PROCESSOR_ARCHITEW6432 仅在 WOW64 子系统中存在
  if env::var("PROCESSOR_ARCHITEW6432").is_ok() {
    // 当前是 64 位系统上的 32 位进程
    return "x64".to_string();
  }

  match env::var("PROCESSOR_ARCHITECTURE") {
    // todo: 没有考虑 arm 架构
    Ok(val) if val.eq_ignore_ascii_case("AMD64") => "x64".to_string(),
    Ok(val) => val.to_ascii_lowercase(),
    Err(_) => String::new(),
  }
}

fn display_current() -> Result<()> {
  let (current_version, _) = get_current_version_and_arch();
  if !current_version.is_empty() {
    println!("current version is {}", current_version);
  } else {
    println!("No current version. Run 'nvm use x.x.x' to set a version.");
  }

  Ok(())
}

fn display_or_update_proxy(
  mut config: model::Config,
  url: Option<String>,
) -> Result<()> {
  if let Some(url) = url {
    config.proxy = if url.eq_ignore_ascii_case("none") {
      None
    } else {
      Some(url)
    };
    config.save()?;
  } else if let Some(proxy) = config.proxy {
    println!("Current Proxy: {:?}", proxy);
  } else {
    println!("No Proxy set.");
  }

  Ok(())
}

fn display_or_update_root<T>(
  mut config: model::Config,
  path: Option<T>,
) -> Result<()>
where
  T: AsRef<Path>,
{
  if let Some(path) = path {
    let p = path.as_ref();
    if !p.exists() {
      bail!("path {:?} does not exist", p);
    }
    if !p.is_dir() {
      bail!("path {:?} is not a directory", p);
    }
    config.root = Some(p.to_path_buf());
    config.save()?;
    println!("Current Root: {:?}", p);
  } else if let Some(root) = config.root {
    println!("Current Root: {:?}", root);
  } else {
    println!("No Root set.");
  }

  Ok(())
}

async fn list_versions(
  config: model::Config,
  is_remote_request: bool,
) -> Result<()> {
  if is_remote_request {
    list_remote_versions(config).await
  } else {
    list_local_versions(config)
  }
}

fn display_or_update_node_mirror(
  mut config: model::Config,
  url: Option<String>,
) -> Result<()> {
  if let Some(url) = url {
    config.node_mirror = Some(url);
    config.save()?;
  } else if let Some(node_mirror) = config.node_mirror {
    println!("Current NodeMirror: {:?}", node_mirror);
  } else {
    println!("No NodeMirror set.");
  }

  Ok(())
}

fn display_or_update_npm_mirror(
  mut config: model::Config,
  url: Option<String>,
) -> Result<()> {
  if let Some(url) = url {
    config.npm_mirror = Some(url);
    config.save()?;
  } else if let Some(npm_mirror) = config.npm_mirror {
    println!("Current NpmMirror: {:?}", npm_mirror);
  } else {
    println!("No NpmMirror set.");
  }

  Ok(())
}

fn activate_version(_config: model::Config) -> Result<()> {
  Ok(())
}

fn uninstall_version(
  config: model::Config,
  version: VersionSpec,
) -> Result<()> {
  let root = get_root(&config)?;
  let versions = get_local_versions(&root)?;
  log::debug!("local versions: {:?}", versions);

  match version {
    VersionSpec::Latest | VersionSpec::Lts => {
      bail!(
        "The version must be a specific version. Can not use 'latest' or 'lts' to uninstall."
      );
    }
    VersionSpec::Exact(ver) => {
      let ver = format!("v{}", ver);
      if !versions.contains(&ver) {
        bail!("version {:?} not installed", ver);
      }
      if get_current_version_and_arch().0 == ver {
        bail!("current version {} is in use, can not uninstall it", ver);
      }

      delete_version(&root, &ver)?;

      println!("uninstall {} completed.", ver);
    }
  }

  Ok(())
}

fn delete_version(root: &Path, ver: &str) -> Result<()> {
  let path = root.join(ver);
  log::debug!("delete: {:?}", path);
  fs::remove_dir_all(&path)?;

  Ok(())
}

async fn install_version(
  config: model::Config,
  version: VersionSpec,
  arch: Option<ArchSpec>,
  skip_checksum: bool,
) -> Result<()> {
  let root = get_root(&config)?;

  let arch = get_arch(&arch);
  log::debug!("install: {:?} {}", version, arch);
  tips(arch);

  let base_url = get_node_mirror(&config);

  let db = get_release_db(&base_url).await?;
  let (exists, ver) = version_exists(&db, &version, &root)?;
  if exists {
    bail!("version {} already installed", ver);
  }

  let url = get_node_file_url(&ver, arch, &base_url);
  log::debug!("download url: {}", url);

  let root = root.as_path();
  let Some(file_name) = Path::new(&url).file_name() else {
    bail!("invalid download url: {url}");
  };
  let zip_path = root.join(file_name);

  download_version(&url, &zip_path).await?;

  if !skip_checksum {
    let url = get_node_file_checksum_url(&ver, &base_url);
    let checksum = load_checksum(&url, &ver, arch).await?;
    let valid = file_validate(&zip_path, &checksum)?;
    if !valid {
      bail!("sha256 checksum failed: {:?}", file_name);
    }

    println!("checksum valid.");
  }

  zip_extract(zip_path, root)?;

  let org_path = root.join(Path::new(file_name).with_extension(""));
  let dist_path = root.join(&ver);
  log::debug!("rename {:?} -> {:?}", org_path, dist_path);
  fs::rename(org_path, dist_path)?;

  delete_zip_files(root)?;

  println!("install {} completed.", ver);

  Ok(())
}

fn delete_junction_link<T>(link: T) -> Result<()>
where
  T: AsRef<Path>,
{
  let path = link.as_ref();
  log::debug!("delete link: {:?}", path);
  // 链接可能本来就不存在，忽略删除失败
  if let Err(e) = fs::remove_dir(path) {
    log::debug!("remove_dir failed: {e}");
  }

  Ok(())
}

fn create_junction_link<T, L>(link: T, target: L) -> Result<()>
where
  T: AsRef<Path>,
  L: AsRef<Path>,
{
  let target = target.as_ref();
  let link = link.as_ref();
  log::debug!("create link: {:?} -> {:?}", target, link);
  #[cfg(target_os = "windows")]
  junction::create(target, link)?;

  Ok(())
}

fn reset_junction_link<T, L>(link: T, target: L) -> Result<()>
where
  T: AsRef<Path>,
  L: AsRef<Path>,
{
  delete_junction_link(&link)?;
  create_junction_link(link, target)?;

  Ok(())
}

async fn switch_version(
  config: model::Config,
  version: VersionSpec,
  arch: Option<ArchSpec>,
) -> Result<()> {
  let root = get_root(&config)?;
  let arch = get_arch(&arch);

  let base_url = get_node_mirror(&config);

  let db = get_release_db(&base_url).await?;
  let (exists, ver) = version_exists(&db, &version, &root)?;
  if !exists {
    bail!("version {:?} not installed", ver);
  }

  log::debug!("ready switch to {:?}({})", version, arch);

  let node_path = root.join(&ver).clean();

  // 创建符号链接, 仅在 Windows 上有效
  reset_junction_link(get_nvm_symlink()?, node_path)?;

  println!("switch to {} success.", ver);

  // 提示用户使用 64 位版本
  tips(arch);

  Ok(())
}

fn tips(arch: &str) {
  if arch != "x64" {
    println!(
      "\n* Notice: Starting from version v23.0.0, 32-bit version are no longer available. Please use the 64-bit version."
    );
  }
}

fn file_validate<T>(path: T, sha256_checksum: &str) -> Result<bool>
where
  T: AsRef<Path>,
{
  use sha2::Digest;

  let file = std::fs::File::open(path)?;
  let mut reader = BufReader::with_capacity(256 * 1024, file);
  let mut hasher = sha2::Sha256::new();
  let mut buf = [0u8; 64 * 1024];
  // 读取错误必须向上传播，避免对损坏的半成品文件计算出错误摘要
  loop {
    let n = reader.read(&mut buf)?;
    if n == 0 {
      break;
    }
    hasher.update(&buf[..n]);
  }

  Ok(hex::encode(hasher.finalize()) == sha256_checksum)
}

/// 检查服务器上的版本是否存在, 如果不存在则报错<br>
/// 检查本地是否存在该版本的目录<br>
/// 如果存在则返回版本号, 例如 v1.1.0
fn version_exists<T>(
  db: &model::ReleaseDatabase,
  version: &VersionSpec,
  root: T,
) -> Result<(bool, String)>
where
  T: AsRef<Path>,
{
  let ver = match version {
    VersionSpec::Latest => {
      if let Some(latest_version) = db.latest() {
        latest_version
      } else {
        bail!("No latest version found.")
      }
    }
    VersionSpec::Lts => {
      if let Some(lts_version) = db.latest_lts() {
        lts_version
      } else {
        bail!("No LTS version found.")
      }
    }
    VersionSpec::Exact(s) => {
      if db.version_exists(&s) {
        s.to_string()
      } else {
        bail!("node v{} not installed.", s)
      }
    }
  };

  let root = root.as_ref();
  let exists = fs::exists(root.join(&ver))?;

  Ok((exists, ver))
}

async fn get_release_db(url: &str) -> Result<model::ReleaseDatabase> {
  let target_url = format!("{}/index.json", url);
  log::debug!("target url: {}", target_url);

  let resp = reqwest::get(target_url).await?.error_for_status()?;
  let body = resp.text().await?;
  log::debug!("body len: {}", body.len());

  let release_db: model::ReleaseDatabase = serde_json::from_str(&body)?;
  log::debug!("node_release_info count: {}", release_db.len());

  Ok(release_db)
}

async fn download_version<T>(url: &str, dest: T) -> Result<()>
where
  T: AsRef<Path>,
{
  let resp = reqwest::get(url).await?.error_for_status()?;
  let total = resp.content_length().unwrap_or(0);
  let mut stream = ProgressStream::new(resp.bytes_stream(), total);

  let mut file = tokio::fs::File::create(dest).await?;
  tokio::io::copy(&mut stream, &mut file).await?;
  stream.finish();

  Ok(())
}

async fn load_checksum(url: &str, version: &str, arch: &str) -> Result<String> {
  let resp = reqwest::get(url).await?.error_for_status()?;
  let body = resp.text().await?;
  log::debug!("body len: {}", body.len());

  let package_name = format!("node-{}-win-{}.zip", version, arch);
  for line in body.lines() {
    // SHASUMS256.txt: "<64位哈希>  node-vX.Y.Z-win-x64.zip"
    let mut parts = line.split_ascii_whitespace();
    let (Some(checksum), Some(name)) = (parts.next(), parts.next()) else {
      continue;
    };
    if name == package_name {
      return Ok(checksum.to_string());
    }
  }

  bail!("checksum not found: {:?}", package_name);
}

fn get_node_file_url(version: &str, arch: &str, base_url: &str) -> String {
  let url =
    format!("{}/{}/node-{}-win-{}.zip", base_url, version, version, arch);
  log::debug!("url: {:?}", url);
  url
}

fn get_node_file_checksum_url(version: &str, base_url: &str) -> String {
  let url = format!("{}/{}/SHASUMS256.txt", base_url, version);
  log::debug!("url: {:?}", url);
  url
}

fn zip_extract<T, R>(source: T, dest: R) -> Result<()>
where
  T: AsRef<Path>,
  R: AsRef<Path>,
{
  let file = std::fs::File::open(source)?;
  let reader = BufReader::new(file);
  let mut archive = ZipArchive::new(reader)?;
  let pb = ProgressBar::new(archive.len() as u64);
  pb.set_style(
    ProgressStyle::default_bar()
      .template("extracting [{bar:40}] [{percent:.2}%]")
      .expect("invalid template")
      .progress_chars("=> "),
  );
  for i in 0..archive.len() {
    let mut entry = archive.by_index(i)?;
    safe_extract(&mut entry, &dest)?;
    pb.inc(1);
  }
  pb.finish();
  log::debug!("extract done");
  Ok(())
}

fn safe_extract<T>(
  entry: &mut zip::read::ZipFile<'_, BufReader<fs::File>>,
  dest: T,
) -> Result<()>
where
  T: AsRef<Path>,
{
  let dest_dir = dest.as_ref();

  // 防止 Zip Slip：解压后的路径必须仍在目标目录内
  let out_path = dest_dir.join(entry.mangled_name());
  if !out_path.starts_with(dest_dir) {
    bail!("illegal path in archive: {}", entry.name());
  }

  if entry.is_dir() {
    std::fs::create_dir_all(&out_path)?;
  } else if let Some(parent) = out_path.parent() {
    std::fs::create_dir_all(parent)?;
    let mut outfile = std::fs::File::create(&out_path)?;
    std::io::copy(&mut entry.take(100 * 1024 * 1024), &mut outfile)?; // 限制100MB
  }
  Ok(())
}

fn delete_zip_files<T>(root: T) -> Result<()>
where
  T: AsRef<Path>,
{
  let root = root.as_ref();
  for entry in fs::read_dir(root)? {
    let entry = entry?;
    if entry.file_type()?.is_file()
      && entry.file_name().to_string_lossy().ends_with(".zip")
    {
      let file_path = entry.path();
      log::debug!("delete zip file: {}", file_path.display());
      fs::remove_file(file_path)?;
    }
  }
  Ok(())
}

fn get_arch(arch: &Option<ArchSpec>) -> &'static str {
  match arch {
    Some(ArchSpec::X64) => "x64",
    Some(ArchSpec::X86) => "x86",
    // todo: 没有考虑 arm 架构
    None if get_processor_architecture().ends_with("64") => "x64",
    None => "x86",
  }
}

fn get_root(config: &model::Config) -> Result<PathBuf> {
  Ok(match &config.root {
    Some(root) => root.clone(),
    None => env::current_dir()?,
  })
}

fn get_node_mirror(config: &model::Config) -> String {
  config
    .node_mirror
    .clone()
    .unwrap_or_else(|| "https://nodejs.org/dist".to_string())
}

fn get_nvm_symlink() -> Result<PathBuf> {
  Ok(env::var("NVM_SYMLINK")?.into())
}

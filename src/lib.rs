use std::{
  env::{self},
  ffi::c_void,
  fs::{self, File},
  io::{self, BufReader, Read, Seek},
  os::windows::ffi::OsStrExt,
  path::{Path, PathBuf},
  time::Duration,
};

use anyhow::bail;
use comfy_table::{Cell, CellAlignment, Table, presets};
use indicatif::{ProgressBar, ProgressStyle};
use inquire::validator::Validation;
use path_clean::PathClean;
use regex::{Regex, RegexBuilder};
use url::Url;
use windows_sys::Win32::Storage::FileSystem::{
  GetFileVersionInfoSizeW, GetFileVersionInfoW, VS_FIXEDFILEINFO,
  VerQueryValueW,
};
use winreg::{
  RegKey, RegValue,
  enums::{HKEY_CURRENT_USER, RegType},
};
use zip::ZipArchive;

use std::sync::LazyLock;
#[cfg(feature = "debug")]
use tracing_appender::{
  non_blocking::WorkerGuard,
  rolling::{RollingFileAppender, Rotation},
};
#[cfg(feature = "debug")]
use tracing_subscriber::{
  EnvFilter, Layer, fmt, layer::SubscriberExt, util::SubscriberInitExt,
};

use crate::model::{ArchSpec, Config, ReleaseDatabase, VersionSpec};

pub mod model;

#[cfg(feature = "debug")]
static LOG_GUARD: std::sync::OnceLock<WorkerGuard> = std::sync::OnceLock::new();

#[cfg(feature = "debug")]
const LOG_FILTER: &str = "info,nvm_windows=debug";

static HTTP_CLIENT: LazyLock<ureq::Agent> = LazyLock::new(|| {
  let agent_config = ureq::Agent::config_builder()
    .timeout_global(Some(Duration::from_secs(15 * 60)))
    .timeout_recv_response(Some(Duration::from_secs(60)))
    .timeout_recv_body(Some(Duration::from_secs(10 * 60)))
    .build();
  ureq::Agent::new_with_config(agent_config)
});

const LIST_COUNT: usize = 20;

static VERSION_REGEX: LazyLock<Regex> =
  LazyLock::new(|| Regex::new(r"^v\d+\.\d+\.\d+$").unwrap());

static NVM_SYMLINK_REGEX: LazyLock<Regex> = LazyLock::new(|| {
  RegexBuilder::new(r"%NVM_SYMLINK%")
    .case_insensitive(true)
    .build()
    .unwrap()
});

pub type Result<T = ()> = anyhow::Result<T>;

pub fn activate_symlink(config: Config) -> Result {
  let symlink = get_nvm_symlink()?;
  if let Ok(metadata) = fs::symlink_metadata(&symlink)
    && metadata.is_symlink()
  {
    println!("node is already activated.");
    return Ok(());
  }

  let root = get_root(&config)?;
  let versions = get_local_versions(&root)?;
  let Some(last) = versions.last() else {
    bail!("no node version installed");
  };

  create_junction_link(symlink, root.join(last))?;

  println!("node {} is activated.", last);

  Ok(())
}

/// Create a junction link
/// # Params
/// * `link` - The symlink path, e.g. c:\\program files\\nodejs
/// * `target` - The target path, e.g. d:\\nodejs_root\\v18.16.0
fn create_junction_link<T, L>(link: T, target: L) -> Result
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

pub fn deactivate_symlink() -> Result {
  let symlink = get_nvm_symlink()?;
  let Ok(_) = fs::symlink_metadata(&symlink) else {
    println!("node is already deactivated.");
    return Ok(());
  };

  delete_junction_link(symlink)?;

  println!("node is deactivated.");

  Ok(())
}

fn delete_junction_link<T>(link: T) -> Result
where
  T: AsRef<Path>,
{
  let path = link.as_ref();
  log::debug!("delete link: {:?}", path);
  // The link might not exist, so ignore failed deletion.
  if let Err(e) = fs::remove_dir(path) {
    log::debug!("remove_dir failed: {e}");
  }

  Ok(())
}

fn delete_version(root: &Path, ver: &str) -> Result {
  let path = root.join(ver);
  log::debug!("delete: {:?}", path);
  fs::remove_dir_all(&path)?;

  Ok(())
}

pub fn display_current() -> Result {
  if let Some(current_version) = get_current_version() {
    println!("current version is {}", current_version);
  } else {
    println!("No current version.");
    println!();
    println!("Tips:");
    println!(" * Run 'nvm use x.x.x' to set a version.");
    println!(" * Run 'nvm ls' to list available versions locally.");
    println!(" * Run 'nvm ls --remote' to list available versions online.");
    println!(" * Run 'nvm install latest' to install the latest version.");
  }

  Ok(())
}

pub fn display_or_update_npm_mirror(
  mut config: Config,
  url: Option<Url>,
) -> Result {
  if url.is_some() {
    config.npm_mirror = url;
    config.save()?;
  }

  if let Some(npm_mirror) = config.npm_mirror {
    println!("Current NpmMirror: {}", npm_mirror);
  } else {
    println!("No NpmMirror set.");
  }

  Ok(())
}

pub fn display_or_update_node_mirror(
  mut config: Config,
  url: Option<Url>,
) -> Result {
  if url.is_some() {
    config.node_mirror = url;
    config.save()?;
  }

  if let Some(node_mirror) = config.node_mirror {
    println!("Current NodeMirror: {}", node_mirror);
  } else {
    println!("No NodeMirror set.");
  }

  Ok(())
}

pub fn display_or_update_root<T>(mut config: Config, path: Option<T>) -> Result
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

fn download_file<U>(url: U, dest: &mut File) -> Result
where
  U: AsRef<str>,
{
  let url = url.as_ref();
  // 1. send request
  let mut resp = HTTP_CLIENT.get(url).call()?;

  // 2. get Content-Length header
  let total_size = resp
    .headers()
    .get("Content-Length")
    .and_then(|v| v.to_str().ok())
    .and_then(|s| s.parse::<u64>().ok())
    .unwrap_or(0);

  // 3. create progress bar
  let pb = if total_size > 0 {
    let pb = ProgressBar::new(total_size);
    pb.set_style(
            ProgressStyle::default_bar()
                .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta})")?
                .progress_chars("#>-"),
        );
    pb
  } else {
    // 4. unknown size, use progress spinner
    let pb = ProgressBar::new_spinner();
    pb.set_style(
      ProgressStyle::default_spinner()
        .template("{spinner:.green} [{elapsed_precise}] {bytes} downloaded")?,
    );
    pb
  };

  // 4. wrap response body
  let reader = resp.body_mut().as_reader();
  let mut progress_reader = pb.wrap_read(reader);

  // 5. write to file
  io::copy(&mut progress_reader, dest)?;

  // 6. complete
  pb.finish_with_message("Download complete!");
  Ok(())
}

fn file_validate(file: &File, sha256_checksum: &str) -> Result<bool> {
  use sha2::Digest;

  let mut file = file.try_clone()?;
  file.rewind()?;

  let mut reader = BufReader::with_capacity(256 * 1024, file);
  let mut hasher = sha2::Sha256::new();
  let mut buf = [0u8; 64 * 1024];

  loop {
    let n = reader.read(&mut buf)?;
    if n == 0 {
      break;
    }
    hasher.update(&buf[..n]);
  }

  Ok(hex::encode(hasher.finalize()) == sha256_checksum)
}

pub fn setup(mut config: Config) -> Result {
  let link_path =
    inquire::Text::new("Which directory to use as the symlink path?")
      .with_default(r"C:\Program Files\nodejs")
      .with_help_message("Tip: The symlink must not already exist.")
      .with_validator(symlink_validator)
      .prompt()?;

  let node_store_root =
    inquire::Text::new("Which directory to use as the node store root?")
      .with_default(&config.dir.data_local_dir().to_string_lossy())
      // .with_help_message("Tip: The node store root must not already exist.")
      .with_validator(node_store_root_validator)
      .prompt()?;

  let node_mirror = node_mirror_prompt()?;

  let npm_mirror = npm_mirror_prompt()?;

  std::fs::create_dir_all(&node_store_root)?;
  config.root = Some(node_store_root.into());
  config.node_mirror = node_mirror;
  config.npm_mirror = npm_mirror;

  update_environment(link_path)?;

  log::debug!("config: {:?}", config);
  config.save()?;

  println!("Finished. Please restart the console to update the environment.");

  Ok(())
}

type ValidatorResult =
  core::result::Result<Validation, Box<dyn core::error::Error + Send + Sync>>;

fn symlink_validator(value: &str) -> ValidatorResult {
  // if std::fs::symlink_metadata(value.trim()).is_ok() {
  //   return Ok(Validation::Invalid(
  //     "The symlink or directory already exists.".into(),
  //   ));
  // }

  let path = std::path::Path::new(value.trim());
  if path.extension().is_some() {
    return Ok(Validation::Invalid(
      "The symlink path must be a directory.".into(),
    ));
  }

  Ok(Validation::Valid)
}

fn node_store_root_validator(value: &str) -> ValidatorResult {
  let path = std::path::Path::new(value.trim());

  if path.extension().is_some() {
    return Ok(Validation::Invalid("Must be a directory.".into()));
  }

  Ok(Validation::Valid)
}

fn url_validator(value: &str) -> ValidatorResult {
  let Ok(path) = url::Url::parse(value.trim()) else {
    return Ok(Validation::Invalid("Must be a valid URL.".into()));
  };

  if !matches!(path.scheme(), "https" | "http") || path.host().is_none() {
    return Ok(Validation::Invalid(
      "Only HTTP and HTTPS protocols are supported.".into(),
    ));
  }

  Ok(Validation::Valid)
}

fn node_mirror_prompt() -> Result<Option<Url>> {
  let node_mirror_list = vec!["https://npmmirror.com/mirrors/node/"];

  mirror_prompt(
    node_mirror_list,
    "Which mirror to use for nodejs?",
    "Please input the node mirror address:",
  )
}

fn npm_mirror_prompt() -> Result<Option<Url>> {
  let npm_mirror_list = vec!["https://npmmirror.com/mirrors/npm/"];

  mirror_prompt(
    npm_mirror_list,
    "Which mirror to use for npm?",
    "Please input the npm mirror address:",
  )
}

/// Prompts the user to select a mirror from a predefined list or enter a custom mirror address.
///
/// This function automatically prepends `"none"` and appends `"custom"` to the provided
/// `options` list, then presents an interactive selection menu. If the user selects
/// `"custom"`, they will be further prompted to input a custom mirror value.
///
/// # Arguments
///
/// * `options` - A list of predefined mirror options (e.g., mirror names or URLs).
/// * `select_prompt` - The prompt message displayed for the selection menu
///   (e.g., "Please select a mirror:").
/// * `text_prompt` - The prompt message displayed in the text input when the user
///   selects "custom" (e.g., "Enter custom mirror URL:").
///
/// # Returns
///
/// * `Ok(String)` - The selected predefined mirror name, or the custom mirror
///   string entered by the user.
///
/// # Errors
///
/// * Returns an `Err` variant of `Result` when the user cancels the interaction
///   (e.g., by pressing `Ctrl+C` / `Esc`) or when an underlying I/O error occurs.
///   The error is typically converted from an `inquire` error.
fn mirror_prompt(
  mut options: Vec<&'static str>,
  select_prompt: &str,
  text_prompt: &str,
) -> Result<Option<Url>> {
  // Pre-allocate capacity to avoid reallocation
  options.reserve(2);
  // Insert "none" at the head
  options.insert(0, "none");
  // Append "custom" at the tail
  options.push("custom");

  let mirror = inquire::Select::new(select_prompt, options).prompt()?;
  match mirror {
    "none" => Ok(None),
    "custom" => {
      let mirror_url = inquire::Text::new(text_prompt)
        // .with_help_message("Tip: The node store root must not already exist.")
        .with_validator(url_validator)
        .prompt()?;

      Ok(Some(Url::parse(&mirror_url)?))
    }
    s => Ok(Some(Url::parse(s)?)),
  }
}

/// Get the current version of Node.js.
/// # Returns
/// A string, the version number.
/// (e.g. "v24.2.2")
pub fn get_current_version() -> Option<String> {
  let Ok(symlink) = get_nvm_symlink() else {
    return None;
  };
  // 1. Convert the path to a wide string (null-terminated).
  let wide_path: Vec<u16> = symlink
    .join("node.exe")
    .as_os_str()
    .encode_wide()
    .chain(std::iter::once(0))
    .collect();

  // 2. Get the version info buffer size.
  // SAFETY: wide_path is valid (null-terminated).
  let size = unsafe {
    GetFileVersionInfoSizeW(wide_path.as_ptr(), std::ptr::null_mut())
  };
  if size == 0 {
    return None; // File does not exist or has no version resource
  }

  // 3. Allocate buffer and read version info.
  let mut buffer: Vec<u8> = vec![0u8; size as usize];
  // SAFETY: buffer length >= size, wide_path is valid (null-terminated).
  let ok = unsafe {
    GetFileVersionInfoW(
      wide_path.as_ptr(),
      0,
      size,
      buffer.as_mut_ptr() as *mut c_void,
    )
  };
  if ok == 0 {
    return None;
  }

  // 4. Query the root block "\" to get VS_FIXEDFILEINFO.
  let query: Vec<u16> = "\\".encode_utf16().chain(std::iter::once(0)).collect();
  let mut info_ptr: *const VS_FIXEDFILEINFO = std::ptr::null();
  let mut info_len: u32 = 0;

  // SAFETY: Buffer contains valid version data; query is a valid wide string.
  let found = unsafe {
    VerQueryValueW(
      buffer.as_ptr() as *const c_void,
      query.as_ptr(),
      &mut info_ptr as *mut _ as *mut *mut c_void,
      &mut info_len,
    )
  };
  if found == 0
    || info_ptr.is_null()
    || info_len < std::mem::size_of::<VS_FIXEDFILEINFO>() as u32
  {
    return None;
  }

  // 5. Extract version number from VS_FIXEDFILEINFO.
  // SAFETY: info_ptr points to a valid VS_FIXEDFILEINFO struct.
  let ffi = unsafe { &*info_ptr };

  let version = format!(
    "v{}.{}.{}",
    ffi.dwFileVersionMS >> 16,
    ffi.dwFileVersionMS & 0xFFFF,
    ffi.dwFileVersionLS >> 16
  );

  Some(version)
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

    if let Some(name) = entry.file_name().to_str()
      && VERSION_REGEX.is_match(name)
    {
      versions.push(name.to_string());
    } else {
      log::debug!("skipping invalid directory entry: {:?}", entry.path());
    }
  }

  versions.sort();
  Ok(versions)
}

fn get_nvm_symlink() -> Result<PathBuf> {
  match env::var("NVM_SYMLINK") {
    Ok(path) => Ok(path.into()),
    Err(e) => {
      log::error!("get nvm symlink failed: {:?}", e);
      bail!("get nvm symlink failed. use 'nvm setup' to set it up.");
    }
  }
}

fn get_root(config: &Config) -> Result<PathBuf> {
  Ok(match &config.root {
    Some(root) => root.clone(),
    None => env::current_dir()?,
  })
}

pub fn install_version(
  config: Config,
  version: VersionSpec,
  skip_checksum: bool,
) -> Result {
  let root = get_root(&config)?;

  let arch = ArchSpec::get_from_machine();
  log::debug!("install: {:?} {}", version, arch);

  let db = ReleaseDatabase::load_from_url(
    &HTTP_CLIENT,
    config.get_node_url(&["index.json"]),
  )?;
  let (exists, ver) = version_exists(&db, &version, &root)?;
  if exists {
    bail!("version {} already installed", ver);
  }

  let url = format!("node-{}-win-{}.zip", ver, arch);
  let url = config.get_node_url(&[&ver, &url]);
  log::debug!("download url: {}", url);

  // full file name: node-version-win-x64.zip
  let Some(file_name) = url
    .path_segments()
    .and_then(|mut segments| segments.next_back())
    .filter(|s| !s.is_empty())
  else {
    bail!("invalid url: {url}");
  };
  let mut zip_path = tempfile::tempfile()?;

  download_file(&url, &mut zip_path)?;

  print!("checksum...");
  if !skip_checksum {
    let url = config.get_node_url(&[&ver, "SHASUMS256.txt"]);
    log::debug!("url: {:?}", url);
    let checksum = load_checksum(&url, &ver, &arch)?;
    let valid = file_validate(&zip_path, &checksum)?;
    if !valid {
      println!("failed.");
      let msg = format!("sha256 checksum failed: {:?}", file_name);
      log::error!("{}", msg);
      bail!(msg);
    }

    println!("OK.");
  } else {
    println!("skip.");
  }

  zip_extract(&zip_path, &root)?;

  let org_path = &root.join(file_name).with_extension("");
  let dist_path = &root.join(&ver);
  log::debug!("rename {:?} -> {:?}", org_path, dist_path);
  fs::rename(org_path, dist_path)?;

  println!("install {} completed.", ver);

  Ok(())
}

fn load_checksum<B, V>(
  base_url: B,
  version: V,
  arch: &ArchSpec,
) -> Result<String>
where
  B: AsRef<str>,
  V: AsRef<str>,
{
  let base_url = base_url.as_ref();
  let version = version.as_ref();

  let text_data = HTTP_CLIENT
    .get(base_url)
    .call()?
    .body_mut()
    .read_to_string()?;
  log::debug!("body len: {}", text_data.len());

  let package_name = format!("node-{}-win-{}.zip", version, arch);
  for line in text_data.lines() {
    // SHASUMS256.txt: "<64-bit hash>  node-vX.Y.Z-win-x64.zip"
    let mut parts = line.split_ascii_whitespace();
    let (Some(checksum), Some(name)) = (parts.next(), parts.next()) else {
      continue;
    };

    if name.trim_start_matches('*') == package_name {
      return Ok(checksum.to_string());
    }
  }

  bail!("checksum not found: {:?}", package_name);
}

pub fn list_local_versions(config: Config) -> Result {
  let current_version = get_current_version();
  log::debug!("current version: {:?}", current_version);

  let path = get_root(&config)?;
  let versions = get_local_versions(path)?;

  if versions.is_empty() {
    println!("No versions are installed.");
  } else {
    println!();

    for version in versions {
      log::debug!("found version: {version}");

      print!("    {}", version);
      if current_version.as_deref() == Some(version.as_str()) {
        println!(" <- In use");
      } else {
        println!();
      }
    }

    println!();
  }

  Ok(())
}

pub fn list_remote_versions(config: Config) -> Result {
  let release_database = ReleaseDatabase::load_from_url(
    &HTTP_CLIENT,
    config.get_node_url(&["index.json"]),
  )?;

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

  println!(
    "\n * Note: The list only shows the latest {} versions. Visit https://nodejs.org/en/ for more info.",
    LIST_COUNT
  );

  Ok(())
}

pub fn list_versions(config: Config, is_remote_request: bool) -> Result {
  if is_remote_request {
    list_remote_versions(config)
  } else {
    list_local_versions(config)
  }
}

#[cfg(feature = "debug")]
pub fn log_init() {
  // 1. Rolling file appender (daily rotation, retain 30 days)
  std::fs::create_dir_all("./logs").expect("failed to create logs directory");
  let file_appender = RollingFileAppender::builder()
    .rotation(Rotation::DAILY)
    .filename_prefix("app")
    .filename_suffix("log")
    .max_log_files(30)
    .build("./logs")
    .expect("failed to create rolling file appender");

  // Non-blocking write to avoid log I/O blocking the main thread
  let (non_blocking_file, guard) =
    tracing_appender::non_blocking(file_appender);

  // Store `guard` in a global static to keep it alive until program exit
  LOG_GUARD.set(guard).expect("logging already initialized");

  // 2. Build layers
  // Console: colored, human-readable, INFO and above
  let console_layer = fmt::layer()
    .with_writer(std::io::stderr)
    .pretty()
    .with_filter(EnvFilter::new(LOG_FILTER));

  // File: no color, with timestamps, DEBUG and above (more detailed)
  let file_layer = fmt::layer()
    .with_writer(non_blocking_file)
    .with_ansi(false)
    .with_target(true)
    .with_line_number(true)
    .with_filter(EnvFilter::new(LOG_FILTER));

  // 3. Combine and initialize
  tracing_subscriber::registry()
    .with(console_layer)
    .with(file_layer)
    .init();
}

/// Reset a junction link
/// # Params
/// * `link` - The symlink path, e.g. c:\\program files\\nodejs
/// * `target` - The target path, e.g. d:\\nodejs_root\\v18.16.0
fn reset_junction_link<T, L>(link: T, target: L) -> Result
where
  T: AsRef<Path>,
  L: AsRef<Path>,
{
  delete_junction_link(&link)?;
  create_junction_link(link, target)?;

  Ok(())
}

fn safe_extract<T>(
  entry: &mut zip::read::ZipFile<'_, BufReader<File>>,
  dest: T,
) -> Result<()>
where
  T: AsRef<Path>,
{
  use std::fs::{File, create_dir_all};
  use std::io::{BufWriter, copy};

  let dest_dir = dest.as_ref();

  // Prevent Zip Slip: extracted path must remain within the destination directory
  let out_path = dest_dir.join(entry.mangled_name());
  if !out_path.starts_with(dest_dir) {
    bail!("illegal path in archive: {}", entry.name());
  }

  if entry.is_dir() {
    create_dir_all(&out_path)?;
  } else if let Some(parent) = out_path.parent() {
    create_dir_all(parent)?;
    let mut writer = BufWriter::new(File::create(&out_path)?);

    copy(entry, &mut writer)?;
  }
  Ok(())
}

pub fn switch_version(config: Config, version: VersionSpec) -> Result {
  let root = get_root(&config)?;
  let arch = ArchSpec::get_from_machine();

  let db = ReleaseDatabase::load_from_url(
    &HTTP_CLIENT,
    config.get_node_url(&["index.json"]),
  )?;
  let (exists, ver) = version_exists(&db, &version, &root)?;
  let current_ver = get_current_version();
  if !exists {
    bail!("version {:?} not installed", ver);
  }
  if let Some(current_ver) = current_ver
    && current_ver == ver
  {
    bail!("version {:?} is already used", ver);
  }

  log::debug!("ready switch to {}({})", ver, arch);

  let node_path = root.join(&ver).clean();

  reset_junction_link(get_nvm_symlink()?, node_path)?;

  println!("switch to {} success.", ver);

  Ok(())
}

pub fn uninstall_version(config: Config, version: VersionSpec) -> Result {
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
      if !versions.contains(&ver) {
        bail!("version {:?} not installed", ver);
      }
      if let Some(current_ver) = get_current_version()
        && current_ver == ver
      {
        bail!("version {:?} is in use, can not uninstall it", ver);
      }

      delete_version(&root, &ver)?;

      println!("uninstall {:?} completed.", ver);
    }
  }

  Ok(())
}

/// Check if the version exists on the server; error if not.<br>
/// Check if the version directory exists locally.<br>
/// Returns the version string if it exists, e.g. v1.1.0
fn version_exists<T>(
  db: &ReleaseDatabase,
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
      if db.version_exists(s) {
        s.clone()
      } else {
        bail!("version {:?} not exists.", s)
      }
    }
  };

  let root = root.as_ref();
  let exists = fs::exists(root.join(&ver))?;

  Ok((exists, ver))
}

fn zip_extract<D>(source: &File, dest: D) -> Result
where
  D: AsRef<Path>,
{
  let mut file = source.try_clone()?;
  file.rewind()?;
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

fn update_environment(symlink: impl AsRef<Path>) -> Result {
  let symlink = symlink.as_ref().to_string_lossy().to_string();
  let root = RegKey::predef(HKEY_CURRENT_USER);
  let (env, _) = root.create_subkey("Environment")?;
  env.set_value("NVM_SYMLINK", &symlink)?;
  let mut path: String = env.get_value("Path")?;

  log::debug!("Environment Path: {}", path);

  if NVM_SYMLINK_REGEX.is_match(&path) {
    log::debug!("NVM_SYMLINK is already in Path");
  } else {
    log::debug!("NVM_SYMLINK is not in Path");

    if !path.ends_with(r";") {
      path.push_str(r";%NVM_SYMLINK%;");
    } else {
      path.push_str(r"%NVM_SYMLINK%;");
    }

    let bytes = path
      .encode_utf16()
      .chain(std::iter::once(0))
      .flat_map(|c| c.to_ne_bytes())
      .collect();
    let value = RegValue {
      vtype: RegType::REG_EXPAND_SZ,
      bytes,
    };
    env.set_raw_value("Path", &value)?;
  }

  Ok(())
}

#[cfg(test)]
mod test {
  use super::*;
  use pretty_assertions::assert_eq;
  use rstest::{fixture, rstest};

  #[fixture]
  fn config() -> Config {
    Config {
      root: Some(PathBuf::from("nvmroot")),
      node_mirror: Some(
        Url::parse("https://npmmirror.com/mirrors/node/").unwrap(),
      ),
      ..Default::default()
    }
  }

  #[rstest]
  fn get_nvm_symlink_test() {
    let nvm_symlink = get_nvm_symlink().unwrap();
    let nvm_symlink = nvm_symlink.to_str().unwrap();
    assert_eq!(nvm_symlink, "C:\\Program Files\\nodejs");
  }

  #[rstest]
  fn get_root_test(config: Config) {
    let root = get_root(&config).unwrap();
    assert_eq!(root, PathBuf::from("nvmroot"));
  }

  #[rstest]
  fn get_local_versions_test(config: Config) {
    let root = get_root(&config).unwrap();

    let local_versions = get_local_versions(root).unwrap();
    assert!(!local_versions.is_empty());
  }

  #[rstest]
  fn get_current_version_test() {
    let current_ver = get_current_version();
    assert_eq!(current_ver.is_some(), true);
  }

  #[rstest]
  fn display_current_test() {
    assert_eq!(display_current().is_ok(), true);
  }

  #[rstest]
  fn deactivate_and_activate_version_test(config: Config) {
    assert_eq!(deactivate_symlink().is_ok(), true);
    assert_eq!(activate_symlink(config.clone()).is_ok(), true);
  }

  #[rstest]
  fn install_and_uninstall_version_test(config: Config) {
    let ver = VersionSpec::Exact("v23.0.0".to_string());

    let r = install_version(config.clone(), ver.clone(), false);
    if let Err(e) = &r {
      println!("{:?}", e);
    }
    assert_eq!(r.is_ok(), true);

    let r = uninstall_version(config, ver);
    if let Err(e) = &r {
      println!("{:?}", e);
    }
    assert_eq!(r.is_ok(), true);
  }

  #[rstest]
  fn list_local_versions_test(config: Config) {
    assert_eq!(list_local_versions(config.clone()).is_ok(), true);
  }

  #[rstest]
  fn list_remote_versions_test(config: Config) {
    assert_eq!(list_remote_versions(config.clone()).is_ok(), true);
  }

  #[rstest]
  fn display_npm_mirror_test(config: Config) {
    assert_eq!(
      display_or_update_npm_mirror(config.clone(), None).is_ok(),
      true
    );
  }

  #[rstest]
  fn display_node_mirror_test(config: Config) {
    assert_eq!(
      display_or_update_node_mirror(config.clone(), None).is_ok(),
      true
    );
  }

  #[rstest]
  fn display_root_test(config: Config) {
    // display current root
    assert_eq!(
      display_or_update_root::<&str>(config.clone(), None).is_ok(),
      true
    );
    // error: non-existent path
    let r = display_or_update_root(
      config.clone(),
      Some(PathBuf::from("nonexistent_dir_xyz")),
    );
    assert_eq!(r.is_err(), true);
  }

  #[rstest]
  fn switch_version_test(config: Config) {
    let ver = VersionSpec::Exact("v24.2.0".to_string());
    let r = switch_version(config.clone(), ver);
    if let Err(e) = &r {
      println!("{:?}", e);
      assert_eq!(e.to_string(), "version \"v24.2.0\" not installed");
    }
    assert_eq!(r.is_err(), true);
  }

  // ---------- symlink_validator ----------

  #[rstest]
  #[case("C:\\Program Files\\nodejs", true)]
  #[case("D:\\nodejs", true)]
  #[case("nodejs", true)]
  #[case("C:\\path\\to\\node.exe", false)]
  #[case("test.txt", false)]
  fn symlink_validator_test(#[case] input: &str, #[case] expect_valid: bool) {
    let result = symlink_validator(input).unwrap();
    if expect_valid {
      assert!(matches!(result, Validation::Valid));
    } else {
      assert!(matches!(result, Validation::Invalid(_)));
    }
  }

  // ---------- node_store_root_validator ----------

  #[rstest]
  #[case("D:\\nvm_root", true)]
  #[case("C:\\Users\\test\\AppData\\Local\\nvm", true)]
  #[case("nvm", true)]
  #[case("test.exe", false)]
  #[case("config.toml", false)]
  fn node_store_root_validator_test(
    #[case] input: &str,
    #[case] expect_valid: bool,
  ) {
    let result = node_store_root_validator(input).unwrap();
    if expect_valid {
      assert!(matches!(result, Validation::Valid));
    } else {
      assert!(matches!(result, Validation::Invalid(_)));
    }
  }

  // ---------- url_validator ----------

  #[rstest]
  #[case("https://nodejs.org/dist/", true)]
  #[case("http://example.com", true)]
  #[case("HTTPS://NPMMIRROR.COM/mirrors/node/", true)]
  #[case("ftp://files.example.com", false)]
  #[case("file:///local/path", false)]
  #[case("not-a-url", false)]
  #[case("https://", false)]
  fn url_validator_test(#[case] input: &str, #[case] expect_valid: bool) {
    let result = url_validator(input).unwrap();
    if expect_valid {
      assert!(matches!(result, Validation::Valid));
    } else {
      assert!(matches!(result, Validation::Invalid(_)));
    }
  }

  // ---------- file_validate ----------

  #[rstest]
  fn file_validate_correct_checksum() {
    use sha2::Digest;
    let content = b"hello world";
    let expected_hash = hex::encode(sha2::Sha256::digest(content));

    let mut tmp = tempfile::tempfile().unwrap();
    std::io::Write::write_all(&mut tmp, content).unwrap();

    assert_eq!(file_validate(&tmp, &expected_hash).unwrap(), true);
  }

  #[rstest]
  fn file_validate_incorrect_checksum() {
    let content = b"hello world";
    let mut tmp = tempfile::tempfile().unwrap();
    std::io::Write::write_all(&mut tmp, content).unwrap();

    assert_eq!(file_validate(&tmp, "deadbeef").unwrap(), false);
  }

  #[rstest]
  fn file_validate_empty_file() {
    use sha2::Digest;
    let expected_hash = hex::encode(sha2::Sha256::digest(b""));

    let tmp = tempfile::tempfile().unwrap();
    assert_eq!(file_validate(&tmp, &expected_hash).unwrap(), true);
  }

  // ---------- get_local_versions ----------

  #[rstest]
  fn get_local_versions_empty_dir() {
    let tmp = tempfile::tempdir().unwrap();
    let versions = get_local_versions(tmp.path()).unwrap();
    assert_eq!(versions.len(), 0);
  }

  #[rstest]
  fn get_local_versions_filters_non_version_dirs() {
    let tmp = tempfile::tempdir().unwrap();
    fs::create_dir(tmp.path().join("v18.0.0")).unwrap();
    fs::create_dir(tmp.path().join("v20.1.0")).unwrap();
    fs::create_dir(tmp.path().join("not-a-version")).unwrap();
    fs::create_dir(tmp.path().join("v1.2")).unwrap();
    fs::create_dir(tmp.path().join("v1.2.3.4")).unwrap();

    let versions = get_local_versions(tmp.path()).unwrap();
    assert_eq!(versions, vec!["v18.0.0", "v20.1.0"]);
  }

  #[rstest]
  fn get_local_versions_skips_files() {
    let tmp = tempfile::tempdir().unwrap();
    fs::create_dir(tmp.path().join("v18.0.0")).unwrap();
    fs::write(tmp.path().join("v20.0.0"), "file not dir").unwrap();

    let versions = get_local_versions(tmp.path()).unwrap();
    assert_eq!(versions, vec!["v18.0.0"]);
  }

  #[rstest]
  fn get_local_versions_sorted() {
    let tmp = tempfile::tempdir().unwrap();
    fs::create_dir(tmp.path().join("v22.0.0")).unwrap();
    fs::create_dir(tmp.path().join("v18.0.0")).unwrap();
    fs::create_dir(tmp.path().join("v20.0.0")).unwrap();

    let versions = get_local_versions(tmp.path()).unwrap();
    assert_eq!(
      versions,
      vec!["v18.0.0", "v20.0.0", "v22.0.0"]
    );
  }

  // ---------- get_root ----------

  #[rstest]
  fn get_root_with_none_falls_back_to_current_dir(config: Config) {
    let mut cfg = config;
    cfg.root = None;
    let root = get_root(&cfg).unwrap();
    assert_eq!(root, env::current_dir().unwrap());
  }

  // ---------- version_exists ----------

  #[rstest]
  fn version_exists_latest(sample_db: ReleaseDatabase) {
    let tmp = tempfile::tempdir().unwrap();
    let version = VersionSpec::Latest;
    let (exists, ver) = version_exists(&sample_db, &version, tmp.path()).unwrap();
    assert_eq!(ver, "v22.0.0");
    assert_eq!(exists, false);
  }

  #[rstest]
  fn version_exists_lts(sample_db: ReleaseDatabase) {
    let tmp = tempfile::tempdir().unwrap();
    let version = VersionSpec::Lts;
    let (exists, ver) = version_exists(&sample_db, &version, tmp.path()).unwrap();
    assert_eq!(ver, "v20.0.0");
    assert_eq!(exists, false);
  }

  #[rstest]
  fn version_exists_exact_found(sample_db: ReleaseDatabase) {
    let tmp = tempfile::tempdir().unwrap();
    let version = VersionSpec::Exact("v18.0.0".to_string());
    let (exists, ver) = version_exists(&sample_db, &version, tmp.path()).unwrap();
    assert_eq!(ver, "v18.0.0");
    assert_eq!(exists, false);
  }

  #[rstest]
  fn version_exists_exact_with_local_dir(sample_db: ReleaseDatabase) {
    let tmp = tempfile::tempdir().unwrap();
    fs::create_dir(tmp.path().join("v18.0.0")).unwrap();
    let version = VersionSpec::Exact("v18.0.0".to_string());
    let (exists, ver) = version_exists(&sample_db, &version, tmp.path()).unwrap();
    assert_eq!(ver, "v18.0.0");
    assert_eq!(exists, true);
  }

  #[rstest]
  fn version_exists_exact_not_in_db(sample_db: ReleaseDatabase) {
    let tmp = tempfile::tempdir().unwrap();
    let version = VersionSpec::Exact("v99.0.0".to_string());
    let result = version_exists(&sample_db, &version, tmp.path());
    assert!(result.is_err(), "expected error for non-existent version");
  }

  #[rstest]
  fn version_exists_latest_empty_db() {
    let db: ReleaseDatabase = serde_json::from_str("[]").unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let version = VersionSpec::Latest;
    let result = version_exists(&db, &version, tmp.path());
    assert!(result.is_err(), "expected error for empty database");
  }

  #[rstest]
  fn version_exists_lts_empty_db() {
    let db: ReleaseDatabase = serde_json::from_str("[]").unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let version = VersionSpec::Lts;
    let result = version_exists(&db, &version, tmp.path());
    assert!(result.is_err(), "expected error for empty database");
  }

  // ---------- delete_version ----------

  #[rstest]
  fn delete_version_removes_directory() {
    let tmp = tempfile::tempdir().unwrap();
    let ver_dir = tmp.path().join("v18.0.0");
    fs::create_dir(&ver_dir).unwrap();
    fs::write(ver_dir.join("node.exe"), "fake").unwrap();

    assert!(ver_dir.exists());
    delete_version(tmp.path(), "v18.0.0").unwrap();
    assert!(!ver_dir.exists());
  }

  #[rstest]
  fn delete_version_nonexistent_errors() {
    let tmp = tempfile::tempdir().unwrap();
    let result = delete_version(tmp.path(), "v99.0.0");
    assert!(result.is_err(), "expected error for non-existent version");
  }

  // ---------- display_or_update_root ----------

  #[rstest]
  fn display_or_update_root_with_file_path(config: Config) {
    let tmp = tempfile::tempdir().unwrap();
    let file_path = tmp.path().join("not_a_dir.txt");
    fs::write(&file_path, "test").unwrap();

    let result = display_or_update_root(config, Some(&file_path));
    assert!(result.is_err(), "expected error for file path");
    assert!(result.unwrap_err().to_string().contains("not a directory"));
  }

  #[rstest]
  fn display_or_update_root_with_valid_dir(config: Config) {
    let tmp = tempfile::tempdir().unwrap();
    let result = display_or_update_root(config, Some(tmp.path()));
    assert!(result.is_ok(), "expected success for valid directory");
  }

  #[rstest]
  fn display_or_update_root_none_with_root_set(config: Config) {
    let result = display_or_update_root::<&str>(config, None);
    assert!(result.is_ok(), "expected success when displaying current root");
  }

  #[rstest]
  fn display_or_update_root_none_with_no_root() {
    let config = Config {
      root: None,
      ..Default::default()
    };
    let result = display_or_update_root::<&str>(config, None);
    assert!(result.is_ok(), "expected success when root is None");
  }

  // ---------- VERSION_REGEX ----------

  #[rstest]
  #[case("v1.0.0", true)]
  #[case("v18.16.0", true)]
  #[case("v24.2.2", true)]
  #[case("1.0.0", false)]
  #[case("v1.0", false)]
  #[case("v1.0.0.0", false)]
  #[case("vX.Y.Z", false)]
  #[case("", false)]
  fn version_regex_test(#[case] input: &str, #[case] expect_match: bool) {
    assert_eq!(VERSION_REGEX.is_match(input), expect_match);
  }

  // ---------- NVM_SYMLINK_REGEX ----------

  #[rstest]
  #[case("C:\\path;%NVM_SYMLINK%;", true)]
  #[case("C:\\path;%nvm_symlink%;", true)]
  #[case("C:\\path\\nodejs", false)]
  #[case("%NVM_SYMLINK%", true)]
  fn nvm_symlink_regex_test(#[case] input: &str, #[case] expect_match: bool) {
    assert_eq!(NVM_SYMLINK_REGEX.is_match(input), expect_match);
  }

  // ---------- fixture: sample_db ----------

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
}

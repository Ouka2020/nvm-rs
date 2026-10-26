#[cfg(feature = "i18n")]
pub mod i18n;
pub mod model;

use std::{
  env::{self},
  ffi::c_void,
  fs::{self, File},
  io::{self, BufReader, Read, Seek},
  os::windows::ffi::OsStrExt,
  path::{Path, PathBuf},
  time::Duration,
};

use anyhow::{anyhow, bail};
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
  enums::{HKEY_CURRENT_USER, KEY_READ, KEY_WRITE, RegType},
};
use zip::ZipArchive;

use std::sync::LazyLock;

use crate::model::{ArchSpec, Config, ReleaseDatabase, VersionSpec};

#[cfg(feature = "debug")]
static LOG_GUARD: std::sync::OnceLock<
  tracing_appender::non_blocking::WorkerGuard,
> = std::sync::OnceLock::new();

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

/// Translate a message by its id from the loaded fluent bundle.
///
/// - `ts!("fallback", "key")` — look up the message by key; when the `i18n`
///   feature is disabled, return the fallback text as-is.
/// - `ts!("fallback", "key", name = value)` — interpolate named arguments
///   into placeholders such as `{$name}` declared in `*.ftl`.
/// - `ts!("fallback", "key", value1, value2)` — positional arguments are
///   mapped to numeric indices (`"0"`, `"1"`, …) for Fluent, or passed to
///   `format!` positionally without the feature.
///
/// # Argument order
///
/// The fallback text comes FIRST and the key SECOND — the reverse of most
/// lookup APIs. Double-check the order when writing or migrating calls.
///
/// # format! limitations
///
/// Without the `i18n` feature the fallback text is fed to `format!`:
///
/// - the `{}` placeholders must match the value count and order, or the
///   call panics at runtime;
/// - both `{name}` named placeholders (with `name = value` arguments) and
///   `{}` positional placeholders work correctly in the fallback text;
/// - literal braces in the output must be escaped as `{{` and `}}`.
///
/// The key must match a message id in the .ftl resource; the call panics if
/// it is missing from the bundle (i18n builds only).
#[macro_export]
#[cfg(feature = "i18n")]
macro_rules! ts {
  // No arguments: ts!("fallback", "key")
  ($msg:literal, $key:literal $(,)?) => {{
    let t = $crate::i18n::t();
    t.get_message($key)
  }};

  // Named arguments: ts!("fallback", "key", name = value)
  ($msg:literal, $key:literal, $($arg_key:ident = $value:expr),+ $(,)?) => {{
    let t = $crate::i18n::t();
    let mut args = fluent::FluentArgs::new();
    $(args.set(stringify!($arg_key), $value);)+
    t.get_message_with_args($key, &args)
  }};

  // Positional arguments: ts!("fallback {}", "key", value1, value2)
  // Each value is stored under its numeric index ("0", "1", …).
  ($msg:literal, $key:literal, $($value:expr),+ $(,)?) => {{
    let t = $crate::i18n::t();
    let mut args = fluent::FluentArgs::new();
    {
      let mut _idx: u32 = 0;
      $(
        args.set(_idx.to_string(), $value);
        _idx += 1;
      )+
    }
    t.get_message_with_args($key, &args)
  }};
}

#[macro_export]
#[cfg(not(feature = "i18n"))]
macro_rules! ts {
  // Named arguments: ts!("fallback {name}", "key", name = value)
  ($msg:literal, $key:literal, $($arg_key:ident = $value:expr),+ $(,)?) => {{
    format!($msg, $($arg_key = $value),+)
  }};

  // No arguments or positional arguments — both collapse to the same
  // behaviour: `format!` with zero-or-more positional values.
  ($msg:literal, $key:literal $(, $value:expr)* $(,)?) => {{
    format!($msg $(, $value)*)
  }};
}

/// Print a translated message via [`ts!`].
///
/// Accepts the same forms as [`ts!`]; see that macro for details.
macro_rules! tp {
  ($($args:tt)*) => {
    println!("{}", ts!($($args)*));
  };
}

pub fn activate_symlink(config: Config) -> Result {
  let symlink = get_env_symlink()?;
  if let Ok(metadata) = fs::symlink_metadata(&symlink)
    && metadata.is_symlink()
  {
    tp!(
      "node is already activated.",
      "activate_symlink_is_activated"
    );

    return Ok(());
  }

  let root = get_root(&config)?;
  let versions = get_local_versions(&root)?;
  let Some(last) = versions.last() else {
    bail!(ts!(
      "no node version installed",
      "activate_symlink_no_version"
    ));
  };

  create_junction_link(symlink, root.join(last))?;

  tp!(
    "node {version} is activated.",
    "activate_symlink_success",
    version = last.as_str()
  );

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
  let symlink = get_env_symlink()?;
  let Ok(_) = fs::symlink_metadata(&symlink) else {
    tp!(
      "node is already deactivated.",
      "deactivate_symlink_is_deactivated"
    );
    return Ok(());
  };
  delete_junction_link(symlink)?;

  tp!("node is deactivated.", "deactivate_symlink_success");

  Ok(())
}

fn delete_junction_link<T>(link: T) -> Result
where
  T: AsRef<Path>,
{
  let path = link.as_ref();
  log::debug!("delete link: {:?}", path);

  fs::remove_dir(path)?;

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
    tp!(
      "current version is {version}",
      "display_current_version",
      version = current_version.as_str()
    );
  } else {
    tp!("No current version.", "display_current_none");
    println!();
    tp!("Tips:", "display_current_tips");
    tp!(
      " * Run 'nvm use x.x.x' to set a version.",
      "display_current_tips_use"
    );
    tp!(
      " * Run 'nvm ls' to list available versions locally.",
      "display_current_tips_ls_local"
    );
    tp!(
      " * Run 'nvm ls --remote' to list available versions online.",
      "display_current_tips_ls_remote"
    );
    tp!(
      " * Run 'nvm install latest' to install the latest version.",
      "display_current_tips_install_latest"
    );
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
    tp!(
      "Current NpmMirror: {mirror}",
      "npm_mirror_current",
      mirror = npm_mirror.as_str()
    );
  } else {
    tp!("No NpmMirror set.", "npm_mirror_none");
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
    tp!(
      "Current NodeMirror: {mirror}",
      "node_mirror_current",
      mirror = node_mirror.as_str()
    );
  } else {
    tp!("No NodeMirror set.", "node_mirror_none");
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
      bail!(ts!(
        "path {path} does not exist",
        "root_path_not_exist",
        path = p.display().to_string()
      ));
    }
    if !p.is_dir() {
      bail!(ts!(
        "path {path} is not a directory",
        "root_path_not_dir",
        path = p.display().to_string()
      ));
    }
    config.root = Some(p.to_path_buf());
    config.save()?;
    tp!(
      "Current Root: {root}",
      "root_current",
      root = p.display().to_string()
    );
  } else if let Some(root) = config.root {
    tp!(
      "Current Root: {root}",
      "root_current",
      root = root.display().to_string()
    );
  } else {
    tp!("No Root set.", "root_none");
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
  let symlink_default = if let Ok(path) = get_env_symlink() {
    path.to_string_lossy().to_string()
  } else {
    r"C:\Program Files\nodejs".to_string()
  };
  let node_store_root_default = if let Some(r) = &config.root {
    r.to_string_lossy().to_string()
  } else {
    config.dir.data_local_dir().to_string_lossy().to_string()
  };

  let link_path =
    inquire::Text::new("Which directory to use as the symlink path?")
      .with_default(&symlink_default)
      .with_help_message("Tip: The symlink must not already exist.")
      .with_validator(symlink_validator)
      .prompt()?;

  let node_store_root =
    inquire::Text::new("Which directory to use as the node store root?")
      .with_default(&node_store_root_default)
      // .with_help_message("Tip: The node store root must not already exist.")
      .with_validator(node_store_root_validator)
      .prompt()?;

  let node_mirror = node_mirror_prompt(
    config.node_mirror,
    vec!["https://npmmirror.com/mirrors/node/"],
  )?;

  let npm_mirror = npm_mirror_prompt(
    config.npm_mirror,
    vec!["https://npmmirror.com/mirrors/npm/"],
  )?;

  std::fs::create_dir_all(&node_store_root)?;
  config.root = Some(node_store_root.into());
  config.node_mirror = node_mirror;
  config.npm_mirror = npm_mirror;

  update_environment(&link_path)?;

  log::debug!("config: {:?}", config);
  config.save()?;

  tp!(
    "Finished. Please restart the console to update the environment.",
    "setup_finished"
  );

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

fn mirror_index(list: &Vec<&str>, url: Option<Url>) -> usize {
  let Some(url) = url else {
    return 0;
  };
  let url = url.as_str();

  match list.iter().position(|&x| x == url) {
    Some(index) => index + 1,
    None => 0,
  }
}

fn node_mirror_prompt(
  default_url: Option<Url>,
  mirror_list: Vec<&str>,
) -> Result<Option<Url>> {
  // let default_mirror_list = vec!["https://npmmirror.com/mirrors/node/"];

  // let node_mirror_list = mirror_list.unwrap_or(default_mirror_list);
  let index = mirror_index(&mirror_list, default_url);
  // let index = default_url
  //   .as_ref()
  //   .and_then(|url| mirror_list.iter().position(|&x| x == url.as_str()))
  //   .map(|v| v + 1);

  // if let Some(url) = default_url {
  //   let index = node_mirror_list.iter().position(|&x| x == url.as_str());
  // }

  mirror_prompt(
    mirror_list,
    index,
    "Which mirror to use for nodejs?",
    "Please input the node mirror address:",
  )
}

fn npm_mirror_prompt(
  default_url: Option<Url>,
  mirror_list: Vec<&str>,
) -> Result<Option<Url>> {
  // let default_mirror_list = vec!["https://npmmirror.com/mirrors/npm/"];
  // let npm_mirror_list = mirror_list.unwrap_or(default_mirror_list);

  let index = mirror_index(&mirror_list, default_url);
  // let index = default_url
  //   .as_ref()
  //   .and_then(|url| mirror_list.iter().position(|&x| x == url.as_str()))
  //   .map(|v| v + 1);

  mirror_prompt(
    mirror_list,
    index,
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
  mut options: Vec<&str>,
  default_index: usize,
  select_prompt: &str,
  text_prompt: &str,
) -> Result<Option<Url>> {
  // Pre-allocate capacity to avoid reallocation
  options.reserve(2);
  // Insert "none" at the head
  options.insert(0, "none");
  // Append "custom" at the tail
  options.push("custom");

  let mirror = inquire::Select::new(select_prompt, options)
    .with_starting_cursor(default_index)
    .prompt()?;
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
  let Ok(symlink) = get_env_symlink() else {
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

fn get_env_symlink() -> Result<PathBuf> {
  env::var("NVM_SYMLINK")
    .map(PathBuf::from)
    .map_err(|e| anyhow!("get nvm symlink failed: {:?}", e))
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
    bail!(ts!(
      "version {version} already installed",
      "install_version_exists",
      version = ver.as_str()
    ));
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
    bail!(ts!(
      "invalid url: {url}",
      "install_invalid_url",
      url = url.as_str()
    ));
  };
  let mut zip_path = tempfile::tempfile()?;

  download_file(&url, &mut zip_path)?;

  print!("{}", ts!("checksum...", "checksum_prompt"));
  if !skip_checksum {
    let url = config.get_node_url(&[&ver, "SHASUMS256.txt"]);
    log::debug!("url: {:?}", url);
    let checksum = load_checksum(&url, &ver, &arch)?;
    let valid = file_validate(&zip_path, &checksum)?;
    if !valid {
      tp!("failed.", "checksum_failed");
      let msg = ts!(
        "sha256 checksum failed: {file}",
        "install_checksum_failed",
        file = file_name
      );
      log::error!("{}", msg);
      bail!(msg);
    }

    tp!("OK.", "checksum_ok");
  } else {
    tp!("skip.", "checksum_skip");
  }

  zip_extract(&zip_path, &root)?;

  let org_path = &root.join(file_name).with_extension("");
  let dist_path = &root.join(&ver);
  log::debug!("rename {:?} -> {:?}", org_path, dist_path);
  fs::rename(org_path, dist_path)?;

  tp!(
    "install {version} completed.",
    "install_success",
    version = ver.as_str()
  );

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

  bail!(ts!(
    "checksum not found: {package}",
    "checksum_not_found",
    package = package_name.as_str()
  ));
}

pub fn list_local_versions(config: Config) -> Result {
  let current_version = if let Some(version) = get_current_version() {
    version
  } else {
    "".to_string()
  };
  log::debug!("current version: {}", current_version);

  let path = get_root(&config)?;
  let versions = get_local_versions(path)?;

  if versions.is_empty() {
    tp!("No versions are installed.", "list_local_none");
  } else {
    println!();

    for version in versions {
      log::debug!("found version: {version}");

      print!("    {}", version);
      if current_version == version {
        tp!(" <- In use", "list_local_in_use");
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

  println!();
  tp!(
    " * Note: The list only shows the latest {count} versions. Visit https://nodejs.org/en/ for more info.",
    "list_remote_note",
    count = LIST_COUNT
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
/// 构建过滤规则
fn build_log_filter(extend_filters: Option<&str>) -> String {
  let base_filter = "info";
  #[cfg(debug_assertions)]
  let filter = if let Some(filters) = extend_filters {
    format!("{},{}", base_filter, filters)
  } else {
    base_filter.to_string()
  };
  #[cfg(not(debug_assertions))]
  let filter = base_filter.to_string();

  #[cfg(debug_assertions)]
  eprintln!("log_filter: {}", filter);

  filter
}

#[cfg(feature = "debug")]
pub fn log_init(extend_filters: Option<&str>) {
  use tracing_appender::rolling::{RollingFileAppender, Rotation};
  use tracing_subscriber::{
    EnvFilter, Layer, fmt, layer::SubscriberExt, util::SubscriberInitExt,
  };

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
  let log_filter = build_log_filter(extend_filters);
  // Console: colored, human-readable, INFO and above
  let console_layer = fmt::layer()
    .with_writer(std::io::stderr)
    .pretty()
    .with_filter(EnvFilter::new(&log_filter));

  // File: no color, with timestamps, DEBUG and above (more detailed)
  let file_layer = fmt::layer()
    .with_writer(non_blocking_file)
    .with_ansi(false)
    .with_target(true)
    .with_line_number(true)
    .with_filter(EnvFilter::new(log_filter));

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
    bail!(ts!(
      "illegal path in archive: {path}",
      "extract_illegal_path",
      path = entry.name()
    ));
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
    bail!(ts!(
      "version {version} not installed",
      "switch_version_not_installed",
      version = ver.as_str()
    ));
  }
  if let Some(current_ver) = current_ver
    && current_ver == ver
  {
    bail!(ts!(
      "version {version} is already used",
      "switch_version_already_used",
      version = ver.as_str()
    ));
  }

  log::debug!("ready switch to {}({})", ver, arch);

  let node_path = root.join(&ver).clean();

  reset_junction_link(get_env_symlink()?, node_path)?;

  tp!(
    "switch to {version} success.",
    "switch_success",
    version = ver.as_str()
  );

  Ok(())
}

pub fn uninstall_version(config: Config, version: VersionSpec) -> Result {
  let root = get_root(&config)?;
  let versions = get_local_versions(&root)?;
  log::debug!("local versions: {:?}", versions);

  match version {
    VersionSpec::Latest | VersionSpec::Lts => {
      bail!(ts!(
        "The version must be a specific version. Can not use 'latest' or 'lts' to uninstall.",
        "uninstall_need_exact_version"
      ));
    }
    VersionSpec::Exact(ver) => {
      if !versions.contains(&ver) {
        bail!(ts!(
          "version {version} not installed",
          "uninstall_version_not_installed",
          version = ver.as_str()
        ));
      }
      if let Some(current_ver) = get_current_version()
        && current_ver == ver
      {
        bail!(ts!(
          "version {version} is in use, can not uninstall it",
          "uninstall_version_in_use",
          version = ver.as_str()
        ));
      }

      delete_version(&root, &ver)?;

      tp!(
        "uninstall {version} completed.",
        "uninstall_success",
        version = ver.as_str()
      );
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
        bail!(ts!("No latest version found.", "version_latest_not_found"))
      }
    }
    VersionSpec::Lts => {
      if let Some(lts_version) = db.latest_lts() {
        lts_version
      } else {
        bail!(ts!("No LTS version found.", "version_lts_not_found"))
      }
    }
    VersionSpec::Exact(s) => {
      if db.version_exists(s) {
        s.clone()
      } else {
        bail!(ts!(
          "version {version} not exists.",
          "version_not_exists",
          version = s.as_str()
        ))
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

fn update_environment(symlink: &str) -> Result {
  // let symlink = symlink.as_ref().to_string_lossy().to_string();
  let root = RegKey::predef(HKEY_CURRENT_USER);
  let env = root.open_subkey_with_flags("Environment", KEY_READ | KEY_WRITE)?;
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
mod tests {
  use super::*;
  #[allow(unused_imports)]
  use pretty_assertions::{assert_eq, assert_ne};
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

  // ---------- tr ----------

  // The i18n path panics on missing keys, so only the non-i18n fallback
  // forms are testable offline.
  #[cfg(not(feature = "i18n"))]
  #[test]
  fn ts_fallback_test() {
    assert_eq!(ts!("plain fallback", "any_key"), "plain fallback");
  }

  // ---------- en-US resource ----------

  // The en-US resource must parse and resolve; a syntax error or missing
  // key would panic i18n builds on English systems at runtime.
  #[cfg(feature = "i18n")]
  #[test]
  fn en_us_resource_test() {
    let i18n = crate::i18n::I18n::init_with_lang_id("en-US");
    assert_eq!(
      i18n.get_message("activate_symlink_no_version"),
      "no node version installed"
    );
    assert_eq!(i18n.get_message("checksum_ok"), "OK.");
    // The StringLiteral placeable must preserve the leading space.
    assert_eq!(i18n.get_message("list_local_in_use"), " <- In use");
    assert_eq!(
      i18n.get_message("display_current_tips_use"),
      " * Run 'nvm use x.x.x' to set a version."
    );
    assert_eq!(
      i18n.get_message("version_latest_not_found"),
      "No latest version found."
    );

    // Region-less English locale (e.g. "en") must resolve too.
    let i18n = crate::i18n::I18n::init_with_lang_id("en");
    assert_eq!(i18n.get_message("list_local_in_use"), " <- In use");
    assert_eq!(
      i18n.get_message("version_latest_not_found"),
      "No latest version found."
    );
  }

  // ---------- ts! positional args ----------

  // Verify that positional arguments are mapped to numeric indices
  // correctly ("0", "1", "2") — not stringified expressions.
  #[cfg(feature = "i18n")]
  #[test]
  fn ts_positional_args_test() {
    let result = ts!(
      "fallback: {} {} {}",
      "test_positional_args",
      "alpha",
      "beta",
      "gamma"
    );
    assert_eq!(result, "first: alpha, second: beta, third: gamma");
  }

  // ---------- get_nvm_symlink ----------

  #[rstest]
  fn get_nvm_symlink_test() {
    let saved = env::var("NVM_SYMLINK").ok();
    unsafe { env::set_var("NVM_SYMLINK", r"C:\Program Files\nodejs") };

    let nvm_symlink = get_env_symlink().unwrap();
    assert_eq!(nvm_symlink, PathBuf::from(r"C:\Program Files\nodejs"));

    // Restore
    match saved {
      Some(val) => unsafe { env::set_var("NVM_SYMLINK", val) },
      None => unsafe { env::remove_var("NVM_SYMLINK") },
    }
  }

  // SAFETY: This test is not run in parallel with other tests that
  // depend on NVM_SYMLINK (single-threaded test harness by default).
  #[rstest]
  fn get_nvm_symlink_fails_without_env_var() {
    // Save and remove the env var for the duration of this test.
    let saved = env::var("NVM_SYMLINK").ok();
    // SAFETY: no other thread should be reading/writing this env var.
    unsafe { env::remove_var("NVM_SYMLINK") };

    let result = get_env_symlink();
    assert!(result.is_err());
    assert!(
      result
        .unwrap_err()
        .to_string()
        .contains("get nvm symlink failed")
    );

    // Restore the env var if it was set before.
    if let Some(val) = saved {
      unsafe { env::set_var("NVM_SYMLINK", val) };
    }
  }

  // ---------- get_root ----------

  #[rstest]
  fn get_root_test(config: Config) {
    let root = get_root(&config).unwrap();
    assert_eq!(root, PathBuf::from("nvmroot"));
  }

  #[rstest]
  fn get_root_with_none_falls_back_to_current_dir(config: Config) {
    let mut cfg = config;
    cfg.root = None;
    let root = get_root(&cfg).unwrap();
    assert_eq!(root, env::current_dir().unwrap());
  }

  // ---------- get_current_version / display_current ----------

  #[rstest]
  fn get_current_version_test() {
    let saved = env::var("NVM_SYMLINK").ok();
    let tmp = tempfile::tempdir().unwrap();
    // Point to a directory that exists but has no node.exe
    unsafe { env::set_var("NVM_SYMLINK", tmp.path()) };

    // Should return None because there's no node.exe in the temp dir
    let result = get_current_version();
    assert!(result.is_none());

    // Restore
    match saved {
      Some(val) => unsafe { env::set_var("NVM_SYMLINK", val) },
      None => unsafe { env::remove_var("NVM_SYMLINK") },
    }
  }

  #[rstest]
  fn display_current_test() {
    let saved = env::var("NVM_SYMLINK").ok();
    let tmp = tempfile::tempdir().unwrap();
    unsafe { env::set_var("NVM_SYMLINK", tmp.path()) };

    assert!(display_current().is_ok());

    match saved {
      Some(val) => unsafe { env::set_var("NVM_SYMLINK", val) },
      None => unsafe { env::remove_var("NVM_SYMLINK") },
    }
  }

  // ---------- deactivate / activate ----------

  #[rstest]
  fn deactivate_and_activate_version_test() {
    let saved = env::var("NVM_SYMLINK").ok();
    let tmp = tempfile::tempdir().unwrap();
    let symlink_path = tmp.path().join("nodejs");
    let root = tmp.path().join("root");
    fs::create_dir_all(&root).unwrap();
    // Create a fake version directory
    let ver_dir = root.join("v18.0.0");
    fs::create_dir_all(&ver_dir).unwrap();

    unsafe { env::set_var("NVM_SYMLINK", &symlink_path) };

    // deactivate when no symlink exists -> should succeed
    assert!(deactivate_symlink().is_ok());

    // activate with a version installed
    let config = Config {
      root: Some(root.clone()),
      ..Default::default()
    };
    assert!(activate_symlink(config).is_ok());

    // The symlink (junction) should now exist
    assert!(symlink_path.exists());

    // deactivate again -> should succeed
    assert!(deactivate_symlink().is_ok());

    // Restore
    match saved {
      Some(val) => unsafe { env::set_var("NVM_SYMLINK", val) },
      None => unsafe { env::remove_var("NVM_SYMLINK") },
    }
  }

  // ---------- install / uninstall ----------

  #[rstest]
  fn install_version_network_error() {
    let tmp = tempfile::tempdir().unwrap();
    let config = Config {
      root: Some(tmp.path().to_path_buf()),
      node_mirror: Some(Url::parse("http://localhost:1/").unwrap()), // unreachable
      ..Default::default()
    };
    let ver = VersionSpec::Exact("v23.0.0".to_string());
    let result = install_version(config, ver, false);
    assert!(result.is_err(), "expected network error");
  }

  // ---------- list_local_versions / list_remote_versions ----------

  #[rstest]
  fn list_local_versions_empty() {
    let saved = env::var("NVM_SYMLINK").ok();
    let tmp = tempfile::tempdir().unwrap();
    unsafe { env::set_var("NVM_SYMLINK", tmp.path()) };

    let config = Config {
      root: Some(tmp.path().to_path_buf()),
      ..Default::default()
    };
    assert!(list_local_versions(config).is_ok());

    match saved {
      Some(val) => unsafe { env::set_var("NVM_SYMLINK", val) },
      None => unsafe { env::remove_var("NVM_SYMLINK") },
    }
  }

  #[rstest]
  fn list_local_versions_with_versions() {
    let saved = env::var("NVM_SYMLINK").ok();
    let tmp = tempfile::tempdir().unwrap();
    unsafe { env::set_var("NVM_SYMLINK", tmp.path()) };

    // Create fake version dirs
    fs::create_dir(tmp.path().join("v18.0.0")).unwrap();
    fs::create_dir(tmp.path().join("v20.0.0")).unwrap();

    let config = Config {
      root: Some(tmp.path().to_path_buf()),
      ..Default::default()
    };
    assert!(list_local_versions(config).is_ok());

    match saved {
      Some(val) => unsafe { env::set_var("NVM_SYMLINK", val) },
      None => unsafe { env::remove_var("NVM_SYMLINK") },
    }
  }

  #[rstest]
  fn list_remote_versions_network_error() {
    let config = Config {
      root: None,
      node_mirror: Some(Url::parse("http://localhost:1/").unwrap()),
      ..Default::default()
    };
    let result = list_remote_versions(config);
    assert!(result.is_err(), "expected network error");
  }

  // ---------- mirror display ----------

  #[rstest]
  fn display_npm_mirror_test(config: Config) {
    assert!(display_or_update_npm_mirror(config.clone(), None).is_ok());
  }

  #[rstest]
  fn display_node_mirror_test(config: Config) {
    assert!(display_or_update_node_mirror(config.clone(), None).is_ok());
  }

  #[rstest]
  fn display_or_update_npm_mirror_with_url(config: Config) {
    let url =
      Url::parse("https://registry.npmmirror.com/-/binary/npm/").unwrap();
    assert!(display_or_update_npm_mirror(config, Some(url)).is_ok());
  }

  #[rstest]
  fn display_or_update_node_mirror_with_url(config: Config) {
    let url = Url::parse("https://cdn.npmmirror.com/binaries/node/").unwrap();
    assert!(display_or_update_node_mirror(config, Some(url)).is_ok());
  }

  // ---------- display_root ----------

  #[rstest]
  fn display_root_test(config: Config) {
    // display current root
    assert!(display_or_update_root::<&str>(config.clone(), None).is_ok());
    // error: non-existent path
    let r = display_or_update_root(
      config.clone(),
      Some(PathBuf::from("nonexistent_dir_xyz")),
    );
    assert!(r.is_err());
  }

  // ---------- switch_version ----------

  #[rstest]
  fn switch_version_not_installed_errors() {
    let tmp = tempfile::tempdir().unwrap();
    let config = Config {
      root: Some(tmp.path().to_path_buf()),
      node_mirror: Some(Url::parse("http://localhost:1/").unwrap()),
      ..Default::default()
    };
    let ver = VersionSpec::Exact("v24.2.0".to_string());
    let result = switch_version(config, ver);
    assert!(result.is_err());
  }

  // ---------- uninstall_version boundary tests ----------

  #[rstest]
  fn uninstall_version_latest_errors() {
    let tmp = tempfile::tempdir().unwrap();
    let config = Config {
      root: Some(tmp.path().to_path_buf()),
      ..Default::default()
    };
    let result = uninstall_version(config, VersionSpec::Latest);
    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(
      err_msg.contains("latest") || err_msg.contains("lts"),
      "unexpected error message: {err_msg}"
    );
  }

  #[rstest]
  fn uninstall_version_lts_errors() {
    let tmp = tempfile::tempdir().unwrap();
    let config = Config {
      root: Some(tmp.path().to_path_buf()),
      ..Default::default()
    };
    let result = uninstall_version(config, VersionSpec::Lts);
    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(
      err_msg.contains("latest") || err_msg.contains("lts"),
      "unexpected error message: {err_msg}"
    );
  }

  #[rstest]
  fn uninstall_version_not_installed_errors() {
    let tmp = tempfile::tempdir().unwrap();
    let config = Config {
      root: Some(tmp.path().to_path_buf()),
      ..Default::default()
    };
    let result =
      uninstall_version(config, VersionSpec::Exact("v99.99.99".to_string()));
    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(
      err_msg.contains("not installed") || err_msg.contains("未安装"),
      "unexpected error message: {err_msg}"
    );
  }

  // ---------- delete_junction_link ----------

  #[rstest]
  fn delete_junction_link_nonexistent_is_ok(_config: Config) {
    let tmp = tempfile::tempdir().unwrap();
    let nonexistent = tmp.path().join("does_not_exist");
    // Deleting a non-existent link should still succeed (the function
    // intentionally ignores errors from `remove_dir`).
    assert!(delete_junction_link(&nonexistent).is_ok());
  }

  // ---------- safe_extract ----------

  /// Helper: create a zip archive on disk with the given entry names and
  /// optional content.  Returns the path to the zip file inside a `TempDir`
  /// (the `TempDir` must be kept alive for the duration of the test).
  fn create_test_zip(
    entries: &[(&str, &[u8], bool)],
  ) -> (tempfile::TempDir, PathBuf) {
    let tmp = tempfile::tempdir().unwrap();
    let zip_path = tmp.path().join("test.zip");
    let file = File::create(&zip_path).unwrap();
    let mut zip = zip::ZipWriter::new(file);

    for &(name, data, is_dir) in entries {
      let options = zip::write::SimpleFileOptions::default();
      if is_dir {
        zip.add_directory(name, options).unwrap();
      } else {
        zip.start_file(name, options).unwrap();
        std::io::Write::write_all(&mut zip, data).unwrap();
      }
    }
    zip.finish().unwrap();
    (tmp, zip_path)
  }

  #[rstest]
  fn safe_extract_normal_file() {
    let (tmp, zip_path) =
      create_test_zip(&[("hello.txt", b"hello world", false)]);
    let dest = tmp.path().join("out");
    fs::create_dir(&dest).unwrap();

    let file = File::open(&zip_path).unwrap();
    let mut archive = ZipArchive::new(BufReader::new(file)).unwrap();
    let mut entry = archive.by_index(0).unwrap();
    safe_extract(&mut entry, &dest).unwrap();

    let extracted = dest.join("hello.txt");
    assert!(extracted.exists());
    assert_eq!(fs::read_to_string(&extracted).unwrap(), "hello world");
  }

  #[rstest]
  fn safe_extract_directory() {
    let (tmp, zip_path) = create_test_zip(&[
      ("subdir/", b"", true),
      ("subdir/file.txt", b"data", false),
    ]);
    let dest = tmp.path().join("out");
    fs::create_dir(&dest).unwrap();

    let file = File::open(&zip_path).unwrap();
    let mut archive = ZipArchive::new(BufReader::new(file)).unwrap();

    // Extract directory entry
    {
      let mut entry = archive.by_index(0).unwrap();
      safe_extract(&mut entry, &dest).unwrap();
    }
    assert!(dest.join("subdir").is_dir());

    // Extract file inside directory
    {
      let mut entry = archive.by_index(1).unwrap();
      safe_extract(&mut entry, &dest).unwrap();
    }
    assert_eq!(
      fs::read_to_string(dest.join("subdir/file.txt")).unwrap(),
      "data"
    );
  }

  #[rstest]
  fn safe_extract_zip_slip_detected() {
    let (tmp, zip_path) =
      create_test_zip(&[("../../evil.txt", b"malicious", false)]);
    let dest = tmp.path().join("out");
    fs::create_dir(&dest).unwrap();

    let file = File::open(&zip_path).unwrap();
    let mut archive = ZipArchive::new(BufReader::new(file)).unwrap();
    let mut entry = archive.by_index(0).unwrap();

    // The zip crate's `mangled_name()` strips leading `../`, so the
    // extraction should succeed but the file must land *inside* `dest`,
    // NOT outside it.
    safe_extract(&mut entry, &dest).unwrap();

    // The evil file must NOT exist outside the destination directory.
    let escaped = tmp.path().join("evil.txt");
    assert!(!escaped.exists(), "Zip Slip: file escaped destination");

    // It should have been sanitized into the destination directory.
    let sanitized = dest.join("evil.txt");
    assert!(
      sanitized.exists(),
      "sanitized file should exist inside dest"
    );
  }

  #[rstest]
  fn safe_extract_nested_dirs() {
    let (tmp, zip_path) = create_test_zip(&[
      ("a/", b"", true),
      ("a/b/", b"", true),
      ("a/b/c/", b"", true),
      ("a/b/c/deep.txt", b"deep content", false),
    ]);
    let dest = tmp.path().join("out");
    fs::create_dir(&dest).unwrap();

    let file = File::open(&zip_path).unwrap();
    let mut archive = ZipArchive::new(BufReader::new(file)).unwrap();

    for i in 0..archive.len() {
      let mut entry = archive.by_index(i).unwrap();
      safe_extract(&mut entry, &dest).unwrap();
    }

    let deep_file = dest.join("a/b/c/deep.txt");
    assert!(deep_file.exists());
    assert_eq!(fs::read_to_string(&deep_file).unwrap(), "deep content");
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

    assert!(file_validate(&tmp, &expected_hash).unwrap());
  }

  #[rstest]
  fn file_validate_incorrect_checksum() {
    let content = b"hello world";
    let mut tmp = tempfile::tempfile().unwrap();
    std::io::Write::write_all(&mut tmp, content).unwrap();

    assert!(!file_validate(&tmp, "deadbeef").unwrap());
  }

  #[rstest]
  fn file_validate_empty_file() {
    use sha2::Digest;
    let expected_hash = hex::encode(sha2::Sha256::digest(b""));

    let tmp = tempfile::tempfile().unwrap();
    assert!(file_validate(&tmp, &expected_hash).unwrap());
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
    assert_eq!(versions, vec!["v18.0.0", "v20.0.0", "v22.0.0"]);
  }

  // ---------- version_exists ----------

  #[rstest]
  fn version_exists_latest(sample_db: ReleaseDatabase) {
    let tmp = tempfile::tempdir().unwrap();
    let version = VersionSpec::Latest;
    let (exists, ver) =
      version_exists(&sample_db, &version, tmp.path()).unwrap();
    assert_eq!(ver, "v22.0.0");
    assert!(!exists);
  }

  #[rstest]
  fn version_exists_lts(sample_db: ReleaseDatabase) {
    let tmp = tempfile::tempdir().unwrap();
    let version = VersionSpec::Lts;
    let (exists, ver) =
      version_exists(&sample_db, &version, tmp.path()).unwrap();
    assert_eq!(ver, "v20.0.0");
    assert!(!exists);
  }

  #[rstest]
  fn version_exists_exact_found(sample_db: ReleaseDatabase) {
    let tmp = tempfile::tempdir().unwrap();
    let version = VersionSpec::Exact("v18.0.0".to_string());
    let (exists, ver) =
      version_exists(&sample_db, &version, tmp.path()).unwrap();
    assert_eq!(ver, "v18.0.0");
    assert!(!exists);
  }

  #[rstest]
  fn version_exists_exact_with_local_dir(sample_db: ReleaseDatabase) {
    let tmp = tempfile::tempdir().unwrap();
    fs::create_dir(tmp.path().join("v18.0.0")).unwrap();
    let version = VersionSpec::Exact("v18.0.0".to_string());
    let (exists, ver) =
      version_exists(&sample_db, &version, tmp.path()).unwrap();
    assert_eq!(ver, "v18.0.0");
    assert!(exists);
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
    let err_msg = result.unwrap_err().to_string();
    assert!(
      err_msg.contains("not a directory") || err_msg.contains("不是目录"),
      "unexpected error message: {err_msg}"
    );
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
    assert!(
      result.is_ok(),
      "expected success when displaying current root"
    );
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

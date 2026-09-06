mod command_args;
mod config;
mod model;
mod system_proxy;

use std::sync::OnceLock;

use clap::Parser;
use comfy_table::{Cell, CellAlignment, Table, presets};
use command_args::CliArgs;
#[cfg(feature = "debug")]
use tracing_appender::{
  non_blocking::WorkerGuard,
  rolling::{RollingFileAppender, Rotation},
};
#[cfg(feature = "debug")]
use tracing_subscriber::{
  EnvFilter, Layer, fmt, layer::SubscriberExt, util::SubscriberInitExt,
};

use crate::command_args::Commands;

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
  let config = config::Config::load();
  log::debug!("config: {:?}", config);

  match args.command {
    Commands::Install {
      version,
      arch,
      insecure,
    } => {
      println!("install: {:?}", version);
    }
    Commands::Uninstall { version } => todo!(),
    Commands::List { available } => {
      if available {
        list_remote_versions(config).await?;
      }
    }
    Commands::On => todo!(),
    Commands::Off => todo!(),
    Commands::Root { path } => todo!(),
    Commands::Arch => todo!(),
    Commands::Proxy { url } => todo!(),
    Commands::Current => todo!(),
    Commands::NodeMirror { url } => todo!(),
    Commands::NpmMirror { url } => todo!(),
    Commands::Use { version, arch } => todo!(),
  }

  Ok(())
}

async fn list_remote_versions(config: config::Config) -> Result {
  let url = config
    .node_mirror
    .unwrap_or("https://nodejs.org/dist/latest".to_string());
  let url = format!("{}/index.json", url);
  log::debug!("url: {}", url);

  let resp = reqwest::get(url).await?;
  let body = resp.text().await?;
  // log::debug!("body: {}", body);

  let node_release_info: Vec<model::NodeReleaseInfo> =
    serde_json::from_str(&body)?;
  log::debug!("node_release_info count: {}", node_release_info.len());

  let release_database = model::ReleaseDatabase::build(node_release_info);
  log::debug!("release_database: ");

  let latest_version = release_database
    .latest_list(LIST_COUNT)
    .iter()
    .map(|r| r.version.clone())
    .collect::<Vec<_>>();

  let lts_version = release_database
    .lts_list(LIST_COUNT)
    .iter()
    .map(|r| r.version.clone())
    .collect::<Vec<_>>();

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

  Ok(())
}

fn get_latest_version(info: &Vec<model::NodeReleaseInfo>) -> Result<String> {
  Ok(info[0].version.clone())
}

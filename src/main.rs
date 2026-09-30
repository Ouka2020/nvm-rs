#[cfg(not(target_os = "windows"))]
compile_error!("only windows is supported.");

use clap::Parser;
use nvm_windows::model::{CliArgs, Commands, Config};
use nvm_windows::{
  Result, activate_symlink, deactivate_symlink, display_current,
  display_or_update_node_mirror, display_or_update_npm_mirror,
  display_or_update_root, install_version, list_versions, setup,
  switch_version, uninstall_version,
};

#[cfg(feature = "debug")]
use nvm_windows::log_init;

fn main() -> Result<()> {
  #[cfg(feature = "debug")]
  log_init();

  nvm_windows::CURRENT_DIR
    .set(std::env::current_dir().unwrap())
    .unwrap();

  let dir = directories::ProjectDirs::from("", "", "nvm")
    .expect("fail to load app root.");
  std::fs::create_dir_all(dir.preference_dir())
    .expect("fail to create preference dir.");
  std::fs::create_dir_all(dir.cache_dir()).expect("fail to create cache dir.");
  nvm_windows::PROJECT_DIR.set(dir).unwrap();

  let args = CliArgs::parse();
  log::debug!("args: {:?}", args);
  let config = Config::load()?;
  log::debug!("config: {:?}", config);
  if !matches!(args.command, Commands::Setup) {
    config.is_valid()?;
  }

  match args.command {
    Commands::Setup => setup(config)?,
    Commands::Install {
      version,
      skip: insecure,
    } => install_version(config, version, insecure)?,
    Commands::Uninstall { version } => uninstall_version(config, version)?,
    Commands::List { remote: available } => list_versions(config, available)?,
    Commands::On => activate_symlink(config)?,
    Commands::Off => deactivate_symlink()?,
    Commands::Root { path } => display_or_update_root(config, path)?,
    Commands::Current => display_current()?,
    Commands::NodeMirror { url } => display_or_update_node_mirror(config, url)?,
    Commands::NpmMirror { url } => display_or_update_npm_mirror(config, url)?,
    Commands::Use { version } => switch_version(config, version)?,
  }

  Ok(())
}

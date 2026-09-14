use clap::Parser;
use nvm_windows::model::{CliArgs, Commands, Config};
use nvm_windows::{
  Result, activate_version, deactivate_version, display_architecture,
  display_current, display_or_update_node_mirror, display_or_update_npm_mirror,
  display_or_update_proxy, display_or_update_root, install_version,
  list_versions, switch_version, uninstall_version,
};

#[cfg(feature = "debug")]
use nvm_windows::log_init;

// #[tokio::main]
fn main() -> Result<()> {
  #[cfg(feature = "debug")]
  log_init();

  let args = CliArgs::parse();
  log::debug!("args: {:?}", args);
  let config = Config::load();
  log::debug!("config: {:?}", config);
  config.is_valid()?;

  match args.command {
    Commands::Install {
      version,
      arch,
      insecure,
    } => install_version(config, version, arch, insecure)?,
    Commands::Uninstall { version } => uninstall_version(config, version)?,
    Commands::List { available } => list_versions(config, available)?,
    Commands::On => activate_version(config)?,
    Commands::Off => deactivate_version()?,
    Commands::Root { path } => display_or_update_root(config, path)?,
    Commands::Arch => display_architecture(config)?,
    Commands::Proxy { url } => display_or_update_proxy(config, url)?,
    Commands::Current => display_current()?,
    Commands::NodeMirror { url } => display_or_update_node_mirror(config, url)?,
    Commands::NpmMirror { url } => display_or_update_npm_mirror(config, url)?,
    Commands::Use { version, arch } => switch_version(config, version, arch)?,
  }

  Ok(())
}

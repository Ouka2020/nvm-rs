#[cfg(not(windows))]
compile_error!("only windows is supported.");

use clap::FromArgMatches;
use nvm_windows::model::{CliArgs, Commands, Config};
use nvm_windows::{
  Result, activate_symlink, deactivate_symlink, display_current,
  display_or_update_node_mirror, display_or_update_npm_mirror,
  display_or_update_root, install_version, list_versions, setup,
  switch_version, uninstall_version,
};

#[cfg(feature = "debug")]
use nvm_windows::log_init;

use windows_sys::Win32::System::Console::{
  ATTACH_PARENT_PROCESS, AttachConsole, FreeConsole, SetConsoleOutputCP,
};

use windows_sys::Win32::Globalization::CP_UTF8;

fn main() -> Result<()> {
  // Attach to parent console for output
  unsafe {
    FreeConsole();
    AttachConsole(ATTACH_PARENT_PROCESS);
    SetConsoleOutputCP(CP_UTF8);
  }

  #[cfg(feature = "debug")]
  log_init(Some("nvm_windows=debug,nvm=debug"));

  let matches = CliArgs::command().get_matches();
  let args = CliArgs::from_arg_matches(&matches)?;
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

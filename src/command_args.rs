use clap::{Parser, Subcommand};

use crate::model::ArchSpec;

#[derive(Parser, Debug)]
#[command(version, about)]
pub struct CliArgs {
  #[command(subcommand)]
  pub command: Commands,
}

#[derive(Clone, Debug)]
pub enum VersionSpec {
  /// Latest LTS version
  Lts,
  /// Latest version
  Latest,
  Exact(#[allow(dead_code)] semver::Version), // TODO: remove dead code
}

// Clap 通过 FromStr 自动将其作为 value_parser
impl std::str::FromStr for VersionSpec {
  type Err = String;

  fn from_str(s: &str) -> Result<Self, Self::Err> {
    match s.to_lowercase().as_str() {
      "lts" => Ok(Self::Lts),
      "latest" => Ok(Self::Latest),
      v => semver::Version::parse(v).map(Self::Exact).map_err(|_| {
        format!(
          "'{s}' invalid. Possible values: <semver>(eg: 1.1.0), lts, latest."
        )
      }),
    }
  }
}

#[derive(Subcommand, Debug)]
pub enum Commands {
  /// The version can be a specific version, "latest" for the latest current version, or "lts" for the
  /// most recent LTS version. Optionally specify whether to install the 32 or 64 bit version (defaults
  /// to system arch). Set [arch] to "all" to install 32 AND 64 bit versions.
  /// Add --insecure to the end of this command to bypass SSL validation of the remote download server.
  #[command(visible_alias = "i")]
  Install {
    /// The version can be a specific version, "latest" for the latest current version, or "lts" for the
    /// most recent LTS version. [possible values: <semver>(eg: 1.1.0), lts, latest]
    version: VersionSpec,
    /// Specify whether to install the 32 or 64 bit version (defaults to system arch).
    arch: Option<ArchSpec>,
    /// Pass SSL validation of the remote download server.
    #[arg(short, long, default_value_t = false)]
    insecure: bool,
  },
  /// The version must be a specific version.
  Uninstall {
    /// The version to uninstall.
    version: VersionSpec,
  },
  /// List the node.js installations. Type "available" at the end to see what can be installed.
  #[command(visible_alias = "ls")]
  List {
    /// Show available versions.
    #[arg(short, long, default_value_t = false)]
    available: bool,
  },
  /// Enable node.js version management.
  On,
  /// Disable node.js version management.
  Off,
  /// Set the directory where nvm should store different versions of node.js.
  /// If <path> is not set, the current root will be displayed.
  Root {
    /// The path to set as the root directory.
    path: Option<String>,
  },
  /// Show if node is running in 32 or 64 bit mode.
  Arch,
  /// Set a proxy to use for downloads.
  Proxy {
    /// Leave [url] blank to see the current proxy.
    /// Set [url] to "none" to remove the proxy.
    url: Option<String>,
  },
  /// Display active version.
  Current,
  /// Set the node mirror. Defaults to https://nodejs.org/dist/. Leave [url] blank to use default url.
  NodeMirror {
    /// The node mirror to use. Leave [url] blank to use default url.
    url: Option<String>,
  },
  /// Set the npm mirror. Defaults to https://github.com/npm/cli/archive/. Leave [url] blank to use default url.
  NpmMirror {
    /// The npm mirror to use. Leave [url] blank to use default url.
    url: Option<String>,
  },
  /// Switch to use the specified version. Optionally use "latest", "lts", or "newest".
  /// "newest" is the latest installed version. Optionally specify 32/64bit architecture.
  /// nvm use <arch> will continue using the selected version, but switch to 32/64 bit mode.
  Use {
    /// The version to use.
    version: VersionSpec,
    /// The architecture to use.
    arch: Option<ArchSpec>,
  },
}

use std::fs;

use serde::{Deserialize, Serialize};

use crate::model::ArchSpec;

#[serde_with::skip_serializing_none]
#[derive(Debug, Default, Clone, Deserialize, Serialize)]
pub struct Config {
  pub root: Option<String>,
  // #[serde(skip_serializing_if = "Option::is_none")]
  pub proxy: Option<String>,
  // #[serde(skip_serializing_if = "Option::is_none")]
  pub node_mirror: Option<String>,
  // #[serde(skip_serializing_if = "Option::is_none")]
  pub npm_mirror: Option<String>,
  // #[serde(skip_serializing_if = "Option::is_none")]
  pub arch: Option<ArchSpec>,
  // #[serde(skip_serializing_if = "Option::is_none")]
  pub originalpath: Option<String>,
  // #[serde(skip_serializing_if = "Option::is_none")]
  pub originalversion: Option<String>,
  // #[serde(skip_serializing_if = "Option::is_none")]
  pub symlink: Option<String>,
}

// impl Default for Config {
//   fn default() -> Self {
//     Self {
//       root: "C:\\nvm".to_string(),
//       proxy: None,
//       node_mirror: None,
//       npm_mirror: None,
//       arch: None,
//       originalpath: None,
//       originalversion: None,
//       symlink: None,
//     }
//   }
// }

impl Config {
  /// Load config from file. Returns None if no config file exists.<br>
  /// If both toml and yaml features are enabled, it will try to load from toml first.
  /// If toml is not enabled, it will try to load from yaml.
  /// If yaml is not enabled, it will return None.
  pub fn load() -> Config {
    #[cfg(feature = "toml")]
    {
      let config = read_from_toml();
      if config.is_some() {
        return config.unwrap();
      }
    }

    #[cfg(feature = "yaml")]
    {
      let config = read_from_txt();
      if config.is_some() {
        return config.unwrap();
      }
    }

    Config::default()
  }
}

#[cfg(feature = "toml")]
fn read_from_toml() -> Option<Config> {
  if let Ok(v) = fs::exists("settings.toml")
    && v == true
  {
    let data = fs::read_to_string("settings.toml").unwrap();
    let config: Config = toml::from_str(&data).unwrap();
    return Some(config);
  }
  None
}

#[cfg(feature = "yaml")]
fn read_from_txt() -> Option<Config> {
  if let Ok(v) = fs::exists("settings.txt")
    && v == true
  {
    let data = fs::read_to_string("settings.txt").unwrap();
    let config: Config = noyalib::from_str(&data).unwrap();

    return Some(config);
  }

  None
}

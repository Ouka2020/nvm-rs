use winreg::{
  RegKey,
  enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ},
};

#[allow(dead_code)]
pub fn get() -> Option<Vec<String>> {
  get_env_proxy().or_else(get_registry_proxy)
}

fn get_registry_proxy() -> Option<Vec<String>> {
  get_registry_hklm_proxy().or_else(get_registry_hkcu_proxy)
}

fn get_registry_hklm_proxy() -> Option<Vec<String>> {
  let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
  let key = hklm
    .open_subkey_with_flags(
      r"SOFTWARE\Policies\Microsoft\Windows\CurrentVersion\Internet Settings",
      KEY_READ,
    )
    .unwrap();

  get_proxy_server(&key)
}

fn get_registry_hkcu_proxy() -> Option<Vec<String>> {
  let hkcu = RegKey::predef(HKEY_CURRENT_USER);
  let key = hkcu
    .open_subkey_with_flags(
      r"Software\Microsoft\Windows\CurrentVersion\Internet Settings",
      KEY_READ,
    )
    .unwrap();

  get_proxy_server(&key)
}

fn get_proxy_server(key: &RegKey) -> Option<Vec<String>> {
  let enabled: u32 = key.get_value("ProxyEnable").unwrap_or(0);
  if enabled == 0 {
    return None;
  }

  let server: String = key.get_value("ProxyServer").unwrap();

  let mut servers: Vec<String> = Vec::new();

  // http=host:port;https=host:port;ftp=host:port
  let servers = if server.contains(';') {
    server
      .split(';')
      .filter(|s| !s.is_empty())
      .map(|s| s.replace('=', "://"))
      .collect()
  } else if server.contains('=') {
    vec![server.replace('=', "://")]
  } else {
    vec![format!("http://{server}")]
  };

  Some(servers)
}

fn get_env_proxy() -> Option<Vec<String>> {
  get_env_hklm_proxy().or_else(get_env_hkcu_proxy)
}

fn get_env_hklm_proxy() -> Option<Vec<String>> {
  let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
  let key = hklm
    .open_subkey_with_flags(
      r"SYSTEM\CurrentControlSet\Control\Session Manager\Environment",
      KEY_READ,
    )
    .unwrap();

  get_env_proxy_server(&key)
}

fn get_env_hkcu_proxy() -> Option<Vec<String>> {
  let hkcu = RegKey::predef(HKEY_CURRENT_USER);
  let key = hkcu.open_subkey_with_flags("Environment", KEY_READ).ok()?;

  get_env_proxy_server(&key)
}

fn get_env_proxy_server(key: &RegKey) -> Option<Vec<String>> {
  let mut proxies: Vec<String> = Vec::new();

  if let Ok(p) = key.get_value("HTTP_PROXY") {
    proxies.push(p);
  }

  if let Ok(p) = key.get_value("HTTPS_PROXY") {
    proxies.push(p);
  }

  (!proxies.is_empty()).then_some(proxies)
}

#[cfg(test)]
mod tests {
  #[test]
  fn env_get() {
    if let Some(proxies) = super::get_env_proxy() {
      assert!(!proxies.is_empty());
    }
  }

  #[test]
  fn registry_get() {
    if let Some(proxies) = super::get_registry_proxy() {
      assert!(!proxies.is_empty());
    }
  }
}

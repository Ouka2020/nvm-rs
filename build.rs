fn main() {
  if std::env::var("CARGO_CFG_TARGET_OS").unwrap() == "windows" {
    let mut res = winresource::WindowsResource::new();
    if let Ok(true) = std::fs::exists("icon.ico") {
      res.set_icon("icon.ico");
    }
    if let Ok(manifest) = std::fs::read_to_string("manifest.xml") {
      res.set_manifest(&manifest);
    }
    res.compile().unwrap();
  }
}

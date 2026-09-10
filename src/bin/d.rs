fn main() -> anyhow::Result<()> {
  let key = std::env::var("NVM_SYMLINK")?;
  println!("key: {:?}", key);
  let s_meta = std::fs::symlink_metadata(&key)?;
  let d_meta = std::fs::metadata(&key)?;
  let link = std::fs::read_link(key)?;
  println!("是符号链接: {}, {}", s_meta.is_dir(), s_meta.is_symlink());
  println!("链接大小: {} bytes", s_meta.len());
  println!("是目录: {}", d_meta.is_dir());
  println!("目录大小: {} bytes", d_meta.len());
  println!("链接目标: {:?}", link.file_name());

  Ok(())
}

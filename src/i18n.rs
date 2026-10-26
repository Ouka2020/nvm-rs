use std::sync::OnceLock;
use std::{borrow::Cow, path::Path};
use windows_sys::Win32::Globalization::GetUserDefaultLocaleName;

pub(crate) static I18N: OnceLock<I18n> = OnceLock::new();

pub(crate) fn t() -> &'static I18n {
  I18N.get_or_init(I18n::init)
}

pub struct FluentI18nProvider<T = fluent::FluentResource> {
  inner: fluent::concurrent::FluentBundle<T>,
}

impl FluentI18nProvider {
  pub fn init(lang_id: &str) -> Self {
    let lang_id: unic_langid::LanguageIdentifier =
      lang_id.parse().expect("invalid language identifier");
    let source = Self::load_string(&lang_id);
    let mut bundle =
      fluent::concurrent::FluentBundle::new_concurrent(vec![lang_id.clone()]);
    if let Some(source) = source {
      let r = fluent::FluentResource::try_new(source).unwrap();
      bundle.add_resource(r).unwrap();
      log::debug!("add resource: {:?}", lang_id);
    } else {
      log::debug!("no resource: {:?}", lang_id);
    }
    Self { inner: bundle }
  }

  pub fn t(
    &self,
    key_id: &str,
    args: Option<&fluent::FluentArgs>,
  ) -> Cow<'_, str> {
    let p = self.inner.get_message(key_id).unwrap().value().unwrap();
    let mut errors = vec![];
    let o = self.inner.format_pattern(p, args, &mut errors);
    log::debug!("error: {:?}", errors);
    o
  }

  fn load_string(lang_id: &unic_langid::LanguageIdentifier) -> Option<String> {
    let file_name = if let Some(region) = lang_id.region {
      format!("{}-{}.ftl", lang_id.language, region)
    } else {
      format!("{}.ftl", lang_id.language)
    };
    let path = Path::new("locales").join(file_name);
    if path.exists() {
      std::fs::read_to_string(path).ok()
    } else {
      None
    }
  }
}

pub struct I18n {
  inner: FluentI18nProvider,
}

impl I18n {
  pub fn init() -> Self {
    I18n {
      inner: FluentI18nProvider::init(&get_locale()),
    }
  }

  pub fn init_with_lang_id(lang_id: &str) -> Self {
    I18n {
      inner: FluentI18nProvider::init(lang_id),
    }
  }

  pub fn get_message(&self, key_id: &str) -> Cow<'_, str> {
    self.inner.t(key_id, None)
  }

  pub fn get_message_with_args(
    &self,
    key_id: &str,
    args: &fluent::FluentArgs,
  ) -> Cow<'_, str> {
    self.inner.t(key_id, Some(args))
  }
}

fn get_locale() -> String {
  let mut buf = [0u16; 85];
  let l =
    unsafe { GetUserDefaultLocaleName(buf.as_mut_ptr(), buf.len() as i32) };
  if l > 0 {
    String::from_utf16_lossy(&buf[0..l as usize])
      .trim_end_matches('\0')
      .to_string()
  } else {
    "en".to_string()
  }
}

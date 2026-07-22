// Internationalization (i18n) helper module

pub fn tr<'a>(lang: &str, uk: &'a str, en: &'a str) -> &'a str {
    if lang == "en" {
        en
    } else {
        uk
    }
}

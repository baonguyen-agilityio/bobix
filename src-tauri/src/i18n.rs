// Strings the app itself shows (notifications, later the tray), read from the pages' own src/ui/i18n.js at build time,
// so both builds and the pages share one list and nothing is translated twice.
use std::{collections::HashMap, sync::LazyLock};

const SOURCE: &str = include_str!("../../src/ui/i18n.js");

// 'key': 'value' pairs of one language block ("    en: {" up to "    vi: {", "    vi: {" to the end).
fn block(lang: &str) -> HashMap<&'static str, String> {
    static PAIR: LazyLock<fancy_regex::Regex> = LazyLock::new(|| fancy_regex::Regex::new(r"'([A-Za-z0-9_.]+)':\s*'((?:[^'\\\n]|\\.)*)'").unwrap());
    let vi = SOURCE.find("\n    vi: {").unwrap_or(SOURCE.len());
    let b = if lang == "vi" { &SOURCE[vi..] } else { &SOURCE[..vi] };
    PAIR.captures_iter(b).flatten().map(|c| {
        let (k, v) = (c.get(1).unwrap().as_str(), c.get(2).unwrap().as_str());
        (k, v.replace("\\n", "\n").replace("\\'", "'").replace("\\\\", "\\"))
    }).collect()
}

static EN: LazyLock<HashMap<&'static str, String>> = LazyLock::new(|| block("en"));
static VI: LazyLock<HashMap<&'static str, String>> = LazyLock::new(|| block("vi"));

// t(lang, key, vars) as in i18n.js: the language's string, else English, else the key; {name} filled in.
pub fn t(lang: &str, key: &str, vars: &[(&str, &str)]) -> String {
    let s = if lang == "vi" { VI.get(key) } else { None }.or_else(|| EN.get(key)).cloned().unwrap_or_else(|| key.to_string());
    vars.iter().fold(s, |s, (k, v)| s.replace(&format!("{{{k}}}"), v))
}

#[cfg(test)]
mod tests {
    use super::t;

    #[test]
    fn strings_come_from_the_pages_list() {
        assert_eq!(t("en", "notify.waiting", &[("name", "shop")]), "shop needs you");
        assert_eq!(t("vi", "notify.done", &[("name", "shop")]), "shop xong nhiệm vụ");
        assert_eq!(t("vi", "ask.allow", &[]), "Cho phép");
        assert_eq!(t("en", "ask.deny", &[]), "Deny");
        assert_eq!(t("en", "test.title", &[]), "Test session");
        assert_eq!(t("en", "no.such.key", &[]), "no.such.key");
    }
}

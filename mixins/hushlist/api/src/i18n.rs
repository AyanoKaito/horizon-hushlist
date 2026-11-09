//! Hushlist's own text in the player's language.
//!
//! Translations live in `data/lang/<code>.lang`, one `key=value` per line,
//! `#` starts a comment. `{n}` and `{list}` are filled in by the code.
//! A key missing from a translation falls back to English.

use std::collections::HashMap;
use std::sync::OnceLock;

const EN_US: &str = include_str!("../data/lang/en_us.lang");
const RU_RU: &str = include_str!("../data/lang/ru_ru.lang");

type Table = HashMap<&'static str, &'static str>;

/// Parses a `.lang` file: `key=value` lines, blank lines and `#` comments ignored.
pub fn parse(text: &'static str) -> Table {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .filter_map(|l| l.split_once('='))
        .map(|(k, v)| (k.trim(), v.trim()))
        .collect()
}

fn english() -> &'static Table {
    static T: OnceLock<Table> = OnceLock::new();
    T.get_or_init(|| parse(EN_US))
}

fn russian() -> &'static Table {
    static T: OnceLock<Table> = OnceLock::new();
    T.get_or_init(|| parse(RU_RU))
}

/// The translation table for a game language code such as `en_us` or `ru_ru`.
fn table(code: &str) -> &'static Table {
    if code.starts_with("ru") { russian() } else { english() }
}

/// The text for `key` in the language `code`, falling back to English.
pub fn lookup(code: &str, key: &str) -> Option<&'static str> {
    table(code).get(key).or_else(|| english().get(key)).copied()
}

/// Like [`lookup`], but shows the key itself when no text exists.
pub fn text(code: &str, key: &'static str) -> &'static str {
    lookup(code, key).unwrap_or(key)
}

/// `text` with `{n}` replaced.
pub fn with_n(code: &str, key: &'static str, n: impl std::fmt::Display) -> String {
    text(code, key).replace("{n}", &n.to_string())
}

/// A count with its noun in the right form: `{base}.one`, `.few`, `.many` for Russian, `.one` and
/// `.other` otherwise, `{n}` replaced (every language file has all four keys).
pub fn plural(code: &str, base: &str, n: u64) -> String {
    let form = if code.starts_with("ru") {
        let (m10, m100) = (n % 10, n % 100);
        if m10 == 1 && m100 != 11 {
            "one"
        } else if (2..=4).contains(&m10) && !(12..=14).contains(&m100) {
            "few"
        } else {
            "many"
        }
    } else if n == 1 {
        "one"
    } else {
        "other"
    };
    lookup(code, &format!("{base}.{form}")).unwrap_or(base).replace("{n}", &n.to_string())
}

#[cfg(test)]
mod tests {
    #[test]
    fn russian_plurals_pick_the_right_form() {
        assert_eq!(plural("ru_ru", "zones.blocks", 1), "1 блок");
        assert_eq!(plural("ru_ru", "zones.blocks", 3), "3 блока");
        assert_eq!(plural("ru_ru", "zones.blocks", 11), "11 блоков");
        assert_eq!(plural("ru_ru", "zones.blocks", 64), "64 блока");
        assert_eq!(plural("ru_ru", "zones.blocks", 121), "121 блок");
        assert_eq!(plural("en_us", "zones.blocks", 1), "1 block");
        assert_eq!(plural("en_us", "zones.blocks", 64), "64 blocks");
    }

    use super::*;

    fn placeholders(s: &str) -> Vec<&str> {
        ["{n}", "{list}", "{time}", "{name}"].into_iter().filter(|p| s.contains(p)).collect()
    }

    #[test]
    fn translations_have_the_same_keys_and_placeholders() {
        let (en, ru) = (english(), russian());
        assert!(!en.is_empty());
        let mut missing: Vec<_> = en.keys().filter(|k| !ru.contains_key(*k)).collect();
        missing.extend(ru.keys().filter(|k| !en.contains_key(*k)));
        assert!(missing.is_empty(), "keys in one file only: {missing:?}");
        for (k, v) in en {
            assert_eq!(placeholders(v), placeholders(ru[k]), "placeholders differ for {k}");
        }
    }

    #[test]
    fn lookup_falls_back_to_english_and_to_the_key() {
        assert_eq!(text("en_us", "tab.recent"), "Recent");
        assert_eq!(text("de_de", "tab.recent"), "Recent");
        assert_ne!(text("ru_ru", "tab.recent"), "Recent");
        assert_eq!(text("ru_ru", "no.such.key"), "no.such.key");
        assert_eq!(with_n("en_us", "when.seconds", 12), "12s");
        assert_eq!(lookup("en_us", "group.entity.baby_chicken"), Some("Chick"));
        assert_eq!(lookup("en_us", "group.entity.cow"), None);
    }

    #[test]
    fn parse_skips_comments_and_blank_lines() {
        let t = parse("# note\n\na.b = One\nbad line\nc=x=y\n");
        assert_eq!(t.len(), 2);
        assert_eq!(t["a.b"], "One");
        assert_eq!(t["c"], "x=y");
    }
}

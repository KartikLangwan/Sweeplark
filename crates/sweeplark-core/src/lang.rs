//! App language. English is the default; Hinglish (Hindi in English letters) is optional.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Lang {
    #[default]
    En,
    Hinglish,
}

impl Lang {
    /// Pick the text for this language.
    pub fn pick<'a>(self, en: &'a str, hi: &'a str) -> &'a str {
        match self {
            Lang::En => en,
            Lang::Hinglish => hi,
        }
    }

    /// Pick an optional translation, falling back to English.
    pub fn pick_opt<'a>(self, en: &'a str, hi: Option<&'a str>) -> &'a str {
        match (self, hi) {
            (Lang::Hinglish, Some(h)) => h,
            _ => en,
        }
    }
}

/// `tr!(lang, "English {x}", "Hinglish {x}")` — format in the chosen language.
#[macro_export]
macro_rules! tr {
    ($lang:expr, $en:literal, $hi:literal $(, $($args:tt)*)?) => {
        match $lang {
            $crate::lang::Lang::En => format!($en $(, $($args)*)?),
            $crate::lang::Lang::Hinglish => format!($hi $(, $($args)*)?),
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_and_formats() {
        assert_eq!(Lang::default(), Lang::En);
        assert_eq!(Lang::Hinglish.pick("Hi", "Namaste"), "Namaste");
        assert_eq!(Lang::Hinglish.pick_opt("Hi", None), "Hi");
        let n = 3;
        assert_eq!(tr!(Lang::En, "{n} apps", "{n} apps chalu"), "3 apps");
        assert_eq!(tr!(Lang::Hinglish, "{n} apps", "{n} apps chalu"), "3 apps chalu");
        let s: Lang = serde_json::from_str("\"hinglish\"").unwrap();
        assert_eq!(s, Lang::Hinglish);
    }
}

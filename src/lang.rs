//! Interface language. The app follows the system's preferred languages unless the person picks
//! Chinese or English in Settings; text is written in both languages next to each other with
//! `tr!`, so a page never mixes languages and nothing needs a lookup table.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lang {
    Zh,
    En,
}

/// What the person chose in Settings; remembered between launches.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LanguageChoice {
    System,
    Zh,
    En,
}

impl LanguageChoice {
    pub const ALL: [LanguageChoice; 3] = [
        LanguageChoice::System,
        LanguageChoice::Zh,
        LanguageChoice::En,
    ];

    /// Stored preference value.
    pub fn key(self) -> &'static str {
        match self {
            LanguageChoice::System => "system",
            LanguageChoice::Zh => "zh",
            LanguageChoice::En => "en",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|choice| choice.key() == key)
    }

    pub fn resolve(self, preferred: &[String]) -> Lang {
        match self {
            LanguageChoice::System => system_lang(preferred),
            LanguageChoice::Zh => Lang::Zh,
            LanguageChoice::En => Lang::En,
        }
    }
}

/// The first preferred language that is Chinese or English decides; any other list gets English.
pub fn system_lang(preferred: &[String]) -> Lang {
    for tag in preferred {
        let tag = tag.to_ascii_lowercase();
        if tag.starts_with("zh") {
            return Lang::Zh;
        }
        if tag.starts_with("en") {
            return Lang::En;
        }
    }
    Lang::En
}

/// The language a page is rendered in, plus the choice behind it for the Settings page.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ui {
    pub lang: Lang,
    pub choice: LanguageChoice,
    /// What "follow system" means right now.
    pub system: Lang,
}

impl Ui {
    pub fn new(choice: LanguageChoice, preferred: &[String]) -> Self {
        Self {
            lang: choice.resolve(preferred),
            choice,
            system: system_lang(preferred),
        }
    }
}

/// Picks the Chinese or English expression; only the chosen one is evaluated.
macro_rules! tr {
    ($lang:expr, $zh:expr, $en:expr $(,)?) => {
        match $lang {
            $crate::lang::Lang::Zh => $zh,
            $crate::lang::Lang::En => $en,
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tags(values: &[&str]) -> Vec<String> {
        values.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn system_language_follows_the_first_chinese_or_english_preference() {
        assert_eq!(system_lang(&tags(&["zh-Hans-CN", "en-US"])), Lang::Zh);
        assert_eq!(system_lang(&tags(&["zh-Hant-TW"])), Lang::Zh);
        assert_eq!(system_lang(&tags(&["en-GB", "zh-Hans"])), Lang::En);
        assert_eq!(system_lang(&tags(&["ja-JP", "zh-Hans"])), Lang::Zh);
        assert_eq!(system_lang(&tags(&["fr-FR"])), Lang::En);
        assert_eq!(system_lang(&[]), Lang::En);
    }

    #[test]
    fn manual_choice_overrides_the_system_and_round_trips() {
        let chinese_system = tags(&["zh-Hans"]);
        assert_eq!(LanguageChoice::En.resolve(&chinese_system), Lang::En);
        assert_eq!(LanguageChoice::System.resolve(&chinese_system), Lang::Zh);
        assert_eq!(LanguageChoice::Zh.resolve(&tags(&["en-US"])), Lang::Zh);
        for choice in LanguageChoice::ALL {
            assert_eq!(LanguageChoice::from_key(choice.key()), Some(choice));
        }
        assert_eq!(LanguageChoice::from_key("de"), None);
        assert_eq!(tr!(Lang::En, "中文", "English"), "English");
    }
}

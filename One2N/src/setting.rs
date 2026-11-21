#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Language {
    Default,
    Korean,
    English,
    Japanese,
    ChineseSimplified,
    ChineseTraditional, // 대만 번체 기준
}

impl Language {
    /// BCP 47 태그
    pub fn code(&self) -> &'static str {
        match self {
            Language::Default => "none",
            Language::Korean => "ko",
            Language::English => "en",
            Language::Japanese => "ja",
            Language::ChineseSimplified => "zh-Hans",
            Language::ChineseTraditional => "zh-Hant",
        }
    }
}

pub struct SettingManager {
    lang : Language
}

impl SettingManager {

    pub fn new() -> Self {
        return Self{lang : Language::Default}
    }

}
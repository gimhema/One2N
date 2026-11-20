#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Language {
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
            Language::Korean => "ko",
            Language::English => "en",
            Language::Japanese => "ja",
            Language::ChineseSimplified => "zh-Hans",
            Language::ChineseTraditional => "zh-Hant",
        }
    }
}

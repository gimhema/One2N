use std::collections::{HashSet};
use crate::agent::ModelOption;

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
    model_path : String,
    model_load_opt : ModelOption,
    input_lang : Language,
    output_lang : HashSet<Language>
}

impl SettingManager {

    pub fn new() -> Self {
        return Self{
            model_path : "".to_string(),
            model_load_opt : ModelOption::Default,
            input_lang : Language::Default,
            output_lang : HashSet::new()
        }
    }

    pub fn read_ini() {
        // 지정된 경로의 ini 파일을 읽음

        // model path 설정

        // model load 방식 설정

        // 출발 언어 설정

        // 도착 언어 설정
    }

}
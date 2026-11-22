use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use crate::agent::ModelOption;
use crate::ini_parser::{IniDocument, IniError, IniResult};

/// 기본 설정 파일 경로
pub const DEFAULT_SETTING_PATH: &str = "model/Setting.ini";

const SEC_MODEL: &str = "Model";
const SEC_LANGUAGE: &str = "Language";

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

    /// 프롬프트에 넣을 언어 이름
    pub fn name(&self) -> &'static str {
        match self {
            Language::Default => "None",
            Language::Korean => "Korean",
            Language::English => "English",
            Language::Japanese => "Japanese",
            Language::ChineseSimplified => "Simplified Chinese",
            Language::ChineseTraditional => "Traditional Chinese (Taiwan)",
        }
    }
}

impl FromStr for Language {
    type Err = String;

    /// BCP 47 코드 또는 영문 이름 (대소문자 무시)
    /// 예: "ko", "en", "ja", "zh-Hans", "zh-CN", "zh-TW", "Korean", "Japanese"
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let key = s.trim().to_ascii_lowercase().replace('_', "-");
        match key.as_str() {
            "ko" | "ko-kr" | "korean" => Ok(Language::Korean),
            "en" | "en-us" | "en-gb" | "english" => Ok(Language::English),
            "ja" | "ja-jp" | "japanese" => Ok(Language::Japanese),
            "zh-hans" | "zh-cn" | "zh" | "chinesesimplified" => Ok(Language::ChineseSimplified),
            "zh-hant" | "zh-tw" | "chinesetraditional" => Ok(Language::ChineseTraditional),
            _ => Err("expected one of: ko, en, ja, zh-Hans, zh-Hant".to_string()),
        }
    }
}

impl FromStr for ModelOption {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "ollama" => Ok(ModelOption::Ollama),
            "gguf" => Ok(ModelOption::GGUF),
            _ => Err("expected Ollama or GGUF".to_string()),
        }
    }
}

pub struct SettingManager {
    model_path: String,
    model_load_opt: ModelOption,
    input_lang: Language,
    output_lang: HashSet<Language>,
}

impl SettingManager {
    pub fn new() -> Self {
        return Self {
            model_path: "".to_string(),
            model_load_opt: ModelOption::Default,
            input_lang: Language::Default,
            output_lang: HashSet::new(),
        };
    }

    /// 지정된 경로의 ini 파일을 읽어 설정을 채운다.
    ///
    /// [Model]
    ///   path : Ollama -> 모델 이름 (예: gemma3:4b)
    ///          GGUF   -> 가중치 파일 경로. 상대경로는 ini 파일이 있는 디렉터리 기준
    ///   mode : Ollama | GGUF
    /// [Language]
    ///   input  : 원문 언어 1개
    ///   output : 번역 대상 언어들, 쉼표 구분
    pub fn read_ini<P: AsRef<Path>>(&mut self, path: P) -> IniResult<()> {
        let path = path.as_ref();

        // 지정된 경로의 ini 파일을 읽음
        let ini = IniDocument::read_file(path)?;

        // model load 방식 설정
        let mode: ModelOption = parse_required(&ini, SEC_MODEL, "mode")?;

        // model path 설정
        let raw_path = ini.require(SEC_MODEL, "path")?;
        let model_path = match mode {
            ModelOption::GGUF => {
                let base = path.parent().unwrap_or(Path::new(""));
                let resolved = resolve_path(base, raw_path);
                if !resolved.is_file() {
                    return Err(invalid(SEC_MODEL, "path", raw_path, format!(
                        "GGUF file not found: {}",
                        resolved.display()
                    )));
                }
                resolved.to_string_lossy().into_owned()
            }
            _ => raw_path.to_string(),
        };

        // 출발 언어 설정
        let input_lang: Language = parse_required(&ini, SEC_LANGUAGE, "input")?;

        // 도착 언어 설정
        let items = ini.get_list(SEC_LANGUAGE, "output");
        if items.is_empty() {
            return Err(IniError::Missing { section: SEC_LANGUAGE.into(), key: "output".into() });
        }
        let mut output_lang = HashSet::new();
        for item in &items {
            let lang: Language = item.parse().map_err(|e| invalid(SEC_LANGUAGE, "output", item, e))?;
            if lang == input_lang {
                return Err(invalid(SEC_LANGUAGE, "output", item, "same as input language".into()));
            }
            output_lang.insert(lang);
        }

        // 모두 검증된 뒤에 반영 (중간 실패 시 기존 값 유지)
        self.model_path = model_path;
        self.model_load_opt = mode;
        self.input_lang = input_lang;
        self.output_lang = output_lang;
        Ok(())
    }

    pub fn model_path(&self) -> &str {
        &self.model_path
    }

    pub fn model_load_opt(&self) -> ModelOption {
        self.model_load_opt
    }

    pub fn input_lang(&self) -> Language {
        self.input_lang
    }

    pub fn output_lang(&self) -> &HashSet<Language> {
        &self.output_lang
    }

    /// 출력 언어를 코드 순으로 정렬 (출력 순서를 매번 같게 하고 싶을 때)
    pub fn output_lang_sorted(&self) -> Vec<Language> {
        let mut v: Vec<Language> = self.output_lang.iter().copied().collect();
        v.sort_by_key(|l| l.code());
        v
    }
}

fn parse_required<T>(ini: &IniDocument, section: &str, key: &str) -> IniResult<T>
where
    T: FromStr<Err = String>,
{
    let raw = ini.require(section, key)?;
    raw.parse().map_err(|e| invalid(section, key, raw, e))
}

fn invalid(section: &str, key: &str, value: &str, reason: String) -> IniError {
    IniError::InvalidValue {
        section: section.to_string(),
        key: key.to_string(),
        value: value.to_string(),
        reason,
    }
}

fn resolve_path(base: &Path, p: &str) -> PathBuf {
    let p = Path::new(p);
    if p.is_absolute() { p.to_path_buf() } else { base.join(p) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn write_ini(name: &str, body: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("one2n_setting_{}_{}", std::process::id(), name));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("Setting.ini");
        fs::write(&path, body).unwrap();
        path
    }

    #[test]
    fn language_parse() {
        assert_eq!("KO".parse::<Language>(), Ok(Language::Korean));
        assert_eq!("zh_TW".parse::<Language>(), Ok(Language::ChineseTraditional));
        assert_eq!("zh-Hans".parse::<Language>(), Ok(Language::ChineseSimplified));
        assert!("fr".parse::<Language>().is_err());
    }

    #[test]
    fn read_ollama_setting() {
        let path = write_ini(
            "ollama",
            "[Model]\npath = gemma3:270m\nmode = Ollama ; Ollama, GGUF\n\
             [Language]\ninput = ko\noutput = en, ja, zh-Hant, en\n",
        );
        let mut s = SettingManager::new();
        s.read_ini(&path).unwrap();
        assert_eq!(s.model_path(), "gemma3:270m");
        assert_eq!(s.model_load_opt(), ModelOption::Ollama);
        assert_eq!(s.input_lang(), Language::Korean);
        assert_eq!(
            s.output_lang_sorted(),
            vec![Language::English, Language::Japanese, Language::ChineseTraditional]
        );
        fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn gguf_path_relative_to_ini() {
        let path = write_ini("gguf", "[Model]\npath = bin/m.gguf\nmode = gguf\n[Language]\ninput=en\noutput=ko\n");
        let dir = path.parent().unwrap();

        let mut s = SettingManager::new();
        assert!(matches!(s.read_ini(&path), Err(IniError::InvalidValue { .. })));

        fs::create_dir_all(dir.join("bin")).unwrap();
        fs::write(dir.join("bin/m.gguf"), b"").unwrap();
        s.read_ini(&path).unwrap();
        assert_eq!(Path::new(s.model_path()), dir.join("bin/m.gguf"));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn invalid_settings() {
        let cases = [
            ("empty", "[Model]\npath=\nmode= ; Ollama, GGUF\n[Language]\ninput=\noutput=\n"),
            ("mode", "[Model]\npath=x\nmode=vllm\n[Language]\ninput=ko\noutput=en\n"),
            ("lang", "[Model]\npath=x\nmode=ollama\n[Language]\ninput=ko\noutput=en, fr\n"),
            ("same", "[Model]\npath=x\nmode=ollama\n[Language]\ninput=ko\noutput=ko\n"),
            ("noout", "[Model]\npath=x\nmode=ollama\n[Language]\ninput=ko\n"),
        ];
        for (name, body) in cases {
            let path = write_ini(name, body);
            let mut s = SettingManager::new();
            assert!(s.read_ini(&path).is_err(), "case {name} should fail");
            assert_eq!(s.model_load_opt(), ModelOption::Default, "case {name} must not partially apply");
            fs::remove_dir_all(path.parent().unwrap()).unwrap();
        }
    }
}

// INI 파서
//
// model/Setting.ini 같은 설정 파일을 읽는다.
//
// 포맷 규칙
// - [Section] 으로 섹션 시작. 첫 섹션 이전의 key=value 는 이름이 "" 인 전역 섹션에 들어감
// - key = value  (앞뒤 공백 제거, '=' 대신 ':'도 허용)
// - ';' 또는 '#' 으로 시작하는 줄은 주석
// - 인라인 주석: 값의 맨 앞이거나 공백 뒤에 오는 ';' / '#' 이후는 주석
//     mode= ; Ollama, GGUF   -> mode = ""
//     path=./a;b             -> path = "./a;b" (공백이 없으면 주석 아님)
// - 값을 "..." 또는 '...' 로 감싸면 그 안의 ';' '#' 공백은 그대로 유지
// - 섹션/키 이름 조회는 대소문자를 구분하지 않음
// - 같은 섹션이 여러 번 나오면 합쳐지고, 같은 키는 마지막 값이 이김
// - UTF-8 BOM, CRLF 허용

use std::fmt;
use std::fs;
use std::io;
use std::path::Path;
use std::str::FromStr;

// ---------------------------------------------------------------------------
// Error
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub enum IniError {
    Io(io::Error),
    /// 문법 오류 (닫히지 않은 섹션, '=' 없는 줄 등)
    Syntax { line: usize, message: String },
    /// 필수 키가 없거나 값이 비어 있음
    Missing { section: String, key: String },
    /// 값을 원하는 타입으로 변환할 수 없음
    InvalidValue { section: String, key: String, value: String, reason: String },
}

impl fmt::Display for IniError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            IniError::Io(e) => write!(f, "INI I/O error: {e}"),
            IniError::Syntax { line, message } => write!(f, "INI syntax error at line {line}: {message}"),
            IniError::Missing { section, key } => write!(f, "INI missing value: [{section}] {key}"),
            IniError::InvalidValue { section, key, value, reason } => {
                write!(f, "INI invalid value for [{section}] {key} = {value:?}: {reason}")
            }
        }
    }
}

impl std::error::Error for IniError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            IniError::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<io::Error> for IniError {
    fn from(e: io::Error) -> Self {
        IniError::Io(e)
    }
}

pub type IniResult<T> = Result<T, IniError>;

// ---------------------------------------------------------------------------
// Data
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct IniSection {
    name: String,
    /// 파일에 적힌 순서 유지
    entries: Vec<(String, String)>,
}

impl IniSection {
    fn new(name: &str) -> Self {
        Self { name: name.to_string(), entries: Vec::new() }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn entries(&self) -> &[(String, String)] {
        &self.entries
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.entries
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(key))
            .map(|(_, v)| v.as_str())
    }

    pub fn contains_key(&self, key: &str) -> bool {
        self.get(key).is_some()
    }

    fn set(&mut self, key: &str, value: String) {
        match self.entries.iter_mut().find(|(k, _)| k.eq_ignore_ascii_case(key)) {
            Some(entry) => entry.1 = value,
            None => self.entries.push((key.to_string(), value)),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct IniDocument {
    sections: Vec<IniSection>,
}

impl IniDocument {
    // --- 파싱 ---------------------------------------------------------------

    pub fn parse_str(text: &str) -> IniResult<Self> {
        let text = text.strip_prefix('\u{feff}').unwrap_or(text);
        let mut doc = IniDocument::default();
        let mut current = String::new();

        for (i, raw) in text.lines().enumerate() {
            let line_no = i + 1;
            let line = raw.trim();

            if line.is_empty() || line.starts_with(';') || line.starts_with('#') {
                continue;
            }

            if let Some(rest) = line.strip_prefix('[') {
                let end = rest.find(']').ok_or_else(|| IniError::Syntax {
                    line: line_no,
                    message: "section header is missing ']'".into(),
                })?;
                let trailing = strip_inline_comment(&rest[end + 1..]);
                if !trailing.trim().is_empty() {
                    return Err(IniError::Syntax {
                        line: line_no,
                        message: format!("unexpected text after section header: {}", trailing.trim()),
                    });
                }
                let name = rest[..end].trim();
                if name.is_empty() {
                    return Err(IniError::Syntax { line: line_no, message: "empty section name".into() });
                }
                current = name.to_string();
                doc.section_mut_or_insert(&current);
                continue;
            }

            let sep = line.find(['=', ':']).ok_or_else(|| IniError::Syntax {
                line: line_no,
                message: format!("expected 'key = value', got: {line}"),
            })?;
            let key = line[..sep].trim();
            if key.is_empty() {
                return Err(IniError::Syntax { line: line_no, message: "empty key".into() });
            }
            let value = parse_value(&line[sep + 1..]).map_err(|message| IniError::Syntax { line: line_no, message })?;

            doc.section_mut_or_insert(&current).set(key, value);
        }

        Ok(doc)
    }

    pub fn read_file<P: AsRef<Path>>(path: P) -> IniResult<Self> {
        let text = fs::read_to_string(path)?;
        Self::parse_str(&text)
    }

    fn section_mut_or_insert(&mut self, name: &str) -> &mut IniSection {
        let idx = match self.sections.iter().position(|s| s.name.eq_ignore_ascii_case(name)) {
            Some(idx) => idx,
            None => {
                self.sections.push(IniSection::new(name));
                self.sections.len() - 1
            }
        };
        &mut self.sections[idx]
    }

    // --- 조회 ---------------------------------------------------------------

    pub fn sections(&self) -> &[IniSection] {
        &self.sections
    }

    pub fn section(&self, name: &str) -> Option<&IniSection> {
        self.sections.iter().find(|s| s.name.eq_ignore_ascii_case(name))
    }

    /// 값 조회 (키가 없으면 None, 빈 값이면 Some(""))
    pub fn get(&self, section: &str, key: &str) -> Option<&str> {
        self.section(section)?.get(key)
    }

    /// 비어 있지 않은 값만 반환
    pub fn get_non_empty(&self, section: &str, key: &str) -> Option<&str> {
        self.get(section, key).filter(|v| !v.is_empty())
    }

    pub fn get_or<'a>(&'a self, section: &str, key: &str, default: &'a str) -> &'a str {
        self.get_non_empty(section, key).unwrap_or(default)
    }

    /// 필수 값. 키가 없거나 비어 있으면 에러
    pub fn require(&self, section: &str, key: &str) -> IniResult<&str> {
        self.get_non_empty(section, key).ok_or_else(|| IniError::Missing {
            section: section.to_string(),
            key: key.to_string(),
        })
    }

    /// 쉼표로 구분된 리스트 (예: output = en, ja, zh-Hans). 빈 항목은 제외
    pub fn get_list(&self, section: &str, key: &str) -> Vec<String> {
        self.get(section, key)
            .map(|v| {
                v.split(',')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default()
    }

    /// FromStr 타입으로 변환. 키가 없거나 비어 있으면 Ok(None)
    pub fn get_parsed<T>(&self, section: &str, key: &str) -> IniResult<Option<T>>
    where
        T: FromStr,
        T::Err: fmt::Display,
    {
        match self.get_non_empty(section, key) {
            None => Ok(None),
            Some(v) => v.parse::<T>().map(Some).map_err(|e| IniError::InvalidValue {
                section: section.to_string(),
                key: key.to_string(),
                value: v.to_string(),
                reason: e.to_string(),
            }),
        }
    }

    /// true/false, yes/no, on/off, 1/0
    pub fn get_bool(&self, section: &str, key: &str) -> IniResult<Option<bool>> {
        let Some(v) = self.get_non_empty(section, key) else {
            return Ok(None);
        };
        match v.to_ascii_lowercase().as_str() {
            "true" | "yes" | "on" | "1" => Ok(Some(true)),
            "false" | "no" | "off" | "0" => Ok(Some(false)),
            _ => Err(IniError::InvalidValue {
                section: section.to_string(),
                key: key.to_string(),
                value: v.to_string(),
                reason: "expected true/false, yes/no, on/off, 1/0".into(),
            }),
        }
    }
}

// ---------------------------------------------------------------------------
// Value helpers
// ---------------------------------------------------------------------------

/// '=' 뒤의 원문에서 값을 추출 (따옴표, 인라인 주석 처리)
fn parse_value(raw: &str) -> Result<String, String> {
    let s = raw.trim_start();

    if let Some(quote) = s.chars().next().filter(|c| *c == '"' || *c == '\'') {
        let body = &s[1..];
        let end = body.find(quote).ok_or_else(|| format!("unterminated {quote} quote"))?;
        let rest = strip_inline_comment(&body[end + 1..]);
        if !rest.trim().is_empty() {
            return Err(format!("unexpected text after quoted value: {}", rest.trim()));
        }
        return Ok(body[..end].to_string());
    }

    Ok(strip_inline_comment(s).trim().to_string())
}

/// 맨 앞이거나 공백 바로 뒤에 오는 ';' / '#' 부터 끝까지 제거
fn strip_inline_comment(s: &str) -> &str {
    let mut prev_ws = true;
    for (i, c) in s.char_indices() {
        if (c == ';' || c == '#') && prev_ws {
            return &s[..i];
        }
        prev_ws = c.is_whitespace();
    }
    s
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    const SETTING: &str = "\
[Model]
path=
mode= ; Ollama, GGUF

[Language]
input= ; 단 하나의 언어만
output= ; 번역대상 언어들을 쉼표로 구분 리스트에 등록된 국가별로 번역 결과가 생성된다.
";

    #[test]
    fn parse_setting_template() {
        let doc = IniDocument::parse_str(SETTING).unwrap();
        assert_eq!(doc.sections().len(), 2);
        assert_eq!(doc.get("Model", "path"), Some(""));
        assert_eq!(doc.get("Model", "mode"), Some(""));
        assert_eq!(doc.get("Language", "input"), Some(""));
        assert!(doc.get_list("Language", "output").is_empty());
        assert!(matches!(doc.require("Model", "mode"), Err(IniError::Missing { .. })));
    }

    #[test]
    fn parse_filled_setting() {
        let text = "\u{feff}[Model]\r\npath = ./model/bin/gemma.gguf\r\nmode = GGUF ; Ollama, GGUF\r\n\
                    [Language]\r\ninput = ko\r\noutput = en, ja ,, zh-Hans ; 쉼표 구분\r\n";
        let doc = IniDocument::parse_str(text).unwrap();
        assert_eq!(doc.require("model", "PATH").unwrap(), "./model/bin/gemma.gguf");
        assert_eq!(doc.get("Model", "mode"), Some("GGUF"));
        assert_eq!(doc.get("Language", "input"), Some("ko"));
        assert_eq!(doc.get_list("Language", "output"), vec!["en", "ja", "zh-Hans"]);
    }

    #[test]
    fn inline_comment_needs_whitespace() {
        let doc = IniDocument::parse_str("a=x;y\nb=x #c\nc=#only comment\nd=C#").unwrap();
        assert_eq!(doc.get("", "a"), Some("x;y"));
        assert_eq!(doc.get("", "b"), Some("x"));
        assert_eq!(doc.get("", "c"), Some(""));
        assert_eq!(doc.get("", "d"), Some("C#"));
    }

    #[test]
    fn quoted_values() {
        let doc = IniDocument::parse_str("a = \" spaced ; kept \" ; comment\nb='x#y'").unwrap();
        assert_eq!(doc.get("", "a"), Some(" spaced ; kept "));
        assert_eq!(doc.get("", "b"), Some("x#y"));
        assert!(IniDocument::parse_str("a=\"open").is_err());
    }

    #[test]
    fn merge_sections_and_override_keys() {
        let doc = IniDocument::parse_str("[A]\nk=1\n[B]\nk=2\n[a]\nk=3\nj=4").unwrap();
        assert_eq!(doc.sections().len(), 2);
        assert_eq!(doc.get("A", "k"), Some("3"));
        assert_eq!(doc.get("A", "j"), Some("4"));
    }

    #[test]
    fn typed_getters() {
        let doc = IniDocument::parse_str("[S]\nn=42\nbad=x\nflag=Yes\nempty=").unwrap();
        assert_eq!(doc.get_parsed::<u32>("S", "n").unwrap(), Some(42));
        assert_eq!(doc.get_parsed::<u32>("S", "empty").unwrap(), None);
        assert!(matches!(doc.get_parsed::<u32>("S", "bad"), Err(IniError::InvalidValue { .. })));
        assert_eq!(doc.get_bool("S", "flag").unwrap(), Some(true));
        assert_eq!(doc.get_or("S", "empty", "dflt"), "dflt");
    }

    #[test]
    fn syntax_errors_report_line() {
        assert!(matches!(
            IniDocument::parse_str("[ok]\n[broken\n"),
            Err(IniError::Syntax { line: 2, .. })
        ));
        assert!(matches!(
            IniDocument::parse_str("[ok]\nno separator\n"),
            Err(IniError::Syntax { line: 2, .. })
        ));
    }

    #[test]
    fn read_real_setting_file() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/model/Setting.ini");
        if Path::new(path).exists() {
            let doc = IniDocument::read_file(path).unwrap();
            assert!(doc.section("Model").is_some());
            assert!(doc.section("Language").is_some());
        }
    }
}

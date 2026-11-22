// TSV 파서
//
// input.rs  : 번역 대상 문장 리스트를 TSV 파일에서 읽어올 때 사용
// output.rs : 번역 결과를 언어(국가)별 TSV 파일로 저장할 때 사용
//
// 포맷 규칙
// - 필드 구분자는 '\t', 레코드 구분자는 '\n' (읽을 때는 "\r\n"도 허용)
// - 문장 안에 탭/개행이 들어갈 수 있으므로 백슬래시 이스케이프를 사용
//     \t -> 탭, \n -> 개행, \r -> CR, \\ -> 백슬래시
//   알 수 없는 이스케이프(예: "C:\path")는 원문 그대로 보존
// - 파일 앞의 UTF-8 BOM은 무시
// - 빈 줄은 건너뜀

use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

const DELIMITER: char = '\t';
const BOM: char = '\u{feff}';

// ---------------------------------------------------------------------------
// Error
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub enum TsvError {
    Io(io::Error),
    /// 헤더(또는 첫 행)와 컬럼 수가 다른 행
    ColumnMismatch { line: usize, expected: usize, found: usize },
    /// 헤더에 같은 이름의 컬럼이 중복됨
    DuplicateHeader(String),
}

impl fmt::Display for TsvError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TsvError::Io(e) => write!(f, "TSV I/O error: {e}"),
            TsvError::ColumnMismatch { line, expected, found } => write!(
                f,
                "TSV column mismatch at line {line}: expected {expected}, found {found}"
            ),
            TsvError::DuplicateHeader(name) => write!(f, "TSV duplicate header: {name}"),
        }
    }
}

impl std::error::Error for TsvError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            TsvError::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<io::Error> for TsvError {
    fn from(e: io::Error) -> Self {
        TsvError::Io(e)
    }
}

pub type TsvResult<T> = Result<T, TsvError>;

// ---------------------------------------------------------------------------
// Table
// ---------------------------------------------------------------------------

/// 파싱된 TSV 데이터. 모든 행은 같은 컬럼 수를 가진다.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TsvTable {
    headers: Option<Vec<String>>,
    rows: Vec<Vec<String>>,
}

impl TsvTable {
    /// 헤더가 있는 빈 테이블
    pub fn new<S: Into<String>>(headers: Vec<S>) -> TsvResult<Self> {
        let headers: Vec<String> = headers.into_iter().map(Into::into).collect();
        check_duplicate_headers(&headers)?;
        Ok(Self { headers: Some(headers), rows: Vec::new() })
    }

    /// 헤더가 없는 빈 테이블
    pub fn without_header() -> Self {
        Self { headers: None, rows: Vec::new() }
    }

    pub fn headers(&self) -> Option<&[String]> {
        self.headers.as_deref()
    }

    pub fn rows(&self) -> &[Vec<String>] {
        &self.rows
    }

    pub fn into_rows(self) -> Vec<Vec<String>> {
        self.rows
    }

    pub fn len(&self) -> usize {
        self.rows.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    /// 컬럼 수 (헤더 기준, 헤더가 없으면 첫 행 기준)
    pub fn column_count(&self) -> Option<usize> {
        self.headers
            .as_ref()
            .map(Vec::len)
            .or_else(|| self.rows.first().map(Vec::len))
    }

    /// 헤더 이름으로 컬럼 인덱스 조회
    pub fn column_index(&self, name: &str) -> Option<usize> {
        self.headers.as_ref()?.iter().position(|h| h == name)
    }

    /// 헤더 이름으로 컬럼 전체 값 조회 (예: 번역 원문 컬럼 "source")
    pub fn column(&self, name: &str) -> Option<Vec<&str>> {
        let idx = self.column_index(name)?;
        Some(self.rows.iter().map(|r| r[idx].as_str()).collect())
    }

    /// 인덱스로 컬럼 전체 값 조회
    pub fn column_at(&self, idx: usize) -> Option<Vec<&str>> {
        if idx >= self.column_count()? {
            return None;
        }
        Some(self.rows.iter().map(|r| r[idx].as_str()).collect())
    }

    /// (행 번호, 헤더 이름)으로 셀 조회
    pub fn get(&self, row: usize, name: &str) -> Option<&str> {
        let idx = self.column_index(name)?;
        self.rows.get(row).map(|r| r[idx].as_str())
    }

    /// 행 추가. 컬럼 수가 맞지 않으면 에러.
    pub fn push_row<S: Into<String>>(&mut self, row: Vec<S>) -> TsvResult<()> {
        let row: Vec<String> = row.into_iter().map(Into::into).collect();
        if let Some(expected) = self.column_count() {
            if row.len() != expected {
                return Err(TsvError::ColumnMismatch {
                    line: self.rows.len() + 1 + self.headers.is_some() as usize,
                    expected,
                    found: row.len(),
                });
            }
        }
        self.rows.push(row);
        Ok(())
    }
}

fn check_duplicate_headers(headers: &[String]) -> TsvResult<()> {
    for (i, h) in headers.iter().enumerate() {
        if headers[..i].contains(h) {
            return Err(TsvError::DuplicateHeader(h.clone()));
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Parser / Writer
// ---------------------------------------------------------------------------

/// TSV 읽기/쓰기 설정
#[derive(Debug, Clone, Copy)]
pub struct TsvParser {
    /// 첫 줄을 헤더로 취급
    pub has_header: bool,
    /// 백슬래시 이스케이프 사용
    pub escape: bool,
}

impl Default for TsvParser {
    fn default() -> Self {
        Self { has_header: true, escape: true }
    }
}

impl TsvParser {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn has_header(mut self, yes: bool) -> Self {
        self.has_header = yes;
        self
    }

    pub fn escape(mut self, yes: bool) -> Self {
        self.escape = yes;
        self
    }

    /// 문자열을 TSV로 파싱
    pub fn parse_str(&self, text: &str) -> TsvResult<TsvTable> {
        let text = text.strip_prefix(BOM).unwrap_or(text);

        let mut headers: Option<Vec<String>> = None;
        let mut rows: Vec<Vec<String>> = Vec::new();
        let mut expected: Option<usize> = None;

        for (i, raw) in text.split('\n').enumerate() {
            let line_no = i + 1;
            let line = raw.strip_suffix('\r').unwrap_or(raw);
            if line.is_empty() {
                continue;
            }

            let fields = self.split_line(line);

            match expected {
                None => expected = Some(fields.len()),
                Some(n) if n != fields.len() => {
                    return Err(TsvError::ColumnMismatch { line: line_no, expected: n, found: fields.len() });
                }
                _ => {}
            }

            if self.has_header && headers.is_none() {
                check_duplicate_headers(&fields)?;
                headers = Some(fields);
            } else {
                rows.push(fields);
            }
        }

        Ok(TsvTable { headers, rows })
    }

    /// 파일을 읽어 TSV로 파싱
    pub fn read_file<P: AsRef<Path>>(&self, path: P) -> TsvResult<TsvTable> {
        let text = fs::read_to_string(path)?;
        self.parse_str(&text)
    }

    /// 테이블을 TSV 문자열로 직렬화 (각 행은 '\n'으로 끝남)
    pub fn to_string(&self, table: &TsvTable) -> String {
        let mut out = String::new();
        if self.has_header {
            if let Some(h) = &table.headers {
                self.write_line(&mut out, h);
            }
        }
        for row in &table.rows {
            self.write_line(&mut out, row);
        }
        out
    }

    /// 테이블을 파일로 저장. 상위 디렉터리가 없으면 생성.
    pub fn write_file<P: AsRef<Path>>(&self, path: P, table: &TsvTable) -> TsvResult<()> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent)?;
            }
        }
        fs::write(path, self.to_string(table))?;
        Ok(())
    }

    fn split_line(&self, line: &str) -> Vec<String> {
        line.split(DELIMITER)
            .map(|f| if self.escape { unescape_field(f) } else { f.to_string() })
            .collect()
    }

    fn write_line(&self, out: &mut String, fields: &[String]) {
        for (i, f) in fields.iter().enumerate() {
            if i > 0 {
                out.push(DELIMITER);
            }
            if self.escape {
                out.push_str(&escape_field(f));
            } else {
                // 이스케이프를 끈 경우 포맷이 깨지지 않도록 구분자 문자는 공백으로 치환
                out.extend(f.chars().map(|c| if matches!(c, '\t' | '\n' | '\r') { ' ' } else { c }));
            }
        }
        out.push('\n');
    }
}

// ---------------------------------------------------------------------------
// Escape helpers
// ---------------------------------------------------------------------------

pub fn escape_field(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\t' => out.push_str("\\t"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            _ => out.push(c),
        }
    }
    out
}

pub fn unescape_field(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.peek() {
            Some('\\') => { out.push('\\'); chars.next(); }
            Some('t') => { out.push('\t'); chars.next(); }
            Some('n') => { out.push('\n'); chars.next(); }
            Some('r') => { out.push('\r'); chars.next(); }
            // 알 수 없는 이스케이프나 끝의 백슬래시는 그대로 보존
            _ => out.push('\\'),
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Output helpers
// ---------------------------------------------------------------------------

/// 언어별 출력 파일 경로 생성: `{dir}/{stem}_{lang_code}.tsv`
/// 예) lang_file_path("out", "result", "ja") -> out/result_ja.tsv
pub fn lang_file_path<P: AsRef<Path>>(dir: P, stem: &str, lang_code: &str) -> PathBuf {
    dir.as_ref().join(format!("{stem}_{lang_code}.tsv"))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_with_header() {
        let t = TsvParser::new().parse_str("id\tsource\n1\t안녕하세요\n2\t반갑습니다\n").unwrap();
        assert_eq!(t.headers().unwrap(), ["id", "source"]);
        assert_eq!(t.len(), 2);
        assert_eq!(t.get(1, "source"), Some("반갑습니다"));
        assert_eq!(t.column("source").unwrap(), vec!["안녕하세요", "반갑습니다"]);
    }

    #[test]
    fn parse_without_header_bom_crlf_blank_lines() {
        let t = TsvParser::new()
            .has_header(false)
            .parse_str("\u{feff}a\tb\r\n\r\nc\td\r\n")
            .unwrap();
        assert!(t.headers().is_none());
        assert_eq!(t.rows(), [vec!["a", "b"], vec!["c", "d"]]);
    }

    #[test]
    fn empty_fields_are_kept() {
        let t = TsvParser::new().has_header(false).parse_str("a\t\t\n").unwrap();
        assert_eq!(t.rows()[0], ["a", "", ""]);
    }

    #[test]
    fn column_mismatch_reports_line() {
        let err = TsvParser::new().parse_str("a\tb\n1\t2\n3\n").unwrap_err();
        assert!(matches!(err, TsvError::ColumnMismatch { line: 3, expected: 2, found: 1 }));
    }

    #[test]
    fn duplicate_header_is_error() {
        assert!(matches!(
            TsvParser::new().parse_str("a\ta\n"),
            Err(TsvError::DuplicateHeader(_))
        ));
    }

    #[test]
    fn escape_roundtrip() {
        let mut t = TsvTable::new(vec!["id", "text"]).unwrap();
        t.push_row(vec!["1", "line1\nline2\ttab \\ back"]).unwrap();
        let p = TsvParser::new();
        let s = p.to_string(&t);
        assert_eq!(s, "id\ttext\n1\tline1\\nline2\\ttab \\\\ back\n");
        assert_eq!(p.parse_str(&s).unwrap(), t);
    }

    #[test]
    fn unknown_escape_preserved() {
        assert_eq!(unescape_field(r"C:\path\x"), r"C:\path\x");
        assert_eq!(unescape_field("end\\"), "end\\");
    }

    #[test]
    fn push_row_checks_columns() {
        let mut t = TsvTable::new(vec!["a", "b"]).unwrap();
        assert!(t.push_row(vec!["only"]).is_err());
    }

    #[test]
    fn write_and_read_file_per_language() {
        let dir = std::env::temp_dir().join(format!("one2n_tsv_test_{}", std::process::id()));
        let p = TsvParser::new();
        for (code, text) in [("en", "Hello"), ("ja", "こんにちは")] {
            let mut t = TsvTable::new(vec!["id", "source", "translated"]).unwrap();
            t.push_row(vec!["1", "안녕하세요", text]).unwrap();
            p.write_file(lang_file_path(&dir, "result", code), &t).unwrap();
        }
        let ja = p.read_file(dir.join("result_ja.tsv")).unwrap();
        assert_eq!(ja.get(0, "translated"), Some("こんにちは"));
        fs::remove_dir_all(&dir).unwrap();
    }
}

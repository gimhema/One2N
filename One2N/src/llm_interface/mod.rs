// LLM 호출 인터페이스
//
// - interface_ollama : 로컬/원격 Ollama 서버의 HTTP API 호출
// - interface_custom : llama.cpp로 GGUF 가중치를 직접 로딩 (cargo feature `llama-cpp`)
//
// 에이전트는 `LlmInterface` trait만 보고 동작하므로 백엔드를 바꿔도 호출부는 그대로 유지된다.

pub mod interface_custom;
pub mod interface_ollama;

use std::fmt;

pub use interface_custom::{CustomConfig, CustomLlm};
pub use interface_ollama::{OllamaConfig, OllamaLlm};

// ---------------------------------------------------------------------------
// Message / Params
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    System,
    User,
    Assistant,
}

impl Role {
    pub fn as_str(&self) -> &'static str {
        match self {
            Role::System => "system",
            Role::User => "user",
            Role::Assistant => "assistant",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatMessage {
    pub role: Role,
    pub content: String,
}

impl ChatMessage {
    pub fn system(content: impl Into<String>) -> Self {
        Self { role: Role::System, content: content.into() }
    }

    pub fn user(content: impl Into<String>) -> Self {
        Self { role: Role::User, content: content.into() }
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Self { role: Role::Assistant, content: content.into() }
    }
}

/// 생성 파라미터. `None`이면 백엔드/모델 기본값을 따른다.
#[derive(Debug, Clone, PartialEq)]
pub struct GenerationParams {
    /// 0 이하이면 greedy 디코딩
    pub temperature: f32,
    pub top_p: Option<f32>,
    pub top_k: Option<i32>,
    pub repeat_penalty: Option<f32>,
    /// 최대 생성 토큰 수
    pub max_tokens: u32,
    pub seed: Option<u32>,
    /// 이 문자열이 생성되면 중단 (결과에는 포함되지 않음)
    pub stop: Vec<String>,
}

impl Default for GenerationParams {
    fn default() -> Self {
        // 번역 용도이므로 낮은 temperature를 기본값으로 둔다
        Self {
            temperature: 0.2,
            top_p: Some(0.9),
            top_k: Some(40),
            repeat_penalty: None,
            max_tokens: 1024,
            seed: None,
            stop: Vec::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// Error
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub enum LlmError {
    /// load() 전에 생성 요청
    NotLoaded,
    /// 모델 파일이 없거나 서버에 모델이 없음
    ModelNotFound(String),
    /// 서버 연결/HTTP 오류
    Http(String),
    /// 응답 파싱 오류
    InvalidResponse(String),
    /// 프롬프트가 컨텍스트 길이를 초과
    ContextOverflow { prompt_tokens: usize, max_tokens: u32, n_ctx: u32 },
    /// 백엔드 내부 오류 (llama.cpp 등)
    Backend(String),
    /// 해당 기능 없이 빌드됨
    Unsupported(String),
}

impl fmt::Display for LlmError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LlmError::NotLoaded => write!(f, "model is not loaded (call load() first)"),
            LlmError::ModelNotFound(m) => write!(f, "model not found: {m}"),
            LlmError::Http(e) => write!(f, "http error: {e}"),
            LlmError::InvalidResponse(e) => write!(f, "invalid response: {e}"),
            LlmError::ContextOverflow { prompt_tokens, max_tokens, n_ctx } => write!(
                f,
                "context overflow: prompt {prompt_tokens} + max_tokens {max_tokens} > n_ctx {n_ctx}"
            ),
            LlmError::Backend(e) => write!(f, "backend error: {e}"),
            LlmError::Unsupported(e) => write!(f, "unsupported: {e}"),
        }
    }
}

impl std::error::Error for LlmError {}

pub type LlmResult<T> = Result<T, LlmError>;

// ---------------------------------------------------------------------------
// Trait
// ---------------------------------------------------------------------------

pub trait LlmInterface {
    /// 로그용 백엔드 이름
    fn name(&self) -> &str;

    /// 모델 준비 (가중치 로딩 / 서버 연결 확인)
    fn load(&mut self) -> LlmResult<()>;

    fn is_loaded(&self) -> bool;

    /// 대화 메시지를 넣고 어시스턴트 응답 텍스트를 받는다
    fn chat(&mut self, messages: &[ChatMessage], params: &GenerationParams) -> LlmResult<String>;

    /// system + user 한 턴짜리 편의 함수
    fn generate(
        &mut self,
        system: Option<&str>,
        prompt: &str,
        params: &GenerationParams,
    ) -> LlmResult<String> {
        let mut messages = Vec::with_capacity(2);
        if let Some(s) = system {
            messages.push(ChatMessage::system(s));
        }
        messages.push(ChatMessage::user(prompt));
        self.chat(&messages, params)
    }
}

/// 생성 결과에서 가장 먼저 나타나는 stop 문자열 앞까지 자른다.
pub(crate) fn truncate_at_stop(text: &str, stop: &[String]) -> Option<usize> {
    stop.iter()
        .filter(|s| !s.is_empty())
        .filter_map(|s| text.find(s.as_str()))
        .min()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stop_truncation() {
        let stop = vec!["</s>".to_string(), "\n\n".to_string()];
        assert_eq!(truncate_at_stop("hello\n\nworld</s>", &stop), Some(5));
        assert_eq!(truncate_at_stop("hello", &stop), None);
        assert_eq!(truncate_at_stop("hello", &[String::new()]), None);
    }
}

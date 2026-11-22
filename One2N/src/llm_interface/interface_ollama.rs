// Ollama 백엔드
//
// Ollama 서버(기본 http://localhost:11434)의 REST API를 호출한다.
//   GET  /api/tags  : 설치된 모델 목록 (load 시 모델 존재 확인)
//   POST /api/pull  : 모델 다운로드 (auto_pull = true 일 때)
//   POST /api/chat  : 대화 생성 (stream = false)

use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use ureq::Agent;

use super::{ChatMessage, GenerationParams, LlmError, LlmInterface, LlmResult};

#[derive(Debug, Clone)]
pub struct OllamaConfig {
    pub host: String,
    /// 예: "gemma3:4b", "qwen2.5:7b"
    pub model: String,
    /// 요청 타임아웃 (모델 첫 로딩까지 고려해 넉넉하게)
    pub timeout: Duration,
    /// 서버에 모델이 없으면 load() 시 자동으로 pull
    pub auto_pull: bool,
    /// 컨텍스트 길이 (None이면 Ollama 기본값)
    pub num_ctx: Option<u32>,
    /// 요청 후 모델을 메모리에 유지할 시간 (예: "10m", "-1" = 무기한)
    pub keep_alive: Option<String>,
}

impl Default for OllamaConfig {
    fn default() -> Self {
        Self {
            host: "http://localhost:11434".to_string(),
            model: String::new(),
            timeout: Duration::from_secs(300),
            auto_pull: false,
            num_ctx: None,
            keep_alive: Some("10m".to_string()),
        }
    }
}

impl OllamaConfig {
    pub fn new(model: impl Into<String>) -> Self {
        Self { model: model.into(), ..Self::default() }
    }
}

pub struct OllamaLlm {
    config: OllamaConfig,
    agent: Agent,
    loaded: bool,
}

// --- API types --------------------------------------------------------------

#[derive(Serialize)]
struct ApiMessage<'a> {
    role: &'a str,
    content: &'a str,
}

#[derive(Serialize)]
struct ChatRequest<'a> {
    model: &'a str,
    messages: Vec<ApiMessage<'a>>,
    stream: bool,
    options: Map<String, Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    keep_alive: Option<&'a str>,
}

#[derive(Deserialize)]
struct ChatResponse {
    message: Option<ResponseMessage>,
    error: Option<String>,
}

#[derive(Deserialize)]
struct ResponseMessage {
    content: String,
}

#[derive(Deserialize)]
struct TagsResponse {
    models: Vec<TagModel>,
}

#[derive(Deserialize)]
struct TagModel {
    name: String,
}

#[derive(Deserialize)]
struct ErrorBody {
    error: String,
}

// --- impl -------------------------------------------------------------------

impl OllamaLlm {
    pub fn new(config: OllamaConfig) -> Self {
        let agent: Agent = Agent::config_builder()
            .timeout_global(Some(config.timeout))
            // 4xx/5xx 응답 본문의 에러 메시지를 직접 읽기 위해 끈다
            .http_status_as_error(false)
            .build()
            .into();
        Self { config, agent, loaded: false }
    }

    pub fn config(&self) -> &OllamaConfig {
        &self.config
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.config.host.trim_end_matches('/'), path)
    }

    /// 서버에 설치된 모델 이름 목록
    pub fn list_models(&self) -> LlmResult<Vec<String>> {
        let mut res = self
            .agent
            .get(&self.url("/api/tags"))
            .call()
            .map_err(|e| LlmError::Http(format!("cannot reach ollama at {}: {e}", self.config.host)))?;
        let tags: TagsResponse = read_json(&mut res)?;
        Ok(tags.models.into_iter().map(|m| m.name).collect())
    }

    /// 모델 다운로드 (완료될 때까지 블로킹)
    pub fn pull_model(&self) -> LlmResult<()> {
        let mut res = self
            .agent
            .post(&self.url("/api/pull"))
            .config()
            .timeout_global(None)
            .build()
            .send_json(json!({ "model": self.config.model, "stream": false }))
            .map_err(|e| LlmError::Http(e.to_string()))?;
        let _: Value = read_json(&mut res)?;
        Ok(())
    }

    fn has_model(&self, installed: &[String]) -> bool {
        let want = &self.config.model;
        // 태그 생략 시 ollama는 ":latest"로 취급
        installed
            .iter()
            .any(|m| m == want || (!want.contains(':') && *m == format!("{want}:latest")))
    }

    fn build_options(&self, params: &GenerationParams) -> Map<String, Value> {
        let mut o = Map::new();
        o.insert("temperature".into(), json!(params.temperature.max(0.0)));
        o.insert("num_predict".into(), json!(params.max_tokens));
        if let Some(v) = params.top_p {
            o.insert("top_p".into(), json!(v));
        }
        if let Some(v) = params.top_k {
            o.insert("top_k".into(), json!(v));
        }
        if let Some(v) = params.repeat_penalty {
            o.insert("repeat_penalty".into(), json!(v));
        }
        if let Some(v) = params.seed {
            o.insert("seed".into(), json!(v));
        }
        if !params.stop.is_empty() {
            o.insert("stop".into(), json!(params.stop));
        }
        if let Some(v) = self.config.num_ctx {
            o.insert("num_ctx".into(), json!(v));
        }
        o
    }
}

impl LlmInterface for OllamaLlm {
    fn name(&self) -> &str {
        "ollama"
    }

    fn load(&mut self) -> LlmResult<()> {
        if self.config.model.is_empty() {
            return Err(LlmError::ModelNotFound("model name is empty".into()));
        }
        let installed = self.list_models()?;
        if !self.has_model(&installed) {
            if !self.config.auto_pull {
                return Err(LlmError::ModelNotFound(format!(
                    "{} (installed: {}) - run `ollama pull {}` or set auto_pull",
                    self.config.model,
                    installed.join(", "),
                    self.config.model
                )));
            }
            self.pull_model()?;
        }
        self.loaded = true;
        Ok(())
    }

    fn is_loaded(&self) -> bool {
        self.loaded
    }

    fn chat(&mut self, messages: &[ChatMessage], params: &GenerationParams) -> LlmResult<String> {
        if !self.loaded {
            return Err(LlmError::NotLoaded);
        }

        let body = ChatRequest {
            model: &self.config.model,
            messages: messages
                .iter()
                .map(|m| ApiMessage { role: m.role.as_str(), content: &m.content })
                .collect(),
            stream: false,
            options: self.build_options(params),
            keep_alive: self.config.keep_alive.as_deref(),
        };

        let mut res = self
            .agent
            .post(&self.url("/api/chat"))
            .send_json(&body)
            .map_err(|e| LlmError::Http(e.to_string()))?;
        let parsed: ChatResponse = read_json(&mut res)?;

        if let Some(err) = parsed.error {
            return Err(LlmError::Backend(err));
        }
        parsed
            .message
            .map(|m| m.content)
            .ok_or_else(|| LlmError::InvalidResponse("missing `message` field".into()))
    }
}

/// 상태 코드 확인 후 JSON 본문 파싱
fn read_json<T: for<'de> Deserialize<'de>>(res: &mut ureq::http::Response<ureq::Body>) -> LlmResult<T> {
    let status = res.status();
    let text = res
        .body_mut()
        .read_to_string()
        .map_err(|e| LlmError::Http(e.to_string()))?;

    if !status.is_success() {
        let msg = serde_json::from_str::<ErrorBody>(&text).map(|e| e.error).unwrap_or(text);
        return Err(if status.as_u16() == 404 {
            LlmError::ModelNotFound(msg)
        } else {
            LlmError::Http(format!("{status}: {msg}"))
        });
    }
    serde_json::from_str(&text).map_err(|e| LlmError::InvalidResponse(format!("{e}: {text}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_name_matching() {
        let llm = OllamaLlm::new(OllamaConfig::new("gemma3"));
        assert!(llm.has_model(&["gemma3:latest".into()]));
        assert!(!llm.has_model(&["gemma3:270m".into()]));

        let llm = OllamaLlm::new(OllamaConfig::new("gemma3:270m"));
        assert!(llm.has_model(&["gemma3:270m".into()]));
    }

    #[test]
    fn chat_before_load_fails() {
        let mut llm = OllamaLlm::new(OllamaConfig::new("x"));
        assert!(matches!(
            llm.generate(None, "hi", &GenerationParams::default()),
            Err(LlmError::NotLoaded)
        ));
    }

    /// 로컬 ollama 서버가 떠 있을 때만: `cargo test -- --ignored ollama_live`
    /// 모델은 OLLAMA_TEST_MODEL 환경변수 (기본 gemma3:270m)
    #[test]
    #[ignore]
    fn ollama_live() {
        let model = std::env::var("OLLAMA_TEST_MODEL").unwrap_or_else(|_| "gemma3:270m".into());
        let mut llm = OllamaLlm::new(OllamaConfig::new(model));
        llm.load().unwrap();
        let params = GenerationParams { max_tokens: 32, seed: Some(1), ..Default::default() };
        let out = llm
            .generate(Some("Translate to English. Output only the translation."), "안녕하세요", &params)
            .unwrap();
        println!("{out}");
        assert!(!out.trim().is_empty());
    }
}

// Custom 백엔드 (llama.cpp 직접 로딩)
//
// GGUF 가중치 파일을 llama.cpp(llama-cpp-2 crate)로 직접 불러와 추론한다.
// llama.cpp를 소스 빌드하므로 cmake, C/C++ 컴파일러, libclang(bindgen)이 필요하다.
//
//   cargo build --features llama-cpp     # CPU
//   cargo build --features cuda          # NVIDIA GPU
//
// feature 없이 빌드하면 load()가 LlmError::Unsupported를 반환한다.

use std::path::PathBuf;

use super::{ChatMessage, GenerationParams, LlmError, LlmInterface, LlmResult};

#[derive(Debug, Clone)]
pub struct CustomConfig {
    /// GGUF 파일 경로
    pub model_path: PathBuf,
    /// 컨텍스트 길이 (None이면 모델 학습 길이 사용)
    pub n_ctx: Option<u32>,
    /// 프롬프트 처리 배치 크기
    pub n_batch: u32,
    /// GPU에 올릴 레이어 수 (0 = CPU only, u32::MAX = 전부)
    pub n_gpu_layers: u32,
    /// 추론 스레드 수 (None이면 llama.cpp 기본값)
    pub n_threads: Option<i32>,
    /// 모델 내장 chat template 대신 사용할 템플릿 (예: "chatml", "llama3", "gemma")
    /// None이면 GGUF에 내장된 템플릿을 쓰고, 없으면 chatml로 대체
    pub chat_template: Option<String>,
}

impl Default for CustomConfig {
    fn default() -> Self {
        Self {
            model_path: PathBuf::new(),
            n_ctx: Some(4096),
            n_batch: 512,
            n_gpu_layers: 0,
            n_threads: None,
            chat_template: None,
        }
    }
}

impl CustomConfig {
    pub fn new(model_path: impl Into<PathBuf>) -> Self {
        Self { model_path: model_path.into(), ..Self::default() }
    }
}

pub struct CustomLlm {
    config: CustomConfig,
    #[cfg(feature = "llama-cpp")]
    model: Option<backend::Loaded>,
}

impl CustomLlm {
    pub fn new(config: CustomConfig) -> Self {
        Self {
            config,
            #[cfg(feature = "llama-cpp")]
            model: None,
        }
    }

    pub fn config(&self) -> &CustomConfig {
        &self.config
    }
}

impl LlmInterface for CustomLlm {
    fn name(&self) -> &str {
        "llama.cpp"
    }

    fn load(&mut self) -> LlmResult<()> {
        if !self.config.model_path.is_file() {
            return Err(LlmError::ModelNotFound(self.config.model_path.display().to_string()));
        }
        #[cfg(feature = "llama-cpp")]
        {
            self.model = Some(backend::Loaded::load(&self.config)?);
            Ok(())
        }
        #[cfg(not(feature = "llama-cpp"))]
        {
            Err(LlmError::Unsupported(
                "built without llama.cpp support; rebuild with `--features llama-cpp`".into(),
            ))
        }
    }

    fn is_loaded(&self) -> bool {
        #[cfg(feature = "llama-cpp")]
        {
            self.model.is_some()
        }
        #[cfg(not(feature = "llama-cpp"))]
        {
            false
        }
    }

    #[allow(unused_variables)]
    fn chat(&mut self, messages: &[ChatMessage], params: &GenerationParams) -> LlmResult<String> {
        #[cfg(feature = "llama-cpp")]
        {
            let loaded = self.model.as_ref().ok_or(LlmError::NotLoaded)?;
            loaded.chat(&self.config, messages, params)
        }
        #[cfg(not(feature = "llama-cpp"))]
        {
            Err(LlmError::NotLoaded)
        }
    }
}

// ---------------------------------------------------------------------------
// llama.cpp 구현부
// ---------------------------------------------------------------------------

#[cfg(feature = "llama-cpp")]
mod backend {
    use std::num::NonZeroU32;
    use std::sync::OnceLock;
    use std::time::{SystemTime, UNIX_EPOCH};

    use llama_cpp_2::context::params::LlamaContextParams;
    use llama_cpp_2::llama_backend::LlamaBackend;
    use llama_cpp_2::llama_batch::LlamaBatch;
    use llama_cpp_2::model::params::LlamaModelParams;
    use llama_cpp_2::model::{LlamaChatMessage, LlamaChatTemplate, LlamaModel};
    use llama_cpp_2::sampling::LlamaSampler;

    use super::CustomConfig;
    use crate::llm_interface::{ChatMessage, GenerationParams, LlmError, LlmResult, truncate_at_stop};

    /// llama.cpp 백엔드는 프로세스당 한 번만 초기화할 수 있다
    static BACKEND: OnceLock<LlamaBackend> = OnceLock::new();

    fn backend() -> LlmResult<&'static LlamaBackend> {
        if let Some(b) = BACKEND.get() {
            return Ok(b);
        }
        let b = LlamaBackend::init().map_err(be)?;
        Ok(BACKEND.get_or_init(|| b))
    }

    fn be(e: impl std::fmt::Display) -> LlmError {
        LlmError::Backend(e.to_string())
    }

    pub struct Loaded {
        model: LlamaModel,
        template: LlamaChatTemplate,
    }

    impl Loaded {
        pub fn load(config: &CustomConfig) -> LlmResult<Self> {
            let backend = backend()?;
            let params = LlamaModelParams::default().with_n_gpu_layers(config.n_gpu_layers);
            let model = LlamaModel::load_from_file(backend, &config.model_path, &params).map_err(be)?;

            let template = match &config.chat_template {
                Some(name) => LlamaChatTemplate::new(name).map_err(be)?,
                None => match model.chat_template(None) {
                    Ok(t) => t,
                    Err(_) => LlamaChatTemplate::new("chatml").map_err(be)?,
                },
            };
            Ok(Self { model, template })
        }

        pub fn chat(
            &self,
            config: &CustomConfig,
            messages: &[ChatMessage],
            params: &GenerationParams,
        ) -> LlmResult<String> {
            let backend = backend()?;

            // 1. chat template 적용
            let chat = messages
                .iter()
                .map(|m| LlamaChatMessage::new(m.role.as_str().to_string(), m.content.clone()))
                .collect::<Result<Vec<_>, _>>()
                .map_err(be)?;
            let prompt = self.model.apply_chat_template(&self.template, &chat, true).map_err(be)?;

            // 2. 토큰화 (템플릿에 BOS 등 특수 토큰이 이미 들어있으므로 add_special = false)
            let vocab = self.model.vocab();
            let tokens = vocab.tokenize(prompt.as_bytes(), false, true);
            if tokens.is_empty() {
                return Err(LlmError::Backend("empty prompt after tokenization".into()));
            }

            // 3. 컨텍스트 생성 (요청마다 새로 만들어 이전 대화의 KV 캐시가 섞이지 않게 함)
            let n_ctx = config.n_ctx.unwrap_or_else(|| self.model.n_ctx_train());
            if tokens.len() + params.max_tokens as usize > n_ctx as usize {
                return Err(LlmError::ContextOverflow {
                    prompt_tokens: tokens.len(),
                    max_tokens: params.max_tokens,
                    n_ctx,
                });
            }
            let n_batch = config.n_batch.max(1);
            let mut ctx_params = LlamaContextParams::default()
                .with_n_ctx(NonZeroU32::new(n_ctx))
                .with_n_batch(n_batch);
            if let Some(t) = config.n_threads {
                ctx_params = ctx_params.with_n_threads(t).with_n_threads_batch(t);
            }
            let mut ctx = self.model.new_context(backend, ctx_params).map_err(be)?;

            // 4. 프롬프트 디코딩 (n_batch 단위로 나눠서)
            let mut batch = LlamaBatch::new(n_batch as usize, 1);
            let last = tokens.len() - 1;
            for (chunk_idx, chunk) in tokens.chunks(n_batch as usize).enumerate() {
                batch.clear();
                for (i, &tok) in chunk.iter().enumerate() {
                    let pos = chunk_idx * n_batch as usize + i;
                    batch.add(tok, pos as i32, &[0], pos == last).map_err(be)?;
                }
                ctx.decode(&mut batch).map_err(be)?;
            }

            // 5. 샘플링 루프
            let mut sampler = build_sampler(&self.model, params);
            let mut pos = tokens.len() as i32;
            let mut out_bytes: Vec<u8> = Vec::new();

            for _ in 0..params.max_tokens {
                let token = sampler.sample(&ctx, batch.n_tokens() - 1);
                if vocab.is_eog(token) {
                    break;
                }
                out_bytes.extend(vocab.token_to_piece(token, false, None));

                if !params.stop.is_empty() {
                    let text = String::from_utf8_lossy(&out_bytes);
                    if let Some(cut) = truncate_at_stop(&text, &params.stop) {
                        return Ok(text[..cut].to_string());
                    }
                }

                batch.clear();
                batch.add(token, pos, &[0], true).map_err(be)?;
                pos += 1;
                ctx.decode(&mut batch).map_err(be)?;
            }

            Ok(String::from_utf8_lossy(&out_bytes).into_owned())
        }
    }

    fn build_sampler(model: &LlamaModel, p: &GenerationParams) -> LlamaSampler {
        let mut chain = Vec::new();
        if let Some(r) = p.repeat_penalty {
            chain.push(LlamaSampler::penalties(model.n_vocab(), 64, r, 0.0, 0.0));
        }
        if p.temperature <= 0.0 {
            chain.push(LlamaSampler::greedy());
        } else {
            if let Some(k) = p.top_k {
                chain.push(LlamaSampler::top_k(k));
            }
            if let Some(tp) = p.top_p {
                chain.push(LlamaSampler::top_p(tp, 1));
            }
            chain.push(LlamaSampler::temp(p.temperature));
            let seed = p.seed.unwrap_or_else(|| {
                SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos() as u32).unwrap_or(0)
            });
            chain.push(LlamaSampler::dist(seed));
        }
        LlamaSampler::chain_simple(chain)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_model_file() {
        let mut llm = CustomLlm::new(CustomConfig::new("/nonexistent/model.gguf"));
        assert!(matches!(llm.load(), Err(LlmError::ModelNotFound(_))));
        assert!(!llm.is_loaded());
    }

    /// GGUF 파일이 있을 때만:
    /// LLAMA_TEST_MODEL=/path/model.gguf cargo test --features llama-cpp -- --ignored custom_live
    #[test]
    #[ignore]
    fn custom_live() {
        let path = std::env::var("LLAMA_TEST_MODEL").expect("set LLAMA_TEST_MODEL");
        let mut llm = CustomLlm::new(CustomConfig::new(path));
        llm.load().unwrap();
        let params = GenerationParams { max_tokens: 32, seed: Some(1), ..Default::default() };
        let out = llm
            .generate(Some("Translate to English. Output only the translation."), "안녕하세요", &params)
            .unwrap();
        println!("{out}");
        assert!(!out.trim().is_empty());
    }
}

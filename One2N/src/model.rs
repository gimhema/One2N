use crate::llm_interface::{GenerationParams, LlmError, LlmInterface, LlmResult};
use crate::setting::Language;

/// LLM 백엔드를 감싸고 번역 프롬프트를 구성해 추론을 수행한다.
pub struct Model
{
    system_prompt : String,
    params : GenerationParams,
    backend : Option<Box<dyn LlmInterface>>
}

impl Model
{
    pub fn new() -> Self
    {
        return Self {
            system_prompt : "".to_string(),
            params : GenerationParams::default(),
            backend : None
        }
    }

    /// 사용할 LLM 백엔드를 지정 (기존 백엔드는 교체됨)
    pub fn set_backend(&mut self, backend : Box<dyn LlmInterface>)
    {
        self.backend = Some(backend);
    }

    /// 기본 번역 지시문 대신 사용할 시스템 프롬프트 (빈 문자열이면 기본값 사용)
    #[allow(dead_code)]
    pub fn set_system_prompt(&mut self, prompt : impl Into<String>)
    {
        self.system_prompt = prompt.into();
    }

    #[allow(dead_code)]
    pub fn set_params(&mut self, params : GenerationParams)
    {
        self.params = params;
    }

    /// 백엔드 모델 준비 (가중치 로딩 / 서버 연결 확인)
    pub fn load(&mut self) -> LlmResult<()>
    {
        let backend = self.backend.as_mut().ok_or(LlmError::NotLoaded)?;
        println!("  backend : {}", backend.name());
        backend.load()
    }

    pub fn is_loaded(&self) -> bool
    {
        self.backend.as_ref().is_some_and(|b| b.is_loaded())
    }

    /// `text`(source 언어)를 target 언어로 번역한 결과를 반환한다.
    pub fn inference(&mut self, text : &str, source : Language, target : Language) -> LlmResult<String>
    {
        if text.trim().is_empty() {
            return Ok(String::new());
        }

        let system = self.build_system_prompt(source, target);
        let backend = self.backend.as_mut().ok_or(LlmError::NotLoaded)?;
        let output = backend.generate(Some(&system), text, &self.params)?;
        Ok(output.trim().to_string())
    }

    fn build_system_prompt(&self, source : Language, target : Language) -> String
    {
        let src = if source == Language::Default { "the source language".to_string() } else { source.name().to_string() };
        let tgt = target.name();

        let mut prompt = format!(
            "You are a professional translator. Translate the user's text from {src} into {tgt}.\n\
             Rules:\n\
             - Output only the translated text, with no explanations, notes, or quotes.\n\
             - Preserve the original meaning, tone, line breaks, and formatting.\n\
             - Keep placeholders, variables, tags, and code (e.g. {{0}}, %s, <b>, \\n) unchanged.\n\
             - Do not translate proper nouns that are normally left as-is."
        );
        if !self.system_prompt.trim().is_empty() {
            prompt.push_str("\n\n");
            prompt.push_str(self.system_prompt.trim());
        }
        prompt
    }
}

use crate::llm_interface::{CustomConfig, CustomLlm, LlmError, LlmInterface, LlmResult, OllamaConfig, OllamaLlm};
use crate::model::Model;
use crate::setting::Language;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ModelOption {
    Default,
    Ollama,
    GGUF
}

pub struct ModelAgent
{
    model : Model,
    model_path : String,
    model_option : ModelOption
}

impl ModelAgent
{
    pub fn new() -> Self {
        return Self{
            model : Model::new(),
            model_path : "".to_string(),
            model_option : ModelOption::Default
        }
    }

    pub fn init(&mut self, _path : String, _opt : ModelOption)
    {
        self.model_path = _path;
        self.model_option = _opt;
    }

    /// 설정된 ModelOption에 맞는 LLM 백엔드를 생성하고 모델을 로딩한다.
    pub fn load_model(&mut self) -> LlmResult<()>
    {
        let current_option : ModelOption = self.model_option;

        let backend : Box<dyn LlmInterface> = match current_option {
            ModelOption::Default => {
                return Err(LlmError::Unsupported("model option is not set (Ollama | GGUF)".into()));
            },
            // Ollama : model_path = 모델 이름 (예: gemma3:4b)
            ModelOption::Ollama => Box::new(OllamaLlm::new(OllamaConfig::new(self.model_path.clone()))),
            // GGUF : model_path = 가중치 파일 경로
            ModelOption::GGUF => Box::new(CustomLlm::new(CustomConfig::new(self.model_path.clone())))
        };

        self.model.set_backend(backend);
        self.model.load()
    }

    #[allow(dead_code)]
    pub fn is_loaded(&self) -> bool
    {
        self.model.is_loaded()
    }

    /// 입력 텍스트를 source 언어에서 target 언어로 번역한 결과를 반환한다.
    #[allow(dead_code)]
    pub fn generate(&mut self, text : &str, source : Language, target : Language) -> LlmResult<String>
    {
        self.model.inference(text, source, target)
    }
}

use crate::model;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ModelOption {
    Default,
    Ollama,
    GGUF
}

pub struct ModelAgent
{
    model_path : String,
    model_option : ModelOption
}

impl ModelAgent
{
    pub fn new() -> Self {
        return Self{
            model_path : "".to_string(),
            model_option : ModelOption::Default
        }
    }

    pub fn init(&mut self, _path : String, _opt : ModelOption) 
    {
        self.model_path = _path;
        self.model_option = _opt;
    }

    pub fn load_model(&mut self)
    {
        let current_option : ModelOption = self.model_option;

        match current_option {
            ModelOption::Default => {},
            ModelOption::Ollama => {},
            ModelOption::GGUF => {}
        }  
    }
    
    pub fn generate(&mut self) 
    {

    }
}
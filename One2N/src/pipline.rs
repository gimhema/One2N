
use crate::input::*;
use crate::output::*;
use crate::setting::*;
use crate::agent::*;

pub struct TransPipline
{
    setting_manager : SettingManager,
    input_module : OneInput,
    output_module : NOutput,
    model_agent : ModelAgent
}

impl TransPipline
{
    pub fn new() -> Self {
        Self {
            setting_manager: SettingManager::new(),
            input_module: OneInput::new(),
            output_module: NOutput::new(),
            model_agent : ModelAgent::new()
        }
    }

    pub fn init(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        println!("Start Setup");

        // 세팅정보 읽기
        println!("Read Setting Info . . . .");
        self.setting_manager.read_ini(DEFAULT_SETTING_PATH)?;
        println!(
            "  model : {:?} ({})\n  input : {}\n  output: {}",
            self.setting_manager.model_load_opt(),
            self.setting_manager.model_path(),
            self.setting_manager.input_lang().code(),
            self.setting_manager
                .output_lang_sorted()
                .iter()
                .map(|l| l.code())
                .collect::<Vec<_>>()
                .join(", ")
        );

        self.model_agent.init(
            self.setting_manager.model_path().to_string(),
            self.setting_manager.model_load_opt(),
        );

        // 모델 로딩
        self.model_agent.load_model();
        Ok(())
    }

    pub fn go(&mut self) {

        // 등록된 입력 리스트 입력 모듈로 전달
        println!("Input Setp . . . .");

        // 입력모듈이 모델 에이전트에게 추론 및 생성 지시
        println!("Translate Step . . . .");

        // 에이전트가 입력데이터에 대한 출력값을 생성
        println!("Generate Completed.");
    }   
}
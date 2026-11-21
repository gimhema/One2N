
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

    pub fn init(&mut self) {
        println!("Start Setup");        

        // 모델 로딩
        self.model_agent.load_model();
    }

    pub fn go(&mut self) {

        // 세팅정보 읽기
        println!("Read Setting Info . . . .");

        // 등록된 입력 리스트 입력 모듈로 전달
        println!("Input Setp . . . .");

        // 입력모듈이 모델 에이전트에게 추론 및 생성 지시
        println!("Translate Step . . . .");

        // 에이전트가 입력데이터에 대한 출력값을 생성
        println!("Generate Completed.");
    }   
}
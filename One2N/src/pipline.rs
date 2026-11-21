
use crate::input::*;
use crate::output::*;
use crate::setting::*;

pub struct TransPipline
{
    setting_manager : 
    input_module : OneInput,
    output_module : NOutput
}

impl TransPipline
{
    pub fn new() -> Self {

        return {
            OneInput::new(),
            NOutput::new()
        }
    }
    pub fn go() {

        // 세팅정보 읽기
        // 등록된 입력 리스트 입력 모듈로 전달
        // 입력모듈이 모델 에이전트에게 추론 및 생성 지시
        // 에이전트가 입력데이터에 대한 출력값을 생성

    }   
}
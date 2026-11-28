use crate::pipline::TransPipline;

mod agent;
mod ini_parser;
mod input;
mod llm_interface;
mod output;
mod pipline;
mod prompt;
mod tsv_parser;
mod setting;
mod model;

fn main() {

    let mut one_to_n_pipiline = TransPipline::new();

    if let Err(e) = one_to_n_pipiline.init() {
        eprintln!("Setup failed: {e}");
        std::process::exit(1);
    }

    one_to_n_pipiline.go();

}

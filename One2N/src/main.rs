use crate::pipline::TransPipline;

mod agent;
mod input;
mod output;
mod pipline;
mod prompt;
mod tsv_parser;
mod setting;

fn main() {

    let mut one_to_n_pipiline = TransPipline::new();

    one_to_n_pipiline.init();

    one_to_n_pipiline.go();

}

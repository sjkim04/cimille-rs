pub mod engine;
pub mod eval;
pub mod search;
pub mod uci;

use std::io::{self, BufRead};
use engine::Engine;

fn main() {
    let mut engine = Engine::new();
    let stdin = io::stdin();
    let reader = stdin.lock();

    for line in reader.lines() {
        if let Ok(line) = line {
            let line = line.trim();
            if !line.is_empty() {
                engine.handle_command(line);
            }
        }
    }
}
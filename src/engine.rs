use cozy_chess::util::parse_uci_move;
use cozy_chess::{Board, Color};
use std::io::{self, Write};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};

use crate::search;
use crate::syzygy;
use crate::uci;

pub struct Engine {
    board: Board,
    stop_flag: Arc<AtomicBool>,
    search_thread: Option<JoinHandle<()>>,
    position_history: Vec<u64>,
}

impl Engine {
    pub fn new() -> Self {
        Engine {
            board: Board::default(),
            stop_flag: Arc::new(AtomicBool::new(false)),
            search_thread: None,
            position_history: Vec::new(),
        }
    }

    pub fn handle_command(&mut self, line: &str) {
        let tokens: Vec<&str> = line.split_whitespace().collect();
        match tokens.first().copied() {
            Some("uci") => self.uci(),
            Some("isready") => {
                println!("readyok");
                let _ = io::stdout().flush();
            }
            Some("setoption") => self.setoption(&tokens),
            Some("position") => self.position(&tokens),
            Some("go") => self.go(&tokens),
            Some("stop") => {
                self.stop_flag.store(true, Ordering::Relaxed);
                if let Some(handle) = self.search_thread.take() {
                    let _ = handle.join();
                }
            }
            Some("ucinewgame") => self.ucinewgame(),
            Some("quit") => {
                self.stop_flag.store(true, Ordering::Relaxed);
                if let Some(handle) = self.search_thread.take() {
                    let _ = handle.join();
                }
                std::process::exit(0)
            }
            _ => {}
        }
    }

    fn uci(&self) {
        println!("id name Cimille 0.1.0");
        println!("id author Ssimille, Phrygia");
        println!("option name SyzygyPath type string default <empty>");
        println!("uciok");
        let _ = io::stdout().flush();
    }

    fn setoption(&mut self, tokens: &[&str]) {
        let name_index = tokens.iter().position(|&t| t == "name");
        let value_index = tokens.iter().position(|&t| t == "value");

        let Some(name_index) = name_index else {
            return;
        };

        let name_end = value_index.unwrap_or(tokens.len());
        if name_index + 1 >= name_end {
            return;
        }

        let name = tokens[name_index + 1..name_end].join(" ");
        let value = if let Some(v_idx) = value_index {
            if v_idx + 1 < tokens.len() {
                tokens[v_idx + 1..].join(" ")
            } else {
                String::new()
            }
        } else {
            String::new()
        };

        if name.eq_ignore_ascii_case("SyzygyPath") {
            if let Err(err) = syzygy::set_path(&value) {
                println!("info string failed to set SyzygyPath: {}", err);
            } else if value.is_empty() || value == "<empty>" {
                println!("info string Syzygy tablebase disabled");
            } else {
                println!("info string SyzygyPath set to {}", value);
            }
            let _ = io::stdout().flush();
        }
    }

    fn position(&mut self, tokens: &[&str]) {
        if tokens.len() < 2 {
            return;
        }
        match tokens[1] {
            "startpos" => {
                self.board = Board::default();
                self.position_history.clear();
                self.position_history.push(self.board.hash());

                if let Some(moves_index) = tokens.iter().position(|&t| t == "moves") {
                    for mv_str in &tokens[moves_index + 1..] {
                        if let Ok(mv) = parse_uci_move(&self.board, mv_str) {
                            self.board.play_unchecked(mv);
                            self.position_history.push(self.board.hash());
                        }
                    }
                }
            }
            "fen" => {
                let fen_parts: Vec<&str> = tokens[2..]
                    .iter()
                    .take_while(|&&t| t != "moves")
                    .cloned()
                    .collect();
                let fen = fen_parts.join(" ");
                if let Ok(board) = fen.parse() {
                    self.board = board;
                    self.position_history.clear();
                    self.position_history.push(self.board.hash());

                    if let Some(moves_index) = tokens.iter().position(|&t| t == "moves") {
                        for mv_str in &tokens[moves_index + 1..] {
                            if let Ok(mv) = parse_uci_move(&self.board, mv_str) {
                                self.board.play_unchecked(mv);
                                self.position_history.push(self.board.hash());
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }

    fn go(&mut self, tokens: &[&str]) {
        // Stop any previous search
        self.stop_flag.store(true, Ordering::Relaxed);
        if let Some(handle) = self.search_thread.take() {
            let _ = handle.join();
        }
        self.stop_flag.store(false, Ordering::Relaxed);

        // Defaults
        let mut depth: u32 = 6;
        let mut depth_specified = false;
        let mut movetime: Option<u64> = None;
        let mut wtime: Option<u64> = None;
        let mut btime: Option<u64> = None;
        let mut winc: u64 = 0;
        let mut binc: u64 = 0;
        let mut infinite = false;

        let mut i = 1;
        while i < tokens.len() {
            match tokens[i] {
                "wtime" if i + 1 < tokens.len() => {
                    wtime = tokens[i + 1].parse().ok();
                    i += 2;
                }
                "btime" if i + 1 < tokens.len() => {
                    btime = tokens[i + 1].parse().ok();
                    i += 2;
                }
                "winc" if i + 1 < tokens.len() => {
                    winc = tokens[i + 1].parse().unwrap_or(0);
                    i += 2;
                }
                "binc" if i + 1 < tokens.len() => {
                    binc = tokens[i + 1].parse().unwrap_or(0);
                    i += 2;
                }
                "movetime" if i + 1 < tokens.len() => {
                    movetime = tokens[i + 1].parse().ok();
                    i += 2;
                }
                "depth" if i + 1 < tokens.len() => {
                    depth = tokens[i + 1].parse().unwrap_or(depth);
                    depth_specified = true;
                    i += 2;
                }
                "infinite" => {
                    infinite = true;
                    i += 1;
                }
                _ => {
                    i += 1;
                }
            }
        }

        // Plain "go" (no limits) behaves like UCI infinite
        if !depth_specified && movetime.is_none() && wtime.is_none() && btime.is_none() {
            infinite = true;
        }

        let time_budget = if infinite {
            u64::MAX
        } else if let Some(mt) = movetime {
            mt
        } else if depth_specified {
            u64::MAX
        } else {
            let (time, inc) = match self.board.side_to_move() {
                Color::White => (wtime, winc),
                Color::Black => (btime, binc),
            };
            time.map(|t| t / 20 + inc / 2).unwrap_or(1_000)
        };

        // If time is given (or infinite) without explicit depth, search as deep as possible within time
        if !depth_specified && (infinite || wtime.is_some() || btime.is_some()) {
            depth = 250;
        }

        // Run search in background so stop can be honored immediately
        let board_clone = self.board.clone();
        let stop_flag = self.stop_flag.clone();
        let game_history = self.position_history.clone();

        self.search_thread = Some(thread::spawn(move || {
            let result =
                search::search(&board_clone, depth, time_budget, &stop_flag, &game_history);

            if let Some(best) = result.best_move {
                println!("bestmove {}", uci::move_to_uci(&board_clone, best));
            } else {
                println!("bestmove 0000");
            }
            let _ = io::stdout().flush();
        }));
    }

    fn ucinewgame(&mut self) {
        self.board = Board::default();
        self.stop_flag.store(false, Ordering::Relaxed);
        self.position_history.clear();
    }
}

impl Default for Engine {
    fn default() -> Self {
        Self::new()
    }
}

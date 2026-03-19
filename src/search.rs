use crate::eval::{self, CHECKMATE_SCORE};
use crate::uci::move_to_uci;
use cozy_chess::{Board, GameStatus, Move};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

pub struct SearchResult {
    pub best_move: Option<Move>,
    pub score: i32,
    pub nodes: u64,
    pub depth: u32,
    pub pv: Vec<Move>,
}

pub fn search(
    board: &Board,
    max_depth: u32,
    max_time_ms: u64,
    is_stopped: &AtomicBool,
    game_history: &[u64],
) -> SearchResult {
    let start_time = Instant::now();
    let mut best_move_overall: Option<Move> = None;
    let mut best_score_overall = -CHECKMATE_SCORE * 2;
    let mut nodes_searched = 0u64;

    for current_depth in 1..=max_depth {
        if is_stopped.load(Ordering::Relaxed)
            || start_time.elapsed().as_millis() as u64 >= max_time_ms
        {
            break;
        }

        // ply 0 is the root
        let (mov, score, pv) = get_best_move(
            board,
            current_depth,
            &start_time,
            max_time_ms,
            is_stopped,
            &mut nodes_searched,
            game_history,
        );

        let time_elapsed = start_time.elapsed().as_millis() as u64;

        // If we ran out of time or got stopped during this depth, keep a fallback move
        if is_stopped.load(Ordering::Relaxed) || time_elapsed >= max_time_ms {
            if best_move_overall.is_none() {
                best_move_overall = mov;
                best_score_overall = score;
            }
            break;
        }

        best_move_overall = mov;
        best_score_overall = score;
        let pv_line = pv.clone();

        let nps = if time_elapsed > 0 {
            (nodes_searched as f64 / (time_elapsed as f64 / 1000.0)) as u64
        } else {
            0
        };

        let score_string = if score.abs() > CHECKMATE_SCORE - 1000 {
            let plies_to_mate = CHECKMATE_SCORE - score.abs();
            let moves_to_mate = (plies_to_mate + 1) / 2;
            format!(
                "mate {}",
                if score > 0 {
                    moves_to_mate
                } else {
                    -moves_to_mate
                }
            )
        } else {
            format!("cp {}", score)
        };

        // Build PV string
        let mut pv_uci = String::new();
        let mut temp_board = board.clone();
        let mut position_hashes = vec![temp_board.hash()];

        for m in &pv_line {
            if !pv_uci.is_empty() {
                pv_uci.push(' ');
            }
            pv_uci.push_str(&move_to_uci(&temp_board, *m));
            temp_board.play_unchecked(*m);

            // Stop PV at repetition
            let hash = temp_board.hash();
            if position_hashes.contains(&hash) {
                break; // Would be repetition
            }
            position_hashes.push(hash);
        }

        println!(
            "info depth {} score {} nodes {} nps {} time {} pv {}",
            current_depth, score_string, nodes_searched, nps, time_elapsed, pv_uci
        );

        if score.abs() >= CHECKMATE_SCORE - 100 {
            break;
        }
    }

    SearchResult {
        best_move: best_move_overall,
        score: best_score_overall,
        nodes: nodes_searched,
        depth: max_depth,
        pv: Vec::new(),
    }
}

fn get_best_move(
    board: &Board,
    depth: u32,
    start_time: &Instant,
    max_time_ms: u64,
    is_stopped: &AtomicBool,
    nodes: &mut u64,
    game_history: &[u64],
) -> (Option<Move>, i32, Vec<Move>) {
    let mut best_move = None;
    let mut best_pv = Vec::new();
    let mut alpha = -CHECKMATE_SCORE * 2;
    let beta = CHECKMATE_SCORE * 2;

    let mut moves = Vec::new();
    board.generate_moves(|m| {
        moves.extend(m);
        false
    });
    moves.sort_by_cached_key(|m| -move_order_score(board, m));

    // Initialize search history (separate from game history)
    let mut search_history = Vec::new();

    for mov in moves {
        let mut new_board = board.clone();
        new_board.play_unchecked(mov);

        // Start ply at 1 because we just made a move
        let (score, mut child_pv) = negamax(
            &new_board,
            depth - 1,
            1,
            -beta,
            -alpha,
            start_time,
            max_time_ms,
            is_stopped,
            nodes,
            &mut search_history,
            game_history,
        );
        let score = -score;

        if score > alpha {
            alpha = score;
            best_move = Some(mov);
            child_pv.insert(0, mov);
            best_pv = child_pv;
        }
    }
    (best_move, alpha, best_pv)
}

fn negamax(
    board: &Board,
    depth: u32,
    ply: i32,
    mut alpha: i32,
    beta: i32,
    start_time: &Instant,
    max_time_ms: u64,
    is_stopped: &AtomicBool,
    nodes: &mut u64,
    search_history: &mut Vec<u64>,
    game_history: &[u64],
) -> (i32, Vec<Move>) {
    if (*nodes).is_multiple_of(1024)
        && (is_stopped.load(Ordering::Relaxed)
            || start_time.elapsed().as_millis() as u64 >= max_time_ms)
    {
        is_stopped.store(true, Ordering::Relaxed);
        return (0, Vec::new());
    }

    // Check for repetition draw (check both game history and search tree)
    let current_hash = board.hash();
    if game_history.contains(&current_hash) || search_history.contains(&current_hash) {
        return (0, Vec::new()); // Draw by repetition
    }

    match board.status() {
        // FIXED: Return score relative to how many moves it took to get here
        GameStatus::Won => return (-CHECKMATE_SCORE + ply, Vec::new()),
        GameStatus::Drawn => return (0, Vec::new()),
        _ => {}
    }

    if depth == 0 {
        let (score, pv) = quiescence(board, alpha, beta, ply, nodes, search_history, game_history);
        return (score, pv);
    }

    let mut moves = Vec::new();
    board.generate_moves(|m| {
        moves.extend(m);
        false
    });
    moves.sort_by_cached_key(|m| -move_order_score(board, m));

    let mut best_score = -CHECKMATE_SCORE * 2;
    let mut best_pv = Vec::new();

    // Add current position to search history
    search_history.push(current_hash);

    for mov in moves {
        let mut new_board = board.clone();
        new_board.play_unchecked(mov);

        let (score, mut child_pv) = negamax(
            &new_board,
            depth - 1,
            ply + 1,
            -beta,
            -alpha,
            start_time,
            max_time_ms,
            is_stopped,
            nodes,
            search_history,
            game_history,
        );
        let score = -score;

        if score >= beta {
            child_pv.insert(0, mov);
            search_history.pop(); // Remove current position before returning
            return (beta, child_pv);
        }
        if score > best_score {
            best_score = score;
            child_pv.insert(0, mov);
            best_pv = child_pv;
            if score > alpha {
                alpha = score;
            }
        }
    }

    // Remove current position from search history
    search_history.pop();
    (best_score, best_pv)
}

fn quiescence(
    board: &Board,
    mut alpha: i32,
    beta: i32,
    ply: i32,
    nodes: &mut u64,
    search_history: &mut Vec<u64>,
    game_history: &[u64],
) -> (i32, Vec<Move>) {
    *nodes += 1;

    // Check for repetition draw
    let current_hash = board.hash();
    if game_history.contains(&current_hash) || search_history.contains(&current_hash) {
        return (0, Vec::new());
    }

    // Check if position is terminal
    match board.status() {
        GameStatus::Won => return (-CHECKMATE_SCORE + ply, Vec::new()),
        GameStatus::Drawn => return (0, Vec::new()),
        _ => {}
    }

    let stand_pat = eval::evaluate(board, 0);

    if stand_pat >= beta {
        return (beta, Vec::new());
    }
    if stand_pat > alpha {
        alpha = stand_pat;
    }

    let mut captures = Vec::new();
    board.generate_moves(|moves| {
        for mov in moves {
            if board.piece_on(mov.to).is_some() {
                captures.push(mov);
            }
        }
        false
    });
    captures.sort_by_cached_key(|m| -move_order_score(board, m));

    search_history.push(current_hash);
    let mut best_pv = Vec::new();

    for mov in captures {
        let mut new_board = board.clone();
        new_board.play_unchecked(mov);
        let (score, mut child_pv) = quiescence(
            &new_board,
            -beta,
            -alpha,
            ply + 1,
            nodes,
            search_history,
            game_history,
        );
        let score = -score;

        if score >= beta {
            child_pv.insert(0, mov);
            search_history.pop();
            return (beta, child_pv);
        }
        if score > alpha {
            child_pv.insert(0, mov);
            best_pv = child_pv;
            alpha = score;
        }
    }

    search_history.pop();
    (alpha, best_pv)
}

fn move_order_score(board: &Board, mov: &Move) -> i32 {
    let mut score = 0;
    if let Some(captured) = board.piece_on(mov.to) {
        score +=
            10 * eval::piece_value(captured) - eval::piece_value(board.piece_on(mov.from).unwrap());
    }
    if mov.promotion.is_some() {
        score += 800;
    }
    score
}

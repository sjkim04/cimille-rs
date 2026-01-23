use cozy_chess::{Board, Move, Color, GameStatus};
use crate::eval::{self, CHECKMATE_SCORE};
use crate::uci::move_to_uci;
use std::time::Instant;
use std::sync::atomic::{AtomicBool, Ordering};

pub struct SearchResult {
    pub best_move: Option<Move>,
    pub score: i32,
    pub nodes: u64,
    pub depth: u32,
}

pub fn search(
    board: &Board,
    max_depth: u32,
    max_time_ms: u64,
    is_stopped: &AtomicBool,
) -> SearchResult {
    let start_time = Instant::now();
    let mut best_move_overall: Option<Move> = None;
    let mut best_score_overall = -CHECKMATE_SCORE * 2;
    let mut nodes_searched = 0u64;

    for current_depth in 1..=max_depth {
        if is_stopped.load(Ordering::Relaxed) || start_time.elapsed().as_millis() as u64 >= max_time_ms {
            break;
        }

        let (mov, score) = get_best_move(
            board,
            current_depth,
            &start_time,
            max_time_ms,
            is_stopped,
            &mut nodes_searched,
        );

        if is_stopped.load(Ordering::Relaxed) {
            break;
        }

        best_move_overall = mov;
        best_score_overall = score;

        let time_elapsed = start_time.elapsed().as_millis() as u64;
        let nps = if time_elapsed > 0 {
            (nodes_searched as f64 / (time_elapsed as f64 / 1000.0)) as u64
        } else {
            0
        };

        // UCI expects score from White's perspective
        let score_white = if board.side_to_move() == Color::White { score } else { -score };
        
        println!(
            "info depth {} score cp {} nodes {} nps {} time {} pv {}",
            current_depth,
            score_white,
            nodes_searched,
            nps,
            time_elapsed,
            best_move_overall.map(|m| move_to_uci(&board, m)).unwrap_or_default()
        );

        // Break if mate found
        if score.abs() >= CHECKMATE_SCORE - (current_depth as i32 + 1) {
            break;
        }
    }

    SearchResult {
        best_move: best_move_overall,
        score: best_score_overall,
        nodes: nodes_searched,
        depth: max_depth,
    }
}

fn get_best_move(
    board: &Board,
    depth: u32,
    start_time: &Instant,
    max_time_ms: u64,
    is_stopped: &AtomicBool,
    nodes: &mut u64,
) -> (Option<Move>, i32) {
    let mut best_move = None;
    let mut alpha = -CHECKMATE_SCORE * 2;
    let beta = CHECKMATE_SCORE * 2;

    let mut moves = Vec::new();
    board.generate_moves(|m| {
        moves.extend(m);
        false
    });

    // Move ordering
    moves.sort_by_cached_key(|m| -move_order_score(board, m));

    for mov in moves {
        let mut new_board = board.clone();
        new_board.play_unchecked(mov);

        // Negamax call: Note the negative sign and swapped/negated alpha/beta
        let score = -negamax(&new_board, depth - 1, -beta, -alpha, start_time, max_time_ms, is_stopped, nodes);

        if score > alpha {
            alpha = score;
            best_move = Some(mov);
        }
    }

    (best_move, alpha)
}

fn negamax(
    board: &Board,
    depth: u32,
    mut alpha: i32,
    beta: i32,
    start_time: &Instant,
    max_time_ms: u64,
    is_stopped: &AtomicBool,
    nodes: &mut u64,
) -> i32 {
    if *nodes % 1024 == 0 && (is_stopped.load(Ordering::Relaxed) || start_time.elapsed().as_millis() as u64 >= max_time_ms) {
        return 0; // The actual value won't matter as the search will discard this result
    }

    match board.status() {
        GameStatus::Won => return -CHECKMATE_SCORE + (depth as i32), // Loss for the side-to-move
        GameStatus::Drawn => return 0,
        _ => {}
    }

    if depth == 0 {
        *nodes += 1;
        return eval::evaluate(board, depth);
    }

    let mut moves = Vec::new();
    board.generate_moves(|m| {
        moves.extend(m);
        false
    });

    let mut best_score = -CHECKMATE_SCORE * 2;

    for mov in moves {
        let mut new_board = board.clone();
        new_board.play_unchecked(mov);

        let score = -negamax(&new_board, depth - 1, -beta, -alpha, start_time, max_time_ms, is_stopped, nodes);

        if score >= beta {
            return beta; // Pruning
        }
        if score > best_score {
            best_score = score;
            if score > alpha {
                alpha = score;
            }
        }
    }
    best_score
}

// Minimal fast move ordering (Avoids board cloning)
fn move_order_score(board: &Board, mov: &Move) -> i32 {
    let mut score = 0;
    if let Some(captured) = board.piece_on(mov.to) {
        score += 10 * eval::piece_value(captured) - eval::piece_value(board.piece_on(mov.from).unwrap());
    }
    if mov.promotion.is_some() {
        score += 800;
    }
    score
}
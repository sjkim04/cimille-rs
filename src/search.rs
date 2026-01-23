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

        // ply 0 is the root
        let (mov, score) = get_best_move(
            board,
            current_depth,
            &start_time,
            max_time_ms,
            is_stopped,
            &mut nodes_searched,
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
        let nps = if time_elapsed > 0 {
            (nodes_searched as f64 / (time_elapsed as f64 / 1000.0)) as u64
        } else {
            0
        };

        // --- STABLE UCI SCORE REPORTING ---
        let score_white = if board.side_to_move() == Color::White { score } else { -score };
        
        let score_string = if score.abs() > CHECKMATE_SCORE - 1000 {
            let plies_to_mate = CHECKMATE_SCORE - score.abs();
            let moves_to_mate = (plies_to_mate + 1) / 2;
            format!("mate {}", if score_white > 0 { moves_to_mate as i32 } else { -(moves_to_mate as i32) })
        } else {
            format!("cp {}", score_white)
        };

        println!(
            "info depth {} score {} nodes {} nps {} time {} pv {}",
            current_depth,
            score_string,
            nodes_searched,
            nps,
            time_elapsed,
            best_move_overall.map(|m| move_to_uci(&board, m)).unwrap_or_default()
        );

        if score.abs() >= CHECKMATE_SCORE - 100 { break; }
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
    board.generate_moves(|m| { moves.extend(m); false });
    moves.sort_by_cached_key(|m| -move_order_score(board, m));

    for mov in moves {
        let mut new_board = board.clone();
        new_board.play_unchecked(mov);

        // Start ply at 1 because we just made a move
        let score = -negamax(&new_board, depth - 1, 1, -beta, -alpha, start_time, max_time_ms, is_stopped, nodes);

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
    ply: i32,
    mut alpha: i32,
    beta: i32,
    start_time: &Instant,
    max_time_ms: u64,
    is_stopped: &AtomicBool,
    nodes: &mut u64,
) -> i32 {
    if *nodes % 1024 == 0 && (is_stopped.load(Ordering::Relaxed) || start_time.elapsed().as_millis() as u64 >= max_time_ms) {
        is_stopped.store(true, Ordering::Relaxed);
        return 0; 
    }

    match board.status() {
        // FIXED: Return score relative to how many moves it took to get here
        GameStatus::Won => return -CHECKMATE_SCORE + ply, 
        GameStatus::Drawn => return 0,
        _ => {}
    }

    if depth == 0 {
        return quiescence(board, alpha, beta, ply, nodes);
    }

    let mut moves = Vec::new();
    board.generate_moves(|m| { moves.extend(m); false });
    moves.sort_by_cached_key(|m| -move_order_score(board, m));

    let mut best_score = -CHECKMATE_SCORE * 2;

    for mov in moves {
        let mut new_board = board.clone();
        new_board.play_unchecked(mov);

        let score = -negamax(&new_board, depth - 1, ply + 1, -beta, -alpha, start_time, max_time_ms, is_stopped, nodes);

        if score >= beta { return beta; }
        if score > best_score {
            best_score = score;
            if score > alpha { alpha = score; }
        }
    }
    best_score
}

fn quiescence(board: &Board, mut alpha: i32, beta: i32, ply: i32, nodes: &mut u64) -> i32 {
    *nodes += 1;
    
    // Check if position is terminal
    match board.status() {
        GameStatus::Won => return -CHECKMATE_SCORE + ply,
        GameStatus::Drawn => return 0,
        _ => {}
    }
    
    let stand_pat = eval::evaluate(board, 0);
    
    if stand_pat >= beta { return beta; }
    if stand_pat > alpha { alpha = stand_pat; }

    let mut captures = Vec::new();
    board.generate_moves(|moves| {
        for mov in moves {
            if board.piece_on(mov.to).is_some() { captures.push(mov); }
        }
        false
    });
    captures.sort_by_cached_key(|m| -move_order_score(board, m));

    for mov in captures {
        let mut new_board = board.clone();
        new_board.play_unchecked(mov);
        let score = -quiescence(&new_board, -beta, -alpha, ply + 1, nodes);
        if score >= beta { return beta; }
        if score > alpha { alpha = score; }
    }
    alpha
}

fn move_order_score(board: &Board, mov: &Move) -> i32 {
    let mut score = 0;
    if let Some(captured) = board.piece_on(mov.to) {
        score += 10 * eval::piece_value(captured) - eval::piece_value(board.piece_on(mov.from).unwrap());
    }
    if mov.promotion.is_some() { score += 800; }
    score
}
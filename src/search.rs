use cozy_chess::{Board, Move};
use crate::eval::{self, CHECKMATE_SCORE};
use std::time::Instant;
use std::sync::atomic::{AtomicBool, Ordering};

pub struct SearchResult {
    pub best_move: Option<Move>,
    pub score: i32,
    pub nodes: u64,
    pub depth: u32,
}

struct MoveScore {
    mov: Move,
    score: i32,
}

pub fn search(
    board: &Board,
    max_depth: u32,
    max_time_ms: u64,
    is_stopped: &AtomicBool,
) -> SearchResult {
    let start_time = Instant::now();
    let mut best_move_overall: Option<MoveScore> = None;
    let mut nodes_searched = 0u64;

    for current_depth in 1..=max_depth {
        let time_elapsed = start_time.elapsed().as_millis() as u64;
        let time_left = max_time_ms.saturating_sub(time_elapsed);

        if is_stopped.load(Ordering::Relaxed) {
            break;
        }
        if max_time_ms != u64::MAX {
            if time_left == 0 {
                break;
            }
            // Minimum time check for next depth
            if current_depth > 1 && time_left < 50 {
                break;
            }
        }

        // Stop early if we found a forced mate
        if let Some(ref best) = best_move_overall {
            if best.score.abs() >= CHECKMATE_SCORE - (current_depth as i32 + 1) {
                break;
            }
        }

        let depth_result = get_best_move(
            board,
            current_depth,
            &start_time,
            max_time_ms,
            is_stopped,
            &mut nodes_searched,
        );

        // Check if search was stopped
        if is_stopped.load(Ordering::Relaxed)
            || (max_time_ms != u64::MAX && start_time.elapsed().as_millis() as u64 > max_time_ms)
        {
            break;
        }

        if !depth_result.is_empty() {
            best_move_overall = Some(depth_result[0].clone());

            let time_elapsed = start_time.elapsed().as_millis() as u64;
            let nps = if time_elapsed > 0 {
                (nodes_searched as f64 / (time_elapsed as f64 / 1000.0)) as u64
            } else {
                0
            };

            // UCI always wants scores from White's perspective
            // Minimax returns scores from White's perspective, so use directly
            let score_cp = best_move_overall.as_ref().unwrap().score;
            
            println!(
                "info depth {} score cp {} nodes {} nps {} time {} pv {}",
                current_depth,
                score_cp,
                nodes_searched,
                nps,
                time_elapsed,
                format_move(&best_move_overall.as_ref().unwrap().mov)
            );

            // Stop if checkmate found
            if best_move_overall.as_ref().unwrap().score.abs()
                >= CHECKMATE_SCORE - (current_depth as i32 + 1)
            {
                break;
            }
        } else {
            break;
        }
    }

    SearchResult {
        best_move: best_move_overall.as_ref().map(|ms| ms.mov),
        score: best_move_overall.as_ref().map_or(0, |ms| ms.score),
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
) -> Vec<MoveScore> {
    let mut legal_moves = Vec::new();
    board.generate_moves(|moves| {
        legal_moves.extend(moves);
        false
    });

    if legal_moves.is_empty() {
        return Vec::new();
    }

    let mut scored_moves = Vec::new();
    let mut alpha = i32::MIN;
    let beta = i32::MAX;

    // Sort moves for better pruning
    legal_moves.sort_by_cached_key(|m| -move_order_score(board, m));

    for mov in legal_moves {
        if is_stopped.load(Ordering::Relaxed)
            || (max_time_ms != u64::MAX && start_time.elapsed().as_millis() as u64 > max_time_ms)
        {
            break;
        }

        let mut new_board = board.clone();
        new_board.play_unchecked(mov);

        // Call minimax for the resulting position
        let score = minimax(&new_board, depth - 1, alpha, beta, start_time, max_time_ms, is_stopped, nodes);

        // Update alpha based on side to move at root
        if board.side_to_move() == cozy_chess::Color::White {
            alpha = alpha.max(score);
        } else {
            // For Black at root, we're minimizing, so update beta instead
            // But since we're collecting all moves, we don't actually update beta here
        }

        scored_moves.push(MoveScore { mov, score });

        if alpha >= beta {
            break;
        }
    }

    // Sort based on who's to move
    if board.side_to_move() == cozy_chess::Color::White {
        scored_moves.sort_by(|a, b| b.score.cmp(&a.score)); // Descending for White (maximize)
    } else {
        scored_moves.sort_by(|a, b| a.score.cmp(&b.score)); // Ascending for Black (minimize)
    }
    
    scored_moves
}

fn minimax(
    board: &Board,
    depth: u32,
    mut alpha: i32,
    mut beta: i32,
    start_time: &Instant,
    max_time_ms: u64,
    is_stopped: &AtomicBool,
    nodes: &mut u64,
) -> i32 {
    if is_stopped.load(Ordering::Relaxed)
        || (max_time_ms != u64::MAX && start_time.elapsed().as_millis() as u64 > max_time_ms)
    {
        return eval::evaluate(board, depth);
    }

    // Check for terminal positions
    match board.status() {
        cozy_chess::GameStatus::Won => {
            return -(CHECKMATE_SCORE - depth as i32);
        }
        cozy_chess::GameStatus::Drawn => {
            return 0;
        }
        cozy_chess::GameStatus::Ongoing => {}
    }

    if depth == 0 {
        *nodes += 1;
        return eval::evaluate(board, depth);
    }

    let mut legal_moves = Vec::new();
    board.generate_moves(|moves| {
        legal_moves.extend(moves);
        false
    });

    // Sort for better pruning
    legal_moves.sort_by_cached_key(|m| -move_order_score(board, m));

    let is_white = board.side_to_move() == cozy_chess::Color::White;
    let mut best_score = if is_white { i32::MIN } else { i32::MAX };

    for mov in legal_moves {
        if is_stopped.load(Ordering::Relaxed)
            || (max_time_ms != u64::MAX && start_time.elapsed().as_millis() as u64 > max_time_ms)
        {
            return eval::evaluate(board, depth);
        }

        let mut new_board = board.clone();
        new_board.play_unchecked(mov);

        let mut score = minimax(&new_board, depth - 1, alpha, beta, start_time, max_time_ms, is_stopped, nodes);

        // Adjust mate scores
        if score.abs() >= CHECKMATE_SCORE - (depth as i32 + 1) {
            if score > 0 {
                score -= 1;
            } else {
                score += 1;
            }
        }

        if is_white {
            // Maximizing player (White)
            best_score = best_score.max(score);
            alpha = alpha.max(score);
            if alpha >= beta {
                break; // Beta cutoff
            }
        } else {
            // Minimizing player (Black)
            best_score = best_score.min(score);
            beta = beta.min(score);
            if alpha >= beta {
                break; // Alpha cutoff
            }
        }
    }

    best_score
}

fn move_order_score(board: &Board, mov: &Move) -> i32 {
    let mut score = 0;

    // Get piece being moved and captured piece
    let from = mov.from;
    let to = mov.to;
    
    let piece = board.piece_on(from);
    let captured = board.piece_on(to);

    // MVV-LVA: prioritize captures
    if let Some(victim) = captured {
        let victim_value = eval::piece_value(victim);
        let attacker_value = piece.map_or(0, |p| eval::piece_value(p));
        score += victim_value * 10 - attacker_value + 1000;
    }

    // Prioritize promotions
    if mov.promotion.is_some() {
        score += 300;
    }

    // Prioritize checks (test by making move)
    let mut test_board = board.clone();
    test_board.play_unchecked(*mov);
    if !test_board.checkers().is_empty() {
        score += 500;
    }

    score
}

fn format_move(mov: &Move) -> String {
    let from = format!("{}", mov.from);
    let to = format!("{}", mov.to);
    let promotion = mov.promotion.map_or(String::new(), |p| {
        match p {
            cozy_chess::Piece::Queen => "q",
            cozy_chess::Piece::Rook => "r",
            cozy_chess::Piece::Bishop => "b",
            cozy_chess::Piece::Knight => "n",
            _ => "",
        }.to_string()
    });
    format!("{}{}{}", from, to, promotion)
}

impl Clone for MoveScore {
    fn clone(&self) -> Self {
        MoveScore {
            mov: self.mov,
            score: self.score,
        }
    }
}

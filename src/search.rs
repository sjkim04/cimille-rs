use crate::syzygy;

use crate::eval::{self, CHECKMATE_SCORE};
use crate::uci::move_to_uci;
use cozy_chess::{Board, GameStatus, Move};
use pyrrhic_rs::{DtzProbeValue, Piece as TbPiece, WdlProbeResult};
use std::collections::{HashMap, HashSet};
use std::io::{self, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

pub struct SearchResult {
    pub best_move: Option<Move>,
    pub score: i32,
    pub nodes: u64,
    pub depth: u32,
    pub pv: Vec<Move>,
}

#[derive(Clone, Copy)]
enum BoundType {
    Exact,
    Lower,
    Upper,
}

#[derive(Clone, Copy)]
pub struct TTEntry {
    best_move: Option<Move>,
    score: i32,
    bound: BoundType,
    depth: u32,
    age: u16,
}

const TIME_CHECK_MASK: u64 = 0x1ff;
const TT_MAX_AGE_GAP: u16 = 8;

fn score_to_tt(score: i32, ply: i32) -> i32 {
    if score >= CHECKMATE_SCORE - 1000 {
        score + ply
    } else if score <= -CHECKMATE_SCORE + 1000 {
        score - ply
    } else {
        score
    }
}

fn score_from_tt(score: i32, ply: i32) -> i32 {
    if score >= CHECKMATE_SCORE - 1000 {
        score - ply
    } else if score <= -CHECKMATE_SCORE + 1000 {
        score + ply
    } else {
        score
    }
}

fn store_tt(tt: &mut HashMap<u64, TTEntry>, hash: u64, entry: TTEntry) {
    if let Some(old) = tt.get(&hash)
        && old.depth > entry.depth
        && old.age >= entry.age
    {
            return;
        }
    tt.insert(hash, entry);
}

fn tt_best_move(tt: &HashMap<u64, TTEntry>, hash: u64, current_age: u16) -> Option<Move> {
    tt.get(&hash).and_then(|entry| {
        if current_age.saturating_sub(entry.age) <= TT_MAX_AGE_GAP {
            entry.best_move
        } else {
            None
        }
    })
}

fn age_is_fresh(entry_age: u16, current_age: u16) -> bool {
    current_age.saturating_sub(entry_age) <= TT_MAX_AGE_GAP
}

fn should_stop(start_time: &Instant, max_time_ms: u64, is_stopped: &AtomicBool, nodes: u64) -> bool {
    if is_stopped.load(Ordering::Relaxed) {
        return true;
    }
    if max_time_ms == u64::MAX {
        return false;
    }
    if (nodes & TIME_CHECK_MASK) != 0 {
        return false;
    }
    start_time.elapsed().as_millis() as u64 >= max_time_ms
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
    let mut emitted_info = false;
    let mut tt: HashMap<u64, TTEntry> = HashMap::new();
    let game_history_set: HashSet<u64> = game_history.iter().copied().collect();

    let mut root_moves = Vec::with_capacity(64);
    board.generate_moves(|moves| {
        root_moves.extend(moves);
        false
    });
    root_moves.sort_unstable_by_key(|m| -move_order_score(board, m));
    if let Some(first_move) = root_moves.first().copied() {
        best_move_overall = Some(first_move);
    }

    for current_depth in 1..=max_depth {
        let tt_age = current_depth as u16;
        if current_depth.is_multiple_of(4) {
            tt.retain(|_, e| age_is_fresh(e.age, tt_age));
        }

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
            &game_history_set,
            &mut tt,
            tt_age,
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
        let pv_line = &pv;

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

        for m in pv_line {
            if !pv_uci.is_empty() {
                pv_uci.push(' ');
            }
            pv_uci.push_str(&move_to_uci(&temp_board, *m));
            temp_board.play_unchecked(*m);

            // Stop PV if the line reaches a terminal or repeated position
            if matches!(temp_board.status(), GameStatus::Won | GameStatus::Drawn) {
                break;
            }
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
        let _ = io::stdout().flush();
        emitted_info = true;

        if score.abs() >= CHECKMATE_SCORE - 100 {
            break;
        }
    }

    if !emitted_info {
        let elapsed = start_time.elapsed().as_millis() as u64;
        println!(
            "info depth 0 score cp 0 nodes {} nps 0 time {} pv",
            nodes_searched, elapsed
        );
        let _ = io::stdout().flush();
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
    game_history: &HashSet<u64>,
    tt: &mut HashMap<u64, TTEntry>,
    tt_age: u16,
) -> (Option<Move>, i32, Vec<Move>) {
    if board.occupied().len() <= 5
        && let Some((tb_move, tb_score)) = tablebase_root_choice(board) {
            return (Some(tb_move), tb_score, vec![tb_move]);
        }

    let mut best_move = None;
    let mut best_pv = Vec::with_capacity(depth as usize);
    let mut alpha = -CHECKMATE_SCORE * 2;
    let beta = CHECKMATE_SCORE * 2;
    let current_hash = board.hash();

    let mut moves = Vec::with_capacity(64);
    board.generate_moves(|m| {
        moves.extend(m);
        false
    });
    moves.sort_unstable_by_key(|m| -move_order_score(board, m));
    if best_move.is_none() {
        best_move = moves.first().copied();
    }
    if let Some(ttm) = tt_best_move(tt, current_hash, tt_age)
        && let Some(pos) = moves.iter().position(|&m| m == ttm) {
            moves.swap(0, pos);
        }

    // Initialize search history (separate from game history)
    let mut search_history = HashSet::new();

    for mov in moves {
        if is_stopped.load(Ordering::Relaxed)
            || (max_time_ms != u64::MAX && start_time.elapsed().as_millis() as u64 >= max_time_ms)
        {
            is_stopped.store(true, Ordering::Relaxed);
            break;
        }

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
            tt,
            tt_age,
        );
        let score = -score;

        if is_stopped.load(Ordering::Relaxed)
            || (max_time_ms != u64::MAX && start_time.elapsed().as_millis() as u64 >= max_time_ms)
        {
            is_stopped.store(true, Ordering::Relaxed);
            break;
        }

        if score > alpha {
            alpha = score;
            best_move = Some(mov);
            child_pv.insert(0, mov);
            best_pv = child_pv;
        }
    }
    (best_move, alpha, best_pv)
}

fn tablebase_root_choice(board: &Board) -> Option<(Move, i32)> {
    let root = syzygy::probe_root(board).ok()?;
    let DtzProbeValue::DtzResult(best) = root.root else {
        return None;
    };

    let mov = Move {
        from: cozy_chess::Square::index(best.from_square as usize),
        to: cozy_chess::Square::index(best.to_square as usize),
        promotion: match best.promotion {
            TbPiece::Knight => Some(cozy_chess::Piece::Knight),
            TbPiece::Bishop => Some(cozy_chess::Piece::Bishop),
            TbPiece::Rook => Some(cozy_chess::Piece::Rook),
            TbPiece::Queen => Some(cozy_chess::Piece::Queen),
            _ => None,
        },
    };

    Some((mov, wdl_to_score(best.wdl)))
}

fn wdl_to_score(wdl: WdlProbeResult) -> i32 {
    match wdl {
        WdlProbeResult::Win => 2000,
        WdlProbeResult::CursedWin => 0,
        WdlProbeResult::Draw => 0,
        WdlProbeResult::BlessedLoss => 0,
        WdlProbeResult::Loss => -2000,
    }
}

fn negamax(
    board: &Board,
    depth: u32,
    ply: i32,
    mut alpha: i32,
    mut beta: i32,
    start_time: &Instant,
    max_time_ms: u64,
    is_stopped: &AtomicBool,
    nodes: &mut u64,
    search_history: &mut HashSet<u64>,
    game_history: &HashSet<u64>,
    tt: &mut HashMap<u64, TTEntry>,
    tt_age: u16,
) -> (i32, Vec<Move>) {
    *nodes += 1;

    if should_stop(start_time, max_time_ms, is_stopped, *nodes)
    {
        is_stopped.store(true, Ordering::Relaxed);
        return (0, Vec::new());
    }

    let current_hash = board.hash();

    // Check for repetition draw (check both game history and search tree)
    if game_history.contains(&current_hash) || search_history.contains(&current_hash) {
        return (0, Vec::new()); // Draw by repetition
    }

    match board.status() {
        // FIXED: Return score relative to how many moves it took to get here
        GameStatus::Won => return (-CHECKMATE_SCORE + ply, Vec::new()),
        GameStatus::Drawn => return (0, Vec::new()),
        _ => {}
    }

    let tt_move = tt_best_move(tt, current_hash, tt_age);
    if let Some(entry) = tt.get(&current_hash).copied()
        && age_is_fresh(entry.age, tt_age)
        && entry.depth >= depth {
        let tt_score = score_from_tt(entry.score, ply);
        let tt_pv = entry.best_move.map_or_else(Vec::new, |m| vec![m]);
        match entry.bound {
            BoundType::Exact => return (tt_score, tt_pv),
            BoundType::Lower => alpha = alpha.max(tt_score),
            BoundType::Upper => beta = beta.min(tt_score),
        }
        if alpha >= beta {
            return (tt_score, tt_pv);
        }
    }
    

    if board.occupied().len() <= 5
        && let Ok(wdl) = syzygy::probe_wdl(board) {
            return (wdl_to_score(wdl), Vec::new());
        }

    if depth == 0 {
        let (score, pv) = quiescence(
            board,
            alpha,
            beta,
            ply,
            start_time,
            max_time_ms,
            is_stopped,
            nodes,
            search_history,
            game_history,
        );
        return (score, pv);
    }

    let alpha_orig = alpha;

    let mut moves = Vec::with_capacity(64);
    board.generate_moves(|m| {
        moves.extend(m);
        false
    });
    moves.sort_unstable_by_key(|m| -move_order_score(board, m));
    if let Some(ttm) = tt_move
        && let Some(pos) = moves.iter().position(|&m| m == ttm) {
            moves.swap(0, pos);
        }

    let mut best_score = -CHECKMATE_SCORE * 2;
    let mut best_pv = Vec::with_capacity(depth as usize);

    // Add current position to search history
    search_history.insert(current_hash);

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
            tt,
            tt_age,
        );
        let score = -score;

        if score >= beta {
            child_pv.insert(0, mov);
            search_history.remove(&current_hash);
            store_tt(
                tt,
                current_hash,
                TTEntry {
                    best_move: Some(mov),
                    score: score_to_tt(score, ply),
                    bound: BoundType::Lower,
                    depth,
                    age: tt_age,
                },
            );

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
    search_history.remove(&current_hash);
    store_tt(
        tt,
        current_hash,
        TTEntry {
            best_move: best_pv.first().cloned(),
            score: score_to_tt(best_score, ply),
            bound: if best_score <= alpha_orig {
                BoundType::Upper
            } else if best_score >= beta {
                BoundType::Lower
            } else {
                BoundType::Exact
            },
            depth,
            age: tt_age,
        },
    );
    (best_score, best_pv)
}

fn quiescence(
    board: &Board,
    mut alpha: i32,
    beta: i32,
    ply: i32,
    start_time: &Instant,
    max_time_ms: u64,
    is_stopped: &AtomicBool,
    nodes: &mut u64,
    search_history: &mut HashSet<u64>,
    game_history: &HashSet<u64>,
) -> (i32, Vec<Move>) {
    *nodes += 1;

    if should_stop(start_time, max_time_ms, is_stopped, *nodes) {
        is_stopped.store(true, Ordering::Relaxed);
        return (0, Vec::new());
    }

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

    if board.occupied().len() <= 5
        && let Ok(wdl) = syzygy::probe_wdl(board) {
            return (wdl_to_score(wdl), Vec::new());
        }

    let stand_pat = eval::evaluate(board);

    if stand_pat >= beta {
        return (beta, Vec::new());
    }
    if stand_pat > alpha {
        alpha = stand_pat;
    }

    let mut captures = Vec::with_capacity(32);
    board.generate_moves(|moves| {
        for mov in moves {
            if board.piece_on(mov.to).is_some() {
                captures.push(mov);
            }
        }
        false
    });
    captures.sort_unstable_by_key(|m| -move_order_score(board, m));

    search_history.insert(current_hash);
    let mut best_pv = Vec::new();

    for mov in captures {
        let mut new_board = board.clone();
        new_board.play_unchecked(mov);
        let (score, mut child_pv) = quiescence(
            &new_board,
            -beta,
            -alpha,
            ply + 1,
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
            search_history.remove(&current_hash);
            return (beta, child_pv);
        }
        if score > alpha {
            child_pv.insert(0, mov);
            best_pv = child_pv;
            alpha = score;
        }
    }

    search_history.remove(&current_hash);
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

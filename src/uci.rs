use cozy_chess::{Board, Move};
use cozy_chess::util::{parse_uci_move, display_uci_move};

/// Parse a UCI move string (supports Chess960 castling via cozy-chess helper).
pub fn parse_move(board: &Board, mov: &str) -> Option<Move> {
    parse_uci_move(board, mov).ok()
}

/// Convert a move to UCI string (handles Chess960 castling conversion).
pub fn move_to_uci(board: &Board, mov: Move) -> String {
    display_uci_move(board, mov).to_string()
}

/// Format a UCI info line (cp scores expected from White's perspective).
pub fn format_info(depth: u32, score_cp: i32, nodes: u64, nps: u64) -> String {
    format!("info depth {} score cp {} nodes {} nps {}", depth, score_cp, nodes, nps)
}

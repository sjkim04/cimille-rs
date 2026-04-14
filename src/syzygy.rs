use cozy_chess::*;
use once_cell::sync::Lazy;
use pyrrhic_rs::{DtzProbeResult, EngineAdapter, TBError, TableBases, WdlProbeResult};
use std::fmt;
use std::sync::RwLock;

#[derive(Clone)]
struct CozyChessAdapter;

impl EngineAdapter for CozyChessAdapter {
    fn pawn_attacks(color: pyrrhic_rs::Color, square: u64) -> u64 {
        let attacks = get_pawn_attacks(
            Square::index(square as usize),
            if color == pyrrhic_rs::Color::White {
                Color::White
            } else {
                Color::Black
            },
        );
        attacks.0
    }
    fn knight_attacks(square: u64) -> u64 {
        get_knight_moves(Square::index(square as usize)).0
    }
    fn bishop_attacks(square: u64, occupied: u64) -> u64 {
        get_bishop_moves(Square::index(square as usize), BitBoard(occupied)).0
    }
    fn rook_attacks(square: u64, occupied: u64) -> u64 {
        get_rook_moves(Square::index(square as usize), BitBoard(occupied)).0
    }
    fn king_attacks(square: u64) -> u64 {
        get_king_moves(Square::index(square as usize)).0
    }
    fn queen_attacks(square: u64, occupied: u64) -> u64 {
        (get_bishop_moves(Square::index(square as usize), BitBoard(occupied))
            | get_rook_moves(Square::index(square as usize), BitBoard(occupied)))
        .0
    }
}

static TABLEBASE: Lazy<RwLock<Option<TableBases<CozyChessAdapter>>>> =
    Lazy::new(|| RwLock::new(None));

#[derive(Debug)]
pub enum SyzygyError {
    LockPoisoned,
    NotConfigured,
    Tablebase(TBError),
}

impl fmt::Display for SyzygyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SyzygyError::LockPoisoned => write!(f, "tablebase lock poisoned"),
            SyzygyError::NotConfigured => write!(f, "tablebase path is not configured"),
            SyzygyError::Tablebase(err) => write!(f, "tablebase error: {:?}", err),
        }
    }
}

impl std::error::Error for SyzygyError {}

struct BoardBits {
    white: u64,
    black: u64,
    kings: u64,
    queens: u64,
    rooks: u64,
    bishops: u64,
    knights: u64,
    pawns: u64,
    ep: u32,
    turn: bool,
}

fn extract_board(board: &Board) -> BoardBits {
    BoardBits {
        white: board.colors(Color::White).0,
        black: board.colors(Color::Black).0,
        kings: board.pieces(Piece::King).0,
        queens: board.pieces(Piece::Queen).0,
        rooks: board.pieces(Piece::Rook).0,
        bishops: board.pieces(Piece::Bishop).0,
        knights: board.pieces(Piece::Knight).0,
        pawns: board.pieces(Piece::Pawn).0,
        ep: board.en_passant().map(|square| (square as u32) + 1).unwrap_or(0),
        turn: board.side_to_move() == Color::White,
    }
}

pub fn set_path(path: &str) -> Result<(), SyzygyError> {
    let mut tb = TABLEBASE.write().map_err(|_| SyzygyError::LockPoisoned)?;

    if path.is_empty() || path == "<empty>" {
        *tb = None;
        return Ok(());
    }

    let loaded = TableBases::new(path).map_err(SyzygyError::Tablebase)?;
    *tb = Some(loaded);
    Ok(())
}

pub fn probe_wdl(board: &Board) -> Result<WdlProbeResult, SyzygyError> {
    let tb_guard = TABLEBASE.read().map_err(|_| SyzygyError::LockPoisoned)?;
    let tb = tb_guard.as_ref().ok_or(SyzygyError::NotConfigured)?;
    let bits = extract_board(board);

    tb.probe_wdl(
        bits.white,
        bits.black,
        bits.kings,
        bits.queens,
        bits.rooks,
        bits.bishops,
        bits.knights,
        bits.pawns,
        bits.ep,
        bits.turn,
    )
    .map_err(SyzygyError::Tablebase)
}

pub fn probe_root(board: &Board) -> Result<DtzProbeResult, SyzygyError> {
    let tb_guard = TABLEBASE.read().map_err(|_| SyzygyError::LockPoisoned)?;
    let tb = tb_guard.as_ref().ok_or(SyzygyError::NotConfigured)?;
    let bits = extract_board(board);
    let rule50 = board.halfmove_clock() as u32;

    tb.probe_root(
        bits.white,
        bits.black,
        bits.kings,
        bits.queens,
        bits.rooks,
        bits.bishops,
        bits.knights,
        bits.pawns,
        rule50,
        bits.ep,
        bits.turn,
    )
    .map_err(SyzygyError::Tablebase)
}
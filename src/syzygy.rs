use cozy_chess::*;
use once_cell::sync::Lazy;
use pyrrhic_rs::{EngineAdapter, TableBases, WdlProbeResult};
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

pub fn set_path(path: &str) -> Result<(), String> {
    let mut tb = TABLEBASE
        .write()
        .map_err(|_| String::from("tablebase lock poisoned"))?;

    if path.is_empty() || path == "<empty>" {
        *tb = None;
        return Ok(());
    }

    let loaded = TableBases::new(path).map_err(|err| format!("{:?}", err))?;
    *tb = Some(loaded);
    Ok(())
}

pub fn probe_wdl(board: &Board) -> Result<WdlProbeResult, ()> {
    let tb_guard = TABLEBASE.read().map_err(|_| ())?;
    let tb = tb_guard.as_ref().ok_or(())?;

    let white_pieces = board.colors(Color::White);
    let black_pieces = board.colors(Color::Black);
    
    let kings = board.pieces(Piece::King);
    let queens = board.pieces(Piece::Queen);
    let rooks = board.pieces(Piece::Rook);
    let bishops = board.pieces(Piece::Bishop);
    let knights = board.pieces(Piece::Knight);
    let pawns = board.pieces(Piece::Pawn);
    
    let ep = board.en_passant()
        .map(|square| (square as u32) + 1)
        .unwrap_or(0);
    
    let turn = board.side_to_move() == Color::White;
    
    tb.probe_wdl(
        white_pieces.0,
        black_pieces.0,
        kings.0,
        queens.0,
        rooks.0,
        bishops.0,
        knights.0,
        pawns.0,
        ep,
        turn,
    ).map_err(|_| ())
}
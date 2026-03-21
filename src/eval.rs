use cozy_chess::*;

pub const CHECKMATE_SCORE: i32 = 30000;

pub mod pst {
    pub const PAWN: [[i32; 8]; 8] = [
        [0, 0, 0, 0, 0, 0, 0, 0],
        [50, 50, 50, -50, -50, 50, 50, 50],
        [10, 10, 20, 30, 30, 20, 10, 10],
        [5, 5, 10, 25, 25, 10, 5, 5],
        [0, 0, 0, 20, 20, 0, 0, 0],
        [5, -5, -10, 0, 0, -10, -5, 5],
        [5, 10, 10, -20, -20, 10, 10, 5],
        [0, 0, 0, 0, 0, 0, 0, 0],
    ];

    pub const KNIGHT: [[i32; 8]; 8] = [
        [-50, -40, -30, -30, -30, -30, -40, -50],
        [-40, -20, 0, 0, 0, 0, -20, -40],
        [-30, 0, 10, 15, 15, 10, 0, -30],
        [-30, 5, 15, 20, 20, 15, 5, -30],
        [-30, 0, 15, 20, 20, 15, 0, -30],
        [-30, 5, 10, 15, 15, 10, 5, -30],
        [-40, -20, 0, 5, 5, 0, -20, -40],
        [-50, -40, -30, -30, -30, -30, -40, -50],
    ];

    pub const BISHOP: [[i32; 8]; 8] = [
        [-20, -10, -10, -10, -10, -10, -10, -20],
        [-10, 0, 0, 0, 0, 0, 0, -10],
        [-10, 0, 5, 10, 10, 5, 0, -10],
        [-10, 5, 5, 10, 10, 5, 5, -10],
        [-10, 0, 10, 10, 10, 10, 0, -10],
        [-10, 10, 10, 10, 10, 10, 10, -10],
        [-10, 5, 0, 0, 0, 0, 5, -10],
        [-20, -10, -10, -10, -10, -10, -10, -20],
    ];

    pub const ROOK: [[i32; 8]; 8] = [
        [0, 0, 0, 5, 5, 0, 0, 0],
        [-5, 0, 0, 0, 0, 0, 0, -5],
        [-5, 0, 0, 0, 0, 0, 0, -5],
        [-5, 0, 0, 0, 0, 0, 0, -5],
        [-5, 0, 0, 0, 0, 0, 0, -5],
        [-5, 0, 0, 0, 0, 0, 0, -5],
        [5, 10, 10, 10, 10, 10, 10, 5],
        [0, 0, 0, 0, 0, 0, 0, 0],
    ];

    pub const QUEEN: [[i32; 8]; 8] = [
        [-20, -10, -10, -5, -5, -10, -10, -20],
        [-10, 0, 0, 0, 0, 0, 0, -10],
        [-10, 0, 5, 5, 5, 5, 0, -10],
        [-5, 0, 5, 5, 5, 5, 0, -5],
        [0, 0, 5, 5, 5, 5, 0, -5],
        [-10, 5, 5, 5, 5, 5, 0, -10],
        [-10, 0, 5, 0, 0, 0, 0, -10],
        [-20, -10, -10, -5, -5, -10, -10, -20],
    ];

    pub const KING: [[i32; 8]; 8] = [
        [-30, -40, -40, -50, -50, -40, -40, -30],
        [-30, -40, -40, -50, -50, -40, -40, -30],
        [-30, -40, -40, -50, -50, -40, -40, -30],
        [-30, -40, -40, -50, -50, -40, -40, -30],
        [-20, -30, -30, -40, -40, -30, -30, -20],
        [-10, -20, -20, -20, -20, -20, -20, -10],
        [20, 20, 0, 0, 0, 0, 20, 20],
        [20, 30, 10, 0, 0, 10, 30, 20],
    ];
}

pub fn piece_value(piece: Piece) -> i32 {
    match piece {
        Piece::Pawn => 100,
        Piece::Knight => 300,
        Piece::Bishop => 300,
        Piece::Rook => 500,
        Piece::Queen => 900,
        Piece::King => 0,
    }
}

pub fn pst_value(piece: Piece, square: Square, color: Color) -> i32 {
    let file = square.file() as usize;
    let rank = square.rank() as usize;

    // Flip rank for White because tables are Rank 8 -> Rank 1
    // Keep rank as-is for Black because the table is mirrored
    let r = if color == Color::White { 7 - rank } else { rank };

    match piece {
        Piece::Pawn => pst::PAWN[r][file],
        Piece::Knight => pst::KNIGHT[r][file],
        Piece::Bishop => pst::BISHOP[r][file],
        Piece::Rook => pst::ROOK[r][file],
        Piece::Queen => pst::QUEEN[r][file],
        Piece::King => pst::KING[r][file],
    }
}

fn side_material_and_pst(board: &Board, color: Color) -> i32 {
    let mut score = 0;
    for piece in Piece::ALL {
        let pieces = board.pieces(piece) & board.colors(color);
        for square in pieces {
            score += piece_value(piece);
            score += pst_value(piece, square, color);
        }
    }
    score
}

pub fn evaluate(board: &Board) -> i32 {
    // Terminal states (mate/draw) are handled in negamax, not here.
    // This returns score relative to the side to move.
    let us = board.side_to_move();
    let them = !us;

    let material_and_pst = side_material_and_pst(board, us) - side_material_and_pst(board, them);

    // Mobility from side-to-move perspective.
    let mut mobility = 0;
    board.generate_moves(|moves| {
        mobility += moves.len() as i32;
        false
    });

    // Being in check is bad for the side to move.
    let check_penalty = if !board.checkers().is_empty() { -30 } else { 0 };

    material_and_pst + (mobility / 10) + check_penalty
}
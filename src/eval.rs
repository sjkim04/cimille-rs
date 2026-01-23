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
    
    // Mirror rank for black pieces (flip vertically)
    let rank = if color == Color::Black { 7 - rank } else { rank };
    
    match piece {
        Piece::Pawn => pst::PAWN[rank][file],
        Piece::Knight => pst::KNIGHT[rank][file],
        Piece::Bishop => pst::BISHOP[rank][file],
        Piece::Rook => pst::ROOK[rank][file],
        Piece::Queen => pst::QUEEN[rank][file],
        Piece::King => pst::KING[rank][file],
    }
}

pub fn evaluate(board: &Board, depth: u32) -> i32 {
    // Handle terminal positions first
    match board.status() {
        GameStatus::Won => return -(CHECKMATE_SCORE - depth as i32),
        GameStatus::Drawn => return 0,
        GameStatus::Ongoing => {}
    }
    
    let mut score = 0;
    
    // Material + PST scoring FROM WHITE'S PERSPECTIVE (absolute, not side-relative)
    for piece in Piece::ALL {
        for square in board.pieces(piece) & board.colors(Color::White) {
            score += piece_value(piece) + pst_value(piece, square, Color::White);
        }
        for square in board.pieces(piece) & board.colors(Color::Black) {
            score -= piece_value(piece) + pst_value(piece, square, Color::Black);
        }
    }
    
    // Game rule score - penalize whoever is in check (from side-to-move perspective)
    if !board.checkers().is_empty() {
        if board.side_to_move() == Color::White {
            score -= 30;
        } else {
            score += 30;  // Positive for White if Black is in check
        }
    }
    
    // Mobility bonus - always from White's perspective
    score += get_mobility_delta(board);
    
    score
}

pub fn get_mobility_delta(board: &Board) -> i32 {
    // Count legal moves for current side
    let mut current_moves = 0;
    board.generate_moves(|moves| {
        current_moves += moves.len() as i32;
        false
    });
    
    // Try to flip active color using null move
    // If in check, null_move() returns None, fall back to FEN manipulation
    let opponent_moves = if let Some(flipped) = board.null_move() {
        let mut count = 0;
        flipped.generate_moves(|moves| {
            count += moves.len() as i32;
            false
        });
        count
    } else {
        // In check - use FEN manipulation to flip color like the original
        let fen = format!("{}", board);
        let parts: Vec<&str> = fen.split_whitespace().collect();
        let new_color = if parts[1] == "w" { "b" } else { "w" };
        let new_fen = format!("{} {} {} - {} {}", parts[0], new_color, parts[2], parts[4], parts[5]);
        
        if let Ok(flipped) = new_fen.parse::<Board>() {
            let mut count = 0;
            flipped.generate_moves(|moves| {
                count += moves.len() as i32;
                false
            });
            count
        } else {
            0
        }
    };
    
    // Calculate mobility from White's perspective
    let white_moves = if board.side_to_move() == Color::White {
        current_moves
    } else {
        opponent_moves
    };
    
    let black_moves = if board.side_to_move() == Color::Black {
        current_moves
    } else {
        opponent_moves
    };
    
    (white_moves - black_moves) / 10
}
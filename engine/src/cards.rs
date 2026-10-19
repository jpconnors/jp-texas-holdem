//! Cards are `u8` values 0..52: rank = c % 13 (0 = deuce .. 12 = ace), suit = c / 13 (s, h, d, c).

pub type Card = u8;

pub const RANKS: &[u8; 13] = b"23456789TJQKA";
pub const SUITS: &[u8; 4] = b"shdc";

pub const RANK_NAMES: [&str; 13] = [
    "Two", "Three", "Four", "Five", "Six", "Seven", "Eight", "Nine", "Ten", "Jack", "Queen", "King", "Ace",
];
pub const RANK_PLURALS: [&str; 13] = [
    "Twos", "Threes", "Fours", "Fives", "Sixes", "Sevens", "Eights", "Nines", "Tens", "Jacks", "Queens", "Kings", "Aces",
];

#[inline]
pub fn rank(c: Card) -> u8 {
    c % 13
}

#[inline]
pub fn suit(c: Card) -> u8 {
    c / 13
}

/// "As" -> ace of spades.
pub fn parse_card(s: &str) -> Result<Card, String> {
    let b = s.as_bytes();
    if b.len() != 2 {
        return Err(format!("Bad card: {s}"));
    }
    let r = RANKS.iter().position(|&x| x == b[0].to_ascii_uppercase());
    let su = SUITS.iter().position(|&x| x == b[1].to_ascii_lowercase());
    match (r, su) {
        (Some(r), Some(su)) => Ok((su * 13 + r) as Card),
        _ => Err(format!("Bad card: {s}")),
    }
}

/// "As Kd 9h" -> three cards.
pub fn parse_cards(s: &str) -> Result<Vec<Card>, String> {
    s.split_whitespace().map(parse_card).collect()
}

pub fn card_str(c: Card) -> String {
    let mut out = String::with_capacity(2);
    out.push(RANKS[rank(c) as usize] as char);
    out.push(SUITS[suit(c) as usize] as char);
    out
}

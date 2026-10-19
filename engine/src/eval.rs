//! Hand evaluator: best 5-card hand from 5-7 cards.
//! Score = category * 13^5 + five kickers in base 13; higher is better.
//! Categories: 0 high card, 1 pair, 2 two pair, 3 trips, 4 straight, 5 flush,
//!             6 full house, 7 quads, 8 straight flush.

use crate::cards::{Card, RANK_NAMES, RANK_PLURALS};

pub const BASE: u32 = 13 * 13 * 13 * 13 * 13;
pub const CATEGORY_NAMES: [&str; 9] = [
    "High Card", "Pair", "Two Pair", "Three of a Kind", "Straight", "Flush", "Full House", "Four of a Kind", "Straight Flush",
];

#[inline]
fn pack(cat: u32, ks: &[u8; 5]) -> u32 {
    let mut v = cat;
    for &k in ks {
        v = v * 13 + k as u32;
    }
    v
}

#[inline]
fn single(cat: u32, r: u8) -> u32 {
    pack(cat, &[r, 0, 0, 0, 0])
}

/// Highest rank of a 5-card straight in a 13-bit rank mask, or -1. The wheel (A-5) returns 3.
#[inline]
pub fn straight_high(mask: u16) -> i32 {
    let mut hi = 12;
    while hi >= 4 {
        let m: u16 = 0x1f << (hi - 4);
        if mask & m == m {
            return hi;
        }
        hi -= 1;
    }
    if mask & 0x100f == 0x100f {
        return 3;
    }
    -1
}

/// Writes up to `n` highest ranks present in `mask` (skipping ex1/ex2) into out[k..]; returns new k.
#[inline]
fn fill_top(mask: u16, n: usize, ex1: i32, ex2: i32, out: &mut [u8; 5], mut k: usize) -> usize {
    let mut taken = 0;
    let mut r: i32 = 12;
    while r >= 0 && taken < n {
        if mask & (1 << r) != 0 && r != ex1 && r != ex2 {
            out[k] = r as u8;
            k += 1;
            taken += 1;
        }
        r -= 1;
    }
    k
}

pub fn evaluate(cards: &[Card]) -> u32 {
    let mut rc = [0u8; 13];
    let mut sc = [0u8; 4];
    let mut sm = [0u16; 4];
    let mut rm: u16 = 0;
    for &c in cards {
        let r = c % 13;
        let s = (c / 13) as usize;
        rc[r as usize] += 1;
        sc[s] += 1;
        sm[s] |= 1 << r;
        rm |= 1 << r;
    }

    let mut flush: i32 = -1;
    for (s, &count) in sc.iter().enumerate() {
        if count >= 5 {
            flush = s as i32;
        }
    }
    if flush >= 0 {
        let sf = straight_high(sm[flush as usize]);
        if sf >= 0 {
            return single(8, sf as u8);
        }
    }

    // Ranks grouped by multiplicity, highest first.
    let (mut quads, mut nq) = ([0u8; 2], 0);
    let (mut trips, mut nt) = ([0u8; 2], 0);
    let (mut pairs, mut np) = ([0u8; 3], 0);
    for r in (0..13u8).rev() {
        match rc[r as usize] {
            4 => { quads[nq] = r; nq += 1; }
            3 => { trips[nt] = r; nt += 1; }
            2 => { pairs[np] = r; np += 1; }
            _ => {}
        }
    }

    let mut ks = [0u8; 5];
    if nq > 0 {
        let q = quads[0];
        ks[0] = q;
        fill_top(rm, 1, q as i32, -1, &mut ks, 1);
        return pack(7, &ks);
    }
    if nt > 0 && (nt > 1 || np > 0) {
        let t = trips[0];
        let second_trip = if nt > 1 { trips[1] as i32 } else { -1 };
        let top_pair = if np > 0 { pairs[0] as i32 } else { -1 };
        ks[0] = t;
        ks[1] = second_trip.max(top_pair) as u8;
        return pack(6, &ks);
    }
    if flush >= 0 {
        fill_top(sm[flush as usize], 5, -1, -1, &mut ks, 0);
        return pack(5, &ks);
    }
    let st = straight_high(rm);
    if st >= 0 {
        return single(4, st as u8);
    }
    if nt > 0 {
        let t = trips[0];
        ks[0] = t;
        fill_top(rm, 2, t as i32, -1, &mut ks, 1);
        return pack(3, &ks);
    }
    if np >= 2 {
        let (p1, p2) = (pairs[0], pairs[1]);
        ks[0] = p1;
        ks[1] = p2;
        fill_top(rm, 1, p1 as i32, p2 as i32, &mut ks, 2);
        return pack(2, &ks);
    }
    if np == 1 {
        let p = pairs[0];
        ks[0] = p;
        fill_top(rm, 3, p as i32, -1, &mut ks, 1);
        return pack(1, &ks);
    }
    fill_top(rm, 5, -1, -1, &mut ks, 0);
    pack(0, &ks)
}

pub fn category(score: u32) -> u32 {
    score / BASE
}

pub fn describe(score: u32) -> String {
    let mut k = [0usize; 5];
    let mut v = score;
    for i in (0..5).rev() {
        k[i] = (v % 13) as usize;
        v /= 13;
    }
    let (n, pl) = (&RANK_NAMES, &RANK_PLURALS);
    match v {
        8 if k[0] == 12 => "Royal Flush".to_string(),
        8 => format!("Straight Flush, {} high", n[k[0]]),
        7 => format!("Four of a Kind, {}", pl[k[0]]),
        6 => format!("Full House, {} full of {}", pl[k[0]], pl[k[1]]),
        5 => format!("Flush, {} high", n[k[0]]),
        4 => format!("Straight, {} high", n[k[0]]),
        3 => format!("Three of a Kind, {}", pl[k[0]]),
        2 => format!("Two Pair, {} and {}", pl[k[0]], pl[k[1]]),
        1 => format!("Pair of {}", pl[k[0]]),
        _ => format!("{} High", n[k[0]]),
    }
}

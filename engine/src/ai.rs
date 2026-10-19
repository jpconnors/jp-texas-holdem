//! Virtual opponents.
//!
//! Bots only see what a real player would: their own hole cards, the board, bets, stacks,
//! and the public action history / stats of other players (see `View`). No peeking.
//!
//!   easy   - "calling station": crude hand reading, calls too much, rarely raises, random noise.
//!   medium - Monte Carlo equity vs. mostly random hands, pot-odds discipline, occasional bluffs.
//!   hard   - Monte Carlo equity vs. modeled ranges (built from each opponent's VPIP/PFR and
//!            actions this hand), positional preflop ranges, push/fold when short, c-bets,
//!            semi-bluffs, check-raise traps, and exploits (bluffs more vs. players who fold a lot,
//!            value-bets thinner vs. calling stations).

use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

use crate::cards::{rank, suit, Card};
use crate::eval::evaluate;
use crate::game::{Action, ActionKind, Stats, Street};
use crate::rng::Rng;

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Hash)]
#[serde(rename_all = "lowercase")]
pub enum Difficulty {
    Easy,
    Medium,
    Hard,
}

impl Difficulty {
    /// Monte Carlo run-outs per postflop decision. Easy stays deliberately sloppy.
    pub fn sims(self) -> usize {
        match self {
            Difficulty::Easy => 150,
            Difficulty::Medium => 1_500,
            Difficulty::Hard => 12_000,
        }
    }
}

/// Small per-bot personality so two bots at the same level don't play identically.
#[derive(Serialize, Clone, Debug)]
pub struct Persona {
    pub loose: f64,
    pub aggr: f64,
}

impl Default for Persona {
    fn default() -> Self {
        Persona { loose: 1.0, aggr: 1.0 }
    }
}

pub fn make_persona(difficulty: Difficulty, rng: &mut Rng) -> Persona {
    let spread = if difficulty == Difficulty::Hard { 0.15 } else { 0.35 };
    let loose = 1.0 + (rng.next_f64() - 0.5) * spread;
    let aggr = 1.0 + (rng.next_f64() - 0.5) * spread * 1.4;
    Persona { loose, aggr }
}

#[derive(Clone, Debug)]
pub struct OppView {
    pub seat: usize,
    pub chips: i64,
    pub bet: i64,
    pub folded: bool,
    pub all_in: bool,
    pub stats: Stats,
    pub raised_preflop: bool,
    pub called_preflop: bool,
    pub aggr_postflop: u32,
}

#[derive(Clone, Debug)]
pub struct View {
    pub seat: usize,
    pub hole: Vec<Card>,
    pub board: Vec<Card>,
    pub street: Street,
    pub pot: i64,
    pub big_blind: i64,
    pub small_blind: i64,
    pub my_chips: i64,
    pub my_bet: i64,
    pub current_bet: i64,
    pub to_call: i64,
    pub can_check: bool,
    pub can_raise: bool,
    pub min_raise_to: i64,
    pub max_raise_to: i64,
    /// 0 = first to act after the button, 1 = last.
    pub position: f64,
    pub in_position: bool,
    pub players_in_hand: usize,
    pub limpers: u32,
    pub raises_this_street: u32,
    pub last_aggressor: Option<usize>,
    pub is_aggressor: bool,
    pub effective_stack: i64,
    pub opponents: Vec<OppView>,
}

#[derive(Serialize, Clone, Debug)]
pub struct Decision {
    #[serde(flatten)]
    pub action: Action,
    pub reason: String,
}

// ---------- preflop hand strength ----------
const CHEN_BASE: [f64; 13] = [1.0, 1.5, 2.0, 2.5, 3.0, 3.5, 4.0, 4.5, 5.0, 6.0, 7.0, 8.0, 10.0];

pub fn chen(c1: Card, c2: Card) -> f64 {
    let (r1, r2) = (rank(c1) as i32, rank(c2) as i32);
    let (hi, lo) = (r1.max(r2), r1.min(r2));
    let suited = suit(c1) == suit(c2);
    let mut s = CHEN_BASE[hi as usize];
    if r1 == r2 {
        return (s * 2.0).max(5.0);
    }
    if suited {
        s += 2.0;
    }
    let gap = hi - lo - 1;
    s -= match gap {
        0 => 0.0,
        1 => 1.0,
        2 => 2.0,
        3 => 4.0,
        _ => 5.0,
    };
    if gap <= 1 && hi < 10 {
        s += 1.0;
    }
    s
}

/// Percentile of every starting hand among all 1326 (0 = best, 1 = worst), indexed [c1][c2].
fn percentile_table() -> &'static [[f32; 52]; 52] {
    static TABLE: OnceLock<Box<[[f32; 52]; 52]>> = OnceLock::new();
    TABLE.get_or_init(|| {
        let key = |a: Card, b: Card| (chen(a, b) * 2.0).round() as i32;
        let mut scores: Vec<i32> = Vec::with_capacity(1326);
        for a in 0..52u8 {
            for b in (a + 1)..52u8 {
                scores.push(key(a, b));
            }
        }
        scores.sort_unstable_by(|x, y| y.cmp(x));
        let total = scores.len() as f64;
        let mut pct_of = std::collections::HashMap::new();
        let mut i = 0;
        while i < scores.len() {
            let mut j = i;
            while j < scores.len() && scores[j] == scores[i] {
                j += 1;
            }
            pct_of.insert(scores[i], (i as f64 + (j - i) as f64 / 2.0) / total);
            i = j;
        }
        let mut t = Box::new([[1.0f32; 52]; 52]);
        for a in 0..52u8 {
            for b in 0..52u8 {
                if a != b {
                    t[a as usize][b as usize] = pct_of[&key(a, b)] as f32;
                }
            }
        }
        t
    })
}

#[inline]
pub fn percentile(c1: Card, c2: Card) -> f64 {
    percentile_table()[c1 as usize][c2 as usize] as f64
}

// ---------- Monte Carlo equity ----------
#[inline]
fn draw(deck: &mut [Card], n: &mut usize, rng: &mut Rng) -> Card {
    let i = rng.below(*n);
    *n -= 1;
    deck.swap(i, *n);
    deck[*n]
}

/// Share of the pot `hole` wins on average against opponents whose hands are restricted to the
/// top `top_fracs[i]` of starting hands (1.0 = any two cards).
pub fn equity(hole: &[Card], board: &[Card], top_fracs: &[f64], sims: usize, rng: &mut Rng) -> f64 {
    if sims == 0 {
        return 0.0;
    }
    let mut known = [false; 52];
    for &c in hole.iter().chain(board) {
        known[c as usize] = true;
    }
    let base: Vec<Card> = (0..52u8).filter(|&c| !known[c as usize]).collect();
    let mut deck = base.clone();
    let mut opp: Vec<[Card; 2]> = vec![[0, 0]; top_fracs.len()];
    let mut full = [0u8; 5];
    let mut mine7 = [0u8; 7];
    let mut opp7 = [0u8; 7];
    let blen = board.len();
    full[..blen].copy_from_slice(board);
    mine7[..2].copy_from_slice(&hole[..2]);
    let mut total = 0.0;

    for _ in 0..sims {
        deck.copy_from_slice(&base);
        let mut n = deck.len();
        for (k, &tf) in top_fracs.iter().enumerate() {
            let mut tries = 0;
            loop {
                let a = draw(&mut deck, &mut n, rng);
                let b = draw(&mut deck, &mut n, rng);
                if tf >= 1.0 || tries >= 20 || percentile(a, b) <= tf {
                    opp[k] = [a, b];
                    break;
                }
                n += 2; // put both back
                tries += 1;
            }
        }
        for slot in full.iter_mut().skip(blen) {
            *slot = draw(&mut deck, &mut n, rng);
        }
        mine7[2..].copy_from_slice(&full);
        opp7[2..].copy_from_slice(&full);
        let mine = evaluate(&mine7);
        let mut best = 0u32;
        let mut ties = 0;
        for h in &opp {
            opp7[0] = h[0];
            opp7[1] = h[1];
            let v = evaluate(&opp7);
            if v > best {
                best = v;
                ties = 0;
            }
            if v == best {
                ties += 1;
            }
        }
        if mine > best {
            total += 1.0;
        } else if mine == best {
            total += 1.0 / (ties as f64 + 1.0);
        }
    }
    total / sims as f64
}

// ---------- helpers ----------
#[inline]
fn smoothed(count: u32, hands: u32, prior: f64, weight: f64) -> f64 {
    (count as f64 + prior * weight) / (hands as f64 + weight)
}

fn decision(kind: ActionKind, reason: impl Into<String>) -> Decision {
    Decision { action: Action::new(kind), reason: reason.into() }
}

fn raise_to(v: &View, target: f64, why: &str) -> Decision {
    if !v.can_raise {
        let kind = if v.can_check { ActionKind::Check } else { ActionKind::Call };
        return decision(kind, format!("{why} (cannot raise)"));
    }
    let unit = v.small_blind.max(1) as f64;
    let mut to = ((target / unit).round() * unit) as i64;
    to = to.min(v.max_raise_to).max(v.min_raise_to);
    // If the raise commits most of the stack, just shove.
    if (to - v.my_bet) as f64 >= 0.7 * v.my_chips as f64 {
        to = v.max_raise_to;
    }
    Decision { action: Action::raise_to(to), reason: why.to_string() }
}

fn check_or_fold(v: &View, why: &str) -> Decision {
    decision(if v.can_check { ActionKind::Check } else { ActionKind::Fold }, why)
}

fn check_or_call(v: &View, why: &str) -> Decision {
    decision(if v.can_check { ActionKind::Check } else { ActionKind::Call }, why)
}

fn all_in(v: &View, why: &str) -> Decision {
    if v.can_raise {
        Decision { action: Action::raise_to(v.max_raise_to), reason: why.to_string() }
    } else {
        check_or_call(v, why)
    }
}

// ---------- preflop ----------
fn preflop(v: &View, level: Difficulty, persona: &Persona, rng: &mut Rng) -> Decision {
    let pct = percentile(v.hole[0], v.hole[1]) / persona.loose; // smaller = stronger
    let bb = v.big_blind as f64;
    let facing_raise = v.current_bet > v.big_blind;
    let to_call = v.to_call as f64;
    let current_bet = v.current_bet as f64;
    let r = rng.next_f64();

    match level {
        Difficulty::Easy => {
            if !facing_raise {
                if pct < 0.06 && r < 0.5 {
                    return raise_to(v, 3.0 * bb, "easy: big hand, raise");
                }
                if pct < 0.75 || v.can_check {
                    return check_or_call(v, "easy: limp along");
                }
                return decision(ActionKind::Fold, "easy: junk");
            }
            if v.to_call >= v.my_chips {
                return if pct < 0.2 { decision(ActionKind::Call, "easy: call shove") } else { decision(ActionKind::Fold, "easy: too much") };
            }
            if pct < 0.04 && r < 0.4 {
                return raise_to(v, current_bet * 2.5, "easy: re-raise monster");
            }
            if pct < 0.45 || (to_call <= 3.0 * bb && pct < 0.7) {
                return decision(ActionKind::Call, "easy: call raise");
            }
            decision(ActionKind::Fold, "easy: fold to raise")
        }

        Difficulty::Medium => {
            if !facing_raise {
                if pct < 0.12 {
                    return raise_to(v, 3.0 * bb + v.limpers as f64 * bb, "medium: open raise");
                }
                if pct < 0.3 {
                    return if r < 0.3 * persona.aggr { raise_to(v, 3.0 * bb, "medium: mix raise") } else { check_or_call(v, "medium: limp") };
                }
                return check_or_fold(v, "medium: weak");
            }
            let price_frac = to_call / (v.my_chips + v.my_bet) as f64;
            if pct < 0.03 {
                return if v.raises_this_street >= 2 { all_in(v, "medium: premium vs 3-bet") } else { raise_to(v, current_bet * 3.0, "medium: 3-bet premium") };
            }
            if pct < 0.1 && price_frac < 0.35 {
                return decision(ActionKind::Call, "medium: call strong");
            }
            if pct < 0.22 && price_frac < 0.1 {
                return decision(ActionKind::Call, "medium: call cheap");
            }
            decision(ActionKind::Fold, "medium: fold to raise")
        }

        Difficulty::Hard => {
            let heads_up = v.players_in_hand == 2;
            let stack_bb = v.effective_stack as f64 / bb;
            let pos = v.position;

            if stack_bb <= 12.0 {
                // Push/fold mode.
                if !facing_raise {
                    let push_range = ((0.12 + 0.3 * (1.0 - stack_bb / 12.0) + 0.15 * pos + if heads_up { 0.2 } else { 0.0 }) * persona.loose).clamp(0.05, 0.75);
                    if pct < push_range {
                        return all_in(v, &format!("hard: push {stack_bb:.1}bb"));
                    }
                    return check_or_fold(v, "hard: fold short");
                }
                let call_range = (0.08 + 0.12 * (1.0 - stack_bb / 12.0) + if heads_up { 0.08 } else { 0.0 }).clamp(0.05, 0.4);
                if pct < call_range {
                    return all_in(v, "hard: call/shove short");
                }
                return decision(ActionKind::Fold, "hard: fold short vs raise");
            }

            if !facing_raise {
                let mut open_range = (if heads_up { 0.55 } else { 0.12 + 0.28 * pos }) * persona.loose;
                open_range *= 0.85f64.powi(v.limpers as i32);
                if pct < open_range {
                    // Isolate limpers bigger; standard 2.5bb otherwise.
                    let size = if heads_up { 2.5 } else { 2.5 + v.limpers as f64 };
                    return raise_to(v, size * bb, "hard: open");
                }
                if v.can_check {
                    return decision(ActionKind::Check, "hard: check option");
                }
                if v.limpers > 0 && pct < open_range * 1.5 && to_call <= bb {
                    return decision(ActionKind::Call, "hard: overlimp");
                }
                return decision(ActionKind::Fold, "hard: outside range");
            }

            // Facing a raise: widen 3-bets vs. loose raisers.
            let raiser = v.opponents.iter().find(|o| Some(o.seat) == v.last_aggressor);
            let raiser_pfr = raiser.map_or(0.15, |o| smoothed(o.stats.pfr, o.stats.hands, 0.15, 10.0));
            let three_bet = (0.035 + 0.3 * (raiser_pfr - 0.15).max(0.0)).clamp(0.035, 0.11);
            let big_price = to_call > 0.25 * (v.my_chips + v.my_bet) as f64;
            let call_range = if big_price { three_bet * 1.3 } else { three_bet + 0.08 + 0.05 * pos };
            if v.raises_this_street >= 2 {
                if pct < 0.02 {
                    return all_in(v, "hard: 5-bet shove");
                }
                if pct < 0.045 && !big_price {
                    return decision(ActionKind::Call, "hard: call 4-bet");
                }
                return decision(ActionKind::Fold, "hard: fold to 4-bet");
            }
            if pct < three_bet {
                return raise_to(v, current_bet * if v.in_position { 3.0 } else { 3.5 }, "hard: 3-bet value");
            }
            if pct > 0.2 && pct < 0.35 && pos > 0.6 && v.players_in_hand <= 3 && r < 0.07 * persona.aggr {
                return raise_to(v, current_bet * 3.0, "hard: 3-bet bluff");
            }
            if pct < call_range {
                return decision(ActionKind::Call, "hard: flat");
            }
            decision(ActionKind::Fold, "hard: fold to raise")
        }
    }
}

// ---------- postflop ----------
fn range_for(o: &OppView, level: Difficulty) -> f64 {
    match level {
        Difficulty::Easy => 1.0,
        Difficulty::Medium => {
            if o.raised_preflop { 0.4 } else { 1.0 }
        }
        Difficulty::Hard => {
            // Model each opponent from public stats and this hand's actions.
            let vpip = smoothed(o.stats.vpip, o.stats.hands, 0.35, 8.0);
            let pfr = smoothed(o.stats.pfr, o.stats.hands, 0.15, 8.0);
            let mut t = 1.0;
            if o.raised_preflop {
                t = pfr.clamp(0.06, 0.5);
            } else if o.called_preflop {
                t = vpip.clamp(0.12, 0.9);
            }
            t *= 0.75f64.powi(o.aggr_postflop as i32);
            t.clamp(0.04, 1.0)
        }
    }
}

fn postflop(v: &View, level: Difficulty, persona: &Persona, rng: &mut Rng) -> Decision {
    let opps: Vec<&OppView> = v.opponents.iter().filter(|o| !o.folded).collect();
    let ranges: Vec<f64> = opps.iter().map(|o| range_for(o, level)).collect();
    let e = equity(&v.hole, &v.board, &ranges, level.sims(), rng);
    let pot = v.pot as f64;
    let to_call = v.to_call as f64;
    let pot_odds = if pot + to_call > 0.0 { to_call / (pot + to_call) } else { 0.0 };
    let river = v.street == Street::River;
    let r = rng.next_f64();
    let n_opp = opps.len();
    let name = match level {
        Difficulty::Easy => "easy",
        Difficulty::Medium => "medium",
        Difficulty::Hard => "hard",
    };
    let tag = |s: &str| format!("{name}: {s} (eq {:.0}%)", e * 100.0);
    let bet_pot = |frac: f64| v.current_bet as f64 + (pot + to_call) * frac;

    match level {
        Difficulty::Easy => {
            let felt = (e + (rng.next_f64() - 0.5) * 0.3).clamp(0.0, 1.0);
            if v.can_check {
                if felt > 0.7 && r < 0.6 {
                    return raise_to(v, bet_pot(0.5), &tag("bet good hand"));
                }
                if r < 0.05 {
                    return raise_to(v, bet_pot(0.4), &tag("random stab"));
                }
                return decision(ActionKind::Check, tag("check"));
            }
            if felt > 0.85 && r < 0.3 {
                return raise_to(v, bet_pot(0.8), &tag("raise"));
            }
            if felt > 0.25 || to_call <= pot * 0.3 {
                return decision(ActionKind::Call, tag("call"));
            }
            decision(ActionKind::Fold, tag("fold"))
        }

        Difficulty::Medium => {
            if v.can_check {
                if e > 0.58 {
                    return raise_to(v, bet_pot(if e > 0.8 { 0.75 } else { 0.5 }), &tag("value bet"));
                }
                if n_opp <= 2 && r < 0.08 * persona.aggr {
                    return raise_to(v, bet_pot(0.5), &tag("bluff"));
                }
                return decision(ActionKind::Check, tag("check"));
            }
            if e > 0.78 && v.can_raise {
                return raise_to(v, bet_pot(0.75), &tag("raise for value"));
            }
            if e > pot_odds + 0.03 {
                return decision(ActionKind::Call, tag("call, odds ok"));
            }
            decision(ActionKind::Fold, tag("fold"))
        }

        Difficulty::Hard => {
            let (fold_rate, looseness) = if opps.is_empty() {
                (0.4, 0.35)
            } else {
                let k = opps.len() as f64;
                (
                    opps.iter().map(|o| smoothed(o.stats.folded_to_bet, o.stats.faced_bet, 0.4, 6.0)).sum::<f64>() / k,
                    opps.iter().map(|o| smoothed(o.stats.vpip, o.stats.hands, 0.35, 8.0)).sum::<f64>() / k,
                )
            };
            let spr = v.my_chips as f64 / pot.max(1.0);

            if v.can_check {
                if e > 0.82 && !river && !v.in_position && r < 0.2 {
                    return decision(ActionKind::Check, tag("trap check"));
                }
                let value_t = 0.55 - if looseness > 0.45 { 0.05 } else { 0.0 };
                if e > value_t {
                    let mut size = if e > 0.8 { 0.75 } else { 0.55 };
                    if river && e > 0.9 && looseness > 0.45 {
                        size = 1.1; // overbet vs. calling stations
                    }
                    return raise_to(v, bet_pot(size), &tag("value bet"));
                }
                // Continuation bet as the preflop aggressor on the flop.
                if v.street == Street::Flop && v.is_aggressor && n_opp <= 2 && r < 0.55 * persona.aggr * (0.6 + fold_rate) {
                    return raise_to(v, bet_pot(0.4), &tag("c-bet"));
                }
                let mut bluff = (if v.in_position { 0.2 } else { 0.1 }) * (if n_opp == 1 { 1.0 } else { 0.3 }) * persona.aggr * (0.5 + fold_rate);
                if !river && e > 0.3 && e < 0.55 {
                    bluff += 0.2; // semi-bluff draws
                }
                if r < bluff {
                    return raise_to(v, bet_pot(0.6), &tag("bluff/semi-bluff"));
                }
                return decision(ActionKind::Check, tag("check"));
            }

            let mut required = pot_odds;
            if !river && spr > 3.0 {
                required *= 0.85; // implied odds
            }
            let left_after_call = (v.my_chips - v.to_call) as f64;
            if left_after_call < pot * 0.25 && e > 0.4 {
                return all_in(v, &tag("committed"));
            }
            if e > 0.82 && v.can_raise {
                if !river && r < 0.25 {
                    return decision(ActionKind::Call, tag("slowplay"));
                }
                return raise_to(v, bet_pot(0.8), &tag("raise for value"));
            }
            if e > 0.68 && v.can_raise && r < 0.5 * persona.aggr {
                return raise_to(v, bet_pot(0.7), &tag("raise/protect"));
            }
            if e >= required {
                return decision(ActionKind::Call, tag("call, odds ok"));
            }
            if !river && v.can_raise && e > 0.3 && n_opp == 1 && r < 0.06 * persona.aggr * (0.5 + fold_rate) {
                return raise_to(v, bet_pot(0.75), &tag("semi-bluff raise"));
            }
            decision(ActionKind::Fold, tag("fold"))
        }
    }
}

pub fn decide(view: &View, difficulty: Difficulty, persona: &Persona, rng: &mut Rng) -> Decision {
    if view.street == Street::Preflop {
        preflop(view, difficulty, persona, rng)
    } else {
        postflop(view, difficulty, persona, rng)
    }
}

//! No-limit Texas Hold'em rules: blinds, betting rounds, min-raise rules, all-ins, side pots, showdown.
//! Pure logic; drive it with `start_hand()` and `act()`. Events accumulate until `take_events()`.

use serde::{Deserialize, Serialize};

use crate::ai::{self, Decision, Difficulty, OppView, Persona, View};
use crate::cards::Card;
use crate::eval;
use crate::rng::Rng;

/// Blind multipliers relative to the starting blinds, one entry per level.
pub const BLIND_SCHEDULE: [f64; 15] = [1.0, 1.5, 2.0, 3.0, 4.0, 6.0, 8.0, 10.0, 15.0, 20.0, 30.0, 40.0, 50.0, 75.0, 100.0];

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum Street {
    Preflop,
    Flop,
    Turn,
    River,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum ActionKind {
    Fold,
    Check,
    Call,
    #[serde(alias = "bet")]
    Raise,
}

/// `amount` is the total "raise to" for the street (only used for raises).
#[derive(Serialize, Deserialize, Clone, Copy, Debug)]
pub struct Action {
    #[serde(rename = "type")]
    pub kind: ActionKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub amount: Option<f64>,
}

impl Action {
    pub fn new(kind: ActionKind) -> Self {
        Action { kind, amount: None }
    }
    pub fn raise_to(amount: i64) -> Self {
        Action { kind: ActionKind::Raise, amount: Some(amount as f64) }
    }
}

#[derive(Serialize, Clone, Default, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Stats {
    pub hands: u32,
    pub vpip: u32,
    pub pfr: u32,
    pub faced_bet: u32,
    pub folded_to_bet: u32,
    pub bets: u32,
    pub calls: u32,
    pub wins: u32,
    pub biggest_pot: i64,
}

#[derive(Clone, Debug)]
pub struct ActRec {
    pub street: Street,
    pub kind: ActionKind,
    pub amount: i64,
}

#[derive(Clone, Debug)]
pub struct Player {
    pub seat: usize,
    pub name: String,
    pub is_human: bool,
    pub difficulty: Option<Difficulty>,
    pub persona: Persona,
    pub chips: i64,
    pub hole: Vec<Card>,
    pub in_hand: bool,
    pub folded: bool,
    pub all_in: bool,
    /// Chips put in on the current street.
    pub bet: i64,
    /// Chips put in this hand.
    pub contrib: i64,
    pub has_acted: bool,
    pub acted_raise_no: i64,
    pub out: bool,
    pub last_action: String,
    pub actions: Vec<ActRec>,
    pub stats: Stats,
    vpip_flag: bool,
    pfr_flag: bool,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct PlayerConfig {
    pub name: String,
    #[serde(default)]
    pub is_human: bool,
    #[serde(default)]
    pub difficulty: Option<Difficulty>,
    pub chips: i64,
}

#[derive(Clone, Debug)]
pub struct GameConfig {
    pub players: Vec<PlayerConfig>,
    pub small_blind: i64,
    pub big_blind: i64,
    /// Raise the blinds every N hands (0 = never).
    pub blind_increase_every: u32,
    pub seed: u64,
    /// Seat that gets the button on hand 1 (random if None).
    pub dealer: Option<usize>,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Legal {
    pub seat: usize,
    pub to_call: i64,
    pub can_check: bool,
    pub can_call: bool,
    pub can_raise: bool,
    pub min_raise_to: i64,
    pub max_raise_to: i64,
    pub current_bet: i64,
    pub pot: i64,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Pot {
    pub amount: i64,
    pub eligible: Vec<usize>,
    pub winners: Vec<usize>,
    /// Only one player was eligible: an uncalled bet going back to its owner.
    pub returned: bool,
    pub hand_name: String,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ShownHand {
    pub seat: usize,
    pub hole: Vec<Card>,
    pub score: u32,
    pub name: String,
}

#[derive(Serialize, Clone, Debug)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum HandResult {
    Showdown {
        board: Vec<Card>,
        hands: Vec<ShownHand>,
        pots: Vec<Pot>,
        /// Indexed by seat.
        payouts: Vec<i64>,
        winners: Vec<usize>,
    },
    Uncontested {
        winner: usize,
        amount: i64,
        winners: Vec<usize>,
        payouts: Vec<i64>,
    },
}

#[derive(Serialize, Clone, Debug)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum Event {
    HandStart { hand_no: u32, dealer: usize, sb_seat: usize, bb_seat: usize, small_blind: i64, big_blind: i64 },
    Blinds { small_blind: i64, big_blind: i64, level: u32 },
    Blind { seat: usize, amount: i64, label: String, all_in: bool },
    Action { seat: usize, #[serde(rename = "action")] kind: ActionKind, amount: i64, total: i64, label: String, street: Street, all_in: bool },
    Street { street: Street, cards: Vec<Card>, board: Vec<Card> },
    Showdown { board: Vec<Card>, hands: Vec<ShownHand>, pots: Vec<Pot> },
    Uncontested { winner: usize, amount: i64 },
    Busted { seat: usize },
}

pub struct Game {
    pub players: Vec<Player>,
    pub base_small: i64,
    pub base_big: i64,
    pub small_blind: i64,
    pub big_blind: i64,
    pub blind_increase_every: u32,
    pub blind_level: u32,
    pub rng: Rng,
    pub dealer: usize,
    pub sb_seat: usize,
    pub bb_seat: usize,
    pub hand_no: u32,
    pub board: Vec<Card>,
    /// Cards are dealt with `pop()`, i.e. from the end.
    pub deck: Vec<Card>,
    pub street: Option<Street>,
    pub current: Option<usize>,
    pub current_bet: i64,
    pub min_raise: i64,
    pub raise_no: i64,
    pub street_raises: u32,
    pub last_aggressor: Option<usize>,
    pub hand_over: bool,
    pub last_result: Option<HandResult>,
    events: Vec<Event>,
}

#[inline]
fn can_act(p: &Player) -> bool {
    p.in_hand && !p.folded && !p.all_in
}

#[inline]
fn live(p: &Player) -> bool {
    p.in_hand && !p.folded
}

impl Game {
    pub fn new(cfg: GameConfig) -> Result<Game, String> {
        let n = cfg.players.len();
        if !(2..=10).contains(&n) {
            return Err("A table needs 2 to 10 players".into());
        }
        if cfg.small_blind < 1 || cfg.big_blind < cfg.small_blind {
            return Err("Invalid blinds".into());
        }
        let mut rng = Rng::new(cfg.seed);
        let players = cfg
            .players
            .iter()
            .enumerate()
            .map(|(seat, p)| Player {
                seat,
                name: p.name.clone(),
                is_human: p.is_human,
                difficulty: if p.is_human { None } else { Some(p.difficulty.unwrap_or(Difficulty::Medium)) },
                persona: if p.is_human { Persona::default() } else { ai::make_persona(p.difficulty.unwrap_or(Difficulty::Medium), &mut rng) },
                chips: p.chips.max(0),
                hole: Vec::new(),
                in_hand: false,
                folded: false,
                all_in: false,
                bet: 0,
                contrib: 0,
                has_acted: false,
                acted_raise_no: -1,
                out: p.chips <= 0,
                last_action: String::new(),
                actions: Vec::new(),
                stats: Stats::default(),
                vpip_flag: false,
                pfr_flag: false,
            })
            .collect();
        let first = cfg.dealer.unwrap_or_else(|| rng.below(n)) % n;
        Ok(Game {
            players,
            base_small: cfg.small_blind,
            base_big: cfg.big_blind,
            small_blind: cfg.small_blind,
            big_blind: cfg.big_blind,
            blind_increase_every: cfg.blind_increase_every,
            blind_level: 0,
            rng,
            // start_hand() moves the button to the next seated player, which lands on `first`.
            dealer: (first + n - 1) % n,
            sb_seat: 0,
            bb_seat: 0,
            hand_no: 0,
            board: Vec::new(),
            deck: Vec::new(),
            street: None,
            current: None,
            current_bet: 0,
            min_raise: 0,
            raise_no: 0,
            street_raises: 0,
            last_aggressor: None,
            hand_over: true,
            last_result: None,
            events: Vec::new(),
        })
    }

    // ---------- helpers ----------
    pub fn next_seat(&self, from: usize, pred: impl Fn(&Player) -> bool) -> Option<usize> {
        let n = self.players.len();
        (1..=n).map(|k| (from + k) % n).find(|&i| pred(&self.players[i]))
    }

    pub fn pot(&self) -> i64 {
        self.players.iter().map(|p| p.contrib).sum()
    }

    pub fn total_chips(&self) -> i64 {
        self.players.iter().map(|p| p.chips + p.contrib).sum()
    }

    pub fn human(&self) -> Option<&Player> {
        self.players.iter().find(|p| p.is_human)
    }

    pub fn take_events(&mut self) -> Vec<Event> {
        std::mem::take(&mut self.events)
    }

    pub fn is_game_over(&self) -> bool {
        if !self.hand_over {
            return false;
        }
        if let Some(h) = self.human() {
            if h.chips <= 0 {
                return true;
            }
        }
        self.players.iter().filter(|p| p.chips > 0).count() <= 1
    }

    fn update_blinds(&mut self) {
        if self.blind_increase_every == 0 {
            return;
        }
        let level = (self.hand_no - 1) / self.blind_increase_every;
        if level == self.blind_level {
            return;
        }
        self.blind_level = level;
        let len = BLIND_SCHEDULE.len() as u32;
        let mult = if level < len {
            BLIND_SCHEDULE[level as usize]
        } else {
            BLIND_SCHEDULE[(len - 1) as usize] * 2f64.powi((level - len + 1) as i32)
        };
        self.small_blind = ((self.base_small as f64 * mult).round() as i64).max(1);
        self.big_blind = ((self.base_big as f64 * mult).round() as i64).max(2);
        self.events.push(Event::Blinds { small_blind: self.small_blind, big_blind: self.big_blind, level });
    }

    fn put(&mut self, seat: usize, amount: i64) -> i64 {
        let p = &mut self.players[seat];
        let a = amount.min(p.chips).max(0);
        p.chips -= a;
        p.bet += a;
        p.contrib += a;
        if p.chips == 0 {
            p.all_in = true;
        }
        a
    }

    // ---------- hand flow ----------
    pub fn start_hand(&mut self) -> Result<(), String> {
        if !self.hand_over {
            return Err("Hand already in progress".into());
        }
        if self.is_game_over() {
            return Err("Game is over".into());
        }
        self.hand_no += 1;
        self.update_blinds();

        for p in &mut self.players {
            p.out = p.chips <= 0;
            p.in_hand = !p.out;
            p.hole.clear();
            p.folded = false;
            p.all_in = false;
            p.bet = 0;
            p.contrib = 0;
            p.has_acted = false;
            p.acted_raise_no = -1;
            p.last_action.clear();
            p.actions.clear();
            p.vpip_flag = false;
            p.pfr_flag = false;
            if p.in_hand {
                p.stats.hands += 1;
            }
        }

        self.dealer = self.next_seat(self.dealer, |p| p.in_hand).ok_or("No players")?;
        self.board.clear();
        self.street = Some(Street::Preflop);
        self.hand_over = false;
        self.last_result = None;
        self.current_bet = 0;
        self.min_raise = self.big_blind;
        self.raise_no = 0;
        self.street_raises = 0;
        self.last_aggressor = None;
        self.deck = (0..52).collect();
        let mut deck = std::mem::take(&mut self.deck);
        self.rng.shuffle(&mut deck);
        self.deck = deck;

        let seated = self.players.iter().filter(|p| p.in_hand).count();
        let mut s = self.dealer;
        for _ in 0..2 {
            for _ in 0..seated {
                s = self.next_seat(s, |p| p.in_hand).unwrap();
                let c = self.deck.pop().unwrap();
                self.players[s].hole.push(c);
            }
        }

        let heads_up = seated == 2;
        self.sb_seat = if heads_up { self.dealer } else { self.next_seat(self.dealer, |p| p.in_hand).unwrap() };
        self.bb_seat = self.next_seat(self.sb_seat, |p| p.in_hand).unwrap();

        self.events.push(Event::HandStart {
            hand_no: self.hand_no,
            dealer: self.dealer,
            sb_seat: self.sb_seat,
            bb_seat: self.bb_seat,
            small_blind: self.small_blind,
            big_blind: self.big_blind,
        });

        self.post_blind(self.sb_seat, self.small_blind, true);
        self.post_blind(self.bb_seat, self.big_blind, false);
        self.current_bet = self.big_blind;

        self.current = Some(self.bb_seat);
        self.proceed();
        Ok(())
    }

    fn post_blind(&mut self, seat: usize, amount: i64, small: bool) {
        let paid = self.put(seat, amount);
        let p = &mut self.players[seat];
        p.last_action = format!("{} {}", if small { "SB" } else { "BB" }, paid);
        let label = if small { "small blind" } else { "big blind" }.to_string();
        let all_in = p.all_in;
        self.events.push(Event::Blind { seat, amount: paid, label, all_in });
    }

    pub fn legal_actions(&self) -> Option<Legal> {
        if self.hand_over {
            return None;
        }
        let seat = self.current?;
        let p = &self.players[seat];
        let to_call = (self.current_bet - p.bet).min(p.chips).max(0);
        let reopened = !p.has_acted || p.acted_raise_no < self.raise_no;
        let others_can_act = self.players.iter().any(|o| o.seat != seat && can_act(o));
        let max_raise_to = p.bet + p.chips;
        let min_raise_to = (self.current_bet + self.min_raise).min(max_raise_to);
        Some(Legal {
            seat,
            to_call,
            can_check: to_call == 0,
            can_call: to_call > 0,
            can_raise: reopened && p.chips > to_call && others_can_act,
            min_raise_to,
            max_raise_to,
            current_bet: self.current_bet,
            pot: self.pot(),
        })
    }

    /// Applies an action for the player whose turn it is. Illegal actions are coerced to the nearest legal one.
    pub fn act(&mut self, action: Action) -> Result<(), String> {
        if self.hand_over {
            return Err("No hand in progress".into());
        }
        let l = self.legal_actions().ok_or("No player to act")?;
        let seat = l.seat;
        let mut kind = action.kind;
        if kind == ActionKind::Raise && !l.can_raise {
            kind = if l.can_check { ActionKind::Check } else { ActionKind::Call };
        }
        if kind == ActionKind::Check && !l.can_check {
            kind = ActionKind::Fold;
        }
        if kind == ActionKind::Call && l.can_check {
            kind = ActionKind::Check;
        }

        let street = self.street.unwrap_or(Street::Preflop);
        let facing_bet = l.to_call > 0;
        let mut amount = 0;
        let label;

        match kind {
            ActionKind::Fold => {
                self.players[seat].folded = true;
                label = "Fold".to_string();
            }
            ActionKind::Check => label = "Check".to_string(),
            ActionKind::Call => {
                amount = self.put(seat, l.to_call);
                let p = &mut self.players[seat];
                p.stats.calls += 1;
                label = if p.all_in { format!("All-in {}", p.bet) } else { format!("Call {amount}") };
            }
            ActionKind::Raise => {
                let mut to = match action.amount {
                    Some(a) if a.is_finite() => a.floor() as i64,
                    _ => l.min_raise_to,
                };
                to = to.min(l.max_raise_to).max(l.min_raise_to);
                let raise_size = to - self.current_bet;
                let was_unopened = self.current_bet == 0;
                let bet_now = self.players[seat].bet;
                amount = self.put(seat, to - bet_now);
                let p_bet = self.players[seat].bet;
                if p_bet > self.current_bet {
                    if raise_size >= self.min_raise {
                        self.min_raise = raise_size;
                        self.raise_no += 1;
                    }
                    self.current_bet = p_bet;
                    self.last_aggressor = Some(seat);
                    self.street_raises += 1;
                }
                let p = &mut self.players[seat];
                p.stats.bets += 1;
                label = if p.all_in {
                    format!("All-in {}", p.bet)
                } else if was_unopened {
                    format!("Bet {}", p.bet)
                } else {
                    format!("Raise to {}", p.bet)
                };
            }
        }

        let raise_no = self.raise_no;
        let p = &mut self.players[seat];
        // Stats the AI uses for opponent modeling.
        if street == Street::Preflop && matches!(kind, ActionKind::Call | ActionKind::Raise) && !p.vpip_flag {
            p.vpip_flag = true;
            p.stats.vpip += 1;
        }
        if street == Street::Preflop && kind == ActionKind::Raise && !p.pfr_flag {
            p.pfr_flag = true;
            p.stats.pfr += 1;
        }
        if street != Street::Preflop && facing_bet {
            p.stats.faced_bet += 1;
            if kind == ActionKind::Fold {
                p.stats.folded_to_bet += 1;
            }
        }

        p.has_acted = true;
        p.acted_raise_no = raise_no;
        p.last_action = label.clone();
        let rec_amount = if kind == ActionKind::Raise { p.bet } else { amount };
        p.actions.push(ActRec { street, kind, amount: rec_amount });
        let (total, all_in) = (p.bet, p.all_in);
        self.events.push(Event::Action { seat, kind, amount, total, label, street, all_in });

        self.proceed();
        Ok(())
    }

    fn round_complete(&self) -> bool {
        let actors: Vec<&Player> = self.players.iter().filter(|p| can_act(p)).collect();
        if actors.is_empty() {
            return true;
        }
        if actors.len() == 1 && actors[0].bet >= self.current_bet {
            return true;
        }
        actors.iter().all(|p| p.has_acted && p.bet == self.current_bet)
    }

    fn proceed(&mut self) {
        let live_seats: Vec<usize> = self.players.iter().filter(|p| live(p)).map(|p| p.seat).collect();
        if live_seats.len() == 1 {
            return self.award_uncontested(live_seats[0]);
        }
        if self.round_complete() {
            return self.next_street();
        }
        let from = self.current.unwrap_or(self.dealer);
        self.current = self.next_seat(from, can_act);
    }

    fn next_street(&mut self) {
        for p in &mut self.players {
            p.bet = 0;
            p.has_acted = false;
            p.acted_raise_no = -1;
        }
        self.current_bet = 0;
        self.min_raise = self.big_blind;
        self.raise_no = 0;
        self.street_raises = 0;

        let (next, count) = match self.street {
            Some(Street::Preflop) => (Street::Flop, 3),
            Some(Street::Flop) => (Street::Turn, 1),
            Some(Street::Turn) => (Street::River, 1),
            _ => return self.showdown(),
        };
        self.street = Some(next);
        let cards: Vec<Card> = (0..count).map(|_| self.deck.pop().unwrap()).collect();
        self.board.extend_from_slice(&cards);
        for p in &mut self.players {
            if p.in_hand && !p.folded && !p.all_in {
                p.last_action.clear();
            }
        }
        self.events.push(Event::Street { street: next, cards, board: self.board.clone() });

        if self.round_complete() {
            return self.next_street(); // everyone is all-in: run it out
        }
        self.current = self.next_seat(self.dealer, can_act);
    }

    fn seat_distance_from_dealer(&self, seat: usize) -> usize {
        let n = self.players.len();
        let d = (seat + n - self.dealer) % n;
        if d == 0 { n } else { d }
    }

    fn showdown(&mut self) {
        let n = self.players.len();
        let live_seats: Vec<usize> = self.players.iter().filter(|p| live(p)).map(|p| p.seat).collect();
        let mut scores = vec![0u32; n];
        let mut cards: Vec<Card> = Vec::with_capacity(7);
        for &s in &live_seats {
            cards.clear();
            cards.extend_from_slice(&self.players[s].hole);
            cards.extend_from_slice(&self.board);
            scores[s] = eval::evaluate(&cards);
        }

        let mut levels: Vec<i64> = live_seats.iter().map(|&s| self.players[s].contrib).collect();
        levels.sort_unstable();
        levels.dedup();
        let mut pots: Vec<Pot> = Vec::new();
        let mut prev = 0;
        for &lvl in &levels {
            let amount: i64 = self.players.iter().map(|p| (p.contrib.min(lvl) - prev).max(0)).sum();
            let eligible: Vec<usize> = live_seats.iter().copied().filter(|&s| self.players[s].contrib >= lvl).collect();
            if amount > 0 {
                pots.push(Pot { amount, eligible, winners: Vec::new(), returned: false, hand_name: String::new() });
            }
            prev = lvl;
        }
        let assigned: i64 = pots.iter().map(|p| p.amount).sum();
        let total = self.pot();
        if total > assigned {
            if let Some(last) = pots.last_mut() {
                last.amount += total - assigned; // dead money from folded players above the top live level
            }
        }

        let mut payouts = vec![0i64; n];
        for pot in &mut pots {
            let best = pot.eligible.iter().map(|&s| scores[s]).max().unwrap_or(0);
            let mut winners: Vec<usize> = pot.eligible.iter().copied().filter(|&s| scores[s] == best).collect();
            winners.sort_by_key(|&s| self.seat_distance_from_dealer(s));
            let share = pot.amount / winners.len() as i64;
            let mut rem = pot.amount - share * winners.len() as i64;
            for &w in &winners {
                let extra = if rem > 0 { 1 } else { 0 };
                rem -= extra;
                payouts[w] += share + extra;
            }
            pot.returned = pot.eligible.len() == 1;
            pot.hand_name = eval::describe(best);
            pot.winners = winners;
        }

        for (seat, &amt) in payouts.iter().enumerate() {
            self.players[seat].chips += amt;
        }
        let mut real_winners: Vec<usize> = Vec::new();
        for pot in pots.iter().filter(|p| !p.returned) {
            for &w in &pot.winners {
                let st = &mut self.players[w].stats;
                st.biggest_pot = st.biggest_pot.max(pot.amount);
                if !real_winners.contains(&w) {
                    real_winners.push(w);
                }
            }
        }
        for &w in &real_winners {
            self.players[w].stats.wins += 1;
        }

        let hands: Vec<ShownHand> = live_seats
            .iter()
            .map(|&s| ShownHand { seat: s, hole: self.players[s].hole.clone(), score: scores[s], name: eval::describe(scores[s]) })
            .collect();
        self.events.push(Event::Showdown { board: self.board.clone(), hands: hands.clone(), pots: pots.clone() });
        self.finish_hand(HandResult::Showdown { board: self.board.clone(), hands, pots, payouts, winners: real_winners });
    }

    fn award_uncontested(&mut self, winner: usize) {
        let amount = self.pot();
        let p = &mut self.players[winner];
        p.chips += amount;
        p.stats.wins += 1;
        p.stats.biggest_pot = p.stats.biggest_pot.max(amount);
        let mut payouts = vec![0i64; self.players.len()];
        payouts[winner] = amount;
        self.events.push(Event::Uncontested { winner, amount });
        self.finish_hand(HandResult::Uncontested { winner, amount, winners: vec![winner], payouts });
    }

    fn finish_hand(&mut self, result: HandResult) {
        self.hand_over = true;
        self.current = None;
        self.last_result = Some(result);
        for i in 0..self.players.len() {
            let p = &mut self.players[i];
            p.bet = 0;
            p.contrib = 0;
            if p.in_hand && p.chips == 0 {
                p.out = true;
                self.events.push(Event::Busted { seat: i });
            }
        }
    }

    // ---------- what a bot is allowed to see ----------
    pub fn view_for(&self, seat: usize) -> View {
        let me = &self.players[seat];
        let l = if self.current == Some(seat) { self.legal_actions() } else { None };
        let n = self.players.len();

        let mut order: Vec<usize> = Vec::with_capacity(n);
        let mut s = self.dealer;
        for _ in 0..n {
            match self.next_seat(s, live) {
                Some(next) if !order.contains(&next) => {
                    order.push(next);
                    s = next;
                }
                _ => break,
            }
        }
        let idx = order.iter().position(|&x| x == seat).map(|i| i as f64).unwrap_or(-1.0);
        let position = if order.len() > 1 { idx / (order.len() - 1) as f64 } else { 1.0 };
        let street = self.street.unwrap_or(Street::Preflop);
        let limpers = if street == Street::Preflop {
            self.players
                .iter()
                .filter(|p| {
                    p.seat != seat
                        && p.actions.iter().any(|a| a.street == Street::Preflop && a.kind == ActionKind::Call)
                        && !p.actions.iter().any(|a| a.kind == ActionKind::Raise)
                })
                .count() as u32
        } else {
            0
        };

        let opponents: Vec<OppView> = self
            .players
            .iter()
            .filter(|p| p.seat != seat && p.in_hand)
            .map(|p| OppView {
                seat: p.seat,
                chips: p.chips,
                bet: p.bet,
                folded: p.folded,
                all_in: p.all_in,
                stats: p.stats.clone(),
                raised_preflop: p.actions.iter().any(|a| a.street == Street::Preflop && a.kind == ActionKind::Raise),
                called_preflop: p.actions.iter().any(|a| a.street == Street::Preflop && a.kind == ActionKind::Call),
                aggr_postflop: p.actions.iter().filter(|a| a.street != Street::Preflop && a.kind == ActionKind::Raise).count() as u32,
            })
            .collect();
        let max_opp_stack = opponents.iter().filter(|o| !o.folded).map(|o| o.chips + o.bet).max().unwrap_or(0).max(0);
        let live_count = self.players.iter().filter(|p| live(p)).count();

        View {
            seat,
            hole: me.hole.clone(),
            board: self.board.clone(),
            street,
            pot: self.pot(),
            big_blind: self.big_blind,
            small_blind: self.small_blind,
            my_chips: me.chips,
            my_bet: me.bet,
            current_bet: self.current_bet,
            to_call: l.as_ref().map_or(0, |l| l.to_call),
            can_check: l.as_ref().is_some_and(|l| l.can_check),
            can_raise: l.as_ref().is_some_and(|l| l.can_raise),
            min_raise_to: l.as_ref().map_or(0, |l| l.min_raise_to),
            max_raise_to: l.as_ref().map_or(0, |l| l.max_raise_to),
            position,
            in_position: order.last() == Some(&seat),
            players_in_hand: live_count,
            limpers,
            raises_this_street: self.street_raises,
            last_aggressor: self.last_aggressor,
            is_aggressor: self.last_aggressor == Some(seat),
            effective_stack: (me.chips + me.bet).min(max_opp_stack),
            opponents,
        }
    }

    /// Lets the bot in the current seat decide, without acting.
    pub fn bot_decide(&mut self) -> Result<Decision, String> {
        let seat = self.current.ok_or("No player to act")?;
        let difficulty = self.players[seat].difficulty.ok_or("The current player is not a bot")?;
        let view = self.view_for(seat);
        let persona = self.players[seat].persona.clone();
        Ok(ai::decide(&view, difficulty, &persona, &mut self.rng))
    }

    /// Lets the bot in the current seat decide and act. Returns its decision.
    pub fn bot_act(&mut self) -> Result<Decision, String> {
        let d = self.bot_decide()?;
        self.act(d.action)?;
        Ok(d)
    }
}

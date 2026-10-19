//! JSON command interface used by the browser (through the wasm exports in lib.rs) and by tests.
//!
//! Request:  {"cmd": "new" | "start" | "act" | "bot" | "state" | "describe" | "version", ...}
//! Response: {"ok": true, "state": {...}, "events": [...], ...} or {"ok": false, "error": "..."}

use serde::Deserialize;
use serde_json::{json, Value};

use crate::cards::Card;
use crate::eval;
use crate::game::{Action, Game, GameConfig, HandResult, PlayerConfig};

#[derive(Deserialize)]
#[serde(tag = "cmd", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum Cmd {
    New {
        players: Vec<PlayerConfig>,
        small_blind: i64,
        big_blind: i64,
        #[serde(default)]
        blind_increase_every: u32,
        /// Up to 2^53 so it survives a JS number; the browser passes a crypto-random value.
        #[serde(default)]
        seed: Option<f64>,
        #[serde(default)]
        dealer: Option<usize>,
    },
    Start,
    Act {
        action: Action,
    },
    /// The bot in the current seat decides and acts.
    Bot,
    State,
    Describe {
        cards: Vec<Card>,
    },
    Version,
}

#[derive(Default)]
pub struct Api {
    game: Option<Game>,
}

impl Api {
    /// Raw bytes in, raw bytes out (what the wasm export uses).
    pub fn handle_bytes(&mut self, input: &[u8]) -> Vec<u8> {
        let resp = match std::str::from_utf8(input) {
            Ok(s) => self.call_json(s),
            Err(e) => json!({ "ok": false, "error": e.to_string() }),
        };
        serde_json::to_vec(&resp).unwrap_or_else(|_| br#"{"ok":false,"error":"could not serialize response"}"#.to_vec())
    }

    pub fn call_json(&mut self, input: &str) -> Value {
        let result = serde_json::from_str::<Cmd>(input).map_err(|e| format!("Bad command: {e}")).and_then(|cmd| self.handle(cmd));
        match result {
            Ok(v) => v,
            Err(e) => json!({ "ok": false, "error": e }),
        }
    }

    pub fn game(&self) -> Option<&Game> {
        self.game.as_ref()
    }

    fn game_mut(&mut self) -> Result<&mut Game, String> {
        self.game.as_mut().ok_or_else(|| "No game. Send {\"cmd\":\"new\"} first.".to_string())
    }

    pub fn handle(&mut self, cmd: Cmd) -> Result<Value, String> {
        let mut decision = Value::Null;
        match cmd {
            Cmd::Version => {
                return Ok(json!({
                    "ok": true,
                    "engine": "rust",
                    "version": env!("CARGO_PKG_VERSION"),
                    "sims": { "easy": crate::ai::Difficulty::Easy.sims(), "medium": crate::ai::Difficulty::Medium.sims(), "hard": crate::ai::Difficulty::Hard.sims() },
                }));
            }
            Cmd::Describe { cards } => {
                if cards.len() < 5 || cards.len() > 7 || cards.iter().any(|&c| c >= 52) {
                    return Err("describe needs 5-7 valid cards".into());
                }
                return Ok(json!({ "ok": true, "description": eval::describe(eval::evaluate(&cards)) }));
            }
            Cmd::New { players, small_blind, big_blind, blind_increase_every, seed, dealer } => {
                let seed = seed.filter(|s| s.is_finite() && *s >= 0.0).map_or(0x5EED, |s| s as u64);
                self.game = Some(Game::new(GameConfig { players, small_blind, big_blind, blind_increase_every, seed, dealer })?);
            }
            Cmd::Start => self.game_mut()?.start_hand()?,
            Cmd::Act { action } => self.game_mut()?.act(action)?,
            Cmd::Bot => {
                let d = self.game_mut()?.bot_act()?;
                decision = serde_json::to_value(d).map_err(|e| e.to_string())?;
            }
            Cmd::State => {}
        }
        let game = self.game_mut()?;
        let events = serde_json::to_value(game.take_events()).map_err(|e| e.to_string())?;
        Ok(json!({ "ok": true, "state": state_json(game), "events": events, "decision": decision }))
    }
}

/// Everything the table UI needs. Opponents' hole cards are only included once shown down.
pub fn state_json(g: &Game) -> Value {
    let shown: Vec<usize> = match (&g.last_result, g.hand_over) {
        (Some(HandResult::Showdown { hands, .. }), true) => hands.iter().map(|h| h.seat).collect(),
        _ => Vec::new(),
    };
    let players: Vec<Value> = g
        .players
        .iter()
        .map(|p| {
            let visible = p.is_human || shown.contains(&p.seat);
            json!({
                "seat": p.seat,
                "name": p.name,
                "isHuman": p.is_human,
                "difficulty": p.difficulty,
                "chips": p.chips,
                "bet": p.bet,
                "inHand": p.in_hand,
                "folded": p.folded,
                "allIn": p.all_in,
                "out": p.out,
                "lastAction": p.last_action,
                "cardCount": p.hole.len(),
                "hole": if visible { p.hole.clone() } else { Vec::new() },
                "stats": p.stats,
            })
        })
        .collect();

    let human_hand_name = g.human().and_then(|h| {
        if h.in_hand && h.hole.len() == 2 && g.board.len() >= 3 {
            let mut cards = h.hole.clone();
            cards.extend_from_slice(&g.board);
            Some(eval::describe(eval::evaluate(&cards)))
        } else {
            None
        }
    });

    json!({
        "handNo": g.hand_no,
        "handOver": g.hand_over,
        "current": g.current.map_or(-1, |c| c as i64),
        "dealer": g.dealer,
        "street": g.street,
        "board": g.board,
        "potTotal": g.pot(),
        "currentBet": g.current_bet,
        "smallBlind": g.small_blind,
        "bigBlind": g.big_blind,
        "totalChipsInPlay": g.total_chips(),
        "gameOver": g.is_game_over(),
        "legal": g.legal_actions(),
        "lastResult": g.last_result,
        "humanHandName": human_hand_name,
        "players": players,
    })
}

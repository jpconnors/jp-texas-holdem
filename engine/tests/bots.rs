//! Whole-game simulations: chip conservation, the JSON API, and the difficulty ladder.
//! Run: cargo test --release -- --nocapture   (prints the ladder results)

use holdem_core::ai::Difficulty;
use holdem_core::api::Api;
use holdem_core::game::{Action, ActionKind, Game, GameConfig, PlayerConfig};
use holdem_core::rng::Rng;
use serde_json::{json, Value};

fn bots(diffs: &[Difficulty], chips: i64) -> Vec<PlayerConfig> {
    diffs
        .iter()
        .enumerate()
        .map(|(i, &d)| PlayerConfig { name: format!("{d:?}{i}"), is_human: false, difficulty: Some(d), chips })
        .collect()
}

/// Plays a bot-only freezeout; returns (hands played, winning difficulty).
fn tournament(diffs: &[Difficulty], seed: u64, max_hands: u32) -> (u32, Option<Difficulty>) {
    let mut g = Game::new(GameConfig {
        players: bots(diffs, 1500),
        small_blind: 10,
        big_blind: 20,
        blind_increase_every: 15,
        seed,
        dealer: None,
    })
    .unwrap();
    let total = g.total_chips();
    let mut steps = 0u64;
    while !g.is_game_over() && g.hand_no < max_hands {
        g.start_hand().unwrap();
        while !g.hand_over {
            g.bot_act().unwrap();
            steps += 1;
            assert!(steps < 1_000_000, "runaway hand");
        }
        assert_eq!(g.total_chips(), total, "chips not conserved on hand {}", g.hand_no);
        assert!(g.players.iter().all(|p| p.chips >= 0));
        g.take_events();
    }
    let alive: Vec<_> = g.players.iter().filter(|p| p.chips > 0).collect();
    (g.hand_no, if alive.len() == 1 { alive[0].difficulty } else { None })
}

#[test]
fn six_handed_bot_games_conserve_chips() {
    use Difficulty::*;
    for seed in 1..=6 {
        tournament(&[Easy, Medium, Hard, Easy, Medium, Hard], seed, 300);
    }
}

fn matchup(a: Difficulty, b: Difficulty, games: u64) -> (u32, u32) {
    let (mut wa, mut wb) = (0, 0);
    for s in 1..=games {
        let seats = if s % 2 == 1 { [a, b] } else { [b, a] };
        match tournament(&seats, 1000 + s, 600).1 {
            Some(d) if d == a => wa += 1,
            Some(_) => wb += 1,
            None => {}
        }
    }
    (wa, wb)
}

#[test]
fn difficulty_ladder() {
    use Difficulty::*;
    let n = 40;
    let he = matchup(Hard, Easy, n);
    let me = matchup(Medium, Easy, n);
    let hm = matchup(Hard, Medium, n);
    println!("Heads-up freezeouts ({n} each): hard-easy {he:?}, medium-easy {me:?}, hard-medium {hm:?}");
    assert!(he.0 > he.1, "hard should beat easy: {he:?}");
    assert!(me.0 > me.1, "medium should beat easy: {me:?}");
    assert!(hm.0 > hm.1, "hard should beat medium: {hm:?}");
}

/// Drives a full game through the same JSON interface the browser uses.
#[test]
fn json_api_plays_a_full_game() {
    let mut api = Api::default();
    let mut rng = Rng::new(99);
    let call = |api: &mut Api, v: Value| -> Value {
        let r = api.call_json(&v.to_string());
        assert_eq!(r["ok"], true, "{v} -> {r}");
        r
    };

    assert_eq!(api.call_json(r#"{"cmd":"start"}"#)["ok"], false, "start before new must fail cleanly");
    assert_eq!(api.call_json("not json")["ok"], false);

    let r = call(&mut api, json!({
        "cmd": "new", "smallBlind": 10, "bigBlind": 20, "blindIncreaseEvery": 10, "seed": 123456789.0,
        "players": [
            {"name": "JP", "isHuman": true, "chips": 2000},
            {"name": "Rita", "difficulty": "easy", "chips": 2000},
            {"name": "Moose", "difficulty": "medium", "chips": 2000},
            {"name": "Vega", "difficulty": "hard", "chips": 2000}
        ]
    }));
    let total = r["state"]["totalChipsInPlay"].as_i64().unwrap();
    assert_eq!(total, 8000);

    let mut hands = 0;
    loop {
        let r = call(&mut api, json!({"cmd": "state"}));
        if r["state"]["gameOver"] == true || hands >= 200 {
            break;
        }
        let mut r = call(&mut api, json!({"cmd": "start"}));
        hands += 1;
        assert!(r["events"].as_array().unwrap().iter().any(|e| e["type"] == "handStart"));
        while r["state"]["handOver"] == false {
            let st = &r["state"];
            let cur = st["current"].as_i64().unwrap() as usize;
            let player = &st["players"][cur];
            // Opponents' cards are never exposed mid-hand.
            for p in st["players"].as_array().unwrap() {
                if p["isHuman"] == false {
                    assert_eq!(p["hole"].as_array().unwrap().len(), 0, "bot cards leaked");
                }
            }
            r = if player["isHuman"] == true {
                let legal = &st["legal"];
                let x = rng.next_f64();
                let action = if x < 0.15 && legal["canRaise"] == true {
                    json!({"type": "raise", "amount": legal["minRaiseTo"]})
                } else if legal["canCheck"] == true {
                    json!({"type": "check"})
                } else if x < 0.8 {
                    json!({"type": "call"})
                } else {
                    json!({"type": "fold"})
                };
                call(&mut api, json!({"cmd": "act", "action": action}))
            } else {
                let r = call(&mut api, json!({"cmd": "bot"}));
                assert!(r["decision"]["type"].is_string());
                r
            };
            assert_eq!(r["state"]["totalChipsInPlay"].as_i64().unwrap(), total);
        }
        let res = &r["state"]["lastResult"];
        assert!(res["type"] == "showdown" || res["type"] == "uncontested");
    }
    assert!(hands > 0);

    let d = call(&mut api, json!({"cmd": "describe", "cards": [12, 11, 10, 9, 8]}));
    assert_eq!(d["description"], "Royal Flush");
    let v = call(&mut api, json!({"cmd": "version"}));
    assert_eq!(v["engine"], "rust");

    // The bot command refuses to play the human's seat.
    let mut api2 = Api::default();
    call(&mut api2, json!({"cmd": "new", "smallBlind": 5, "bigBlind": 10, "dealer": 0,
        "players": [{"name": "A", "isHuman": true, "chips": 100}, {"name": "B", "difficulty": "easy", "chips": 100}]}));
    call(&mut api2, json!({"cmd": "start"}));
    assert_eq!(api2.call_json(r#"{"cmd":"bot"}"#)["ok"], false);
    let _ = Action::new(ActionKind::Fold);
}

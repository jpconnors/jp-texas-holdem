//! Rules and evaluator tests (ported from the original JavaScript suite).
//! Run: cargo test --release

use holdem_core::ai;
use holdem_core::cards::parse_cards;
use holdem_core::eval::{category, describe, evaluate, CATEGORY_NAMES};
use holdem_core::game::{Action, ActionKind, Game, GameConfig, HandResult, PlayerConfig};
use holdem_core::rng::Rng;

fn ev(s: &str) -> u32 {
    evaluate(&parse_cards(s).unwrap())
}

fn cfg(stacks: &[i64], sb: i64, bb: i64, dealer: usize, seed: u64) -> GameConfig {
    GameConfig {
        players: stacks
            .iter()
            .enumerate()
            .map(|(i, &c)| PlayerConfig { name: format!("P{i}"), is_human: false, difficulty: None, chips: c })
            .collect(),
        small_blind: sb,
        big_blind: bb,
        blind_increase_every: 0,
        seed,
        dealer: Some(dealer),
    }
}

fn call() -> Action {
    Action::new(ActionKind::Call)
}

#[test]
fn category_ladder() {
    let ladder = [
        ("As Kd 9h 7c 3s 2d 4h", "High Card"),
        ("As Ad 9h 7c 3s 2d Jh", "Pair"),
        ("As Ad 9h 9c 3s 2d Jh", "Two Pair"),
        ("As Ad Ah 9c 3s 2d Jh", "Three of a Kind"),
        ("As 2d 3h 4c 5s 9d Jh", "Straight"),
        ("2h 7h 9h Jh Kh As Ad", "Flush"),
        ("As Ad Ah 9c 9s 2d Jh", "Full House"),
        ("As Ad Ah Ac 9s 2d Jh", "Four of a Kind"),
        ("9h Th Jh Qh Kh 2d 3c", "Straight Flush"),
    ];
    for (i, (hand, name)) in ladder.iter().enumerate() {
        assert_eq!(CATEGORY_NAMES[category(ev(hand)) as usize], *name, "{hand}");
        if i > 0 {
            assert!(ev(hand) > ev(ladder[i - 1].0), "{name} should beat {}", ladder[i - 1].1);
        }
    }
}

#[test]
fn evaluator_edge_cases() {
    assert_eq!(describe(ev("Ah Kh Qh Jh Th 2c 3d")), "Royal Flush");
    assert_eq!(describe(ev("As 2d 3h 4c 5s 9d Jh")), "Straight, Five high");
    assert!(ev("6s 2d 3h 4c 5s 9d Jh") > ev("As 2d 3h 4c 5s 9d Jh"), "6-high straight beats wheel");
    assert!(ev("As Ad 9h 9c 3s 2d Kh") > ev("As Ad 9h 9c 3s 2d Qh"), "two pair kicker");
    assert_eq!(ev("As Ad 9h 9c 3s 3d Kh"), ev("As Ad 9h 9c 3s 3d Kc"), "third pair ignored");
    assert!(ev("Ks Kd Kh 9c 9s 9d 2h") > ev("Qs Qd Qh Ac As 9d 2h"), "full house trips rank first");
    assert_eq!(describe(ev("Ks Kd Kh 9c 9s 9d 2h")), "Full House, Kings full of Nines");
    assert_eq!(ev("2h 3h 4h 5h 7h 6c 8c"), ev("2c 3c 4c 5c 7c 6d 8d"), "suits are irrelevant");
    assert!(ev("Ah Kh 4h 5h 7h 6c 8c") > ev("Ah Qh 4h 5h 7h 6c 8c"), "flush kicker order");
    assert_eq!(describe(ev("2c 2d 5h 5s 9c")), "Two Pair, Fives and Twos");
}

/// Brute force: the 7-card score must equal the best of its 21 five-card subsets.
#[test]
fn seven_card_matches_best_five() {
    let mut rng = Rng::new(42);
    for _ in 0..3000 {
        let mut deck: Vec<u8> = (0..52).collect();
        rng.shuffle(&mut deck);
        let seven = &deck[..7];
        let mut best = 0;
        for a in 0..7 {
            for b in (a + 1)..7 {
                let five: Vec<u8> = (0..7).filter(|&i| i != a && i != b).map(|i| seven[i]).collect();
                best = best.max(evaluate(&five));
            }
        }
        assert_eq!(evaluate(seven), best, "{seven:?}");
    }
}

#[test]
fn preflop_percentiles_and_equity() {
    let pct = |s: &str| {
        let c = parse_cards(s).unwrap();
        ai::percentile(c[0], c[1])
    };
    assert!(pct("As Ad") < 0.01);
    assert!(pct("As Ks") < pct("7h 2c"));
    assert!(pct("7h 2c") > 0.9);

    let mut rng = Rng::new(1);
    let aa = ai::equity(&parse_cards("As Ah").unwrap(), &[], &[1.0], 20_000, &mut rng);
    assert!((0.83..0.87).contains(&aa), "AA vs random ~85%, got {aa}");
    let royal = ai::equity(&parse_cards("Ah Kh").unwrap(), &parse_cards("Qh Jh Th").unwrap(), &[1.0, 1.0], 2_000, &mut rng);
    assert_eq!(royal, 1.0);
}

#[test]
fn side_pots() {
    let mut g = Game::new(cfg(&[100, 300, 1000], 5, 10, 0, 7)).unwrap();
    g.start_hand().unwrap();
    g.players[0].hole = parse_cards("As Ah").unwrap();
    g.players[1].hole = parse_cards("Ks Kh").unwrap();
    g.players[2].hole = parse_cards("7c 2d").unwrap();
    g.deck = parse_cards("3c 4d 9h 8s 2s Jd Qc").unwrap(); // dealt from the end
    let before = g.total_chips();
    // dealer 0 (A), sb 1 (B), bb 2 (C); A acts first preflop.
    g.act(Action::raise_to(100)).unwrap();
    g.act(Action::raise_to(300)).unwrap();
    g.act(call()).unwrap();
    assert!(g.hand_over);
    match g.last_result.as_ref().unwrap() {
        HandResult::Showdown { pots, .. } => {
            assert_eq!(pots.len(), 2);
            assert_eq!((pots[0].amount, pots[0].winners.clone()), (300, vec![0]));
            assert_eq!((pots[1].amount, pots[1].winners.clone()), (400, vec![1]));
        }
        _ => panic!("expected showdown"),
    }
    let stacks: Vec<i64> = g.players.iter().map(|p| p.chips).collect();
    assert_eq!(stacks, vec![300, 400, 700]);
    assert_eq!(g.total_chips(), before);
}

#[test]
fn uncalled_bet_is_returned() {
    let mut g = Game::new(cfg(&[1000, 200], 5, 10, 0, 3)).unwrap();
    g.start_hand().unwrap(); // heads-up: dealer posts SB and acts first
    g.act(Action::raise_to(800)).unwrap();
    g.act(call()).unwrap();
    assert!(g.hand_over);
    assert_eq!(g.total_chips(), 1200);
    assert!(g.players[0].chips >= 800);
}

#[test]
fn short_all_in_does_not_reopen_raising() {
    let mut g = Game::new(cfg(&[1000, 1000, 130], 10, 20, 0, 5)).unwrap();
    g.start_hand().unwrap();
    g.act(Action::raise_to(100)).unwrap(); // A raises to 100
    g.act(call()).unwrap(); // B calls
    g.act(Action::raise_to(130)).unwrap(); // C all-in, a short raise of 30
    let l = g.legal_actions().unwrap();
    assert_eq!((l.seat, l.to_call), (0, 30));
    assert!(!l.can_raise);
}

#[test]
fn big_blind_gets_option() {
    let mut g = Game::new(cfg(&[1000, 1000, 1000], 10, 20, 0, 9)).unwrap();
    g.start_hand().unwrap();
    g.act(call()).unwrap(); // A limps
    g.act(call()).unwrap(); // SB completes
    let l = g.legal_actions().unwrap();
    assert_eq!(l.seat, 2);
    assert!(l.can_check && l.can_raise);
}

#[test]
fn three_way_split_pot() {
    let mut g = Game::new(cfg(&[1000, 1000, 1000], 5, 10, 0, 11)).unwrap();
    g.start_hand().unwrap();
    g.players[0].hole = parse_cards("2c 3d").unwrap();
    g.players[1].hole = parse_cards("4c 5d").unwrap();
    g.players[2].hole = parse_cards("4h 5s").unwrap();
    // Board As Ks Qs Js Ts: everyone plays the royal flush; 3-way split.
    g.deck = parse_cards("Ts Js Qs Ks As").unwrap(); // flop As Ks Qs, turn Js, river Ts
    g.act(call()).unwrap();
    g.act(call()).unwrap();
    g.act(Action::new(ActionKind::Check)).unwrap();
    for _ in 0..3 {
        for _ in 0..3 {
            g.act(Action::new(ActionKind::Check)).unwrap();
        }
    }
    assert!(g.hand_over);
    assert_eq!(g.total_chips(), 3000);
    let stacks: Vec<i64> = g.players.iter().map(|p| p.chips).collect();
    assert_eq!(stacks, vec![1000, 1000, 1000]);
}

#[test]
fn illegal_actions_are_coerced() {
    let mut g = Game::new(cfg(&[1000, 1000], 10, 20, 0, 13)).unwrap();
    g.start_hand().unwrap();
    // SB faces 10 more; "check" becomes a fold.
    g.act(Action::new(ActionKind::Check)).unwrap();
    assert!(g.hand_over);
    assert_eq!(g.players[1].chips, 1010);
}

#[test]
fn blinds_rise_on_schedule() {
    let mut c = cfg(&[5000, 5000], 10, 20, 0, 17);
    c.blind_increase_every = 2;
    let mut g = Game::new(c).unwrap();
    let mut seen = vec![];
    for _ in 0..6 {
        g.start_hand().unwrap();
        seen.push(g.big_blind);
        while !g.hand_over {
            g.act(Action::new(ActionKind::Fold)).unwrap();
        }
    }
    assert_eq!(seen, vec![20, 20, 30, 30, 40, 40]);
}

// Run with:  node tests/run-tests.js
'use strict';
const path = require('path');
for (const f of ['cards', 'evaluator', 'engine', 'ai']) require(path.join(__dirname, '..', 'js', f + '.js'));
const P = globalThis.Poker;

let passed = 0, failed = 0;
function ok(cond, msg) {
  if (cond) passed++;
  else { failed++; console.error('FAIL:', msg); }
}
const ev = (s) => P.evaluate(P.parseCards(s));

// ---------- evaluator ----------
const ladder = [
  ['As Kd 9h 7c 3s 2d 4h', 'High Card'],
  ['As Ad 9h 7c 3s 2d Jh', 'Pair'],
  ['As Ad 9h 9c 3s 2d Jh', 'Two Pair'],
  ['As Ad Ah 9c 3s 2d Jh', 'Three of a Kind'],
  ['As 2d 3h 4c 5s 9d Jh', 'Straight'],
  ['2h 7h 9h Jh Kh As Ad', 'Flush'],
  ['As Ad Ah 9c 9s 2d Jh', 'Full House'],
  ['As Ad Ah Ac 9s 2d Jh', 'Four of a Kind'],
  ['9h Th Jh Qh Kh 2d 3c', 'Straight Flush'],
];
for (let i = 0; i < ladder.length; i++) {
  ok(P.CATEGORY_NAMES[P.categoryOf(ev(ladder[i][0]))] === ladder[i][1], `category ${ladder[i][1]}`);
  if (i) ok(ev(ladder[i][0]) > ev(ladder[i - 1][0]), `${ladder[i][1]} beats ${ladder[i - 1][1]}`);
}
ok(P.describe(ev('Ah Kh Qh Jh Th 2c 3d')) === 'Royal Flush', 'royal flush name');
ok(P.describe(ev('As 2d 3h 4c 5s 9d Jh')) === 'Straight, Five high', 'wheel is five high');
ok(ev('6s 2d 3h 4c 5s 9d Jh') > ev('As 2d 3h 4c 5s 9d Jh'), '6-high straight beats wheel');
ok(ev('As Ad 9h 9c 3s 2d Kh') > ev('As Ad 9h 9c 3s 2d Qh'), 'two pair kicker');
ok(ev('As Ad 9h 9c 3s 3d Kh') === ev('As Ad 9h 9c 3s 3d Kc'), 'two pair ignores third pair, ties');
ok(ev('Ks Kd Kh 9c 9s 9d 2h') > ev('Qs Qd Qh Ac As 9d 2h'), 'full house trips rank first');
ok(P.describe(ev('Ks Kd Kh 9c 9s 9d 2h')) === 'Full House, Kings full of Nines', 'two trips -> full house');
ok(ev('2h 3h 4h 5h 7h 6c 8c') === ev('2c 3c 4c 5c 7c 6d 8d'), 'suits irrelevant for equal hands');
ok(ev('Ah Kh 4h 5h 7h 6c 8c') > ev('Ah Qh 4h 5h 7h 6c 8c'), 'flush kicker order');

// ---------- chen / percentiles ----------
const pct = (s) => { const [a, b] = P.parseCards(s); return P.AI.percentile(a, b); };
ok(pct('As Ad') < 0.01, 'AA top');
ok(pct('As Ks') < pct('7h 2c'), 'AKs beats 72o');
ok(pct('7h 2c') > 0.9, '72o bottom');

// ---------- equity ----------
const [aa1, aa2] = P.parseCards('As Ah');
const eqAA = P.AI.equity([aa1, aa2], [], [{ topFrac: 1 }], 4000, P.seededRandom(1));
ok(eqAA > 0.82 && eqAA < 0.88, `AA vs random ~85% (got ${eqAA.toFixed(3)})`);
const nutted = P.AI.equity(P.parseCards('Ah Kh'), P.parseCards('Qh Jh Th'), [{ topFrac: 1 }], 500);
ok(nutted === 1, 'royal flush has 100% equity');

// ---------- side pots ----------
{
  const g = new P.Game({
    players: [
      { name: 'A', chips: 100 },
      { name: 'B', chips: 300 },
      { name: 'C', chips: 1000 },
    ],
    smallBlind: 5, bigBlind: 10, dealer: 0, rng: P.seededRandom(7),
  });
  g.startHand();
  // Force cards: A has the nuts, B second best, C worst.
  g.players[0].hole = P.parseCards('As Ah');
  g.players[1].hole = P.parseCards('Ks Kh');
  g.players[2].hole = P.parseCards('7c 2d');
  g.deck = P.parseCards('3c 4d 9h 8s 2s Jd Qc'); // pops from the end: Qc Jd 2s | 8s | 9h
  const before = g.totalChips();
  // dealer 0 (A), sb 1 (B), bb 2 (C). First to act preflop is A.
  g.act({ type: 'raise', amount: 100 }); // A all-in 100
  g.act({ type: 'raise', amount: 300 }); // B all-in 300
  g.act({ type: 'call' });               // C calls 300
  ok(g.handOver, 'hand runs out when all-in');
  const r = g.lastResult;
  ok(r.type === 'showdown', 'showdown happened');
  ok(r.pots.length === 2, `two pots (got ${r.pots.length})`);
  ok(r.pots[0].amount === 300 && r.pots[0].winners[0] === 0, 'A wins main pot 300');
  ok(r.pots[1].amount === 400 && r.pots[1].winners[0] === 1, 'B wins side pot 400');
  ok(g.players[0].chips === 300 && g.players[1].chips === 400 && g.players[2].chips === 700, 'stacks after side pots');
  ok(g.totalChips() === before, 'chips conserved in side pot hand');
}

// ---------- uncalled bet returned ----------
{
  const g = new P.Game({ players: [{ name: 'A', chips: 1000 }, { name: 'B', chips: 200 }], smallBlind: 5, bigBlind: 10, dealer: 0, rng: P.seededRandom(3) });
  g.startHand();
  // heads-up: dealer A is SB and acts first preflop.
  g.act({ type: 'raise', amount: 800 });
  g.act({ type: 'call' }); // B all-in 200
  ok(g.handOver, 'heads-up all-in resolves');
  ok(g.totalChips() === 1200, 'chips conserved with uncalled excess');
  ok(g.players[0].chips >= 800, 'A gets the uncalled 600 back at minimum');
}

// ---------- min-raise / short all-in does not reopen ----------
{
  const g = new P.Game({ players: [{ name: 'A', chips: 1000 }, { name: 'B', chips: 1000 }, { name: 'C', chips: 130 }], smallBlind: 10, bigBlind: 20, dealer: 0, rng: P.seededRandom(5) });
  g.startHand(); // dealer A, sb B, bb C; A first
  g.act({ type: 'raise', amount: 100 }); // A raises to 100 (raise of 80)
  g.act({ type: 'call' });               // B calls 100
  g.act({ type: 'raise', amount: 130 }); // C all-in 130: short raise of 30
  const L = g.legalActions();
  ok(g.current === 0 && L.toCall === 30, 'A faces 30 more');
  ok(!L.canRaise, 'short all-in does not reopen raising for A');
}

// ---------- BB option ----------
{
  const g = new P.Game({ players: [{ name: 'A', chips: 1000 }, { name: 'B', chips: 1000 }, { name: 'C', chips: 1000 }], smallBlind: 10, bigBlind: 20, dealer: 0, rng: P.seededRandom(9) });
  g.startHand();
  g.act({ type: 'call' }); // A limps
  g.act({ type: 'call' }); // SB completes
  ok(g.street === 'preflop' && g.current === 2, 'BB gets the option');
  ok(g.legalActions().canCheck && g.legalActions().canRaise, 'BB can check or raise');
}

// ---------- bot-only simulation: conservation + no crashes ----------
function tournament(diffs, seed, maxHands) {
  const rng = P.seededRandom(seed);
  const g = new P.Game({
    players: diffs.map((d, i) => ({ name: d + i, difficulty: d, persona: P.AI.makePersona(d, rng), chips: 1500 })),
    smallBlind: 10, bigBlind: 20, blindIncreaseEvery: 15, rng,
  });
  const total = g.totalChips();
  let hands = 0, steps = 0;
  while (!g.isGameOver() && hands < maxHands) {
    g.startHand();
    hands++;
    while (!g.handOver) {
      const p = g.players[g.current];
      g.act(P.AI.decide(g.viewFor(p.seat), p.difficulty, p.persona, rng));
      if (++steps > 1e6) throw new Error('infinite loop');
    }
    if (g.totalChips() !== total) throw new Error(`chips not conserved on hand ${hands}: ${g.totalChips()} vs ${total}`);
    if (g.players.some((p) => p.chips < 0)) throw new Error('negative stack');
  }
  const alive = g.players.filter((p) => p.chips > 0);
  return { hands, winner: alive.length === 1 ? alive[0].difficulty : null, g };
}

let crashed = null;
try {
  for (let s = 1; s <= 10; s++) tournament(['easy', 'medium', 'hard', 'easy', 'medium', 'hard'], s, 400);
} catch (e) { crashed = e; }
ok(!crashed, 'bot simulations run clean' + (crashed ? ': ' + crashed.stack : ''));

// Strength check: Hard vs Easy heads-up, and Medium vs Easy.
function matchup(a, b, n) {
  const w = { [a]: 0, [b]: 0 };
  for (let s = 1; s <= n; s++) {
    const t = tournament(s % 2 ? [a, b] : [b, a], 1000 + s, 600);
    if (t.winner) w[t.winner]++;
  }
  return w;
}
const t0 = Date.now();
const hv = matchup('hard', 'easy', 30);
const mv = matchup('medium', 'easy', 30);
const hm = matchup('hard', 'medium', 30);
console.log('Heads-up match wins (30 games each):', JSON.stringify({ hardVsEasy: hv, mediumVsEasy: mv, hardVsMedium: hm }), `in ${((Date.now() - t0) / 1000).toFixed(1)}s`);
ok(hv.hard > hv.easy, 'hard beats easy more often');
ok(mv.medium > mv.easy, 'medium beats easy more often');

console.log(`\n${passed} passed, ${failed} failed`);
process.exit(failed ? 1 : 0);

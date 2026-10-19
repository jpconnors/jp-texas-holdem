#!/usr/bin/env node
// Loads the compiled engine exactly as the browser does (js/holdem-wasm.js + js/wasm-game.js)
// and plays full games through Poker.Game. Run after building:  node tests/wasm-smoke.js
'use strict';
const path = require('path');
require(path.join(__dirname, '..', 'js', 'holdem-wasm.js'));
require(path.join(__dirname, '..', 'js', 'wasm-game.js'));
const P = globalThis.Poker;

function assert(cond, msg) {
  if (!cond) throw new Error('FAIL: ' + msg);
}

(async () => {
  const info = await P.ready;
  console.log(`Engine: ${info.engine} ${info.version}, sims ${JSON.stringify(info.sims)}, wasm ${(P.WASM_BASE64.length * 0.75 / 1024).toFixed(0)} KB`);

  const timing = { easy: [], medium: [], hard: [] };
  let games = 0, hands = 0;
  for (const difficulty of ['easy', 'medium', 'hard']) {
    for (let g = 0; g < 3; g++) {
      const players = [{ name: 'You', isHuman: true, chips: 1500 }];
      for (let i = 0; i < 3; i++) players.push({ name: 'Bot' + i, difficulty, chips: 1500 });
      const game = new P.Game({ players, smallBlind: 10, bigBlind: 20, blindIncreaseEvery: 8 });
      const total = game.totalChips();
      let events = 0;
      game.on(() => events++);
      while (!game.isGameOver() && game.handNo < 120) {
        game.startHand();
        hands++;
        while (!game.handOver) {
          const p = game.players[game.current];
          if (p.isHuman) {
            const L = game.legalActions();
            const r = Math.random();
            if (r < 0.1 && L.canRaise) game.act({ type: 'raise', amount: L.minRaiseTo });
            else if (L.canCheck) game.act({ type: 'check' });
            else game.act({ type: r < 0.75 ? 'call' : 'fold' });
          } else {
            const t0 = process.hrtime.bigint();
            const d = game.botAct();
            timing[difficulty].push(Number(process.hrtime.bigint() - t0) / 1e6);
            assert(typeof d.type === 'string' && typeof d.reason === 'string', 'decision shape');
          }
          assert(game.totalChips() === total, `chips conserved (hand ${game.handNo})`);
          for (const q of game.players) {
            if (!q.isHuman && !game.handOver) assert(q.hole.length === 0, 'bot cards hidden mid-hand');
          }
        }
        assert(['showdown', 'uncontested'].includes(game.lastResult.type), 'hand result');
      }
      assert(events > 0, 'events delivered');
      games++;
    }
  }

  let threw = false;
  try { P.engineCall({ cmd: 'act', action: { type: 'nonsense' } }); } catch (e) { threw = true; }
  assert(threw, 'bad commands raise a JS error');

  const fmt = (a) => {
    const s = a.slice().sort((x, y) => x - y);
    return `avg ${(a.reduce((x, y) => x + y, 0) / a.length).toFixed(1)} ms, p95 ${s[Math.floor(s.length * 0.95)].toFixed(1)} ms, max ${s[s.length - 1].toFixed(1)} ms`;
  };
  for (const d of Object.keys(timing)) console.log(`${d.padEnd(6)} bot decision: ${fmt(timing[d])}`);
  console.log(`OK: ${games} games, ${hands} hands through the WebAssembly engine.`);
})().catch((e) => {
  console.error(e);
  process.exit(1);
});

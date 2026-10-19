/* Virtual opponents.
 *
 * Bots only see what a real player would: their own hole cards, the board, bets, stacks,
 * and the public action history / stats of other players. No peeking.
 *
 *   easy   - "calling station": crude hand reading, calls too much, rarely raises, random noise.
 *   medium - Monte Carlo equity vs. mostly random hands, pot-odds discipline, occasional bluffs.
 *   hard   - Monte Carlo equity vs. modeled ranges (built from each opponent's VPIP/PFR and
 *            actions this hand), positional preflop ranges, push/fold when short, c-bets,
 *            semi-bluffs, check-raise traps, and exploits (bluffs more vs. players who fold a lot,
 *            value-bets thinner vs. calling stations).
 */
(function (root) {
  'use strict';
  const P = (root.Poker = root.Poker || {});

  const DIFFICULTIES = ['easy', 'medium', 'hard'];

  // ---------- preflop hand strength ----------
  const CHEN_BASE = [1, 1.5, 2, 2.5, 3, 3.5, 4, 4.5, 5, 6, 7, 8, 10];

  function chen(c1, c2) {
    const r1 = c1 % 13, r2 = c2 % 13;
    const hi = Math.max(r1, r2), lo = Math.min(r1, r2);
    const suited = ((c1 / 13) | 0) === ((c2 / 13) | 0);
    let s = CHEN_BASE[hi];
    if (r1 === r2) return Math.max(5, s * 2);
    if (suited) s += 2;
    const gap = hi - lo - 1;
    if (gap === 1) s -= 1;
    else if (gap === 2) s -= 2;
    else if (gap === 3) s -= 4;
    else if (gap >= 4) s -= 5;
    if (gap <= 1 && hi < 10) s += 1;
    return s;
  }

  // Percentile of each Chen score among all 1326 starting hands (0 = best, 1 = worst).
  let PCT = null;
  function buildPercentiles() {
    const scores = [];
    for (let a = 0; a < 52; a++) for (let b = a + 1; b < 52; b++) scores.push(chen(a, b));
    scores.sort((x, y) => y - x);
    PCT = new Map();
    let i = 0;
    while (i < scores.length) {
      let j = i;
      while (j < scores.length && scores[j] === scores[i]) j++;
      PCT.set(scores[i], (i + (j - i) / 2) / scores.length);
      i = j;
    }
  }
  function percentile(c1, c2) {
    if (!PCT) buildPercentiles();
    return PCT.get(chen(c1, c2));
  }

  // ---------- Monte Carlo equity ----------
  // opps: [{ topFrac }] where topFrac in (0,1] restricts that opponent's hand to the top X% of starting hands.
  function equity(hole, board, opps, sims, rng) {
    rng = rng || Math.random;
    const known = new Set(hole.concat(board));
    const base = [];
    for (let c = 0; c < 52; c++) if (!known.has(c)) base.push(c);
    const need = 5 - board.length;
    let total = 0;

    for (let s = 0; s < sims; s++) {
      const deck = base.slice();
      let n = deck.length;
      const draw = () => {
        const i = (rng() * n) | 0;
        const c = deck[i];
        deck[i] = deck[--n];
        deck[n] = c;
        return c;
      };

      const oppHands = [];
      for (const o of opps) {
        let a, b, tries = 0;
        for (;;) {
          a = draw();
          b = draw();
          if (o.topFrac >= 1 || tries >= 20 || percentile(a, b) <= o.topFrac) break;
          n += 2; // put both back
          tries++;
        }
        oppHands.push([a, b]);
      }
      const full = board.slice();
      for (let i = 0; i < need; i++) full.push(draw());

      const mine = P.evaluate(hole.concat(full));
      let best = -1, ties = 0;
      for (const h of oppHands) {
        const v = P.evaluate(h.concat(full));
        if (v > best) { best = v; ties = 0; }
        if (v === best) ties++;
      }
      if (mine > best) total += 1;
      else if (mine === best) total += 1 / (ties + 1);
    }
    return total / sims;
  }

  // ---------- helpers ----------
  const clamp = (x, a, b) => Math.max(a, Math.min(b, x));

  function smoothed(count, hands, prior, weight) {
    return (count + prior * weight) / (hands + weight);
  }

  function raiseTo(v, target, why) {
    if (!v.canRaise) return v.canCheck ? { type: 'check', reason: why + ' (cannot raise)' } : { type: 'call', reason: why + ' (cannot raise)' };
    const unit = Math.max(1, v.smallBlind);
    let to = Math.round(target / unit) * unit;
    to = clamp(to, v.minRaiseTo, v.maxRaiseTo);
    // If the raise commits most of the stack, just shove.
    if (to - v.myBet >= 0.7 * (v.myChips)) to = v.maxRaiseTo;
    return { type: 'raise', amount: to, reason: why };
  }
  const checkOrFold = (v, why) => (v.canCheck ? { type: 'check', reason: why } : { type: 'fold', reason: why });
  const checkOrCall = (v, why) => (v.canCheck ? { type: 'check', reason: why } : { type: 'call', reason: why });
  const allIn = (v, why) => (v.canRaise ? { type: 'raise', amount: v.maxRaiseTo, reason: why } : checkOrCall(v, why));

  function makePersona(difficulty, rng) {
    rng = rng || Math.random;
    const spread = difficulty === 'hard' ? 0.15 : 0.35;
    return {
      loose: 1 + (rng() - 0.5) * spread,
      aggr: 1 + (rng() - 0.5) * spread * 1.4,
    };
  }

  // ---------- preflop ----------
  function preflop(v, level, persona, rng) {
    const pct = percentile(v.hole[0], v.hole[1]) / persona.loose; // smaller = stronger
    const bb = v.bigBlind;
    const facingRaise = v.currentBet > bb;
    const r = rng();

    if (level === 'easy') {
      if (!facingRaise) {
        if (pct < 0.06 && r < 0.5) return raiseTo(v, 3 * bb, 'easy: big hand, raise');
        if (pct < 0.75 || v.canCheck) return checkOrCall(v, 'easy: limp along');
        return { type: 'fold', reason: 'easy: junk' };
      }
      if (v.toCall >= v.myChips) return pct < 0.2 ? { type: 'call', reason: 'easy: call shove' } : { type: 'fold', reason: 'easy: too much' };
      if (pct < 0.04 && r < 0.4) return raiseTo(v, v.currentBet * 2.5, 'easy: re-raise monster');
      if (pct < 0.45 || (v.toCall <= 3 * bb && pct < 0.7)) return { type: 'call', reason: 'easy: call raise' };
      return { type: 'fold', reason: 'easy: fold to raise' };
    }

    if (level === 'medium') {
      if (!facingRaise) {
        if (pct < 0.12) return raiseTo(v, 3 * bb + v.limpers * bb, 'medium: open raise');
        if (pct < 0.3) return r < 0.3 * persona.aggr ? raiseTo(v, 3 * bb, 'medium: mix raise') : checkOrCall(v, 'medium: limp');
        return checkOrFold(v, 'medium: weak');
      }
      const priceFrac = v.toCall / (v.myChips + v.myBet);
      if (pct < 0.03) return v.raisesThisStreet >= 2 ? allIn(v, 'medium: premium vs 3-bet') : raiseTo(v, v.currentBet * 3, 'medium: 3-bet premium');
      if (pct < 0.1 && priceFrac < 0.35) return { type: 'call', reason: 'medium: call strong' };
      if (pct < 0.22 && priceFrac < 0.1) return { type: 'call', reason: 'medium: call cheap' };
      return { type: 'fold', reason: 'medium: fold to raise' };
    }

    // ---- hard ----
    const headsUp = v.playersInHand === 2;
    const stackBB = v.effectiveStack / bb;
    const pos = v.position;

    if (stackBB <= 12) {
      // Push/fold mode.
      if (!facingRaise) {
        const pushRange = clamp((0.12 + 0.3 * (1 - stackBB / 12) + 0.15 * pos + (headsUp ? 0.2 : 0)) * persona.loose, 0.05, 0.75);
        if (pct < pushRange) return allIn(v, `hard: push ${stackBB.toFixed(1)}bb`);
        return checkOrFold(v, 'hard: fold short');
      }
      const callRange = clamp(0.08 + 0.12 * (1 - stackBB / 12) + (headsUp ? 0.08 : 0), 0.05, 0.4);
      if (pct < callRange) return allIn(v, 'hard: call/shove short');
      return { type: 'fold', reason: 'hard: fold short vs raise' };
    }

    if (!facingRaise) {
      let openRange = (headsUp ? 0.55 : 0.12 + 0.28 * pos) * persona.loose;
      openRange *= Math.pow(0.85, v.limpers);
      if (pct < openRange) {
        // Isolate limpers bigger; standard 2.5bb otherwise.
        return raiseTo(v, (headsUp ? 2.5 : 2.5 + v.limpers) * bb, 'hard: open');
      }
      if (v.canCheck) return { type: 'check', reason: 'hard: check option' };
      if (v.limpers > 0 && pct < openRange * 1.5 && v.toCall <= bb) return { type: 'call', reason: 'hard: overlimp' };
      return { type: 'fold', reason: 'hard: outside range' };
    }

    // Facing a raise: widen 3-bets vs. loose raisers.
    const raiser = v.opponents.find((o) => o.seat === v.lastAggressor);
    const raiserPfr = raiser ? smoothed(raiser.stats.pfr, raiser.stats.hands, 0.15, 10) : 0.15;
    const threeBet = clamp(0.035 + 0.3 * Math.max(0, raiserPfr - 0.15), 0.035, 0.11);
    const bigPrice = v.toCall > 0.25 * (v.myChips + v.myBet);
    let callRange = bigPrice ? threeBet * 1.3 : threeBet + 0.08 + 0.05 * pos;
    if (v.raisesThisStreet >= 2) {
      if (pct < 0.02) return allIn(v, 'hard: 5-bet shove');
      if (pct < 0.045 && !bigPrice) return { type: 'call', reason: 'hard: call 4-bet' };
      return { type: 'fold', reason: 'hard: fold to 4-bet' };
    }
    if (pct < threeBet) return raiseTo(v, v.currentBet * (v.inPosition ? 3 : 3.5), 'hard: 3-bet value');
    if (pct > 0.2 && pct < 0.35 && pos > 0.6 && v.playersInHand <= 3 && r < 0.07 * persona.aggr) {
      return raiseTo(v, v.currentBet * 3, 'hard: 3-bet bluff');
    }
    if (pct < callRange) return { type: 'call', reason: 'hard: flat' };
    return { type: 'fold', reason: 'hard: fold to raise' };
  }

  // ---------- postflop ----------
  function rangeFor(o, level) {
    if (level === 'medium') return o.raisedPreflop ? 0.4 : 1;
    // hard: model from public stats and this hand's actions
    const vpip = smoothed(o.stats.vpip, o.stats.hands, 0.35, 8);
    const pfr = smoothed(o.stats.pfr, o.stats.hands, 0.15, 8);
    let t = 1;
    if (o.raisedPreflop) t = clamp(pfr, 0.06, 0.5);
    else if (o.calledPreflop) t = clamp(vpip, 0.12, 0.9);
    t *= Math.pow(0.75, o.aggrPostflop);
    return clamp(t, 0.04, 1);
  }

  function postflop(v, level, persona, rng) {
    const opps = v.opponents.filter((o) => !o.folded);
    const sims = level === 'hard' ? 1100 : level === 'medium' ? 450 : 150;
    const ranged = opps.map((o) => ({ topFrac: level === 'easy' ? 1 : rangeFor(o, level) }));
    const e = equity(v.hole, v.board, ranged, sims, rng);
    const pot = v.pot;
    const potOdds = v.toCall / (pot + v.toCall);
    const river = v.street === 'river';
    const r = rng();
    const nOpp = opps.length;
    const tag = (s) => `${level}: ${s} (eq ${(e * 100).toFixed(0)}%)`;
    const betPot = (frac) => v.currentBet + (pot + v.toCall) * frac;

    if (level === 'easy') {
      const felt = clamp(e + (rng() - 0.5) * 0.3, 0, 1);
      if (v.canCheck) {
        if (felt > 0.7 && r < 0.6) return raiseTo(v, betPot(0.5), tag('bet good hand'));
        if (r < 0.05) return raiseTo(v, betPot(0.4), tag('random stab'));
        return { type: 'check', reason: tag('check') };
      }
      if (felt > 0.85 && r < 0.3) return raiseTo(v, betPot(0.8), tag('raise'));
      if (felt > 0.25 || v.toCall <= pot * 0.3) return { type: 'call', reason: tag('call') };
      return { type: 'fold', reason: tag('fold') };
    }

    if (level === 'medium') {
      if (v.canCheck) {
        if (e > 0.58) return raiseTo(v, betPot(e > 0.8 ? 0.75 : 0.5), tag('value bet'));
        if (nOpp <= 2 && r < 0.08 * persona.aggr) return raiseTo(v, betPot(0.5), tag('bluff'));
        return { type: 'check', reason: tag('check') };
      }
      if (e > 0.78 && v.canRaise) return raiseTo(v, betPot(0.75), tag('raise for value'));
      if (e > potOdds + 0.03) return { type: 'call', reason: tag('call, odds ok') };
      return { type: 'fold', reason: tag('fold') };
    }

    // ---- hard ----
    const foldRate = opps.length
      ? opps.reduce((s, o) => s + smoothed(o.stats.foldedToBet, o.stats.facedBet, 0.4, 6), 0) / opps.length
      : 0.4;
    const looseness = opps.length
      ? opps.reduce((s, o) => s + smoothed(o.stats.vpip, o.stats.hands, 0.35, 8), 0) / opps.length
      : 0.35;
    const spr = v.myChips / Math.max(1, pot);

    if (v.canCheck) {
      if (e > 0.82 && !river && !v.inPosition && r < 0.2) return { type: 'check', reason: tag('trap check') };
      const valueT = 0.55 - (looseness > 0.45 ? 0.05 : 0);
      if (e > valueT) {
        let size = e > 0.8 ? 0.75 : 0.55;
        if (river && e > 0.9 && looseness > 0.45) size = 1.1; // overbet vs. calling stations
        return raiseTo(v, betPot(size), tag('value bet'));
      }
      // Continuation bet as preflop aggressor on the flop.
      if (v.street === 'flop' && v.isAggressor && nOpp <= 2 && r < 0.55 * persona.aggr * (0.6 + foldRate)) {
        return raiseTo(v, betPot(0.4), tag('c-bet'));
      }
      let bluff = (v.inPosition ? 0.2 : 0.1) * (nOpp === 1 ? 1 : 0.3) * persona.aggr * (0.5 + foldRate);
      if (!river && e > 0.3 && e < 0.55) bluff += 0.2; // semi-bluff draws
      if (r < bluff) return raiseTo(v, betPot(0.6), tag('bluff/semi-bluff'));
      return { type: 'check', reason: tag('check') };
    }

    let required = potOdds;
    if (!river && spr > 3) required *= 0.85; // implied odds
    const leftAfterCall = v.myChips - v.toCall;
    if (leftAfterCall < pot * 0.25 && e > 0.4) return allIn(v, tag('committed'));
    if (e > 0.82 && v.canRaise) {
      if (!river && r < 0.25) return { type: 'call', reason: tag('slowplay') };
      return raiseTo(v, betPot(0.8), tag('raise for value'));
    }
    if (e > 0.68 && v.canRaise && r < 0.5 * persona.aggr) return raiseTo(v, betPot(0.7), tag('raise/protect'));
    if (e >= required) return { type: 'call', reason: tag('call, odds ok') };
    if (!river && v.canRaise && e > 0.3 && nOpp === 1 && r < 0.06 * persona.aggr * (0.5 + foldRate)) {
      return raiseTo(v, betPot(0.75), tag('semi-bluff raise'));
    }
    return { type: 'fold', reason: tag('fold') };
  }

  function decide(view, difficulty, persona, rng) {
    rng = rng || Math.random;
    const level = DIFFICULTIES.includes(difficulty) ? difficulty : 'medium';
    persona = persona || { loose: 1, aggr: 1 };
    if (view.street === 'preflop') return preflop(view, level, persona, rng);
    return postflop(view, level, persona, rng);
  }

  P.AI = { decide, equity, chen, percentile, makePersona, DIFFICULTIES };

  if (typeof module !== 'undefined' && module.exports) module.exports = P;
})(typeof window !== 'undefined' ? window : globalThis);

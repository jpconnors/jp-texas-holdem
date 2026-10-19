/* Hand evaluator: best 5-card hand from 5-7 cards.
 * Returns a single integer score; higher is better. Score = category * 13^5 + kickers (base 13).
 * Categories: 0 high card, 1 pair, 2 two pair, 3 trips, 4 straight, 5 flush,
 *             6 full house, 7 quads, 8 straight flush.
 */
(function (root) {
  'use strict';
  const P = (root.Poker = root.Poker || {});

  const CATEGORY_NAMES = ['High Card', 'Pair', 'Two Pair', 'Three of a Kind', 'Straight', 'Flush', 'Full House', 'Four of a Kind', 'Straight Flush'];
  const BASE = 13 ** 5;

  function pack(cat, ks) {
    let v = cat;
    for (let i = 0; i < 5; i++) v = v * 13 + (ks[i] || 0);
    return v;
  }

  // Highest rank of a 5-card straight in a 13-bit rank mask, or -1. Wheel (A-5) returns 3 (five-high).
  function straightHigh(mask) {
    for (let hi = 12; hi >= 4; hi--) {
      const m = 0x1f << (hi - 4);
      if ((mask & m) === m) return hi;
    }
    if ((mask & 0x100f) === 0x100f) return 3;
    return -1;
  }

  function topRanks(mask, n, exclude) {
    const out = [];
    for (let r = 12; r >= 0 && out.length < n; r--) {
      if (mask & (1 << r) && !(exclude && exclude.includes(r))) out.push(r);
    }
    return out;
  }

  function evaluate(cards) {
    const rc = [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
    const sc = [0, 0, 0, 0];
    const sm = [0, 0, 0, 0];
    let rm = 0;
    for (let i = 0; i < cards.length; i++) {
      const c = cards[i];
      const r = c % 13;
      const s = (c / 13) | 0;
      rc[r]++;
      sc[s]++;
      sm[s] |= 1 << r;
      rm |= 1 << r;
    }

    let flushSuit = -1;
    for (let s = 0; s < 4; s++) if (sc[s] >= 5) flushSuit = s;
    if (flushSuit >= 0) {
      const sf = straightHigh(sm[flushSuit]);
      if (sf >= 0) return pack(8, [sf]);
    }

    const quads = [], trips = [], pairs = [];
    for (let r = 12; r >= 0; r--) {
      if (rc[r] === 4) quads.push(r);
      else if (rc[r] === 3) trips.push(r);
      else if (rc[r] === 2) pairs.push(r);
    }

    if (quads.length) {
      const q = quads[0];
      return pack(7, [q, topRanks(rm, 1, [q])[0]]);
    }
    if (trips.length && (trips.length > 1 || pairs.length)) {
      const t = trips[0];
      const p = Math.max(trips.length > 1 ? trips[1] : -1, pairs.length ? pairs[0] : -1);
      return pack(6, [t, p]);
    }
    if (flushSuit >= 0) return pack(5, topRanks(sm[flushSuit], 5));
    const st = straightHigh(rm);
    if (st >= 0) return pack(4, [st]);
    if (trips.length) {
      const t = trips[0];
      return pack(3, [t].concat(topRanks(rm, 2, [t])));
    }
    if (pairs.length >= 2) {
      const [p1, p2] = pairs;
      return pack(2, [p1, p2].concat(topRanks(rm, 1, [p1, p2])));
    }
    if (pairs.length) {
      const p = pairs[0];
      return pack(1, [p].concat(topRanks(rm, 3, [p])));
    }
    return pack(0, topRanks(rm, 5));
  }

  function categoryOf(score) {
    return Math.floor(score / BASE);
  }

  function describe(score) {
    const k = [];
    let v = score;
    for (let i = 0; i < 5; i++) {
      k.unshift(v % 13);
      v = Math.floor(v / 13);
    }
    const N = P.RANK_NAMES, PL = P.RANK_PLURALS;
    switch (v) {
      case 8: return k[0] === 12 ? 'Royal Flush' : `Straight Flush, ${N[k[0]]} high`;
      case 7: return `Four of a Kind, ${PL[k[0]]}`;
      case 6: return `Full House, ${PL[k[0]]} full of ${PL[k[1]]}`;
      case 5: return `Flush, ${N[k[0]]} high`;
      case 4: return `Straight, ${N[k[0]]} high`;
      case 3: return `Three of a Kind, ${PL[k[0]]}`;
      case 2: return `Two Pair, ${PL[k[0]]} and ${PL[k[1]]}`;
      case 1: return `Pair of ${PL[k[0]]}`;
      default: return `${N[k[0]]} High`;
    }
  }

  Object.assign(P, { evaluate, describe, categoryOf, CATEGORY_NAMES, straightHigh });

  if (typeof module !== 'undefined' && module.exports) module.exports = P;
})(typeof window !== 'undefined' ? window : globalThis);

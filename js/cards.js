/* Card display helpers for the UI (the engine itself is Rust; see engine/src/cards.rs).
 * A card is an int 0..51: rank = c % 13 (0 = deuce .. 12 = ace), suit = floor(c / 13).
 * Works as a plain <script> in the browser and via require() in Node.
 */
(function (root) {
  'use strict';
  const P = (root.Poker = root.Poker || {});

  const RANKS = '23456789TJQKA';
  const SUITS = ['s', 'h', 'd', 'c'];
  const SUIT_SYMBOLS = { s: '♠', h: '♥', d: '♦', c: '♣' };
  const RANK_NAMES = ['Two', 'Three', 'Four', 'Five', 'Six', 'Seven', 'Eight', 'Nine', 'Ten', 'Jack', 'Queen', 'King', 'Ace'];
  const RANK_PLURALS = ['Twos', 'Threes', 'Fours', 'Fives', 'Sixes', 'Sevens', 'Eights', 'Nines', 'Tens', 'Jacks', 'Queens', 'Kings', 'Aces'];

  const rankOf = (c) => c % 13;
  const suitOf = (c) => (c / 13) | 0;

  function cardToString(c) {
    return RANKS[rankOf(c)] + SUITS[suitOf(c)];
  }

  function cardLabel(c) {
    const r = RANKS[rankOf(c)];
    return (r === 'T' ? '10' : r) + SUIT_SYMBOLS[SUITS[suitOf(c)]];
  }

  function parseCard(s) {
    const r = RANKS.indexOf(s[0].toUpperCase());
    const su = SUITS.indexOf(s[1].toLowerCase());
    if (r < 0 || su < 0) throw new Error('Bad card: ' + s);
    return su * 13 + r;
  }

  function parseCards(str) {
    return str.trim().split(/\s+/).filter(Boolean).map(parseCard);
  }

  function newDeck() {
    const d = new Array(52);
    for (let i = 0; i < 52; i++) d[i] = i;
    return d;
  }

  // Cryptographically strong RNG for dealing when available; Math.random otherwise.
  function secureRandom() {
    const c = root.crypto;
    if (c && typeof c.getRandomValues === 'function') {
      const buf = new Uint32Array(1);
      c.getRandomValues(buf);
      return buf[0] / 4294967296;
    }
    return Math.random();
  }

  // Seeded RNG (mulberry32) for reproducible tests.
  function seededRandom(seed) {
    let a = seed >>> 0;
    return function () {
      a = (a + 0x6d2b79f5) >>> 0;
      let t = a;
      t = Math.imul(t ^ (t >>> 15), t | 1);
      t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
      return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
    };
  }

  function shuffle(arr, rng) {
    rng = rng || secureRandom;
    for (let i = arr.length - 1; i > 0; i--) {
      const j = Math.floor(rng() * (i + 1));
      const t = arr[i];
      arr[i] = arr[j];
      arr[j] = t;
    }
    return arr;
  }

  Object.assign(P, {
    RANKS, SUITS, SUIT_SYMBOLS, RANK_NAMES, RANK_PLURALS,
    rankOf, suitOf, cardToString, cardLabel, parseCard, parseCards,
    newDeck, shuffle, secureRandom, seededRandom,
  });

  if (typeof module !== 'undefined' && module.exports) module.exports = P;
})(typeof window !== 'undefined' ? window : globalThis);

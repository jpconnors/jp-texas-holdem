/* Loads the Rust engine (WebAssembly, embedded in js/holdem-wasm.js) and exposes it as Poker.Game.
 *
 * The wasm module exports three functions that exchange JSON through its memory:
 *   jp_alloc(len) -> ptr, jp_call(ptr, len) -> outLen, jp_out_ptr() -> ptr
 * The wasm is embedded as base64 (not fetched) so the game also works when index.html is
 * opened straight from disk (file://), where browsers block fetch().
 */
(function (root) {
  'use strict';
  const P = (root.Poker = root.Poker || {});

  let wasm = null;
  const encoder = new TextEncoder();
  const decoder = new TextDecoder();

  function base64ToBytes(b64) {
    const bin = atob(b64);
    const bytes = new Uint8Array(bin.length);
    for (let i = 0; i < bin.length; i++) bytes[i] = bin.charCodeAt(i);
    return bytes;
  }

  // Resolves once the engine is ready; rejects with a readable error if it can't load.
  P.ready = (async function load() {
    if (typeof WebAssembly !== 'object') throw new Error('This browser does not support WebAssembly.');
    if (!P.WASM_BASE64) throw new Error('The game engine file (js/holdem-wasm.js) is missing. See README: "Rebuilding the engine".');
    const { instance } = await WebAssembly.instantiate(base64ToBytes(P.WASM_BASE64), {});
    wasm = instance.exports;
    return call({ cmd: 'version' });
  })();

  function call(cmd) {
    if (!wasm) throw new Error('Engine not loaded yet');
    const input = encoder.encode(JSON.stringify(cmd));
    const ptr = wasm.jp_alloc(input.length);
    new Uint8Array(wasm.memory.buffer, ptr, input.length).set(input);
    const outLen = wasm.jp_call(ptr, input.length); // frees the input buffer
    const outPtr = wasm.jp_out_ptr();
    // Re-read memory.buffer after the call: it is replaced whenever wasm memory grows.
    const res = JSON.parse(decoder.decode(new Uint8Array(wasm.memory.buffer, outPtr, outLen)));
    if (!res.ok) throw new Error(res.error);
    return res;
  }

  function randomSeed() {
    const c = root.crypto;
    if (c && c.getRandomValues) {
      const a = new Uint32Array(2);
      c.getRandomValues(a);
      return (a[0] & 0x1fffff) * 4294967296 + a[1]; // 53 random bits: exact as a JS number
    }
    return Math.floor(Math.random() * Number.MAX_SAFE_INTEGER);
  }

  /* Same shape the UI used with the old JavaScript engine: fields are refreshed from the Rust
   * state after every call, and engine events are forwarded to listeners registered with on(). */
  class Game {
    constructor(opts) {
      this.listeners = [];
      this._apply(call({
        cmd: 'new',
        players: opts.players.map((p) => ({ name: p.name, isHuman: !!p.isHuman, difficulty: p.difficulty || null, chips: p.chips })),
        smallBlind: opts.smallBlind,
        bigBlind: opts.bigBlind,
        blindIncreaseEvery: opts.blindIncreaseEvery || 0,
        seed: opts.seed != null ? opts.seed : randomSeed(),
        dealer: opts.dealer != null ? opts.dealer : null,
      }));
    }

    on(fn) {
      this.listeners.push(fn);
      return () => { this.listeners = this.listeners.filter((f) => f !== fn); };
    }

    _apply(res) {
      Object.assign(this, res.state);
      for (const ev of res.events || []) {
        for (const fn of this.listeners) {
          try { fn(ev.type, ev, this); } catch (e) { console.error(e); }
        }
      }
      return res;
    }

    startHand() { this._apply(call({ cmd: 'start' })); }
    act(action) { this._apply(call({ cmd: 'act', action })); }
    /** The bot whose turn it is decides and acts; returns {type, amount?, reason}. */
    botAct() { return this._apply(call({ cmd: 'bot' })).decision; }
    legalActions() { return this.legal; }
    pot() { return this.potTotal; }
    totalChips() { return this.totalChipsInPlay; }
    isGameOver() { return this.gameOver; }
    human() { return this.players.find((p) => p.isHuman) || null; }
  }

  P.Game = Game;
  P.engineCall = call; // for debugging in the console
})(typeof window !== 'undefined' ? window : globalThis);

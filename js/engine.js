/* No-limit Texas Hold'em game engine.
 * Pure logic, no DOM. Drive it with startHand() and act({type, amount}).
 * Action types: 'fold' | 'check' | 'call' | 'raise' (alias 'bet'). amount = total "raise to" for the street.
 * Events (game.on(fn)): handStart, blinds, blind, deal, action, street, showdown, uncontested, busted, handEnd.
 */
(function (root) {
  'use strict';
  const P = (root.Poker = root.Poker || {});

  // Blind schedule as multipliers of the starting blinds.
  const BLIND_SCHEDULE = [1, 1.5, 2, 3, 4, 6, 8, 10, 15, 20, 30, 40, 50, 75, 100];

  function makePlayer(p, seat) {
    return {
      seat,
      name: p.name,
      isHuman: !!p.isHuman,
      difficulty: p.difficulty || null,
      persona: p.persona || null,
      chips: p.chips,
      hole: [],
      inHand: false,
      folded: false,
      allIn: false,
      bet: 0,          // chips put in on the current street
      contrib: 0,      // chips put in this hand
      hasActed: false,
      actedRaiseNo: -1,
      out: p.chips <= 0,
      lastAction: '',
      actions: [],     // this hand: {street, type, amount}
      stats: { hands: 0, vpip: 0, pfr: 0, facedBet: 0, foldedToBet: 0, bets: 0, calls: 0, wins: 0, biggestPot: 0 },
      _vpip: false,
      _pfr: false,
    };
  }

  class Game {
    constructor(opts) {
      if (!opts || !Array.isArray(opts.players) || opts.players.length < 2) throw new Error('Need at least 2 players');
      this.players = opts.players.map(makePlayer);
      this.baseSmall = opts.smallBlind != null ? opts.smallBlind : 10;
      this.baseBig = opts.bigBlind != null ? opts.bigBlind : 20;
      this.smallBlind = this.baseSmall;
      this.bigBlind = this.baseBig;
      this.blindIncreaseEvery = opts.blindIncreaseEvery || 0;
      this.blindLevel = 0;
      this.rng = opts.rng || P.secureRandom;
      const n = this.players.length;
      this.dealer = typeof opts.dealer === 'number' ? opts.dealer - 1 : Math.floor(this.rng() * n) - 1;
      this.handNo = 0;
      this.board = [];
      this.deck = [];
      this.street = null;
      this.current = -1;
      this.currentBet = 0;
      this.minRaise = 0;
      this.raiseNo = 0;
      this.streetRaises = 0;
      this.lastAggressor = -1;
      this.handOver = true;
      this.lastResult = null;
      this.listeners = [];
    }

    on(fn) { this.listeners.push(fn); return () => { this.listeners = this.listeners.filter((f) => f !== fn); }; }
    emit(type, data) {
      for (const fn of this.listeners) {
        try { fn(type, data || {}, this); } catch (e) { if (typeof console !== 'undefined') console.error(e); }
      }
    }

    // ---------- helpers ----------
    nextSeat(from, pred) {
      const n = this.players.length;
      for (let k = 1; k <= n; k++) {
        const i = (((from + k) % n) + n) % n;
        if (pred(this.players[i])) return i;
      }
      return -1;
    }
    canAct(p) { return p.inHand && !p.folded && !p.allIn; }
    contenders() { return this.players.filter((p) => p.inHand && !p.folded); }
    pot() { return this.players.reduce((s, p) => s + p.contrib, 0); }
    totalChips() { return this.players.reduce((s, p) => s + p.chips + p.contrib, 0); }
    human() { return this.players.find((p) => p.isHuman) || null; }

    isGameOver() {
      if (!this.handOver) return false;
      const alive = this.players.filter((p) => p.chips > 0);
      const h = this.human();
      if (h && h.chips <= 0) return true;
      return alive.length <= 1;
    }

    updateBlinds() {
      if (!this.blindIncreaseEvery) return;
      const level = Math.floor((this.handNo - 1) / this.blindIncreaseEvery);
      if (level === this.blindLevel) return;
      this.blindLevel = level;
      const mult = level < BLIND_SCHEDULE.length
        ? BLIND_SCHEDULE[level]
        : BLIND_SCHEDULE[BLIND_SCHEDULE.length - 1] * 2 ** (level - BLIND_SCHEDULE.length + 1);
      this.smallBlind = Math.max(1, Math.round(this.baseSmall * mult));
      this.bigBlind = Math.max(2, Math.round(this.baseBig * mult));
      this.emit('blinds', { smallBlind: this.smallBlind, bigBlind: this.bigBlind, level });
    }

    put(p, amount) {
      const a = Math.max(0, Math.min(amount, p.chips));
      p.chips -= a;
      p.bet += a;
      p.contrib += a;
      if (p.chips === 0) p.allIn = true;
      return a;
    }

    // ---------- hand flow ----------
    startHand() {
      if (!this.handOver) throw new Error('Hand already in progress');
      if (this.isGameOver()) throw new Error('Game is over');
      this.handNo++;
      this.updateBlinds();

      for (const p of this.players) {
        p.out = p.chips <= 0;
        p.inHand = !p.out;
        p.hole = [];
        p.folded = false;
        p.allIn = false;
        p.bet = 0;
        p.contrib = 0;
        p.hasActed = false;
        p.actedRaiseNo = -1;
        p.lastAction = '';
        p.actions = [];
        p._vpip = false;
        p._pfr = false;
        if (p.inHand) p.stats.hands++;
      }

      this.dealer = this.nextSeat(this.dealer, (p) => p.inHand);
      this.board = [];
      this.street = 'preflop';
      this.handOver = false;
      this.lastResult = null;
      this.currentBet = 0;
      this.minRaise = this.bigBlind;
      this.raiseNo = 0;
      this.streetRaises = 0;
      this.lastAggressor = -1;
      this.deck = P.shuffle(P.newDeck(), this.rng);

      const seated = this.players.filter((p) => p.inHand);
      let s = this.dealer;
      for (let round = 0; round < 2; round++) {
        for (let k = 0; k < seated.length; k++) {
          s = this.nextSeat(s, (p) => p.inHand);
          this.players[s].hole.push(this.deck.pop());
        }
      }

      const headsUp = seated.length === 2;
      this.sbSeat = headsUp ? this.dealer : this.nextSeat(this.dealer, (p) => p.inHand);
      this.bbSeat = this.nextSeat(this.sbSeat, (p) => p.inHand);

      this.emit('handStart', {
        handNo: this.handNo, dealer: this.dealer, sbSeat: this.sbSeat, bbSeat: this.bbSeat,
        smallBlind: this.smallBlind, bigBlind: this.bigBlind,
      });

      this.postBlind(this.sbSeat, this.smallBlind, 'small blind');
      this.postBlind(this.bbSeat, this.bigBlind, 'big blind');
      this.currentBet = this.bigBlind;

      this.current = this.bbSeat;
      this.proceed();
    }

    postBlind(seat, amount, label) {
      const p = this.players[seat];
      const paid = this.put(p, amount);
      p.lastAction = label === 'small blind' ? `SB ${paid}` : `BB ${paid}`;
      this.emit('blind', { seat, amount: paid, label, allIn: p.allIn });
    }

    legalActions() {
      if (this.handOver || this.current < 0) return null;
      const p = this.players[this.current];
      const toCall = Math.max(0, Math.min(this.currentBet - p.bet, p.chips));
      const reopened = !p.hasActed || p.actedRaiseNo < this.raiseNo;
      const othersCanAct = this.players.some((o) => o !== p && this.canAct(o));
      const maxRaiseTo = p.bet + p.chips;
      const minRaiseTo = Math.min(this.currentBet + this.minRaise, maxRaiseTo);
      return {
        seat: p.seat,
        toCall,
        canCheck: toCall === 0,
        canCall: toCall > 0,
        canRaise: reopened && p.chips > toCall && othersCanAct,
        minRaiseTo,
        maxRaiseTo,
        currentBet: this.currentBet,
        pot: this.pot(),
      };
    }

    // Applies an action for the player whose turn it is. Illegal actions are coerced to the nearest legal one.
    act(action) {
      if (this.handOver) throw new Error('No hand in progress');
      const p = this.players[this.current];
      const L = this.legalActions();
      let type = action && action.type === 'bet' ? 'raise' : action && action.type;

      if (type === 'raise' && !L.canRaise) type = L.canCheck ? 'check' : 'call';
      if (type === 'check' && !L.canCheck) type = 'fold';
      if (type === 'call' && L.canCheck) type = 'check';
      if (!['fold', 'check', 'call', 'raise'].includes(type)) type = L.canCheck ? 'check' : 'fold';

      const street = this.street;
      const facingBet = L.toCall > 0;
      let amount = 0;
      let label = '';

      if (type === 'fold') {
        p.folded = true;
        label = 'Fold';
      } else if (type === 'check') {
        label = 'Check';
      } else if (type === 'call') {
        amount = this.put(p, L.toCall);
        label = p.allIn ? `All-in ${p.bet}` : `Call ${amount}`;
        p.stats.calls++;
      } else {
        let to = Math.floor(Number(action.amount));
        if (!Number.isFinite(to)) to = L.minRaiseTo;
        to = Math.max(L.minRaiseTo, Math.min(L.maxRaiseTo, to));
        const raiseSize = to - this.currentBet;
        const wasUnopened = this.currentBet === 0;
        amount = this.put(p, to - p.bet);
        if (p.bet > this.currentBet) {
          if (raiseSize >= this.minRaise) {
            this.minRaise = raiseSize;
            this.raiseNo++;
          }
          this.currentBet = p.bet;
          this.lastAggressor = p.seat;
          this.streetRaises++;
        }
        label = p.allIn ? `All-in ${p.bet}` : wasUnopened ? `Bet ${p.bet}` : `Raise to ${p.bet}`;
        p.stats.bets++;
      }

      // Stats used by the AI for opponent modeling.
      if (street === 'preflop' && (type === 'call' || type === 'raise') && !p._vpip) { p._vpip = true; p.stats.vpip++; }
      if (street === 'preflop' && type === 'raise' && !p._pfr) { p._pfr = true; p.stats.pfr++; }
      if (street !== 'preflop' && facingBet) {
        p.stats.facedBet++;
        if (type === 'fold') p.stats.foldedToBet++;
      }

      p.hasActed = true;
      p.actedRaiseNo = this.raiseNo;
      p.lastAction = label;
      p.actions.push({ street, type, amount: type === 'raise' ? p.bet : amount });
      this.emit('action', { seat: p.seat, type, amount, total: p.bet, label, street, allIn: p.allIn });

      this.proceed();
    }

    roundComplete() {
      const actors = this.players.filter((p) => this.canAct(p));
      if (actors.length === 0) return true;
      if (actors.length === 1 && actors[0].bet >= this.currentBet) {
        // Lone player with chips left: nothing to do unless they still have to respond.
        return true;
      }
      return actors.every((p) => p.hasActed && p.bet === this.currentBet);
    }

    proceed() {
      const live = this.contenders();
      if (live.length === 1) return this.awardUncontested(live[0]);
      if (this.roundComplete()) return this.nextStreet();
      this.current = this.nextSeat(this.current, (p) => this.canAct(p));
    }

    nextStreet() {
      for (const p of this.players) {
        p.bet = 0;
        p.hasActed = false;
        p.actedRaiseNo = -1;
      }
      this.currentBet = 0;
      this.minRaise = this.bigBlind;
      this.raiseNo = 0;
      this.streetRaises = 0;

      if (this.street === 'river') return this.showdown();

      let cards;
      if (this.street === 'preflop') { this.street = 'flop'; cards = [this.deck.pop(), this.deck.pop(), this.deck.pop()]; }
      else if (this.street === 'flop') { this.street = 'turn'; cards = [this.deck.pop()]; }
      else { this.street = 'river'; cards = [this.deck.pop()]; }
      this.board.push(...cards);
      for (const p of this.players) if (p.inHand && !p.folded && !p.allIn) p.lastAction = '';
      this.emit('street', { street: this.street, cards, board: this.board.slice() });

      if (this.roundComplete()) return this.nextStreet(); // everyone all-in: run it out
      this.current = this.nextSeat(this.dealer, (p) => this.canAct(p));
    }

    seatDistanceFromDealer(seat) {
      const n = this.players.length;
      return (seat - this.dealer + n) % n || n;
    }

    showdown() {
      const live = this.contenders();
      const scores = {};
      for (const p of live) scores[p.seat] = P.evaluate(p.hole.concat(this.board));

      const levels = [...new Set(live.map((p) => p.contrib))].sort((a, b) => a - b);
      const pots = [];
      let prev = 0;
      for (const L of levels) {
        let amount = 0;
        for (const p of this.players) amount += Math.max(0, Math.min(p.contrib, L) - prev);
        const eligible = live.filter((p) => p.contrib >= L).map((p) => p.seat);
        if (amount > 0) pots.push({ amount, eligible });
        prev = L;
      }
      const assigned = pots.reduce((s, x) => s + x.amount, 0);
      const total = this.pot();
      if (total > assigned && pots.length) pots[pots.length - 1].amount += total - assigned;

      const payouts = {};
      for (const pot of pots) {
        const best = Math.max(...pot.eligible.map((s) => scores[s]));
        const winners = pot.eligible
          .filter((s) => scores[s] === best)
          .sort((a, b) => this.seatDistanceFromDealer(a) - this.seatDistanceFromDealer(b));
        const share = Math.floor(pot.amount / winners.length);
        let rem = pot.amount - share * winners.length;
        for (const w of winners) {
          const extra = rem > 0 ? 1 : 0;
          rem -= extra;
          payouts[w] = (payouts[w] || 0) + share + extra;
        }
        pot.winners = winners;
        pot.returned = pot.eligible.length === 1;
        pot.handName = P.describe(best);
      }

      for (const seat of Object.keys(payouts)) {
        const p = this.players[seat];
        p.chips += payouts[seat];
      }
      for (const pot of pots) {
        if (pot.returned) continue;
        for (const w of pot.winners) {
          const p = this.players[w];
          p.stats.biggestPot = Math.max(p.stats.biggestPot, pot.amount);
        }
      }
      const realWinners = new Set(pots.filter((x) => !x.returned).flatMap((x) => x.winners));
      for (const w of realWinners) this.players[w].stats.wins++;

      const result = {
        type: 'showdown',
        board: this.board.slice(),
        hands: live.map((p) => ({ seat: p.seat, hole: p.hole.slice(), score: scores[p.seat], name: P.describe(scores[p.seat]) })),
        pots,
        payouts,
        winners: [...realWinners],
      };
      this.emit('showdown', result);
      this.finishHand(result);
    }

    awardUncontested(winner) {
      const amount = this.pot();
      winner.chips += amount;
      winner.stats.wins++;
      winner.stats.biggestPot = Math.max(winner.stats.biggestPot, amount);
      const result = { type: 'uncontested', winner: winner.seat, amount, winners: [winner.seat], payouts: { [winner.seat]: amount } };
      this.emit('uncontested', result);
      this.finishHand(result);
    }

    finishHand(result) {
      this.handOver = true;
      this.current = -1;
      this.lastResult = result;
      for (const p of this.players) {
        p.bet = 0;
        if (p.inHand && p.chips === 0) {
          p.out = true;
          this.emit('busted', { seat: p.seat });
        }
      }
      // contrib is kept until the next hand so the UI can show what was in the pot; zero it for chip accounting.
      for (const p of this.players) p.contrib = 0;
      this.emit('handEnd', result);
    }

    // ---------- information a bot is allowed to see ----------
    viewFor(seat) {
      const me = this.players[seat];
      const L = seat === this.current ? this.legalActions() : null;
      const live = this.contenders();
      const order = [];
      let s = this.dealer;
      for (let k = 0; k < this.players.length; k++) {
        s = this.nextSeat(s, (p) => p.inHand && !p.folded);
        if (s < 0 || order.includes(s)) break;
        order.push(s);
      }
      const idx = order.indexOf(seat);
      const position = order.length > 1 ? idx / (order.length - 1) : 1;
      const limpers = this.street === 'preflop'
        ? this.players.filter((p) => p.seat !== seat && p.actions.some((a) => a.street === 'preflop' && a.type === 'call') && !p.actions.some((a) => a.type === 'raise')).length
        : 0;

      const opponents = this.players
        .filter((p) => p.seat !== seat && p.inHand)
        .map((p) => ({
          seat: p.seat,
          name: p.name,
          isHuman: p.isHuman,
          chips: p.chips,
          bet: p.bet,
          folded: p.folded,
          allIn: p.allIn,
          stats: Object.assign({}, p.stats),
          raisedPreflop: p.actions.some((a) => a.street === 'preflop' && a.type === 'raise'),
          calledPreflop: p.actions.some((a) => a.street === 'preflop' && a.type === 'call'),
          aggrPostflop: p.actions.filter((a) => a.street !== 'preflop' && a.type === 'raise').length,
        }));

      const liveOpp = opponents.filter((o) => !o.folded);
      const maxOppStack = Math.max(0, ...liveOpp.map((o) => o.chips + o.bet));

      return {
        seat,
        hole: me.hole.slice(),
        board: this.board.slice(),
        street: this.street,
        pot: this.pot(),
        bigBlind: this.bigBlind,
        smallBlind: this.smallBlind,
        myChips: me.chips,
        myBet: me.bet,
        currentBet: this.currentBet,
        toCall: L ? L.toCall : 0,
        canCheck: L ? L.canCheck : false,
        canRaise: L ? L.canRaise : false,
        minRaiseTo: L ? L.minRaiseTo : 0,
        maxRaiseTo: L ? L.maxRaiseTo : 0,
        position,
        inPosition: order[order.length - 1] === seat,
        playersInHand: live.length,
        limpers,
        raisesThisStreet: this.streetRaises,
        lastAggressor: this.lastAggressor,
        isAggressor: this.lastAggressor === seat,
        effectiveStack: Math.min(me.chips + me.bet, maxOppStack),
        opponents,
      };
    }
  }

  P.Game = Game;
  P.BLIND_SCHEDULE = BLIND_SCHEDULE;

  if (typeof module !== 'undefined' && module.exports) module.exports = P;
})(typeof window !== 'undefined' ? window : globalThis);

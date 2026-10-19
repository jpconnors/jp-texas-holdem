/* Table UI + game loop: renders state, runs bots, handles your input. */
(function () {
  'use strict';
  const P = window.Poker;
  const $ = (s) => document.querySelector(s);

  const BOT_NAMES = ['Lefty', 'Dutch', 'Rita', 'Sly', 'Moose', 'Vega', 'Bishop', 'Kit', 'Tex', 'Nova'];
  const SPEED_MS = { slow: 1500, normal: 850, fast: 300 };
  const LAYOUTS = {
    1: ['top'],
    2: ['left', 'right'],
    3: ['left', 'top', 'right'],
    4: ['left', 'top-left', 'top-right', 'right'],
    5: ['left', 'top-left', 'top', 'top-right', 'right'],
  };
  const DEFAULTS = { name: 'JP', opponents: 3, difficulty: 'medium', chips: 2000, bb: 20, blindUp: 10, speed: 'normal', autoDeal: false };

  let game = null;
  let settings = null;
  let botTimer = null;
  let autoTimer = null;
  let seatEls = [];

  // ---------- persistence (optional; page works without it) ----------
  function load(key, fallback) {
    try { const v = localStorage.getItem(key); return v ? JSON.parse(v) : fallback; } catch (e) { return fallback; }
  }
  function save(key, value) {
    try { localStorage.setItem(key, JSON.stringify(value)); } catch (e) { /* ignore */ }
  }

  // ---------- setup ----------
  function openSetup() {
    clearTimers();
    const s = Object.assign({}, DEFAULTS, load('jpholdem.settings', {}));
    const f = $('#setupForm');
    for (const [k, v] of Object.entries(s)) {
      const el = f.elements[k];
      if (!el) continue;
      if (el.type === 'checkbox') el.checked = !!v; else el.value = String(v);
    }
    const rec = load('jpholdem.record', {});
    const parts = ['easy', 'medium', 'hard', 'mixed']
      .filter((d) => rec[d])
      .map((d) => `${d[0].toUpperCase() + d.slice(1)} ${rec[d].w}-${rec[d].l}`);
    $('#record').textContent = parts.length ? 'Your record (W-L): ' + parts.join(' · ') : '';
    $('#gameOver').hidden = true;
    $('#setup').hidden = false;
  }

  $('#setupForm').addEventListener('submit', (e) => {
    e.preventDefault();
    const f = e.target.elements;
    const s = {
      name: (f.name.value || 'You').trim().slice(0, 16) || 'You',
      opponents: Number(f.opponents.value),
      difficulty: f.difficulty.value,
      chips: Number(f.chips.value),
      bb: Number(f.bb.value),
      blindUp: Number(f.blindUp.value),
      speed: f.speed.value,
      autoDeal: f.autoDeal.checked,
    };
    save('jpholdem.settings', s);
    $('#setup').hidden = true;
    startGame(s);
  });

  function startGame(s) {
    clearTimers();
    settings = s;
    const names = P.shuffle(BOT_NAMES.slice(), Math.random);
    const mixed = P.shuffle(['easy', 'medium', 'hard', 'easy', 'medium'].slice(), Math.random);
    const players = [{ name: s.name, isHuman: true, chips: s.chips }];
    for (let i = 0; i < s.opponents; i++) {
      const d = s.difficulty === 'mixed' ? (i < 3 ? ['easy', 'medium', 'hard'][i] : mixed[i]) : s.difficulty;
      players.push({ name: names[i], isHuman: false, difficulty: d, persona: P.AI.makePersona(d), chips: s.chips });
    }
    if (s.difficulty === 'mixed') {
      // Shuffle bot seats so the hard bot isn't always on your left.
      const bots = P.shuffle(players.slice(1), Math.random);
      players.splice(1, bots.length, ...bots);
    }
    game = new P.Game({ players, smallBlind: Math.round(s.bb / 2), bigBlind: s.bb, blindIncreaseEvery: s.blindUp });
    game.on(onEvent);
    $('#log').innerHTML = '';
    buildSeats();
    showBanner(`Welcome, <strong>${esc(s.name)}</strong>. Click <strong>I'm Ready</strong> to deal.`);
    render();
    setReadyEnabled(true);
  }

  function buildSeats() {
    const wrap = $('#seats');
    wrap.innerHTML = '';
    seatEls = [];
    const layout = ['bottom'].concat(LAYOUTS[game.players.length - 1]);
    game.players.forEach((p, i) => {
      const pos = layout[i];
      const vertical = pos === 'left' || pos === 'right';
      const el = document.createElement('div');
      el.className = `seat pos-${pos}`;
      el.innerHTML = `
        <div class="slots ${vertical ? 'vertical' : 'horizontal'}"><div class="slot"></div><div class="slot"></div></div>
        <div class="plate">
          <div class="name"><span class="nm"></span>${p.isHuman ? '' : `<span class="badge ${p.difficulty}">${p.difficulty}</span>`}</div>
          <div class="chips"></div>
          <div class="act"></div>
          <div class="dealer" hidden>D</div>
        </div>
        <div class="betchip" hidden></div>
        <div class="hand-name" hidden></div>`;
      el.querySelector('.nm').textContent = p.isHuman ? `${p.name} (you)` : p.name;
      wrap.appendChild(el);
      seatEls.push(el);
    });
  }

  // ---------- rendering ----------
  function cardEl(c, opts) {
    const d = document.createElement('div');
    if (c == null) { d.className = 'card back'; return d; }
    const suit = P.SUITS[P.suitOf(c)];
    const sym = P.SUIT_SYMBOLS[suit];
    const r = P.RANKS[P.rankOf(c)];
    d.className = 'card' + (suit === 'h' || suit === 'd' ? ' red' : '');
    d.innerHTML = `<span class="rk">${r === 'T' ? '10' : r}</span><span class="st">${sym}</span><span class="big">${sym}</span>`;
    d.setAttribute('aria-label', P.cardLabel(c));
    if (opts && opts.delay) d.style.animationDelay = opts.delay + 's';
    return d;
  }

  // Only touches a slot when what it should show changes, so cards don't re-animate.
  function setSlot(slot, key, make) {
    if (slot.dataset.key === key) return slot.firstChild;
    slot.dataset.key = key;
    slot.innerHTML = '';
    if (key === '') return null;
    const el = make();
    slot.appendChild(el);
    return el;
  }

  function render() {
    if (!game) return;
    const result = game.handOver ? game.lastResult : null;
    const showdown = result && result.type === 'showdown';
    const winners = new Set(result ? result.winners : []);

    $('#pot').textContent = 'Pot: ' + fmt(game.handOver ? 0 : game.pot());
    $('#blinds').textContent = `Blinds ${fmt(game.smallBlind)} / ${fmt(game.bigBlind)} · Hand #${game.handNo}`;

    // Board
    const boardSlots = $('#board').querySelectorAll('.slot');
    let newIdx = 0;
    boardSlots.forEach((slot, i) => {
      const c = game.board[i];
      const key = c == null ? '' : 'c' + c;
      const isNew = slot.dataset.key !== key && key !== '';
      setSlot(slot, key, () => cardEl(c, { delay: isNew ? 0.12 * newIdx++ : 0 }));
    });

    game.players.forEach((p, i) => {
      const el = seatEls[i];
      const slots = el.querySelectorAll('.slot');
      const revealed = p.isHuman || (showdown && result.hands.some((h) => h.seat === p.seat));
      slots.forEach((slot, k) => {
        let key = '';
        if (p.inHand && p.hole[k] != null && !(p.folded && !p.isHuman)) key = revealed ? 'c' + p.hole[k] : 'back';
        if (p.isHuman && p.folded && p.hole[k] != null) key = 'c' + p.hole[k] + 'f';
        const card = setSlot(slot, key, () => (key === 'back' ? cardEl(null) : cardEl(p.hole[k])));
        if (card) {
          card.classList.toggle('dim', p.folded);
          card.classList.toggle('win', showdown && winners.has(p.seat));
        }
      });

      el.classList.toggle('out', p.out && !p.inHand);
      el.classList.toggle('folded', p.folded);
      el.classList.toggle('active', !game.handOver && game.current === p.seat);
      el.classList.toggle('winner', !!result && winners.has(p.seat));
      el.querySelector('.chips').textContent = p.out && !p.inHand ? 'Out' : fmt(p.chips) + ' chips';
      const act = el.querySelector('.act');
      const thinking = !game.handOver && game.current === p.seat && !p.isHuman;
      act.textContent = thinking ? 'Thinking' : p.lastAction;
      act.classList.toggle('thinking', thinking);
      el.querySelector('.dealer').hidden = game.handNo === 0 || game.dealer !== p.seat;

      const chip = el.querySelector('.betchip');
      const won = result && result.payouts[p.seat];
      if (!game.handOver && p.bet > 0) { chip.hidden = false; chip.textContent = fmt(p.bet); }
      else if (won) { chip.hidden = false; chip.textContent = '+' + fmt(won); }
      else chip.hidden = true;

      const hn = el.querySelector('.hand-name');
      const h = showdown && result.hands.find((x) => x.seat === p.seat);
      hn.hidden = !h;
      if (h) hn.textContent = h.name;
    });

    renderControls();
  }

  function renderControls() {
    const me = game.human();
    $('#money').textContent = 'Money: ' + fmt(me ? me.chips : 0);
    const myTurn = isHumanTurn();
    const L = myTurn ? game.legalActions() : null;

    $('#btnFold').disabled = !myTurn;
    $('#btnCheck').disabled = !myTurn || !L.canCheck;
    $('#btnCall').disabled = !myTurn || !L.canCall;
    $('#btnCall').textContent = myTurn && L.canCall ? `Call ${fmt(L.toCall)}` : 'Call';
    $('#btnBet').disabled = !myTurn || !L.canRaise;
    $('#btnBet').textContent = myTurn && game.currentBet > 0 ? 'Raise' : 'Bet';
    const amt = $('#amount');
    amt.disabled = !myTurn || !L.canRaise;
    document.querySelectorAll('[data-preset]').forEach((b) => { b.disabled = !myTurn || !L.canRaise; });
    if (myTurn && L.canRaise) {
      amt.min = L.minRaiseTo;
      amt.max = L.maxRaiseTo;
      amt.step = Math.max(1, game.smallBlind);
      const v = Number(amt.value);
      if (!amt.dataset.turn || amt.dataset.turn !== turnKey() || !(v >= L.minRaiseTo && v <= L.maxRaiseTo)) {
        amt.value = L.minRaiseTo;
        amt.dataset.turn = turnKey();
      }
    } else {
      amt.value = '';
      delete amt.dataset.turn;
    }

    // Hand label
    let label = '';
    if (me && me.inHand && me.hole.length === 2) {
      if (game.board.length >= 3) label = 'You have: ' + P.describe(P.evaluate(me.hole.concat(game.board)));
      else {
        const [a, b] = me.hole;
        const ra = P.RANKS[P.rankOf(a)], rb = P.RANKS[P.rankOf(b)];
        const hi = P.rankOf(a) >= P.rankOf(b) ? [ra, rb] : [rb, ra];
        label = 'You have: ' + (ra === rb ? `Pocket ${P.RANK_PLURALS[P.rankOf(a)]}` : `${hi[0]}${hi[1]} ${P.suitOf(a) === P.suitOf(b) ? 'suited' : 'offsuit'}`);
      }
      if (me.folded) label += ' (folded)';
      if (myTurn) label = 'Your turn. ' + label;
    }
    $('#handLabel').textContent = label;
  }

  function turnKey() { return `${game.handNo}-${game.street}-${game.currentBet}`; }

  // ---------- game loop ----------
  function isHumanTurn() {
    return !!game && !game.handOver && game.current >= 0 && game.players[game.current].isHuman;
  }

  function deal() {
    if (!game || !game.handOver || game.isGameOver()) return;
    clearTimers();
    hideBanner();
    setReadyEnabled(false);
    // Clear last hand's cards immediately.
    document.querySelectorAll('.slot').forEach((s) => { s.dataset.key = ''; s.innerHTML = ''; });
    game.startHand();
    step();
  }

  function step() {
    render();
    if (!game) return;
    if (game.handOver) return onHandOver();
    const p = game.players[game.current];
    if (p.isHuman) {
      const amt = $('#amount');
      if (document.activeElement !== amt) $('#btnFold').blur();
      return;
    }
    const delay = SPEED_MS[settings.speed] * (0.7 + Math.random() * 0.6);
    botTimer = setTimeout(() => {
      botTimer = null;
      let decision;
      try {
        decision = P.AI.decide(game.viewFor(p.seat), p.difficulty, p.persona);
      } catch (err) {
        console.error(err);
        decision = { type: game.legalActions().canCheck ? 'check' : 'fold' };
      }
      if (window.JP_DEBUG) console.log(p.name, decision);
      game.act(decision);
      step();
    }, delay);
  }

  function humanAct(type) {
    if (!isHumanTurn()) return;
    const action = { type };
    if (type === 'raise') {
      const L = game.legalActions();
      let v = Math.round(Number($('#amount').value));
      if (!Number.isFinite(v) || v <= 0) v = L.minRaiseTo;
      action.amount = Math.max(L.minRaiseTo, Math.min(L.maxRaiseTo, v));
    }
    game.act(action);
    step();
  }

  function onHandOver() {
    const r = game.lastResult;
    const name = (s) => game.players[s].isHuman ? 'You' : esc(game.players[s].name);
    let msg;
    if (r.type === 'uncontested') {
      msg = `<strong>${name(r.winner)}</strong> win${game.players[r.winner].isHuman ? '' : 's'} ${fmt(r.amount)}. Everyone else folded.`;
    } else {
      msg = r.pots.filter((x) => !x.returned).map((pot, i, arr) => {
        const who = pot.winners.map(name).join(' & ');
        const plural = pot.winners.length > 1 || (pot.winners.length === 1 && game.players[pot.winners[0]].isHuman) ? '' : 's';
        const label = arr.length > 1 ? (i === 0 ? ' the main pot' : ' a side pot') : '';
        return `<strong>${who}</strong> win${plural}${label ? label + ' of' : ''} ${fmt(pot.amount)} with ${pot.handName}`;
      }).join('<br>');
    }
    showBanner(msg);
    render();

    if (game.isGameOver()) {
      setReadyEnabled(false);
      setTimeout(showGameOver, 1800);
      return;
    }
    setReadyEnabled(true);
    if (settings.autoDeal) autoTimer = setTimeout(deal, 3200);
  }

  function showGameOver() {
    const me = game.human();
    const won = me.chips > 0;
    const rec = load('jpholdem.record', {});
    const d = settings.difficulty;
    rec[d] = rec[d] || { w: 0, l: 0 };
    rec[d][won ? 'w' : 'l']++;
    save('jpholdem.record', rec);

    const standings = game.players.slice().sort((a, b) => b.chips - a.chips);
    $('#goTitle').textContent = won ? 'You took every chip.' : 'Busted.';
    $('#goText').textContent = won
      ? `You won in ${game.handNo} hands against ${settings.difficulty} opponents.`
      : `You went out on hand #${game.handNo}. Biggest pot you won: ${fmt(me.stats.biggestPot)}.`;
    $('#goStandings').innerHTML = standings
      .map((p) => `<li>${esc(p.isHuman ? p.name + ' (you)' : p.name)}${p.difficulty ? ` <span class="badge ${p.difficulty}">${p.difficulty}</span>` : ''}: ${fmt(p.chips)}</li>`)
      .join('');
    $('#gameOver').hidden = false;
  }

  // ---------- log ----------
  function onEvent(type, d, g) {
    const nm = (s) => g.players[s].name;
    switch (type) {
      case 'handStart': log(`Hand #${d.handNo}: blinds ${d.smallBlind}/${d.bigBlind}, dealer ${nm(d.dealer)}`, 'hand'); break;
      case 'blinds': log(`Blinds up to ${d.smallBlind}/${d.bigBlind}`, 'street'); break;
      case 'blind': log(`${nm(d.seat)} posts ${d.label} ${d.amount}`); break;
      case 'action': log(`${nm(d.seat)}: ${d.label}`); break;
      case 'street': log(`${d.street[0].toUpperCase() + d.street.slice(1)}: ${d.board.map(P.cardLabel).join(' ')}`, 'street'); break;
      case 'showdown':
        for (const h of d.hands) log(`${nm(h.seat)} shows ${h.hole.map(P.cardLabel).join(' ')} (${h.name})`);
        for (const pot of d.pots) {
          if (pot.returned) log(`${nm(pot.winners[0])} gets ${pot.amount} back (uncalled)`);
          else log(`${pot.winners.map(nm).join(' & ')} win${pot.winners.length > 1 ? '' : 's'} ${pot.amount}`, 'win');
        }
        break;
      case 'uncontested': log(`${nm(d.winner)} wins ${d.amount}`, 'win'); break;
      case 'busted': log(`${nm(d.seat)} is out of chips`, 'street'); break;
      default: break;
    }
  }

  function log(text, cls) {
    const ol = $('#log');
    const li = document.createElement('li');
    li.textContent = text;
    if (cls) li.className = cls;
    ol.appendChild(li);
    while (ol.children.length > 400) ol.removeChild(ol.firstChild);
    ol.scrollTop = ol.scrollHeight;
  }

  // ---------- misc ----------
  function showBanner(html) { const b = $('#banner'); b.innerHTML = html; b.hidden = false; }
  function hideBanner() { $('#banner').hidden = true; }
  function setReadyEnabled(on) { $('#btnReady').disabled = !on; }
  function clearTimers() { clearTimeout(botTimer); clearTimeout(autoTimer); botTimer = autoTimer = null; }
  function fmt(n) { return Number(n).toLocaleString('en-US'); }
  function esc(s) { return String(s).replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c])); }

  function applyPreset(kind) {
    if (!isHumanTurn()) return;
    const L = game.legalActions();
    if (!L.canRaise) return;
    let to;
    if (kind === 'allin') to = L.maxRaiseTo;
    else if (kind === 'min') to = L.minRaiseTo;
    else {
      const potAfterCall = game.pot() + L.toCall;
      to = game.currentBet + Math.round(potAfterCall * Number(kind));
      const unit = Math.max(1, game.smallBlind);
      to = Math.round(to / unit) * unit;
    }
    $('#amount').value = Math.max(L.minRaiseTo, Math.min(L.maxRaiseTo, to));
  }

  // ---------- wiring ----------
  $('#btnReady').addEventListener('click', deal);
  $('#btnFold').addEventListener('click', () => humanAct('fold'));
  $('#btnCheck').addEventListener('click', () => humanAct('check'));
  $('#btnCall').addEventListener('click', () => humanAct('call'));
  $('#btnBet').addEventListener('click', () => humanAct('raise'));
  document.querySelectorAll('[data-preset]').forEach((b) => b.addEventListener('click', () => applyPreset(b.dataset.preset)));
  $('#btnHistory').addEventListener('click', () => { $('#history').hidden = !$('#history').hidden; });
  $('#btnCloseHistory').addEventListener('click', () => { $('#history').hidden = true; });
  $('#btnNewGame').addEventListener('click', () => {
    if (game && !game.isGameOver() && game.handNo > 0 && !confirm('Abandon this game and start a new one?')) return;
    openSetup();
  });
  $('#btnAgain').addEventListener('click', () => { $('#gameOver').hidden = true; startGame(settings); });
  $('#btnSettings').addEventListener('click', openSetup);

  $('#amount').addEventListener('keydown', (e) => {
    if (e.key === 'Enter') { e.preventDefault(); humanAct('raise'); }
  });
  document.addEventListener('keydown', (e) => {
    if (e.target.matches('input, select, textarea') || e.metaKey || e.ctrlKey || e.altKey) return;
    if (!$('#setup').hidden || !$('#gameOver').hidden) return;
    const k = e.key.toLowerCase();
    if ((k === ' ' || k === 'enter') && game && game.handOver) { e.preventDefault(); deal(); return; }
    if (k === 'h') { $('#history').hidden = !$('#history').hidden; return; }
    if (!isHumanTurn()) return;
    const L = game.legalActions();
    if (k === 'f') humanAct('fold');
    else if (k === 'k' && L.canCheck) humanAct('check');
    else if (k === 'c') humanAct(L.canCheck ? 'check' : 'call');
    else if ((k === 'b' || k === 'r') && L.canRaise) humanAct('raise');
  });

  window.addEventListener('beforeunload', clearTimers);

  // Disable everything until a game exists, then show setup.
  $('#btnReady').disabled = true;
  ['#btnFold', '#btnCheck', '#btnCall', '#btnBet', '#amount'].forEach((s) => { $(s).disabled = true; });
  document.querySelectorAll('[data-preset]').forEach((b) => { b.disabled = true; });
  openSetup();

  // Handy for debugging in the browser console.
  window.JP = { get game() { return game; } };
})();

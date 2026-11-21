(function () {
  var cs = getComputedStyle(document.documentElement);
  function v(n) { return cs.getPropertyValue(n).trim(); }
  var K = { water: v('--water'), water2: v('--water2'), event: v('--event'), price: v('--price'), dir: v('--director'), drought: v('--drought'), normal: v('--normal'), flood: v('--flood'), hot: v('--hot'), cycle: v('--cycle'), ink: v('--ink'), muted: v('--muted'), faint: v('--faint'), line: v('--line'), line2: v('--line2'), panel: v('--panel'), bg: v('--bg') };
  var fmt = function (x, d) { return Number(x).toLocaleString('en-US', { maximumFractionDigits: d || 0, minimumFractionDigits: d || 0 }); };

  // ================= the model =================
  var P = { Capacity: 1000, LowMark: 260, HighMark: 820, RainBase: 34, RainSeason: .75, RainMult: 1, TurbineMax: 70, Efficiency: .9, Rate: .07, OpCost: 350, SweepShare: .6, DebtInitial: 150000, LevelInitial: 600, PriceInitial: 52 };
  function noise(t, k) { var x = Math.sin(t * 12.9898 + (k || 0) * 78.233) * 43758.5453; return x - Math.floor(x); }
  var S;
  function reset() {
    S = { Tick: 0, Rain: 0, Inflow: 0, Level: P.LevelInitial, Spill: 0, Release: 0, Mode: 'Normal', Power: 0, Price: P.PriceInitial, Revenue: 0, Interest: 0, Repay: 0, Debt: P.DebtInitial, Cash: 5000, Iter: 0, IterDeltas: [], Head: 0 };
    H = []; events = []; fan = null;
  }
  function rain(t, mult) { return Math.max(0, P.RainBase * (1 + P.RainSeason * Math.sin(2 * Math.PI * (t - 8) / 52)) * (0.55 + 0.9 * noise(t, 1)) * mult); }
  function modeNext(mode, level) {
    if (mode === 'Drought') return level > P.LowMark + 70 ? 'Normal' : 'Drought';
    if (mode === 'Flood') return level < P.HighMark - 70 ? 'Normal' : 'Flood';
    return level < P.LowMark ? 'Drought' : level > P.HighMark ? 'Flood' : 'Normal';
  }
  function releaseRule(mode, level, inflow) {
    if (mode === 'Drought') return Math.min(8, level);
    if (mode === 'Flood') return Math.min(P.TurbineMax, level + inflow);
    return Math.min(P.TurbineMax * .62, Math.max(0, level + inflow - P.LowMark * .5));
  }
  // Finance is a circular block: Interest needs this tick's Debt, Debt needs Repay, Repay needs Interest.
  function settleFinance(prevDebt) {
    var debt = prevDebt, deltas = [], interest = 0, repay = 0;
    for (var i = 1; i <= 100; i++) {
      interest = P.Rate / 52 * (prevDebt + debt) / 2;
      repay = Math.min(debt, Math.max(0, (S.Revenue - P.OpCost - interest) * P.SweepShare));
      var next = Math.max(0, prevDebt - repay);
      var d = Math.abs(next - debt); deltas.push(d); debt = next;
      if (d < .001) break;
    }
    return { debt: debt, interest: interest, repay: repay, iter: deltas.length, deltas: deltas };
  }
  // A clock event: the whole tick, in calc-chain order. Returns the actors that recalculated.
  function tickEvent() {
    var prev = { Level: S.Level, Mode: S.Mode, Debt: S.Debt, Cash: S.Cash };
    S.Tick += 1;
    S.Rain = rain(S.Tick, P.RainMult); S.Inflow = S.Rain;
    S.Mode = modeNext(prev.Mode, prev.Level);
    S.Release = releaseRule(S.Mode, prev.Level, S.Inflow);
    var raw = prev.Level + S.Inflow - S.Release;
    S.Spill = Math.max(0, raw - P.Capacity);
    S.Level = Math.min(P.Capacity, Math.max(0, raw));
    S.Head = S.Level / P.Capacity;
    financeAfterPower(prev);
    H.push({ t: S.Tick, Level: S.Level, Inflow: S.Inflow, Release: S.Release, Spill: S.Spill, Price: S.Price, Debt: S.Debt, Mode: S.Mode });
    if (H.length > 160) H.shift();
    fan = sweep();
    return ['Clock', 'Tick', 'Weather', 'Rain', 'Inflow', 'Operator', 'Mode', 'Release', 'Reservoir', 'Level', 'Spill', 'Head', 'Turbine', 'Power', 'Finance', 'Revenue', 'Interest', 'Repay', 'Debt', 'Cash', 'Outlook'];
  }
  function financeAfterPower(prev) {
    S.Power = S.Release * P.Efficiency * (.5 + .5 * S.Head);
    S.Revenue = S.Power * S.Price;
    var f = settleFinance(prev.Debt);
    S.Debt = f.debt; S.Interest = f.interest; S.Repay = f.repay; S.Iter = f.iter; S.IterDeltas = f.deltas;
    S.Cash = prev.Cash + S.Revenue - P.OpCost - S.Interest - S.Repay;
  }
  // A market event: only the price and what depends on it recalculate.
  var priceSeed = 3;
  function priceEvent() {
    priceSeed = (priceSeed * 16807) % 2147483647;
    var shock = (priceSeed / 2147483647 - .5) * .16;
    S.Price = Math.max(18, Math.min(140, S.Price * (1 + shock) + (P.PriceInitial - S.Price) * .03));
    var prevDebt = S.Debt + S.Repay, prevCash = S.Cash - S.Revenue + P.OpCost + S.Interest + S.Repay;
    financeAfterPower({ Debt: prevDebt, Cash: prevCash });
    return ['Market', 'Price', 'Turbine', 'Finance', 'Revenue', 'Interest', 'Repay', 'Debt', 'Cash'];
  }
  // The Sweep director: a data table over RainMult, each row a forward run of the reservoir and operator.
  function sweep() {
    var N = 25, T = 52, paths = [];
    for (var k = 0; k < N; k++) {
      var mult = .5 + k / (N - 1), lvl = S.Level, mode = S.Mode, path = [];
      for (var t = 1; t <= T; t++) {
        var inflow = rain(S.Tick + t, mult * P.RainMult);
        mode = modeNext(mode, lvl);
        var rel = releaseRule(mode, lvl, inflow);
        lvl = Math.min(P.Capacity, Math.max(0, lvl + inflow - rel));
        path.push(lvl);
      }
      paths.push(path);
    }
    var bands = [];
    for (var t2 = 0; t2 < T; t2++) {
      var col = paths.map(function (p) { return p[t2]; }).sort(function (a, b) { return a - b; });
      bands.push({ min: col[0], q1: col[6], med: col[12], q3: col[18], max: col[24] });
    }
    return bands;
  }
  var H = [], events = [], fan = null;

  // ================= diagram levels =================
  // Each actor: id, label, kind, x, y, w, h, ins, excel formula, note. Composite kinds open an inner level.
  var LEVELS = {
    top: { title: 'Dam', director: 'Events director · RTD', actors: [
      ['Clock', 'Clock', 'rtd', 30, 40, 200, 104, [], '=RTD("DnaCalc.Clock",, "Dam", "tick")', 'The RTD clock. Each update is an event; what depends on the tick recalculates.'],
      ['Market', 'Market', 'rtd', 30, 420, 200, 120, [], '=RTD("DnaCalc.Market",, "power.price", 7)', 'A seeded random-walk price feed at its own rate. Its events recalculate only Turbine and Finance.'],
      ['Weather', 'Weather', 'comp', 300, 30, 210, 120, ['Clock'], 'sheet Weather · Ticks director', 'Seasonal rain with weather noise, scaled by Weather.RainMult.'],
      ['Reservoir', 'Reservoir', 'comp', 590, 30, 230, 220, ['Weather', 'Operator'], 'sheet Reservoir · Ticks director', 'The water balance: level carried from tick to tick, spill when full.'],
      ['Operator', 'Operator', 'comp', 300, 250, 230, 150, ['Reservoir'], 'sheet Operator · modal model', 'Drought, Normal and Flood, each with its own release rule.'],
      ['Turbine', 'Turbine', 'comp', 590, 320, 230, 110, ['Reservoir', 'Market'], 'sheet Turbine · recalc', 'Power from released water; more head, more power.'],
      ['Finance', 'Finance', 'comp', 890, 300, 260, 170, ['Turbine', 'Market', 'Clock'], 'sheet Finance · Iteration director', 'Revenue, interest on average debt, repayment: a circular block settled by iteration.'],
      ['Outlook', 'Outlook', 'comp', 890, 30, 260, 220, ['Reservoir'], 'sheet Outlook · Sweep director', 'A data table running 25 rainfall futures for the next 52 ticks.'],
      ['Plotter', 'Plotter', 'plot', 300, 470, 520, 100, ['Reservoir', 'Market', 'Finance'], 'charts', 'History and outlook charts below.']
    ] },
    Weather: { title: 'Weather', director: 'Ticks director', parent: 'top', actors: [
      ['Tick', 'Clock.Tick', 'rtd', 40, 60, 190, 80, [], '=RTD("DnaCalc.Clock",, "Dam", "tick")', 'The tick, from the clock.'],
      ['RainMult', 'Weather.RainMult', 'input', 40, 240, 190, 70, [], '1.00', 'Parameter: wetter or drier than usual.'],
      ['Rain', 'Weather.Rain', 'calc', 320, 140, 260, 90, ['Tick', 'RainMult'], '=MAX(0, Weather.Base * (1 + Weather.Season * SIN(2 * PI() * (Clock.Tick - 8) / 52)) * Weather.Noise * Weather.RainMult)', 'Seasonal rain with noise.'],
      ['Inflow', 'Weather.Inflow', 'calc', 680, 140, 220, 90, ['Rain'], '=Weather.Rain * Weather.Catchment', 'Water reaching the reservoir.']
    ] },
    Reservoir: { title: 'Reservoir', director: 'Ticks director', parent: 'top', actors: [
      ['Inflow', 'Weather.Inflow', 'calc', 30, 60, 200, 80, [], '=Weather.Rain * Weather.Catchment', 'From Weather.'],
      ['Release', 'Operator.Release', 'calc', 30, 300, 200, 80, [], '=CHOOSE(Operator.ModeIndex, Operator.DroughtRule, Operator.NormalRule, Operator.FloodRule)', 'From the operator\'s current mode.'],
      ['Level', 'Reservoir.Level', 'state', 380, 160, 250, 110, ['Inflow', 'Release', 'Level'], '=IF(Clock.Reset, Reservoir.Initial, MIN(Reservoir.Capacity, MAX(0, Reservoir.Level + Weather.Inflow - Operator.Release)))', 'State: last tick\'s level plus inflow minus release, capped at capacity.'],
      ['Spill', 'Reservoir.Spill', 'calc', 760, 60, 200, 80, ['Level', 'Inflow', 'Release'], '=MAX(0, Reservoir.Level + Weather.Inflow - Operator.Release - Reservoir.Capacity)', 'What goes over the top.'],
      ['Head', 'Reservoir.Head', 'calc', 760, 300, 200, 80, ['Level'], '=Reservoir.Level / Reservoir.Capacity', 'Fullness, which sets the turbine head.']
    ] },
    Operator: { title: 'Operator', director: 'Modal model', parent: 'top', fsm: true, actors: [] },
    Turbine: { title: 'Turbine', director: 'Recalc', parent: 'top', actors: [
      ['Release', 'Operator.Release', 'calc', 40, 60, 200, 80, [], '=CHOOSE(Operator.ModeIndex, ...)', 'Water through the turbine.'],
      ['Head', 'Reservoir.Head', 'calc', 40, 280, 200, 80, [], '=Reservoir.Level / Reservoir.Capacity', 'Fullness.'],
      ['Power', 'Turbine.Power', 'calc', 420, 160, 260, 100, ['Release', 'Head'], '=Operator.Release * Turbine.Efficiency * (0.5 + 0.5 * Reservoir.Head)', 'Energy this tick.'],
      ['Price', 'Market.Price', 'rtd', 420, 380, 260, 90, [], '=RTD("DnaCalc.Market",, "power.price", 7)', 'From the market feed.'],
      ['Revenue', 'Finance.Revenue', 'calc', 820, 260, 240, 90, ['Power', 'Price'], '=Turbine.Power * Market.Price', 'Sold at the current price.']
    ] },
    Finance: { title: 'Finance', director: 'Iteration director · max 100 · max change 0.001', parent: 'top', actors: [
      ['Revenue', 'Finance.Revenue', 'calc', 30, 70, 210, 80, [], '=Turbine.Power * Market.Price', 'Revenue this tick.'],
      ['Interest', 'Finance.Interest', 'cycle', 330, 70, 250, 90, [], '=Finance.Rate / 52 * AVERAGE(Finance.DebtStart, Finance.Debt)', 'Interest on the average of opening and closing debt: needs this tick\'s Debt.'],
      ['Repay', 'Finance.Repay', 'cycle', 660, 70, 250, 90, ['Revenue', 'Interest'], '=MIN(Finance.Debt, MAX(0, (Finance.Revenue - Finance.OpCost - Finance.Interest) * Finance.SweepShare))', 'Free cash after interest, part of it repays debt.'],
      ['Debt', 'Finance.Debt', 'cycle', 500, 280, 250, 100, ['Repay'], '=MAX(0, Finance.DebtStart - Finance.Repay)', 'Closing debt. Interest reads it: the circle closes here.'],
      ['Cash', 'Finance.Cash', 'state', 870, 300, 220, 90, ['Revenue', 'Interest', 'Repay'], '=Finance.Cash + Finance.Revenue - Finance.OpCost - Finance.Interest - Finance.Repay', 'State: cash carried between ticks.']
    ] },
    Outlook: { title: 'Outlook', director: 'Sweep director · data table', parent: 'top', table: true, actors: [] }
  };
  var level = 'top', selected = 'Reservoir', view = 'actors';
  var NS = 'http://www.w3.org/2000/svg', dg = document.getElementById('dg');
  function el(tag, attrs, parent) { var e = document.createElementNS(NS, tag); for (var k in attrs) if (attrs[k] !== null && attrs[k] !== undefined) e.setAttribute(k, attrs[k]); (parent || dg).appendChild(e); return e; }
  function txt(parent, x, y, cls, s, anchor) { var t = el('text', { x: x, y: y, class: cls, 'text-anchor': anchor || null }, parent); t.textContent = s; return t; }
  var boxes = {}, wireEls = [];
  function A(row) { return { id: row[0], label: row[1], kind: row[2], x: row[3], y: row[4], w: row[5], h: row[6], ins: row[7], xf: row[8], note: row[9] }; }

  function build() {
    while (dg.firstChild) dg.removeChild(dg.firstChild);
    boxes = {}; wireEls = [];
    var L = LEVELS[level];
    var defs = el('defs', {});
    var grad = el('linearGradient', { id: 'wgrad', x1: 0, y1: 0, x2: 0, y2: 1 }, defs); el('stop', { offset: 0, 'stop-color': K.water }, grad); el('stop', { offset: 1, 'stop-color': K.water2 }, grad);
    el('rect', { x: 0, y: 0, width: 1180, height: 640, fill: 'transparent' });
    if (L.fsm) return buildFSM();
    if (L.table) return buildTable();
    var acts = L.actors.map(A), byId = {}; acts.forEach(function (a) { byId[a.id] = a; });
    var gw = el('g', {}), gb = el('g', {});
    acts.forEach(function (a) {
      a.ins.forEach(function (src, i) {
        var s = byId[src]; if (!s) return;
        var x0 = s.x + s.w, y0 = s.y + s.h / 2, x1 = a.x, y1 = a.y + a.h * (i + 1) / (a.ins.length + 1), d, cls = 'w';
        if (src === a.id) {
          var ty = a.y - 26; d = 'M' + x0 + ',' + y0 + ' C' + (x0 + 40) + ',' + y0 + ' ' + (x0 + 40) + ',' + ty + ' ' + (a.x + a.w / 2) + ',' + ty + ' C' + (a.x - 40) + ',' + ty + ' ' + (a.x - 40) + ',' + y1 + ' ' + x1 + ',' + y1;
          cls = 'w loop';
          var p = el('path', { d: d, class: cls }, gw); wireEls.push({ from: src, to: a.id, el: p });
          var z = el('g', { class: 'zd' }, gw); el('rect', { x: a.x + a.w / 2 - 17, y: ty - 10, width: 34, height: 20, rx: 3 }, z); txt(z, a.x + a.w / 2, ty + 4, '', 'z⁻¹', 'middle');
          return;
        }
        if (x1 < x0 + 10) { x0 = s.x + s.w / 2; y0 = s.y + (s.y < a.y ? s.h : 0); x1 = a.x + a.w / 2; y1 = a.y + (s.y < a.y ? 0 : a.h); d = 'M' + x0 + ',' + y0 + ' C' + x0 + ',' + (y0 + y1) / 2 + ' ' + x1 + ',' + (y0 + y1) / 2 + ' ' + x1 + ',' + y1; }
        else { var mx = (x0 + x1) / 2; d = 'M' + x0 + ',' + y0 + ' C' + mx + ',' + y0 + ' ' + mx + ',' + y1 + ' ' + x1 + ',' + y1; }
        if (level === 'Finance' && (a.kind === 'cycle') && (s.kind === 'cycle')) cls = 'w cyc';
        var pth = el('path', { d: d, class: cls }, gw); wireEls.push({ from: src, to: a.id, el: pth });
      });
    });
    if (level === 'Finance') { // the circle closes: Debt feeds Interest
      var D = byId.Debt, I = byId.Interest, d2 = 'M' + D.x + ',' + (D.y + D.h / 2) + ' C' + (I.x + 40) + ',' + (D.y + D.h / 2) + ' ' + (I.x + 40) + ',' + (I.y + I.h + 60) + ' ' + (I.x + I.w / 2) + ',' + (I.y + I.h);
      wireEls.push({ from: 'Debt', to: 'Interest', el: el('path', { d: d2, class: 'w cyc' }, gw) });
      var lab = txt(gw, 330, 250, 's', 'circular: Interest → Repay → Debt → Interest'); lab.setAttribute('fill', K.cycle); lab.style.font = '600 12px "IBM Plex Mono",monospace';
    }
    acts.forEach(function (a) { boxes[a.id] = drawBox(a, gb); });
  }
  function drawBox(a, parent) {
    var g = el('g', { class: 'box ' + a.kind, tabindex: 0, role: 'button', 'aria-label': a.label }, parent);
    el('rect', { class: 'b', x: a.x, y: a.y, width: a.w, height: a.h, rx: a.kind === 'comp' ? 10 : 6 }, g);
    if (a.kind === 'comp') el('rect', { class: 'b2', x: a.x + 4, y: a.y + 4, width: a.w - 8, height: a.h - 8, rx: 7 }, g);
    txt(g, a.x + 12, a.y + 20, 't', a.label);
    var b = { g: g, a: a, parts: {} };
    if (a.kind === 'comp') { var dl = (LEVELS[a.id] || {}).director || ''; txt(g, a.x + a.w - 12, a.y + 20, 'd', dl.split(' · ')[0] + '  ⤵', 'end'); }
    b.parts.v = txt(g, a.x + 12, a.y + 44, 'v', '');
    b.parts.s = txt(g, a.x + 12, a.y + a.h - 10, 's', '');
    var viz = el('g', {}, g); b.parts.viz = viz;
    g.addEventListener('click', function (e) { e.stopPropagation(); if (a.kind === 'comp' && LEVELS[a.id]) { enter(a.id); } else { selected = a.id; inspect(); } });
    g.addEventListener('keydown', function (e) { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); g.dispatchEvent(new MouseEvent('click')); } });
    g.addEventListener('mouseenter', function () { selected = a.id; inspect(); });
    return b;
  }

  // ----- Operator: a modal model, states with refinements -----
  var fsmEls = {};
  function buildFSM() {
    fsmEls = {};
    var st = [['Drought', 200, 330, K.drought, 'release = MIN(8, Level)', 'Hold water back: only a trickle for the river.'], ['Normal', 590, 170, K.normal, 'release = MIN(0.62 × TurbineMax, Level + Inflow − LowMark / 2)', 'Run the turbine at a steady share.'], ['Flood', 980, 330, K.flood, 'release = MIN(TurbineMax, Level + Inflow)', 'Run flat out to make room.']];
    var arrows = [['Normal', 'Drought', 'Level < LowMark', -1], ['Drought', 'Normal', 'Level > LowMark + 70', 1], ['Normal', 'Flood', 'Level > HighMark', 1], ['Flood', 'Normal', 'Level < HighMark − 70', -1]];
    var pos = {}; st.forEach(function (s) { pos[s[0]] = s; });
    arrows.forEach(function (ar) {
      var a = pos[ar[0]], b = pos[ar[1]], mx = (a[1] + b[1]) / 2, my = (a[2] + b[2]) / 2 + ar[3] * 70;
      el('path', { d: 'M' + a[1] + ',' + a[2] + ' Q' + mx + ',' + my + ' ' + b[1] + ',' + b[2], class: 'w', 'marker-end': null, id: 'tr-' + ar[0] + ar[1] });
      var t = txt(dg, mx, (a[2] + b[2]) / 2 + ar[3] * 44, 's', ar[2], 'middle'); t.style.font = '600 12px "IBM Plex Mono",monospace'; t.style.fill = K.muted;
    });
    st.forEach(function (s) {
      var g = el('g', {});
      var c = el('circle', { cx: s[1], cy: s[2], r: 62, fill: K.panel, stroke: s[3], 'stroke-width': 2.4 }, g);
      var nm = txt(g, s[1], s[2] - 4, 'v', s[0], 'middle'); nm.style.font = '800 20px "Bricolage Grotesque",system-ui'; nm.style.fill = K.ink;
      var sub = txt(g, s[1], s[2] + 20, 's', '', 'middle'); sub.style.font = '400 11px "IBM Plex Mono",monospace'; sub.style.fill = K.muted;
      var box = el('rect', { x: s[1] - 150, y: s[2] + 80, width: 300, height: 62, rx: 6, fill: K.bg, stroke: s[3], 'stroke-dasharray': '4 3' }, g);
      var r1 = txt(g, s[1] - 140, s[2] + 100, 's', 'refinement'); r1.style.font = '600 10px "IBM Plex Mono",monospace'; r1.style.fill = s[3];
      var r2 = txt(g, s[1] - 140, s[2] + 120, 's', s[4]); r2.style.font = '400 11.5px "IBM Plex Mono",monospace'; r2.style.fill = K.ink;
      var r3 = txt(g, s[1] - 140, s[2] + 135, 's', s[5]); r3.style.font = '400 11px "IBM Plex Sans",sans-serif'; r3.style.fill = K.muted;
      fsmEls[s[0]] = { c: c, sub: sub, color: s[3] };
    });
    var f = txt(dg, 40, 40, 's', 'Operator.Mode =IFS(Operator.Mode="Drought", IF(Reservoir.Level > LowMark+70, "Normal", "Drought"), Operator.Mode="Flood", IF(Reservoir.Level < HighMark-70, "Normal", "Flood"), …)');
    f.style.font = '400 12px "IBM Plex Mono",monospace'; f.style.fill = K.price;
    var f2 = txt(dg, 40, 60, 's', 'A self-referencing mode cell: the guards read its previous state, so leaving a mode needs a margin (hysteresis).');
    f2.style.font = '400 12.5px "IBM Plex Sans",sans-serif'; f2.style.fill = K.muted;
  }
  // ----- Outlook: the data table behind the sweep -----
  function buildTable() {
    var t = txt(dg, 40, 40, 's', '{=TABLE(, Weather.RainMult)}  over 25 multipliers × 52 ticks: one forward run of Weather, Operator and Reservoir per row');
    t.style.font = '400 12.5px "IBM Plex Mono",monospace'; t.style.fill = K.price;
    fsmEls.table = el('g', {});
  }

  // ================= painting =================
  function modeColor(m) { return m === 'Drought' ? K.drought : m === 'Flood' ? K.flood : K.normal; }
  function paint() {
    var L = LEVELS[level];
    if (L.fsm) {
      Object.keys(fsmEls).forEach(function (k) { var e = fsmEls[k]; var on = S.Mode === k; e.c.setAttribute('fill', on ? e.color : K.panel); e.c.setAttribute('fill-opacity', on ? .35 : 1); e.c.setAttribute('stroke-width', on ? 4 : 2); e.sub.textContent = on ? 'current · tick ' + S.Tick : ''; });
    } else if (L.table) paintTable();
    else Object.keys(boxes).forEach(function (id) { paintBox(boxes[id]); });
    paintSheet(); paintQueue(); paintHist(); paintFan();
    document.getElementById('run').textContent = running ? '❚❚ Pause' : '▶ Run';
  }
  function setV(b, s, sub) { b.parts.v.textContent = s; if (sub !== undefined) b.parts.s.textContent = sub; }
  function clearViz(b) { var z = b.parts.viz; while (z.firstChild) z.removeChild(z.firstChild); return z; }
  function paintBox(b) {
    var a = b.a, id = a.id, z = clearViz(b);
    switch (level === 'top' ? id : 'atom:' + id) {
      case 'Clock': setV(b, 'tick ' + S.Tick, (running ? 'running' : 'paused') + ' · ' + rate + '/s · reset: ' + document.getElementById('resetWhen').selectedOptions[0].text);
        for (var i = 0; i < 16; i++) el('rect', { x: a.x + 12 + i * 11, y: a.y + 56, width: 7, height: 14, rx: 1.5, fill: i === S.Tick % 16 ? K.event : K.line2 }, z); break;
      case 'Market': setV(b, '$' + fmt(S.Price, 2) + '/MWh', 'feed · ' + mktRate + ' updates/s');
        var ps = H.slice(-40).map(function (h) { return h.Price; }); if (ps.length > 1) { var mn = Math.min.apply(null, ps), mx = Math.max.apply(null, ps) || 1; el('path', { d: ps.map(function (p, i) { return (i ? 'L' : 'M') + (a.x + 12 + i * 4.4) + ',' + (a.y + 92 - (p - mn) / ((mx - mn) || 1) * 34); }).join(' '), fill: 'none', stroke: K.price, 'stroke-width': 1.6 }, z); } break;
      case 'Weather': setV(b, fmt(S.Rain, 1) + ' rain', 'RainMult ' + P.RainMult.toFixed(2));
        H.slice(-30).forEach(function (h, i) { var hh = Math.min(30, h.Inflow / 2); el('rect', { x: a.x + 12 + i * 6.2, y: a.y + 92 - hh, width: 4, height: hh, fill: K.water, opacity: .7 }, z); }); break;
      case 'Reservoir':
        var top = a.y + 52, bot = a.y + a.h - 28, wl = bot - (bot - top) * S.Level / P.Capacity, t = performance.now() / 600;
        el('rect', { x: a.x + 14, y: top, width: a.w - 28, height: bot - top, fill: 'none', stroke: K.line2, rx: 4 }, z);
        var wave = 'M' + (a.x + 15) + ',' + bot; for (var xx = 0; xx <= a.w - 30; xx += 6) wave += ' L' + (a.x + 15 + xx) + ',' + (wl + Math.sin(t + xx / 14) * 2.2); wave += ' L' + (a.x + a.w - 15) + ',' + bot + ' Z';
        el('path', { d: wave, fill: 'url(#wgrad)', opacity: .85 }, z);
        el('line', { x1: a.x + 14, x2: a.x + a.w - 14, y1: bot - (bot - top) * P.HighMark / P.Capacity, y2: bot - (bot - top) * P.HighMark / P.Capacity, stroke: K.flood, 'stroke-dasharray': '4 3' }, z);
        el('line', { x1: a.x + 14, x2: a.x + a.w - 14, y1: bot - (bot - top) * P.LowMark / P.Capacity, y2: bot - (bot - top) * P.LowMark / P.Capacity, stroke: K.drought, 'stroke-dasharray': '4 3' }, z);
        setV(b, fmt(S.Level) + ' / ' + fmt(P.Capacity), S.Spill > 0 ? 'spilling ' + fmt(S.Spill, 1) : 'inflow ' + fmt(S.Inflow, 1) + ' · release ' + fmt(S.Release, 1));
        b.parts.v.setAttribute('y', a.y + 44); break;
      case 'Operator': setV(b, S.Mode, 'release ' + fmt(S.Release, 1));
        ['Drought', 'Normal', 'Flood'].forEach(function (m, i) { el('circle', { cx: a.x + 40 + i * 75, cy: a.y + 92, r: 14, fill: S.Mode === m ? modeColor(m) : 'none', stroke: modeColor(m), 'stroke-width': 2 }, z); txt(z, a.x + 40 + i * 75, a.y + 120, 's', m, 'middle'); });
        b.parts.s.textContent = ''; break;
      case 'Turbine': setV(b, fmt(S.Power, 1) + ' MWh', 'head ' + Math.round(S.Head * 100) + '%');
        var cx = a.x + a.w - 44, cy = a.y + 64, rot = (performance.now() / 1000 * S.Power * 9) % 360;
        var bl = el('g', { transform: 'rotate(' + rot + ' ' + cx + ' ' + cy + ')' }, z);
        for (var k = 0; k < 3; k++) el('path', { d: 'M' + cx + ',' + cy + ' q 10,-6 22,-2 q -10,6 -22,2', fill: K.water, transform: 'rotate(' + (k * 120) + ' ' + cx + ' ' + cy + ')' }, bl);
        el('circle', { cx: cx, cy: cy, r: 4, fill: K.ink }, z); break;
      case 'Finance': setV(b, '$' + fmt(S.Debt) + ' debt', 'circular block, settled · cash $' + fmt(S.Cash));
        H.slice(-14).forEach(function (h, i, arr) { var prevD = i ? arr[i - 1].Debt : h.Debt, rep = Math.max(0, prevD - h.Debt), hh = Math.max(2, Math.min(40, rep / 25)); el('rect', { x: a.x + 14 + i * 16, y: a.y + 110 - hh, width: 10, height: hh, fill: K.cycle, opacity: .8 }, z); });
        txt(z, a.x + 14, a.y + 128, 's', 'debt repaid per tick'); break;
      case 'Outlook':
        if (fan) { var w = a.w - 28, h = a.h - 74, x0 = a.x + 14, y0 = a.y + 40;
          function X(i) { return x0 + i / (fan.length - 1) * w; } function Y(vv) { return y0 + h - vv / P.Capacity * h; }
          el('path', { d: fan.map(function (f, i) { return (i ? 'L' : 'M') + X(i) + ',' + Y(f.max); }).join(' ') + ' ' + fan.slice().reverse().map(function (f, i) { return 'L' + X(fan.length - 1 - i) + ',' + Y(f.min); }).join(' ') + 'Z', fill: K.water, opacity: .15 }, z);
          el('path', { d: fan.map(function (f, i) { return (i ? 'L' : 'M') + X(i) + ',' + Y(f.q3); }).join(' ') + ' ' + fan.slice().reverse().map(function (f, i) { return 'L' + X(fan.length - 1 - i) + ',' + Y(f.q1); }).join(' ') + 'Z', fill: K.water, opacity: .35 }, z);
          el('path', { d: fan.map(function (f, i) { return (i ? 'L' : 'M') + X(i) + ',' + Y(f.med); }).join(' '), fill: 'none', stroke: K.water, 'stroke-width': 2 }, z); }
        setV(b, '', '52 ticks ahead · 25 futures'); break;
      case 'Plotter': setV(b, '', 'History and outlook charts, below'); break;
      default:
        var val = { Tick: S.Tick, RainMult: P.RainMult.toFixed(2), Rain: fmt(S.Rain, 1), Inflow: fmt(S.Inflow, 1), Release: fmt(S.Release, 1), Level: fmt(S.Level, 1), Spill: fmt(S.Spill, 1), Head: Math.round(S.Head * 100) + '%', Power: fmt(S.Power, 1), Price: '$' + fmt(S.Price, 2), Revenue: '$' + fmt(S.Revenue), Interest: '$' + fmt(S.Interest, 2), Repay: '$' + fmt(S.Repay, 2), Debt: '$' + fmt(S.Debt, 2), Cash: '$' + fmt(S.Cash) }[id];
        setV(b, val === undefined ? '' : String(val), a.xf.length > a.w / 6.7 ? a.xf.slice(0, Math.floor(a.w / 6.7) - 1) + '…' : a.xf);

    }
  }
  function paintTable() {
    var g = fsmEls.table; if (!g) return; while (g.firstChild) g.removeChild(g.firstChild);
    if (!fan) return;
    var rowsY = 70, rowH = 20;
    var hdr = txt(g, 40, rowsY, 's', 'RainMult'); hdr.style.font = '600 11px "IBM Plex Mono",monospace'; hdr.style.fill = K.muted;
    [1, 13, 26, 39, 52].forEach(function (t, i) { var h = txt(g, 150 + i * 70, rowsY, 's', 't+' + t, 'end'); h.style.font = '600 11px "IBM Plex Mono",monospace'; h.style.fill = K.muted; });
    var N = 25;
    for (var k = 0; k < N; k++) {
      var mult = .5 + k / (N - 1), lvl = S.Level, mode = S.Mode, path = [];
      for (var t = 1; t <= 52; t++) { var inflow = rain(S.Tick + t, mult * P.RainMult); mode = modeNext(mode, lvl); lvl = Math.min(P.Capacity, Math.max(0, lvl + inflow - releaseRule(mode, lvl, inflow))); path.push(lvl); }
      var y = rowsY + (k + 1) * rowH;
      var m = txt(g, 40, y, 's', mult.toFixed(2)); m.style.font = '400 11.5px "IBM Plex Mono",monospace'; m.style.fill = K.price;
      [0, 12, 25, 38, 51].forEach(function (t, i) { var c = txt(g, 150 + i * 70, y, 's', fmt(path[t]), 'end'); c.style.font = '400 11.5px "IBM Plex Mono",monospace'; c.style.fill = K.ink; });
      el('path', { d: path.map(function (p, i) { return (i ? 'L' : 'M') + (520 + i * 11.5) + ',' + (y - 6 - p / P.Capacity * 14); }).join(' '), fill: 'none', stroke: K.water, 'stroke-width': 1.2, opacity: .8 }, g);
    }
    var note = txt(g, 520, rowsY, 's', 'level path per row, next 52 ticks'); note.style.font = '600 11px "IBM Plex Mono",monospace'; note.style.fill = K.muted;
  }

  // ================= sheet view =================
  var SHEET = [
    ['Clock', [['Clock.Tick', '=RTD("DnaCalc.Clock",, "Dam", "tick")', function () { return S.Tick; }, 'Clock'], ['Clock.Rate', '4', function () { return rate; }, ''], ['Clock.ResetWhen', 'never', function () { return document.getElementById('resetWhen').selectedOptions[0].text; }, '']]],
    ['Market', [['Market.Price', '=RTD("DnaCalc.Market",, "power.price", 7)', function () { return fmt(S.Price, 2); }, 'Price']]],
    ['Weather', [['Weather.RainMult', '1', function () { return P.RainMult.toFixed(2); }, ''], ['Weather.Rain', '=MAX(0, Weather.Base*(1+Weather.Season*SIN(2*PI()*(Clock.Tick-8)/52))*Weather.Noise*Weather.RainMult)', function () { return fmt(S.Rain, 2); }, 'Rain'], ['Weather.Inflow', '=Weather.Rain*Weather.Catchment', function () { return fmt(S.Inflow, 2); }, 'Inflow']]],
    ['Operator', [['Operator.Mode', '=IFS(Operator.Mode="Drought", IF(Reservoir.Level>Operator.LowMark+70,"Normal","Drought"), …)', function () { return S.Mode; }, 'Mode'], ['Operator.Release', '=CHOOSE(Operator.ModeIndex, Operator.DroughtRule, Operator.NormalRule, Operator.FloodRule)', function () { return fmt(S.Release, 2); }, 'Release']]],
    ['Reservoir', [['Reservoir.Level', '=IF(Clock.Reset, Reservoir.Initial, MIN(Reservoir.Capacity, MAX(0, Reservoir.Level+Weather.Inflow-Operator.Release)))', function () { return fmt(S.Level, 2); }, 'Level'], ['Reservoir.Spill', '=MAX(0, Reservoir.Level+Weather.Inflow-Operator.Release-Reservoir.Capacity)', function () { return fmt(S.Spill, 2); }, 'Spill'], ['Reservoir.Head', '=Reservoir.Level/Reservoir.Capacity', function () { return (S.Head * 100).toFixed(1) + '%'; }, 'Head']]],
    ['Turbine', [['Turbine.Power', '=Operator.Release*Turbine.Efficiency*(0.5+0.5*Reservoir.Head)', function () { return fmt(S.Power, 2); }, 'Power']]],
    ['Finance', [['Finance.Revenue', '=Turbine.Power*Market.Price', function () { return fmt(S.Revenue, 2); }, 'Revenue'], ['Finance.Interest', '=Finance.Rate/52*AVERAGE(Finance.DebtStart, Finance.Debt)', function () { return fmt(S.Interest, 2); }, 'Interest'], ['Finance.Repay', '=MIN(Finance.Debt, MAX(0, (Finance.Revenue-Finance.OpCost-Finance.Interest)*Finance.SweepShare))', function () { return fmt(S.Repay, 2); }, 'Repay'], ['Finance.Debt', '=MAX(0, Finance.DebtStart-Finance.Repay)', function () { return fmt(S.Debt, 2); }, 'Debt'], ['Finance.Cash', '=Finance.Cash+Finance.Revenue-Finance.OpCost-Finance.Interest-Finance.Repay', function () { return fmt(S.Cash, 2); }, 'Cash']]],
    ['Outlook', [['Outlook.Median52', '=MEDIAN(INDEX(Outlook.Table, , 52))', function () { return fan ? fmt(fan[51].med) : ''; }, 'Outlook']]]
  ];
  var sheetBuilt = false, lastDirty = [], lastKind = '';
  function paintSheet() {
    if (view !== 'sheet') return;
    var tb = document.getElementById('sheetTbl');
    if (!sheetBuilt) {
      var h = '<thead><tr><th>Sheet / name</th><th>Value</th><th>Formula</th></tr></thead><tbody>';
      SHEET.forEach(function (s) { h += '<tr class="grp"><td colspan="3">' + s[0] + '</td></tr>'; s[1].forEach(function (r, i) { h += '<tr data-k="' + r[3] + '" data-i="' + s[0] + i + '"><td>' + r[0] + '</td><td class="v"></td><td class="f">' + r[1].replace(/</g, '&lt;') + '</td></tr>'; }); });
      tb.innerHTML = h + '</tbody>'; sheetBuilt = true;
    }
    SHEET.forEach(function (s) { s[1].forEach(function (r, i) { var tr = tb.querySelector('tr[data-i="' + s[0] + i + '"]'); tr.children[1].textContent = r[2](); tr.className = lastDirty.indexOf(r[3]) >= 0 ? (lastKind === 'price' ? 'flashp' : 'flash') : ''; }); });
  }

  // ================= event queue strip =================
  function paintQueue() {
    var q = document.getElementById('queue'); while (q.firstChild) q.removeChild(q.firstChild);
    var now = simTime, span = 12, W = 1300;
    function X(t) { return 120 + (t - (now - span * .8)) / span * (W - 140); }
    [['clock', 26, K.event, 'Clock'], ['price', 56, K.price, 'Market']].forEach(function (lane) {
      el('line', { x1: 120, x2: W - 10, y1: lane[1], y2: lane[1], stroke: K.line, 'stroke-width': 1 }, q);
      var t = el('text', { x: 10, y: lane[1] + 4, fill: lane[2] }, q); t.style.font = '600 12px "IBM Plex Mono",monospace'; t.textContent = lane[3];
    });
    el('line', { x1: X(now), x2: X(now), y1: 8, y2: 70, stroke: K.ink, 'stroke-width': 1.2 }, q);
    events.forEach(function (e) { var x = X(e.t); if (x < 118 || x > W) return; el('circle', { cx: x, cy: e.k === 'clock' ? 26 : 56, r: e.t > now ? 3.5 : 5, fill: e.k === 'clock' ? K.event : K.price, opacity: e.t > now ? .45 : .95 }, q); });
    var nt = el('text', { x: X(now) + 6, y: 12, fill: K.muted }, q); nt.style.font = '400 10px "IBM Plex Mono",monospace'; nt.textContent = 'now · upcoming events faded';
  }

  // ================= charts =================
  var HS = document.getElementById('hist'), FS = document.getElementById('fan');
  renderChartMarkup(document.getElementById('hkey'), [[K.water, 'Level'], [K.event, 'Inflow ×8'], [K.normal, 'Release ×8'], [K.price, 'Price ×10']].map(function (k) { return '<span><i data-background="' + k[0] + '"></i>' + k[1] + '</span>'; }).join('') + '<span><i data-background="' + K.drought + '" class="watershed-mode"></i>mode band</span>');
  function paintHist() {
    while (HS.firstChild) HS.removeChild(HS.firstChild);
    var W = 760, Hh = 250, m = { l: 40, r: 10, t: 10, b: 30 }, n = Math.max(80, H.length), t0 = H.length ? H[0].t : 0;
    function X(t) { return m.l + (t - t0) / (n - 1) * (W - m.l - m.r); } function Y(vv) { return Hh - m.b - Math.min(1.05, vv / P.Capacity) * (Hh - m.t - m.b - 8); }
    [0, 250, 500, 750, 1000].forEach(function (g) { el('line', { x1: m.l, x2: W - m.r, y1: Y(g), y2: Y(g), stroke: K.line }, HS); var t = el('text', { x: m.l - 6, y: Y(g) + 4, 'text-anchor': 'end', fill: K.faint }, HS); t.style.font = '400 10px "IBM Plex Mono",monospace'; t.textContent = g; });
    H.forEach(function (h) { el('rect', { x: X(h.t) - 2, y: Hh - m.b + 4, width: (W - m.l - m.r) / (n - 1) + 1, height: 6, fill: modeColor(h.Mode) }, HS); });
    if (H.length > 1) {
      el('path', { d: 'M' + X(H[0].t) + ',' + Y(0) + ' ' + H.map(function (h) { return 'L' + X(h.t) + ',' + Y(h.Level); }).join(' ') + ' L' + X(H[H.length - 1].t) + ',' + Y(0) + ' Z', fill: K.water, opacity: .22 }, HS);
      [['Level', 1, K.water, 2.4], ['Inflow', 8, K.event, 1.3], ['Release', 8, K.normal, 1.3], ['Price', 10, K.price, 1.3]].forEach(function (s) {
        el('path', { d: H.map(function (h, i) { return (i ? 'L' : 'M') + X(h.t).toFixed(1) + ',' + Y(h[s[0]] * s[1]).toFixed(1); }).join(' '), fill: 'none', stroke: s[2], 'stroke-width': s[3] }, HS);
      });
      H.forEach(function (h) { if (h.Spill > 0) el('circle', { cx: X(h.t), cy: Y(P.Capacity) - 4, r: 2.2, fill: K.flood }, HS); });
    }
    var lab = el('text', { x: m.l, y: Hh - 6, fill: K.faint }, HS); lab.style.font = '400 10px "IBM Plex Mono",monospace'; lab.textContent = H.length ? 'ticks ' + t0 + '–' + (t0 + n - 1) + ' · dots: spill' : 'Press Run: the RTD clock starts ticking.';
  }
  function paintFan() {
    while (FS.firstChild) FS.removeChild(FS.firstChild);
    var W = 560, Hh = 250, m = { l: 40, r: 10, t: 10, b: 30 }, back = Math.min(26, H.length), total = back + 52;
    function X(i) { return m.l + i / (total - 1) * (W - m.l - m.r); } function Y(vv) { return Hh - m.b - vv / P.Capacity * (Hh - m.t - m.b); }
    [0, 500, 1000].forEach(function (g) { el('line', { x1: m.l, x2: W - m.r, y1: Y(g), y2: Y(g), stroke: K.line }, FS); var t = el('text', { x: m.l - 6, y: Y(g) + 4, 'text-anchor': 'end', fill: K.faint }, FS); t.style.font = '400 10px "IBM Plex Mono",monospace'; t.textContent = g; });
    el('line', { x1: X(back - 1), x2: X(back - 1), y1: m.t, y2: Hh - m.b, stroke: K.muted, 'stroke-dasharray': '3 3' }, FS);
    el('line', { x1: m.l, x2: W - m.r, y1: Y(P.LowMark), y2: Y(P.LowMark), stroke: K.drought, 'stroke-dasharray': '4 3', opacity: .7 }, FS);
    el('line', { x1: m.l, x2: W - m.r, y1: Y(P.HighMark), y2: Y(P.HighMark), stroke: K.flood, 'stroke-dasharray': '4 3', opacity: .7 }, FS);
    var past = H.slice(-back);
    if (past.length > 1) el('path', { d: past.map(function (h, i) { return (i ? 'L' : 'M') + X(i) + ',' + Y(h.Level); }).join(' '), fill: 'none', stroke: K.ink, 'stroke-width': 2 }, FS);
    if (fan) {
      var o = back - 1;
      function band(lo, hi, op) { el('path', { d: fan.map(function (f, i) { return (i ? 'L' : 'M') + X(o + 1 + i) + ',' + Y(f[hi]); }).join(' ') + ' ' + fan.slice().reverse().map(function (f, i) { return 'L' + X(o + fan.length - i) + ',' + Y(f[lo]); }).join(' ') + 'Z', fill: K.water, opacity: op }, FS); }
      band('min', 'max', .15); band('q1', 'q3', .35);
      el('path', { d: fan.map(function (f, i) { return (i ? 'L' : 'M') + X(o + 1 + i) + ',' + Y(f.med); }).join(' '), fill: 'none', stroke: K.water, 'stroke-width': 2.2 }, FS);
    }
    var lab = el('text', { x: m.l, y: Hh - 8, fill: K.faint }, FS); lab.style.font = '400 10px "IBM Plex Mono",monospace'; lab.textContent = 'past ' + back + ' ticks · next 52 ticks from the data table · dashed: low and high marks';
  }

  // ================= inspector =================
  function findActor(id) { var lv = LEVELS[level]; var row = (lv.actors || []).find(function (r) { return r[0] === id; }); return row ? A(row) : null; }
  function inspect() {
    var a = findActor(selected), box = document.getElementById('insp');
    Object.keys(boxes).forEach(function (k) { boxes[k].g.classList.toggle('sel', k === selected); });
    if (!a) { box.innerHTML = '<h2>Inspector</h2><p>Hover or click a box to see its formula and role.</p>'; return; }
    box.innerHTML = '<h2>' + (a.kind === 'comp' ? 'Composite · click to look inside' : 'Actor') + '</h2><div class="n">' + a.label + '</div><div class="k">' + ({ rtd: 'RTD event source', comp: (LEVELS[a.id] || {}).director || 'composite', calc: 'calculation', state: 'state with a z⁻¹ self loop', input: 'parameter', cycle: 'part of a circular block', plot: 'plotter' })[a.kind] + '</div><div class="f">' + a.xf.replace(/</g, '&lt;') + '</div><p>' + a.note + '</p>';
  }

  // ================= navigation =================
  function enter(id) { level = id; selected = null; build(); paint(); crumbs(); }
  function crumbs() {
    var c = document.getElementById('crumbs'), L = LEVELS[level];
    c.innerHTML = level === 'top' ? '<b>Dam</b>' : '<button type="button" id="upBtn">Dam</button><span>›</span><b>' + L.title + '</b>';
    if (level !== 'top') document.getElementById('upBtn').addEventListener('click', function () { enter('top'); selected = 'Reservoir'; inspect(); });
    document.getElementById('dirtag').textContent = L.director;
  }
  document.querySelectorAll('#viewSeg button').forEach(function (b) { b.addEventListener('click', function () { view = b.dataset.v; document.querySelectorAll('#viewSeg button').forEach(function (x) { x.setAttribute('aria-pressed', String(x === b)); }); document.getElementById('diagramWrap').hidden = view !== 'actors'; document.getElementById('sheetWrap').hidden = view !== 'sheet'; paint(); }); });

  // ================= the event loop: RTD clock and market =================
  var running = false, rate = 4, mktRate = 1.5, simTime = 0, nextClock = 0, nextPrice = .5, lastFrame = null, flashUntil = 0;
  function schedule() {
    events = events.filter(function (e) { return e.t > simTime - 12; });
    var horizon = simTime + 3;
    var lastC = events.filter(function (e) { return e.k === 'clock'; }).reduce(function (m, e) { return Math.max(m, e.t); }, nextClock - 1 / rate);
    for (var t = Math.max(lastC + 1 / rate, nextClock); t < horizon; t += 1 / rate) events.push({ t: t, k: 'clock', future: true });
    if (mktRate > 0) { var lastP = events.filter(function (e) { return e.k === 'price'; }).reduce(function (m, e) { return Math.max(m, e.t); }, nextPrice - .01); var tp = Math.max(lastP, nextPrice); while (tp < horizon) { tp += (.4 + noise(Math.floor(tp * 97), 5) * 1.2) / mktRate; events.push({ t: tp, k: 'price', future: true }); } }
  }
  function fire(kind) {
    var dirty = kind === 'clock' ? tickEvent() : priceEvent();
    lastDirty = dirty; lastKind = kind; flashUntil = performance.now() + 380;
    Object.keys(boxes).forEach(function (k) { boxes[k].g.classList.toggle(kind === 'clock' ? 'fired' : 'firedp', dirty.indexOf(k) >= 0); boxes[k].g.classList.remove(kind === 'clock' ? 'firedp' : 'fired'); });
    wireEls.forEach(function (w) { var hot = dirty.indexOf(w.from) >= 0 && dirty.indexOf(w.to) >= 0; w.el.classList.toggle(kind === 'clock' ? 'hot' : 'hotp', hot); w.el.classList.remove(kind === 'clock' ? 'hotp' : 'hot'); });
    var rw = document.getElementById('resetWhen').value;
    if ((rw === '52' && S.Tick >= 52) || (rw === '156' && S.Tick >= 156) || (rw === 'debt' && S.Debt <= 0) || (rw === 'dry' && S.Level <= 0)) { reset(); }
  }
  // The event loop runs on a timer (RTD updates do not wait for the screen); drawing follows animation frames.
  var lastTick = performance.now();
  function advance() {
    var nowMs = performance.now(), dt = Math.min(.25, (nowMs - lastTick) / 1000); lastTick = nowMs;
    if (!running) return;
    simTime += dt; schedule();
    events.filter(function (e) { return e.future && e.t <= simTime; }).sort(function (a, b) { return a.t - b.t; }).forEach(function (e) { e.future = false; fire(e.k); if (e.k === 'clock') nextClock = e.t + 1 / rate; else nextPrice = e.t; });
  }
  setInterval(advance, 30);
  function frame() {
    if (performance.now() > flashUntil) { Object.keys(boxes).forEach(function (k) { boxes[k].g.classList.remove('fired', 'firedp'); }); wireEls.forEach(function (w) { w.el.classList.remove('hot', 'hotp'); }); lastDirty = []; }
    paint();
    requestAnimationFrame(frame);
  }
  document.getElementById('run').addEventListener('click', function () { running = !running; if (running) { nextClock = simTime + .05; } });
  document.getElementById('step').addEventListener('click', function () { events.push({ t: simTime, k: 'clock' }); fire('clock'); paint(); });
  document.getElementById('resetNow').addEventListener('click', function () { reset(); });
  document.getElementById('rate').addEventListener('input', function () { rate = Number(this.value); document.getElementById('rateOut').textContent = rate; events = events.filter(function (e) { return !(e.future && e.k === 'clock'); }); nextClock = simTime + 1 / rate; });
  document.getElementById('mkt').addEventListener('input', function () { mktRate = Number(this.value); document.getElementById('mktOut').textContent = mktRate; events = events.filter(function (e) { return !(e.future && e.k === 'price'); }); nextPrice = simTime; });
  function param(id, key, out, d) { document.getElementById(id).addEventListener('input', function () { P[key] = Number(this.value); document.getElementById(out).textContent = Number(this.value).toFixed(d); fan = sweep(); }); }
  param('p-rain', 'RainMult', 'o-rain', 2); param('p-high', 'HighMark', 'o-high', 0); param('p-sweep', 'SweepShare', 'o-sweep', 2);

  // Open with a season already simulated, so the page is complete at rest.
  reset();
  for (var i = 0; i < 60; i++) { fire(i % 3 === 2 ? 'price' : 'clock'); if (i % 3 !== 2) { events.push({ t: simTime - (60 - i) * .25, k: 'clock' }); } else events.push({ t: simTime - (60 - i) * .25 + .1, k: 'price' }); }
  lastDirty = []; flashUntil = 0;
  build(); crumbs(); inspect(); paint();
  requestAnimationFrame(frame);
})();

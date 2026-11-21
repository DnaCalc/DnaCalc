(function () {
  // ---------------- the model: Excel formulas, and their meaning per tick ----------------
  var P = { Base: 100, Season: .35, LeadTime: 3, ReorderPoint: 260, OrderQty: 420, Price: 12, UnitCost: 7, HoldCost: .05, StockInitial: 400, CashInitial: 2000 };
  var PARAMS = [
    ['Base', 'Demand.Base', 40, 200, 1], ['Season', 'Demand.Season', 0, .8, .05], ['LeadTime', 'Supply.LeadTime', 1, 8, 1],
    ['ReorderPoint', 'Supply.ReorderPoint', 0, 600, 10], ['OrderQty', 'Supply.OrderQty', 50, 900, 10], ['Price', 'Finance.Price', 8, 20, .5], ['UnitCost', 'Finance.UnitCost', 3, 12, .5]
  ];
  var A = [
    // id, label, kind, x, y, w, h, Excel formula, presented formula, inputs (ids), group
    ['Tick', 'Tick', 'state', 214, 92, 150, 64, '=IF(Shop.Reset, 0, Shop.Tick + 1)', 'Tick⁻ + 1', ['Tick'], null],
    ['Base', 'Demand.Base', 'input', 60, 220, 150, 58, '100', 'parameter', [], 'Demand'],
    ['Season', 'Demand.Season', 'input', 60, 300, 150, 58, '0.35', 'parameter', [], 'Demand'],
    ['Demand', 'Demand', 'calc', 290, 246, 170, 70, '=ROUND(Demand.Base * (1 + Demand.Season * SIN(2 * PI() * Shop.Tick / 12)) * Demand.Noise, 0)', 'Base × (1 + Season × sin(2π·Tick/12)) × Noise', ['Base', 'Season', 'Tick'], 'Demand'],
    ['LeadTime', 'Supply.LeadTime', 'input', 60, 470, 150, 58, '3', 'parameter', [], 'Supply'],
    ['Pipeline', 'Supply.Pipeline', 'state', 290, 470, 170, 70, '=IF(Shop.Reset, SEQUENCE(1, Supply.LeadTime, 0, 0), HSTACK(DROP(Supply.Pipeline, , 1), Supply.Order))', 'shift(Pipeline⁻) ⟵ Order⁻', ['LeadTime', 'Pipeline', 'Order'], 'Supply'],
    ['Arrivals', 'Supply.Arrivals', 'calc', 520, 470, 150, 64, '=INDEX(Supply.Pipeline, 1)', 'first(Pipeline⁻): the order placed LeadTime ticks ago', ['Pipeline'], 'Supply'],
    ['Sales', 'Store.Sales', 'calc', 560, 246, 150, 64, '=MIN(Demand, Store.Stock + Supply.Arrivals)', 'min(Demand, Stock⁻ + Arrivals)', ['Demand', 'Stock', 'Arrivals'], 'Store'],
    ['Stock', 'Store.Stock', 'state', 770, 246, 160, 70, '=IF(Shop.Reset, Store.Initial, Store.Stock + Supply.Arrivals - Store.Sales)', 'Stock⁻ + Arrivals − Sales', ['Stock', 'Arrivals', 'Sales'], 'Store'],
    ['Lost', 'Store.Lost', 'calc', 770, 120, 150, 58, '=Demand - Store.Sales', 'Demand − Sales', ['Demand', 'Sales'], 'Store'],
    ['Position', 'Supply.Position', 'calc', 770, 470, 160, 64, '=Store.Stock + SUM(Supply.Pipeline)', 'Stock + Σ Pipeline', ['Stock', 'Pipeline'], 'Supply'],
    ['RP', 'Supply.ReorderPoint', 'input', 520, 600, 150, 58, '260', 'parameter', [], 'Supply'],
    ['OQ', 'Supply.OrderQty', 'input', 770, 600, 160, 58, '420', 'parameter', [], 'Supply'],
    ['Order', 'Supply.Order', 'calc', 1000, 520, 150, 64, '=IF(Supply.Position < Supply.ReorderPoint, Supply.OrderQty, 0)', 'Position < ReorderPoint ? OrderQty : 0', ['Position', 'RP', 'OQ'], 'Supply'],
    ['Mode', 'Store.Mode', 'mode', 1000, 92, 190, 120, '=IFS(Store.Lost > 0, "Stockout", Supply.Order > 0, "Ordering", TRUE, "Normal")', 'state machine over Lost and Order', ['Lost', 'Order'], 'Store'],
    ['Price', 'Finance.Price', 'input', 1000, 280, 150, 58, '12', 'parameter', [], 'Finance'],
    ['Cash', 'Finance.Cash', 'state', 1000, 370, 190, 70, '=IF(Shop.Reset, Finance.Initial, Finance.Cash + Store.Sales * Finance.Price - Supply.Order * Finance.UnitCost - Store.Stock * Finance.HoldCost)', 'Cash⁻ + Sales × Price − Order × UnitCost − Stock × Hold', ['Cash', 'Sales', 'Price', 'Order', 'Stock'], 'Finance']
  ];
  var ORDER = ['Tick', 'Demand', 'Arrivals', 'Pipeline', 'Sales', 'Stock', 'Lost', 'Position', 'Order', 'Mode', 'Cash'];
  var GROUPS = [['Demand', 40, 200, 440, 190], ['Supply', 40, 445, 1130, 240], ['Store', 540, 100, 420, 240], ['Finance', 980, 262, 230, 200]];
  var byId = {}; A.forEach(function (a) { byId[a[0]] = { id: a[0], label: a[1], kind: a[2], x: a[3], y: a[4], w: a[5], h: a[6], xf: a[7], pf: a[8], ins: a[9], group: a[10] }; });

  var S, hist;
  function noise(t) { var x = Math.sin(t * 12.9898 + 78.233) * 43758.5453; return 0.88 + 0.24 * (x - Math.floor(x)); }
  function reset() {
    S = { Tick: 0, Pipeline: new Array(P.LeadTime).fill(0), Stock: P.StockInitial, Cash: P.CashInitial, Demand: 0, Arrivals: 0, Sales: 0, Lost: 0, Position: 0, Order: 0, Mode: 'Normal' };
    hist = [];
  }
  // One recalculation = one tick, firing in calc-chain order. Each step returns the actor that fired.
  function tickSteps() {
    var prev = { Stock: S.Stock, Cash: S.Cash, Pipeline: S.Pipeline.slice(), Order: S.Order, Tick: S.Tick };
    return [
      function () { S.Tick = prev.Tick + 1; return 'Tick'; },
      function () { S.Demand = Math.round(P.Base * (1 + P.Season * Math.sin(2 * Math.PI * S.Tick / 12)) * noise(S.Tick)); return 'Demand'; },
      function () { S.Arrivals = prev.Pipeline[0] || 0; return 'Arrivals'; },
      function () { var pl = prev.Pipeline.slice(1); pl.push(prev.Order); while (pl.length < P.LeadTime) pl.unshift(0); while (pl.length > P.LeadTime) { pl[1] += pl[0]; pl.shift(); } S.Pipeline = pl; return 'Pipeline'; },
      function () { S.Sales = Math.min(S.Demand, prev.Stock + S.Arrivals); return 'Sales'; },
      function () { S.Stock = prev.Stock + S.Arrivals - S.Sales; return 'Stock'; },
      function () { S.Lost = S.Demand - S.Sales; return 'Lost'; },
      function () { S.Position = S.Stock + S.Pipeline.reduce(function (s, v) { return s + v; }, 0); return 'Position'; },
      function () { S.Order = S.Position < P.ReorderPoint ? P.OrderQty : 0; return 'Order'; },
      function () { S.Mode = S.Lost > 0 ? 'Stockout' : S.Order > 0 ? 'Ordering' : 'Normal'; return 'Mode'; },
      function () { S.Cash = prev.Cash + S.Sales * P.Price - S.Order * P.UnitCost - S.Stock * P.HoldCost; hist.push({ t: S.Tick, Stock: S.Stock, Demand: S.Demand, Sales: S.Sales, Cash: S.Cash, Mode: S.Mode, Order: S.Order }); if (hist.length > 120) hist.shift(); return 'Cash'; }
    ];
  }

  // ---------------- drawing ----------------
  var NS = 'http://www.w3.org/2000/svg', svg = document.getElementById('sch');
  function el(tag, attrs, parent) { var e = document.createElementNS(NS, tag); for (var k in attrs) e.setAttribute(k, attrs[k]); (parent || svg).appendChild(e); return e; }
  var view = 'present';
  var gFrames = el('g', {}), gWires = el('g', {}), gActors = el('g', {}), gTop = el('g', {});
  GROUPS.forEach(function (g) { var f = el('g', { class: 'frame' }, gFrames); el('rect', { x: g[1], y: g[2], width: g[3], height: g[4], rx: 6 }, f); el('text', { x: g[1] + 10, y: g[2] + 16 }, f).textContent = 'Shop.' + g[0]; });
  // Director, Ptolemy style: a green box naming the model of computation and its parameters.
  var dir = el('g', { class: 'director' }, gTop);
  el('rect', { x: 20, y: 18, width: 176, height: 58, rx: 4 }, dir);
  var dt = el('text', { x: 32, y: 40 }, dir); dt.style.font = '800 15px "Bricolage Grotesque",system-ui'; dt.textContent = 'Ticks director';
  var dp = el('text', { x: 32, y: 58 }, dir); dp.style.font = '400 10.5px "IBM Plex Mono",monospace'; dp.textContent = 'iterations: 1 per tick';
  var dp2 = el('text', { x: 32, y: 71 }, dir); dp2.style.font = '400 10.5px "IBM Plex Mono",monospace'; dp2.textContent = 'max change: 0.001';
  // Ports: outputs on the right edge; inputs spread along the left edge.
  function outPort(a) { return [a.x + a.w, a.y + a.h / 2]; }
  function inPort(a, i, n) { return [a.x, a.y + a.h * (i + 1) / (n + 1)]; }
  var wires = [];
  A.forEach(function (row) {
    var a = byId[row[0]], n = a.ins.length;
    a.ins.forEach(function (src, i) {
      var s = byId[src], p0 = outPort(s), p1 = inPort(a, i, n), d;
      if (src === a.id) {
        // Self reference: a feedback loop over the top with a z⁻¹ delay, the previous tick's value.
        var lx = a.x + a.w + 16, ty = a.y - 22;
        d = 'M' + p0[0] + ',' + p0[1] + ' C' + (lx + 24) + ',' + p0[1] + ' ' + (lx + 24) + ',' + ty + ' ' + (a.x + a.w / 2 + 30) + ',' + ty + ' L' + (a.x - 26) + ',' + ty + ' C' + (a.x - 40) + ',' + ty + ' ' + (a.x - 40) + ',' + p1[1] + ' ' + p1[0] + ',' + p1[1];
        var w = el('path', { d: d, class: 'wire loop' }, gWires);
        var dl = el('g', { class: 'delay' }, gWires);
        el('rect', { x: a.x + a.w / 2 - 16, y: ty - 10, width: 32, height: 20, rx: 3 }, dl);
        el('text', { x: a.x + a.w / 2, y: ty + 4, 'text-anchor': 'middle' }, dl).textContent = 'z⁻¹';
        wires.push({ from: src, to: a.id, el: w });
      } else {
        var mx = (p0[0] + p1[0]) / 2, back = p1[0] < p0[0] + 20;
        d = back ? 'M' + p0[0] + ',' + p0[1] + ' C' + (p0[0] + 70) + ',' + p0[1] + ' ' + (p1[0] - 70) + ',' + p1[1] + ' ' + p1[0] + ',' + p1[1]
                 : 'M' + p0[0] + ',' + p0[1] + ' C' + mx + ',' + p0[1] + ' ' + mx + ',' + p1[1] + ' ' + p1[0] + ',' + p1[1];
        wires.push({ from: src, to: a.id, el: el('path', { d: d, class: 'wire' }, gWires) });
      }
      el('polygon', { points: (p1[0] - 7) + ',' + (p1[1] - 4) + ' ' + p1[0] + ',' + p1[1] + ' ' + (p1[0] - 7) + ',' + (p1[1] + 4), class: 'port' }, gActors);
    });
  });
  // Relations: a diamond where one output fans out to several inputs (Ptolemy's relation icon).
  var fan = {}; wires.forEach(function (w) { if (w.from !== w.to) fan[w.from] = (fan[w.from] || 0) + 1; });
  Object.keys(fan).forEach(function (id) { if (fan[id] < 2) return; var p = outPort(byId[id]); el('polygon', { points: (p[0] + 12) + ',' + p[1] + ' ' + (p[0] + 18) + ',' + (p[1] - 6) + ' ' + (p[0] + 24) + ',' + p[1] + ' ' + (p[0] + 18) + ',' + (p[1] + 6), fill: 'var(--ink)' }, gTop); });
  var nodes = {};
  A.forEach(function (row) {
    var a = byId[row[0]], g = el('g', { class: 'actor ' + a.kind, tabindex: 0, role: 'button', 'aria-label': a.label }, gActors);
    el('rect', { class: 'glow', x: a.x - 6, y: a.y - 6, width: a.w + 12, height: a.h + 12, rx: 10, fill: 'transparent' }, g);
    el('rect', { class: 'body', x: a.x, y: a.y, width: a.w, height: a.h, rx: 5 }, g);
    el('polygon', { points: (a.x + a.w) + ',' + (a.y + a.h / 2 - 5) + ' ' + (a.x + a.w + 8) + ',' + (a.y + a.h / 2) + ' ' + (a.x + a.w) + ',' + (a.y + a.h / 2 + 5), class: 'port' }, g);
    el('text', { class: 'name', x: a.x + 10, y: a.y + 18 }, g).textContent = a.label.replace(/^(Demand|Supply|Store|Finance)\./, '');
    var v = el('text', { class: 'val', x: a.x + 10, y: a.y + 41 }, g);
    var sub = el('text', { class: 'sub', x: a.x + 10, y: a.y + a.h - 9 }, g);
    nodes[a.id] = { g: g, v: v, sub: sub, a: a };
    if (a.kind === 'mode') {
      var f = el('g', { class: 'fsm' }, g), cx = a.x + 30, cy = a.y + 86;
      [['Normal', 0], ['Ordering', 1], ['Stockout', 2]].forEach(function (s) { var x = cx + s[1] * 64; el('circle', { cx: x, cy: cy, r: 8, 'data-s': s[0] }, f); el('text', { x: x, y: cy + 21, 'text-anchor': 'middle' }, f).textContent = s[0]; });
      el('path', { d: 'M' + (cx + 9) + ',' + (cy - 3) + ' Q' + (cx + 32) + ',' + (cy - 16) + ' ' + (cx + 55) + ',' + (cy - 3) }, f);
      el('path', { d: 'M' + (cx + 73) + ',' + (cy - 3) + ' Q' + (cx + 96) + ',' + (cy - 16) + ' ' + (cx + 119) + ',' + (cy - 3) }, f);
      el('path', { d: 'M' + (cx + 120) + ',' + (cy + 6) + ' Q' + (cx + 64) + ',' + (cy + 36) + ' ' + (cx + 8) + ',' + (cy + 6) }, f);
      nodes[a.id].fsm = f; v.setAttribute('y', a.y + 40);
    }
    g.addEventListener('click', function () { inspect(a.id); });
    g.addEventListener('keydown', function (e) { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); inspect(a.id); } });
  });
  // The plotter actor, wired from Stock, Demand and Cash.
  var pl = { x: 1040, y: 640, w: 150, h: 58 };
  var pg = el('g', { class: 'actor plot' }, gActors);
  el('rect', { class: 'body', x: pl.x, y: pl.y, width: pl.w, height: pl.h, rx: 5 }, pg);
  el('text', { class: 'name', x: pl.x + 10, y: pl.y + 20 }, pg).textContent = 'Plotter';
  el('text', { class: 'sub', x: pl.x + 10, y: pl.y + 40 }, pg).textContent = 'Stock · Demand · Cash';
  ['Stock', 'Demand', 'Cash'].forEach(function (id, i) { var s = byId[id], p0 = outPort(s), p1 = [pl.x, pl.y + pl.h * (i + 1) / 4]; el('path', { d: 'M' + p0[0] + ',' + p0[1] + ' C' + (p0[0] + 60) + ',' + p0[1] + ' ' + (p1[0] - 90) + ',' + p1[1] + ' ' + p1[0] + ',' + p1[1], class: 'wire', opacity: .35 }, gWires); });

  function fmtv(id) {
    var v = S[id];
    if (id === 'Pipeline') return '{' + v.join(', ') + '}';
    if (id === 'Mode') return v;
    if (id === 'Cash') return '$' + Math.round(v).toLocaleString('en-US');
    return Math.round(v).toLocaleString('en-US');
  }
  function paramVal(a) {
    var map = { Base: P.Base, Season: P.Season, LeadTime: P.LeadTime, RP: P.ReorderPoint, OQ: P.OrderQty, Price: P.Price };
    var v = map[a.id]; return a.id === 'Price' ? '$' + v : String(v);
  }
  function paint() {
    Object.keys(nodes).forEach(function (id) {
      var n = nodes[id], a = n.a;
      n.v.textContent = a.kind === 'input' ? paramVal(a) : fmtv(id);
      var t = view === 'excel' ? a.xf : (a.kind === 'input' ? 'parameter' : a.pf);
      n.sub.textContent = t.length > Math.floor(a.w / 6.6) ? t.slice(0, Math.floor(a.w / 6.6) - 1) + '…' : t;
      if (n.fsm) n.fsm.querySelectorAll('circle').forEach(function (c) { c.setAttribute('class', c.getAttribute('data-s') === S.Mode ? 'on' : ''); });
    });
    document.getElementById('clock').textContent = 'Tick ' + S.Tick + (S.Tick ? '  ·  ' + S.Mode : '');
    plot();
    if (selected) inspect(selected, true);
  }
  function fire(id) {
    Object.keys(nodes).forEach(function (k) { nodes[k].g.classList.toggle('fired', k === id); });
    wires.forEach(function (w) { w.el.classList.toggle('hot', w.from === id && w.to !== id); });
  }
  function clearFire() { Object.keys(nodes).forEach(function (k) { nodes[k].g.classList.remove('fired'); }); wires.forEach(function (w) { w.el.classList.remove('hot'); }); }

  // ---------------- running ----------------
  var running = false, timer = null, steps = [], speed = 3;
  function stepDelay() { return [0, 220, 120, 60, 25, 0][speed]; }
  function runTick(done) {
    steps = tickSteps();
    var d = stepDelay();
    if (!d) { var last; steps.forEach(function (s) { last = s(); }); clearFire(); paint(); return done && done(); }
    (function next(i) {
      if (i >= steps.length) { setTimeout(function () { clearFire(); done && done(); }, d); return; }
      var id = steps[i](); fire(id); paint();
      setTimeout(function () { next(i + 1); }, d);
    })(0);
  }
  function loop() { if (!running) return; runTick(function () { if (running) timer = setTimeout(loop, speed >= 5 ? 30 : 140); }); }
  var runBtn = document.getElementById('run');
  runBtn.addEventListener('click', function () { running = !running; runBtn.textContent = running ? '❚❚ Pause' : '▶ Run'; if (running) loop(); else clearTimeout(timer); });
  document.getElementById('step').addEventListener('click', function () { if (running) return; runTick(); });
  document.getElementById('reset').addEventListener('click', function () { reset(); clearFire(); paint(); });
  document.getElementById('speed').addEventListener('input', function () { speed = Number(this.value); });
  document.querySelectorAll('#view button').forEach(function (b) { b.addEventListener('click', function () { view = b.dataset.v; document.querySelectorAll('#view button').forEach(function (x) { x.setAttribute('aria-pressed', String(x === b)); }); paint(); }); });

  // ---------------- parameters ----------------
  var pc = document.getElementById('params');
  PARAMS.forEach(function (p) {
    var row = document.createElement('div'); row.className = 'param';
    row.innerHTML = '<span>' + p[1] + '</span><output id="o-' + p[0] + '">' + P[p[0]] + '</output><input type="range" id="p-' + p[0] + '" min="' + p[2] + '" max="' + p[3] + '" step="' + p[4] + '" value="' + P[p[0]] + '" aria-label="' + p[1] + '">';
    pc.appendChild(row);
    row.querySelector('input').addEventListener('input', function () {
      P[p[0]] = Number(this.value); document.getElementById('o-' + p[0]).textContent = this.value;
      var id = { ReorderPoint: 'RP', OrderQty: 'OQ' }[p[0]] || p[0];
      if (nodes[id]) { nodes[id].g.classList.add('fired'); wires.forEach(function (w) { w.el.classList.toggle('hot', w.from === id); }); setTimeout(clearFire, 500); }
      paint();
    });
  });

  // ---------------- inspector ----------------
  var selected = null;
  var WHY = {
    Tick: 'The clock. It refers to itself, so under iterative calculation each recalculation adds one: a z⁻¹ delay feeding back into itself.',
    Demand: 'A seasonal demand curve with fixed noise, recalculated from the tick on every recalculation.',
    Pipeline: 'A delay line built from dynamic arrays: last tick\'s order enters at the end and the array shifts one place per tick. LeadTime sets its length.',
    Arrivals: 'Whatever reaches the front of the pipeline this tick.',
    Sales: 'You cannot sell what you do not have: the smaller of demand and the stock on hand plus arrivals.',
    Stock: 'State. Its previous value (Stock⁻) plus arrivals minus sales. In Excel this is a cell that refers to itself.',
    Lost: 'Demand that went unserved this tick. Anything above zero puts the store into Stockout.',
    Position: 'Stock on hand plus everything still on its way: what the reorder policy looks at.',
    Order: 'The reorder policy: order a fixed quantity whenever the position falls below the reorder point.',
    Mode: 'A modal model. The IFS guards become transitions between three states; the lit state is the current one.',
    Cash: 'State. Revenue in, purchases and holding cost out, carried from tick to tick.',
    Base: 'Parameter: average demand per tick.', Season: 'Parameter: how strongly demand swings over a 12-tick year.', LeadTime: 'Parameter: ticks between ordering and arrival.',
    RP: 'Parameter: reorder when the position falls below this.', OQ: 'Parameter: how much to order.', Price: 'Parameter: selling price.'
  };
  function inspect(id, quiet) {
    selected = id; var a = byId[id], n = nodes[id];
    Object.keys(nodes).forEach(function (k) { nodes[k].g.classList.toggle('sel', k === id); });
    var deps = a.ins.filter(function (x) { return x !== id; }).map(function (x) { return byId[x].label; });
    var users = A.filter(function (r) { return r[9].indexOf(id) >= 0 && r[0] !== id; }).map(function (r) { return r[1]; });
    document.getElementById('inspect').innerHTML = '<h2>Actor</h2><div class="n">' + a.label + '</div><div class="k">' + ({ input: 'parameter', state: 'state, with a z⁻¹ self loop', calc: 'calculation', mode: 'modal model' })[a.kind] + ' · now ' + (a.kind === 'input' ? paramVal(a) : fmtv(id)) + '</div>' +
      '<div class="f">' + (a.kind === 'input' ? 'Defined name ' + a.label + ' = ' + paramVal(a).replace('$', '') : a.xf) + '</div>' +
      (a.kind !== 'input' ? '<div class="f formula-preview">' + a.pf + '</div>' : '') +
      '<p>' + WHY[id] + '</p>' +
      (deps.length ? '<p><b>Reads</b> ' + deps.join(', ') + (a.ins.indexOf(id) >= 0 ? ', and itself one tick back' : '') + '</p>' : '') +
      (users.length ? '<p><b>Read by</b> ' + users.join(', ') + '</p>' : '');
  }

  // ---------------- plotter ----------------
  var PS = document.getElementById('plot'), PW = 1240, PH = 260, pm = { l: 54, r: 70, t: 14, b: 28 };
  var SERIES = [['Stock', 'var(--stateInk)'], ['Demand', 'var(--inputInk)'], ['Sales', 'var(--plotInk)'], ['Cash', 'var(--modeInk)']];
  renderChartMarkup(document.getElementById('pkey'), SERIES.map(function (s) { return '<span><i data-background="' + s[1] + '"></i>' + s[0] + (s[0] === 'Cash' ? ' (right axis)' : '') + '</span>'; }).join('') + '<span><i class="stockout-key"></i>stockout ticks</span>');
  function plot() {
    while (PS.firstChild) PS.removeChild(PS.firstChild);
    var n = Math.max(60, hist.length), x0 = hist.length ? hist[0].t : 0;
    var maxL = Math.max(500, Math.max.apply(null, hist.map(function (h) { return Math.max(h.Stock, h.Demand); }).concat([0])) * 1.1);
    var cashs = hist.map(function (h) { return h.Cash; }); var cmin = Math.min.apply(null, cashs.concat([0])), cmax = Math.max.apply(null, cashs.concat([P.CashInitial * 2]));
    function X(t) { return pm.l + (t - x0) / (n - 1) * (PW - pm.l - pm.r); }
    function Y(v) { return PH - pm.b - v / maxL * (PH - pm.t - pm.b); }
    function YC(v) { return PH - pm.b - (v - cmin) / (cmax - cmin || 1) * (PH - pm.t - pm.b); }
    hist.forEach(function (h) { if (h.Mode === 'Stockout') el('rect', { x: X(h.t) - (PW - pm.l - pm.r) / (n - 1) / 2, y: pm.t, width: (PW - pm.l - pm.r) / (n - 1), height: PH - pm.t - pm.b, fill: 'var(--hotTint)' }, PS); });
    [0, .25, .5, .75, 1].forEach(function (f) { var v = maxL * f; el('line', { x1: pm.l, x2: PW - pm.r, y1: Y(v), y2: Y(v), stroke: 'var(--grid)' }, PS); var t = el('text', { x: pm.l - 8, y: Y(v) + 4, 'text-anchor': 'end', fill: 'var(--faint)' }, PS); t.style.font = '400 11px "IBM Plex Mono",monospace'; t.textContent = Math.round(v); var c = el('text', { x: PW - pm.r + 8, y: YC(cmin + (cmax - cmin) * f) + 4, fill: 'var(--modeInk)' }, PS); c.style.font = '400 11px "IBM Plex Mono",monospace'; c.textContent = '$' + Math.round((cmin + (cmax - cmin) * f) / 100) / 10 + 'k'; });
    if (hist.length > 1) SERIES.forEach(function (s) {
      var d = hist.map(function (h, i) { return (i ? 'L' : 'M') + X(h.t).toFixed(1) + ',' + (s[0] === 'Cash' ? YC(h.Cash) : Y(h[s[0]])).toFixed(1); }).join(' ');
      el('path', { d: d, fill: 'none', stroke: s[1], 'stroke-width': s[0] === 'Stock' ? 2.6 : 1.8, 'stroke-dasharray': s[0] === 'Sales' ? '4 3' : null }, PS);
    });
    hist.forEach(function (h) { if (h.Order) el('circle', { cx: X(h.t), cy: PH - pm.b + 9, r: 3, fill: 'var(--calcInk)' }, PS); });
    var lab = el('text', { x: pm.l, y: PH - 4, fill: 'var(--faint)' }, PS); lab.style.font = '400 11px "IBM Plex Mono",monospace'; lab.textContent = 'ticks ' + x0 + '–' + (x0 + n - 1) + '   ● order placed';
    if (!hist.length) { var e = el('text', { x: PW / 2, y: PH / 2, 'text-anchor': 'middle', fill: 'var(--muted)' }, PS); e.style.font = '400 14px "IBM Plex Sans",sans-serif'; e.textContent = 'Press Run or Step one tick: each tick is one recalculation.'; }
  }

  reset();
  // Open with a few ticks already run, so the drawing shows a working model at rest.
  for (var i = 0; i < 18; i++) tickSteps().forEach(function (s) { s(); });
  paint(); inspect('Stock');
})();

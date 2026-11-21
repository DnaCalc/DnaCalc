(function () {
  var MONTHS = ['Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun'];
  var CASES = [
    { name: 'A small budget.', cols: 3, rows: ['Units', 'Price', 'Revenue', 'Cost', 'Margin'] },
    { name: 'Quarterly sales.', cols: 4, rows: ['Units', 'Price', 'Revenue', 'Unit cost', 'Cost', 'Margin', 'Margin %'] },
    { name: 'A shop with overheads.', cols: 4, rows: ['Units', 'Price', 'Discount', 'Revenue', 'Unit cost', 'Overhead', 'Cost', 'Profit'] },
    { name: 'Half-year plan.', cols: 6, rows: ['Units', 'Price', 'Revenue', 'Unit cost', 'Cost', 'Margin', 'Margin %'] },
    { name: 'A growth model.', cols: 6, rows: ['Growth', 'Units', 'Price', 'Revenue', 'Unit cost', 'Staff', 'Cost', 'Profit', 'Profit %'] },
    { name: 'The board pack.', cols: 6, rows: ['Units', 'Price', 'Discount', 'Revenue', 'Unit cost', 'Staff', 'Rent', 'Cost', 'Profit', 'Tax', 'Net', 'Net %'] },
    { name: 'Two products.', cols: 6, rows: ['A units', 'A price', 'A revenue', 'B units', 'B price', 'B revenue', 'Revenue', 'Unit cost', 'Cost', 'Profit', 'Profit %'] },
    { name: 'The founder\'s spreadsheet.', cols: 6, rows: ['Growth', 'Units', 'Price', 'Discount', 'A revenue', 'Upsell', 'Revenue', 'Unit cost', 'Staff', 'Rent', 'Cost', 'Profit', 'Tax', 'Net', 'Net %'] }
  ];
  var tbl = document.getElementById('tbl'), svg = document.getElementById('svg'), sheet = document.getElementById('sheet');
  var level = 0, score = 0, model, traces, seen, sel, solved, wrong;

  function col(c) { return String.fromCharCode(66 + c); }
  function ref(r, c) { return col(c) + (r + 2); }
  function isErr(v) { return typeof v === 'string' && v[0] === '#'; }
  function rnd(a, b) { return a + Math.floor(Math.random() * (b - a + 1)); }
  function pick(a) { return a[Math.floor(Math.random() * a.length)]; }

  // A cell is { r, c, input: value } or { r, c, op, args: [[r,c],...], k?: constant }. Formulas only refer to rows above, or to the left.
  function build(spec) {
    var R = spec.rows.length, C = spec.cols, cells = [];
    for (var r = 0; r < R; r++) { cells.push([]); }
    var kind = spec.rows.map(function (name, r) {
      if (/units$|^Units$/.test(name)) return 'units';
      if (/price$|^Price$/.test(name)) return 'price';
      if (name === 'Growth') return 'growth';
      if (name === 'Discount') return 'pct';
      if (/^Unit cost$/.test(name)) return 'unitcost';
      if (/^(Overhead|Staff|Rent)$/.test(name)) return 'fixed';
      return 'formula';
    });
    function rowIndex(name) { return spec.rows.indexOf(name); }
    function above(r, test) { for (var i = r - 1; i >= 0; i--) if (test(spec.rows[i], i)) return i; return -1; }
    for (r = 0; r < R; r++) for (var c = 0; c < C; c++) {
      var name = spec.rows[r], k = kind[r], cell = { r: r, c: c };
      if (k === 'units') cell.input = c > 0 && rowIndex('Growth') >= 0 && rowIndex('Growth') < r ? null : rnd(40, 400);
      else if (k === 'price') cell.input = rnd(5, 60) + pick([0, .5, .25, .99]);
      else if (k === 'growth') cell.input = c === 0 ? pick([.05, .08, .1, .12]) : null;
      else if (k === 'pct') cell.input = pick([0, .05, .1, .15]);
      else if (k === 'unitcost') cell.input = rnd(2, 30) + pick([0, .5]);
      else if (k === 'fixed') cell.input = rnd(5, 40) * 100;
      if (k === 'growth' && c > 0) { delete cell.input; cell.op = 'copy'; cell.args = [[r, c - 1]]; }
      if (k === 'units' && cell.input === null) { var g = rowIndex('Growth'); cell.op = 'grow'; cell.args = [[r, c - 1], [g, c]]; delete cell.input; }
      if (k === 'formula') {
        var u = above(r, function (n) { return /units$|^Units$/.test(n); }), p = above(r, function (n) { return /price$|^Price$/.test(n); });
        if (/revenue$/i.test(name) && name !== 'Revenue' || (name === 'Revenue' && spec.rows.indexOf('A revenue') < 0)) {
          var d = above(r, function (n) { return n === 'Discount'; });
          cell.op = d >= 0 ? 'rev_disc' : 'mul'; cell.args = d >= 0 ? [[u, c], [p, c], [d, c]] : [[u, c], [p, c]];
        } else if (name === 'Revenue') {
          var parts = []; spec.rows.forEach(function (n, i) { if (i < r && /revenue$/.test(n) && n !== 'Revenue') parts.push([i, c]); if (i < r && n === 'Upsell') parts.push([i, c]); });
          cell.op = 'sum'; cell.args = parts;
        } else if (name === 'Upsell') { cell.op = 'mulk'; cell.k = .15; cell.args = [[above(r, function (n) { return /revenue$/.test(n); }), c]]; }
        else if (name === 'Cost' && spec.rows.indexOf('Unit cost') >= 0) {
          var uc = rowIndex('Unit cost'), fixed = []; spec.rows.forEach(function (n, i) { if (i < r && /^(Overhead|Staff|Rent)$/.test(n)) fixed.push([i, c]); });
          var uu = above(uc, function (n) { return /units$|^Units$/.test(n); });
          cell.op = fixed.length ? 'cost_fixed' : 'mul'; cell.args = [[uu, c], [uc, c]].concat(fixed);
        } else if (/^(Margin|Profit)$/.test(name)) { cell.op = 'sub'; cell.args = [[rowIndex('Revenue') >= 0 ? rowIndex('Revenue') : r - 3, c], [rowIndex('Cost'), c]]; }
        else if (name === 'Tax') { cell.op = 'mulk'; cell.k = .28; cell.args = [[rowIndex('Profit'), c]]; }
        else if (name === 'Net') { cell.op = 'sub'; cell.args = [[rowIndex('Profit'), c], [rowIndex('Tax'), c]]; }
        else if (/%$/.test(name)) { var top = rowIndex(name.replace(' %', '')); cell.op = 'div'; cell.args = [[top, c], [rowIndex('Revenue'), c]]; }
        else { cell.op = 'sum'; cell.args = [[r - 1, c]]; }
      }
      cells[r].push(cell);
    }
    return { spec: spec, cells: cells, R: R, C: C };
  }
  function formulaText(m, cell) {
    var a = (cell.args || []).map(function (x) { return ref(x[0], x[1]); });
    switch (cell.op) {
      case 'mul': return '=' + a[0] + '*' + a[1];
      case 'rev_disc': return '=' + a[0] + '*' + a[1] + '*(1-' + a[2] + ')';
      case 'sum': return '=SUM(' + a.join(',') + ')';
      case 'mulk': return '=' + a[0] + '*' + cell.k;
      case 'cost_fixed': return '=' + a[0] + '*' + a[1] + '+' + a.slice(2).join('+');
      case 'sub': return '=' + a[0] + '-' + a[1];
      case 'div': return '=' + a[0] + '/' + a[1];
      case 'grow': return '=ROUND(' + a[0] + '*(1+' + a[1] + '),0)';
      case 'copy': return '=' + a[0];
      case 'na': return '=NA()';
      case 'ref': return '=' + a[0] + '*#REF!';
    }
    return '';
  }
  function num(v) { if (isErr(v)) return v; if (typeof v === 'number') return v; if (v === null || v === undefined || v === '') return 0; var n = Number(v); return isNaN(n) || /^\s*$/.test(v) ? '#VALUE!' : n; }
  function evaluate(m) {
    var val = [];
    for (var r = 0; r < m.R; r++) { val.push([]); for (var c = 0; c < m.C; c++) {
      var cell = m.cells[r][c], v;
      if ('input' in cell) v = cell.input;
      else {
        var xs = cell.args.map(function (x) { return val[x[0]][x[1]]; });
        if (cell.op === 'sum') { v = 0; for (var i = 0; i < xs.length; i++) { if (isErr(xs[i])) { v = xs[i]; break; } if (typeof xs[i] === 'number') v += xs[i]; } }
        else if (cell.op === 'na') v = '#N/A';
        else if (cell.op === 'ref') v = '#REF!';
        else if (cell.op === 'copy') v = isErr(xs[0]) ? xs[0] : xs[0];
        else {
          var n = xs.map(num), e = n.find(isErr);
          if (e) v = e;
          else if (cell.op === 'mul') v = n[0] * n[1];
          else if (cell.op === 'rev_disc') v = n[0] * n[1] * (1 - n[2]);
          else if (cell.op === 'mulk') v = n[0] * cell.k;
          else if (cell.op === 'cost_fixed') v = n[0] * n[1] + n.slice(2).reduce(function (s, x) { return s + x; }, 0);
          else if (cell.op === 'sub') v = n[0] - n[1];
          else if (cell.op === 'div') v = n[1] === 0 ? '#DIV/0!' : n[0] / n[1];
          else if (cell.op === 'grow') v = Math.round(n[0] * (1 + n[1]));
        }
      }
      val[r].push(v);
    } }
    return val;
  }
  function inject(m) {
    var formulas = [], inputs = [];
    m.cells.forEach(function (row) { row.forEach(function (c) { ('input' in c ? inputs : formulas).push(c); }); });
    var kinds = ['text', 'text', 'text', 'text', 'ref', 'ref', 'ref', 'na', 'na', 'na'];
    for (var tries = 0; tries < 60; tries++) {
      var k = pick(kinds), saved = JSON.stringify(m.cells);
      if (k === 'text') { var t = pick(inputs.filter(function (c) { return typeof c.input === 'number' && c.input >= 10; })); if (!t) continue; t.input = String(t.input).replace(/0/, 'O').replace(/^(\d)(\d)/, '$1$2'); if (!/O/.test(t.input)) t.input = String(t.input).replace(/1/, 'l'); if (!/[Ol]/.test(t.input)) { m.cells = JSON.parse(saved); continue; } }
      else if (k === 'zero') { var dv = pick(formulas.filter(function (c) { return c.op === 'div'; })); if (!dv) continue; var den = dv.args[1], src = m.cells[den[0]][den[1]]; var u = m.cells.findIndex(function (row) { return /units$|^Units$/.test(m.spec.rows[row[0].r]); }); if (u < 0) continue; m.cells[u][den[1]].input = 0; if (!('input' in m.cells[u][den[1]])) { m.cells = JSON.parse(saved); continue; } }
      else if (k === 'na') { var f = pick(inputs.filter(function (c) { return m.spec.rows[c.r] !== 'Growth'; })); delete f.input; f.op = 'na'; f.args = []; }
      else { var g = pick(formulas.filter(function (c) { return c.args && c.args.length; })); g.op = 'ref'; g.args = [g.args[0]]; }
      var val = evaluate(m), origin = findOrigin(m, val);
      var spread = 0; val.forEach(function (row) { row.forEach(function (v) { if (isErr(v)) spread++; }); });
      if (origin && spread >= 3) return { val: val, origin: origin, kind: k };
      m.cells = JSON.parse(saved);
    }
    return null;
  }
  function findOrigin(m, val) {
    var found = [];
    for (var r = 0; r < m.R; r++) for (var c = 0; c < m.C; c++) {
      if (!isErr(val[r][c])) continue;
      var cell = m.cells[r][c], clean = (cell.args || []).every(function (x) { return !isErr(val[x[0]][x[1]]); });
      if (clean) found.push(cell);
    }
    return found.length === 1 ? found[0] : null;
  }
  function par(m, val, origin) {
    // Fewest traces: follow error arrows from the bottom-right error cell back to the origin.
    var start = null;
    for (var r = m.R - 1; r >= 0 && !start; r--) for (var c = m.C - 1; c >= 0; c--) if (isErr(val[r][c])) { start = m.cells[r][c]; break; }
    var steps = 1, cur = start;
    while (cur && cur !== origin && steps < 40) { var nxt = (cur.args || []).map(function (x) { return m.cells[x[0]][x[1]]; }).find(function (p) { return isErr(val[p.r][p.c]); }); if (!nxt) break; cur = nxt; steps++; }
    return steps;
  }

  var game;
  function newCase() {
    var spec = CASES[level], m, g;
    for (var i = 0; i < 30 && !g; i++) { m = build(spec); g = inject(m); }
    game = { m: m, val: g.val, origin: g.origin, kind: g.kind, par: par(m, g.val, g.origin) };
    // A text value where a number belongs is the cause; the formula that turns it into #VALUE! is only where it shows.
    game.cause = null;
    if (g.kind === 'text') (g.origin.args || []).forEach(function (x) { var v = g.val[x[0]][x[1]]; if (typeof v === 'string' && !isErr(v)) game.cause = m.cells[x[0]][x[1]]; });
    if (game.cause) game.par += 1;
    traces = 0; seen = {}; sel = null; solved = false; wrong = 0;
    document.getElementById('level').textContent = (level + 1) + ' of ' + CASES.length;
    document.getElementById('caseText').textContent = spec.name;
    document.getElementById('par').textContent = game.par;
    document.getElementById('traces').textContent = 0;
    document.getElementById('msg').textContent = ''; document.getElementById('msg').className = 'msg';
    document.getElementById('accuse').disabled = true; document.getElementById('accuse').hidden = false;
    document.getElementById('next').hidden = true;
    document.getElementById('nameBox').textContent = 'A1';
    document.getElementById('formula').textContent = 'Click a cell with an error to trace where its inputs come from.';
    render();
  }
  function fmt(v) {
    if (isErr(v)) return v;
    if (typeof v === 'string') return v;
    if (v === null || v === undefined) return '';
    if (Math.abs(v) < 1 && v !== 0) return (Math.round(v * 1000) / 10) + '%';
    return Number.isInteger(v) ? v.toLocaleString('en-US') : v.toLocaleString('en-US', { minimumFractionDigits: 2, maximumFractionDigits: 2 });
  }
  function render() {
    var m = game.m, h = '<thead><tr><th class="rowh"></th><th class="lab">A</th>';
    for (var c = 0; c < m.C; c++) h += '<th>' + col(c) + '</th>';
    h += '</tr><tr><th class="rowh">1</th><td class="label"></td>';
    for (c = 0; c < m.C; c++) h += '<td class="label month-label">' + MONTHS[c] + '</td>';
    h += '</tr></thead><tbody>';
    for (var r = 0; r < m.R; r++) {
      h += '<tr><th class="rowh">' + (r + 2) + '</th><td class="label">' + m.spec.rows[r] + '</td>';
      for (c = 0; c < m.C; c++) {
        var v = game.val[r][c], cls = [];
        if (isErr(v)) cls.push('err'); else if (typeof v === 'string') cls.push('txt');
        if (seen[r + ',' + c]) cls.push('seen');
        if (sel && sel.r === r && sel.c === c) cls.push('sel');
        var ans = game.cause || game.origin; if (solved && ans.r === r && ans.c === c) cls.push('found');
        h += '<td tabindex="0" data-r="' + r + '" data-c="' + c + '" class="' + cls.join(' ') + '">' + fmt(v) + '</td>';
      }
      h += '</tr>';
    }
    tbl.innerHTML = h + '</tbody>';
    tbl.querySelectorAll('td[data-r]').forEach(function (td) {
      td.addEventListener('click', function () { choose(Number(td.dataset.r), Number(td.dataset.c)); });
      td.addEventListener('keydown', function (e) { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); choose(Number(td.dataset.r), Number(td.dataset.c)); } });
    });
    arrows();
  }
  function choose(r, c) {
    var cell = game.m.cells[r][c]; sel = cell;
    if (!seen[r + ',' + c]) { seen[r + ',' + c] = true; if (!solved) traces++; }
    document.getElementById('traces').textContent = traces;
    document.getElementById('nameBox').textContent = ref(r, c);
    var v = game.val[r][c];
    document.getElementById('formula').textContent = ('input' in cell ? (typeof cell.input === 'string' ? '\'' + cell.input + '   (text, not a number)' : String(cell.input)) : formulaText(game.m, cell)) + '   → ' + fmt(v);
    document.getElementById('accuse').disabled = solved;
    render();
  }
  function centre(r, c) {
    var td = tbl.querySelector('td[data-r="' + r + '"][data-c="' + c + '"]'); if (!td) return null;
    var a = td.getBoundingClientRect(), b = sheet.getBoundingClientRect();
    return { x: a.left - b.left + a.width / 2, y: a.top - b.top + a.height / 2 };
  }
  function arrows() {
    var cs = getComputedStyle(document.documentElement), blue = cs.getPropertyValue('--arrow').trim(), red = cs.getPropertyValue('--errarrow').trim();
    var out = '<defs><marker id="hb" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="7" markerHeight="7" orient="auto-start-reverse"><path d="M0,0 L10,5 L0,10 z" fill="' + blue + '"/></marker><marker id="hr" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="7" markerHeight="7" orient="auto-start-reverse"><path d="M0,0 L10,5 L0,10 z" fill="' + red + '"/></marker></defs>';
    Object.keys(seen).forEach(function (key) {
      var p = key.split(',').map(Number), cell = game.m.cells[p[0]][p[1]], to = centre(p[0], p[1]); if (!to || !cell.args) return;
      cell.args.forEach(function (x) {
        var from = centre(x[0], x[1]); if (!from) return;
        var e = isErr(game.val[x[0]][x[1]]), colr = e ? red : blue, mk = e ? 'hr' : 'hb';
        var dx = to.x - from.x, dy = to.y - from.y, len = Math.hypot(dx, dy) || 1, ux = dx / len, uy = dy / len;
        var bend = (from.x === to.x) ? 14 : 0;
        var tx = to.x - ux * 12, ty = to.y - uy * 10;
        out += '<circle cx="' + from.x + '" cy="' + from.y + '" r="3.5" fill="' + colr + '"/>';
        out += bend ? '<path d="M' + from.x + ',' + from.y + ' Q' + (from.x + bend + Math.abs(dy) * .12) + ',' + ((from.y + ty) / 2) + ' ' + tx + ',' + ty + '" fill="none" stroke="' + colr + '" stroke-width="1.8" marker-end="url(#' + mk + ')"/>'
          : '<line x1="' + from.x + '" y1="' + from.y + '" x2="' + tx + '" y2="' + ty + '" stroke="' + colr + '" stroke-width="1.8" marker-end="url(#' + mk + ')"/>';
      });
    });
    svg.innerHTML = out;
    svg.setAttribute('width', sheet.clientWidth); svg.setAttribute('height', sheet.clientHeight);
  }
  var EXPLAIN = {
    text: function (o, v) { return 'The cause is the text "' + v + '": a letter where a digit belongs. Arithmetic such as =A*B gives #VALUE! for text it cannot read as a number, while SUM quietly skips text, so some totals look fine.'; },
    zero: function () { return 'A percentage divides by a revenue of zero, so it is #DIV/0!. Zero units is a legal input; the division is where the error is born.'; },
    na: function () { return 'Someone left =NA() as a placeholder for data that never arrived. #N/A then flows into everything downstream.'; },
    ref: function () { return 'This formula refers to a cell that was deleted, so Excel rewrote the reference as #REF!. Every dependent is #REF! too.'; }
  };
  document.getElementById('accuse').addEventListener('click', function () {
    if (!sel || solved) return;
    var msg = document.getElementById('msg');
    if (sel === game.origin || sel === game.cause) {
      solved = true;
      var pts = Math.max(1, 10 - Math.max(0, traces - game.par) - 3 * wrong); score += pts;
      document.getElementById('score').textContent = score;
      var textCell = null; if (game.kind === 'text') (game.origin.args || []).forEach(function (x) { var v = game.val[x[0]][x[1]]; if (typeof v === 'string' && !isErr(v)) textCell = v; });
      var lead = game.cause && sel === game.origin ? 'Close: ' + ref(sel.r, sel.c) + ' is where the error first shows; the cause is the text in ' + ref(game.cause.r, game.cause.c) + '. +' + pts + '. ' : 'Found it: ' + ref(sel.r, sel.c) + '. +' + pts + '. ';
      msg.className = 'msg good'; msg.textContent = lead + EXPLAIN[game.kind](sel, textCell);
      document.getElementById('accuse').hidden = true;
      var nx = document.getElementById('next'); nx.hidden = false; nx.textContent = level + 1 < CASES.length ? 'Next case' : 'See the verdict'; nx.focus();
      render();
    } else {
      wrong++;
      var v = game.val[sel.r][sel.c];
      msg.className = 'msg bad';
      msg.textContent = isErr(v) ? ref(sel.r, sel.c) + ' shows ' + v + ', but it only passes on an error from one of its inputs. Follow the red arrow.' : ref(sel.r, sel.c) + ' has no error. The source is a cell that shows an error while all its own inputs are clean.';
    }
  });
  document.getElementById('next').addEventListener('click', function () {
    if (level + 1 < CASES.length) { level++; newCase(); return; }
    var msg = document.getElementById('msg'); msg.className = 'msg good';
    msg.textContent = 'All eight models fixed. Final score ' + score + ' of 80. ' + (score >= 70 ? 'Auditors will want your number.' : score >= 50 ? 'A solid reviewer.' : 'The models are fixed; your trace count could be shorter.');
    this.textContent = 'Start over'; var self = this;
    this.onclick = function () { self.onclick = null; level = 0; score = 0; document.getElementById('score').textContent = 0; newCase(); };
  });
  addEventListener('resize', arrows);
  new MutationObserver(arrows).observe(document.documentElement, { attributes: true, attributeFilter: ['data-theme'] });
  try { matchMedia('(prefers-color-scheme: dark)').addEventListener('change', arrows); } catch (e) {}
  document.querySelector('.sheetwrap').addEventListener('scroll', arrows);
  newCase();
})();

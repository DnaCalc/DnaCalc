(function () {
  var css = getComputedStyle(document.documentElement);
  function v(n) { return css.getPropertyValue(n).trim(); }
  var C = { exact: v('--exact'), near: v('--near'), mid: v('--mid'), far: v('--far'), bad: v('--bad'), kind: v('--kind'), accent: v('--accent'), ring: v('--ring'), night: v('--night'), line: v('--line'), line2: v('--line2'), ink: v('--ink'), muted: v('--muted'), faint: v('--faint'), panel: v('--panel') };
  var ramp = d3.scaleLinear().domain([0, .25, .5, .75, 1]).range([C.bad, C.far, C.mid, C.near, C.exact]).interpolate(d3.interpolateRgb);
  var fmt = d3.format(','), fmtShort = d3.format('.3~s');
  var CLASS_TEXT = {
    exact: 'Every recorded answer matches Excel bit for bit.',
    last_bit: 'Every miss is in the last bit: one ulp from Excel.',
    numeric: 'Some answers are numerically off; the worst is shown in ulps.',
    structural: 'Some answers differ in kind from Excel: an error where Excel has a number, another type, a refusal or a different array shape.',
    excluded: 'Not judged in this bank.'
  };
  var CLASS_NAME = { exact: 'Exact', last_bit: 'Last bit', numeric: 'Numeric', structural: 'In kind', excluded: 'Excluded' };
  var CLASS_COLOR = { exact: C.exact, last_bit: C.near, numeric: C.far, structural: C.kind, excluded: C.faint };
  var FIX = { COUNT: 'Statistical', SUM: 'Math & trigonometry', IF: 'Logical', FILTER: 'Lookup & reference', UNIQUE: 'Lookup & reference', XLOOKUP: 'Lookup & reference', TEXTBEFORE: 'Text', EUROCONVERT: 'Add-ins', '_EUROTOOL.EUROCONVERT': 'Add-ins' };
  var FUZZ_MAP = (typeof FUZZ !== 'undefined' && FUZZ) ? FUZZ : {};
  function parseFuzz(raw) {
    if (!raw || !raw.length) return null;
    return { swept: raw[0], matched: raw[1], failed: raw[2], regionsCovered: raw[3], regionsTotal: raw[4], frontiers: raw[5], openClusters: raw[6], lastDate: raw[7], sweeps: raw[8] || 1, commit: raw[9] || '-', harnessFaults: raw[10] || 0 };
  }
  var data = BANK.map(function (r) {
    var cat = FIX[r[0]] || r[5] || 'Add-ins';
    if (/^User defined/.test(cat)) cat = 'Add-ins';
    if (cat === 'Cubes') cat = 'Cube';
    return { name: r[0], rows: r[1], matched: r[2], ulp: r[3], cls: r[4], fam: cat, state: r[6], desc: (r[7] || '').replace(/:$/, '.'), notes: r[8] || [], beads: r[9] || [], hist: HIST.fn[r[0]] || [], fuzz: parseFuzz(FUZZ_MAP[r[0]]) };
  });
  var DAYS = HIST.dates, LAST = DAYS.length - 1;
  var state = { color: 'match', size: 'answers', day: LAST, hidden: {}, selected: null, focus: null };

  function rate(rows, matched) { return matched / Math.max(1, rows); }
  function closenessOf(rows, matched) { var miss = 1 - rate(rows, matched); if (miss <= 0) return 1; return 1 - Math.max(0, Math.min(1, (Math.log10(miss) + 5) / 5)); }
  function pct(rows, matched) { var r = rate(rows, matched) * 100; return r === 100 ? '100%' : (r >= 99.99 ? r.toFixed(3) : r.toFixed(2)) + '%'; }
  function ulpText(u) { if (!u) return '-'; if (u < 1e6) return fmt(Math.round(u)) + ' ulp'; var e = Math.floor(Math.log10(u)); return (u / Math.pow(10, e)).toFixed(1) + '×10^' + e + ' ulp'; }
  function dayValue(d, i) { return i === LAST ? [d.rows, d.matched] : d.hist[i]; }
  function fillOf(d) {
    if (state.color === 'ulp' && state.day === LAST) {
      if (d.cls === 'exact') return C.exact;
      if (!d.ulp) return d.cls === 'structural' ? C.kind : C.exact;
      return ramp(.75 - .75 * Math.max(0, Math.min(1, Math.log10(Math.max(1, d.ulp)) / 4.5)));
    }
    var dv = dayValue(d, state.day); if (!dv) return 'transparent';
    return ramp(closenessOf(dv[0], dv[1]));
  }
  function present(d) { return !!dayValue(d, state.day); }
  function visible(d) { return !state.hidden[d.cls]; }

  // ---------- masthead totals and timeline ----------
  var total = d3.sum(data, function (d) { return d.rows; }), matched = d3.sum(data, function (d) { return d.matched; });
  var fuzzedFns = data.filter(function (d) { return d.fuzz && d.fuzz.swept > 0; }).length;
  document.getElementById('totals').innerHTML = [
    [fmt(data.length), 'functions'], [fmt(total), 'Excel answers'], [(matched / total * 100).toFixed(2) + '%', 'matched exactly'],
    [fmt(data.filter(function (d) { return d.cls === 'exact'; }).length), 'with no miss at all'],
    [fmt(fuzzedFns), 'functions fuzzed']
  ].map(function (t) { return '<div><b>' + t[0] + '</b><span>' + t[1] + '</span></div>'; }).join('');
  var TL = d3.select('#timeline'), TW = 560, TH = 210, tm = { l: 46, r: 40, t: 14, b: 30 };
  var tx = d3.scaleBand().domain(d3.range(DAYS.length)).range([tm.l, TW - tm.r]).padding(.22);
  var tyMax = d3.max(HIST.totals, function (t) { return t[0]; });
  var ty = d3.scaleLinear().domain([0, tyMax * 1.05]).range([TH - tm.b, tm.t]);
  var tf = d3.scaleLinear().domain([0, 560]).range([TH - tm.b, tm.t]);
  [0, 1e6, 2e6, 3e6, 4e6].forEach(function (t) { TL.append('line').attr('x1', tm.l).attr('x2', TW - tm.r).attr('y1', ty(t)).attr('y2', ty(t)).attr('stroke', C.line).attr('stroke-width', .7); TL.append('text').attr('x', tm.l - 6).attr('y', ty(t) + 4).attr('text-anchor', 'end').attr('fill', C.faint).style('font', '400 10px "IBM Plex Mono", monospace').text(t ? (t / 1e6) + 'M' : '0'); });
  var tbars = TL.append('g').selectAll('g').data(HIST.totals).join('g').style('cursor', 'pointer').on('click', function (e, t) { setDay(HIST.totals.indexOf(t)); });
  tbars.append('rect').attr('x', function (t, i) { return tx(i); }).attr('width', tx.bandwidth()).attr('y', function (t) { return ty(t[0]); }).attr('height', function (t) { return ty(0) - ty(t[0]); }).attr('fill', C.bad).attr('rx', 2);
  tbars.append('rect').attr('class', 'm').attr('x', function (t, i) { return tx(i); }).attr('width', tx.bandwidth()).attr('y', function (t) { return ty(t[1]); }).attr('height', function (t) { return ty(0) - ty(t[1]); }).attr('fill', C.exact).attr('rx', 2);
  tbars.append('title').text(function (t, i) { return DAYS[i] + ': ' + fmt(t[1]) + ' of ' + fmt(t[0]) + ' answers matched (' + (t[1] / t[0] * 100).toFixed(2) + '%), ' + t[2] + ' functions'; });
  var fline = d3.line().x(function (t, i) { return tx(i) + tx.bandwidth() / 2; }).y(function (t) { return tf(t[2]); }).curve(d3.curveMonotoneX);
  TL.append('path').attr('d', fline(HIST.totals)).attr('fill', 'none').attr('stroke', C.accent).attr('stroke-width', 2);
  TL.append('g').selectAll('circle').data(HIST.totals).join('circle').attr('cx', function (t, i) { return tx(i) + tx.bandwidth() / 2; }).attr('cy', function (t) { return tf(t[2]); }).attr('r', 2.6).attr('fill', C.accent);
  [0, 250, 500].forEach(function (f) { TL.append('text').attr('x', TW - tm.r + 6).attr('y', tf(f) + 4).attr('fill', C.accent).style('font', '400 10px "IBM Plex Mono", monospace').text(f); });
  DAYS.forEach(function (d, i) { if (i % 3 === 0 || i === LAST) TL.append('text').attr('x', tx(i) + tx.bandwidth() / 2).attr('y', TH - 12).attr('text-anchor', 'middle').attr('fill', C.faint).style('font', '400 10px "IBM Plex Mono", monospace').text(d.slice(5)); });
  TL.append('text').attr('x', tm.l).attr('y', 10).attr('fill', C.muted).style('font', '400 10.5px "IBM Plex Sans", sans-serif').text('answers: green matched, red missed · blue line: functions in the bank');
  var tcursor = TL.append('rect').attr('fill', 'none').attr('stroke', C.ink).attr('stroke-width', 1.5).attr('rx', 3);

  // ---------- packed map ----------
  var W = 900, H = 900;
  var famNames = Array.from(d3.group(data, function (d) { return d.fam; }).keys());
  function layout(sizeMode) {
    var groups = d3.group(data, function (d) { return d.fam; });
    var root = d3.hierarchy({ name: 'bank', children: famNames.map(function (f) { return { name: f, children: groups.get(f) }; }) })
      .sum(function (d) { return d.rows === undefined ? 0 : sizeMode === 'equal' ? 1 : Math.sqrt(d.rows); })
      .sort(function (a, b) { return b.value - a.value; });
    d3.pack().size([W, H]).padding(function (n) { return n.depth === 0 ? 20 : 3; })(root);
    return root;
  }
  var root = layout('answers');
  var famByName = {}; root.children.forEach(function (n) { famByName[n.data.name] = n; });
  var leafByName = {}; root.leaves().forEach(function (n) { leafByName[n.data.name] = n; });
  var svg = d3.select('#map').insert('svg', '.scrub').attr('class', 'packed').attr('viewBox', [0, 0, W, H].join(' ')).attr('role', 'img').attr('aria-label', 'Packed circles, one per function, grouped by family');
  var defs = svg.append('defs');
  var glow = defs.append('filter').attr('id', 'glow').attr('x', '-50%').attr('y', '-50%').attr('width', '200%').attr('height', '200%');
  glow.append('feGaussianBlur').attr('stdDeviation', 3.2).attr('result', 'b');
  var mg = glow.append('feMerge'); mg.append('feMergeNode').attr('in', 'b'); mg.append('feMergeNode').attr('in', 'SourceGraphic');
  var g = svg.append('g');
  var famG = g.append('g'), dotG = g.append('g'), labG = g.append('g'), famLabG = g.append('g');
  state.focus = root;
  var view = [root.x, root.y, root.r * 2];
  function famStats(name) { var fs = data.filter(function (d) { return d.fam === name; }); var rr = d3.sum(fs, function (d) { return d.rows; }), mm = d3.sum(fs, function (d) { return d.matched; }); return { n: fs.length, rows: rr, matched: mm }; }
  var famCircle = famG.selectAll('circle').data(root.children, function (d) { return d.data.name; }).join('circle')
    .attr('fill', 'rgba(143,180,255,0.045)').attr('stroke', C.line2).attr('stroke-width', 1.2)
    .style('cursor', 'zoom-in').on('click', function (e, d) { e.stopPropagation(); zoomTo(state.focus === d ? root : d); });
  var dot = dotG.selectAll('circle').data(root.leaves(), function (d) { return d.data.name; }).join('circle')
    .style('cursor', 'pointer')
    .on('mouseenter', function (e, d) { tip(e, d.data); hover(d.data.name); })
    .on('mousemove', moveTip)
    .on('mouseleave', function () { tipEl.hidden = true; hover(null); })
    .on('click', function (e, d) { e.stopPropagation(); select(d.data); if (state.focus === root) zoomTo(famByName[d.data.fam]); });
  var fnLabel = labG.selectAll('text').data(root.leaves(), function (d) { return d.data.name; }).join('text')
    .attr('text-anchor', 'middle').attr('dy', '0.35em').attr('pointer-events', 'none')
    .style('font-family', '"IBM Plex Mono", Consolas, monospace').style('font-weight', 600).attr('fill', '#0e1322').text(function (d) { return d.data.name; });
  var famLabel = famLabG.selectAll('g').data(root.children, function (d) { return d.data.name; }).join('g').attr('pointer-events', 'none');
  famLabel.append('text').attr('class', 'n').attr('text-anchor', 'middle').style('font-family', '"Bricolage Grotesque", system-ui, sans-serif').style('font-weight', 800).attr('fill', C.ink)
    .attr('paint-order', 'stroke').attr('stroke', 'rgba(14,19,34,.9)').attr('stroke-width', 5).attr('stroke-linejoin', 'round').text(function (d) { return d.data.name; });
  famLabel.append('text').attr('class', 'c').attr('text-anchor', 'middle').style('font', '400 12px "IBM Plex Mono", monospace').attr('fill', C.muted)
    .attr('paint-order', 'stroke').attr('stroke', 'rgba(14,19,34,.9)').attr('stroke-width', 4)
    .text(function (d) { var s = famStats(d.data.name); return s.n + ' functions · ' + (s.matched / s.rows * 100).toFixed(2) + '%'; });
  svg.on('click', function () { zoomTo(root); });
  document.getElementById('up').addEventListener('click', function () { zoomTo(root); });

  function styleDots() {
    dot.attr('fill', function (d) { return fillOf(d.data); })
      .attr('fill-opacity', function (d) { return visible(d.data) ? .93 : .06; })
      .attr('stroke', function (d) {
        if (state.selected === d.data) return C.accent;
        if (!present(d.data)) return C.faint;
        return d.data.cls === 'exact' || d.data.cls === 'last_bit' ? C.ring : C.night;
      })
      .attr('stroke-width', function (d) { return state.selected === d.data ? 4 : !present(d.data) ? 1 : d.data.cls === 'structural' ? 2.4 : d.data.cls === 'exact' ? 1.3 : 1; })
      .attr('stroke-dasharray', function (d) { return !present(d.data) ? '3 3' : d.data.cls === 'last_bit' ? '2 2' : null; })
      .attr('stroke-opacity', function (d) { return visible(d.data) ? .9 : .1; })
      .attr('filter', function (d) { return state.day === LAST && d.data.cls === 'exact' && d.r > 9 && visible(d.data) ? 'url(#glow)' : null; })
      .style('pointer-events', function (d) { return visible(d.data) ? null : 'none'; });
    fnLabel.attr('fill-opacity', function (d) { return visible(d.data) && present(d.data) ? 1 : .2; });
  }
  function apply(vw) {
    view = vw; var k = W / vw[2], top = state.focus === root;
    function px(d) { return (d.x - vw[0]) * k + W / 2; }
    function py(d) { return (d.y - vw[1]) * k + H / 2; }
    famCircle.attr('cx', px).attr('cy', py).attr('r', function (d) { return d.r * k; });
    dot.attr('cx', px).attr('cy', py).attr('r', function (d) { return Math.max(.8, d.r * k); });
    fnLabel.attr('x', px).attr('y', py)
      .style('font-size', function (d) { return Math.max(7, Math.min(16, 2 * d.r * k / Math.max(3, d.data.name.length * .62))) + 'px'; })
      .attr('opacity', function (d) { return d.r * k > 8 + d.data.name.length * 2.2 ? 1 : 0; });
    famLabel.select('text.n').attr('x', px).attr('y', function (d) { return top ? (d.r > 70 ? py(d) : py(d) + d.r * k + 15) : py(d) - d.r * k + 36; })
      .style('font-size', function (d) { return top ? (d.r > 70 ? 21 : 13) + 'px' : '32px'; })
      .attr('opacity', function (d) { return top || d === state.focus ? 1 : 0; });
    famLabel.select('text.c').attr('x', px).attr('y', function (d) { return top ? py(d) + 20 : py(d) - d.r * k + 58; })
      .attr('opacity', function (d) { return top ? (d.r > 90 ? 1 : 0) : (d === state.focus ? 1 : 0); });
  }
  function zoomTo(d) {
    state.focus = d;
    document.getElementById('crumb').textContent = d === root ? 'All families' : d.data.name;
    document.getElementById('up').hidden = d === root;
    svg.style('cursor', d === root ? 'zoom-in' : 'zoom-out');
    var target = [d.x, d.y, d.r * 2 + (d === root ? 0 : 34)];
    if (matchMedia('(prefers-reduced-motion: reduce)').matches) return apply(target);
    var i = d3.interpolateZoom(view, target);
    svg.transition().duration(750).ease(d3.easeCubicInOut).tween('zoom', function () { return function (t) { apply(i(t)); }; });
  }
  function relayout(mode) {
    var next = layout(mode), from = {}, to = {};
    root.descendants().forEach(function (n) { from[n.depth === 2 ? n.data.name : n.depth === 1 ? 'fam:' + n.data.name : 'root'] = [n.x, n.y, n.r]; });
    next.descendants().forEach(function (n) { to[n.depth === 2 ? n.data.name : n.depth === 1 ? 'fam:' + n.data.name : 'root'] = [n.x, n.y, n.r]; });
    var nodes = root.descendants();
    function key(n) { return n.depth === 2 ? n.data.name : n.depth === 1 ? 'fam:' + n.data.name : 'root'; }
    var focusKey = key(state.focus);
    svg.transition().duration(900).ease(d3.easeCubicInOut).tween('relayout', function () {
      return function (t) {
        nodes.forEach(function (n) { var a = from[key(n)], b = to[key(n)]; n.x = a[0] + (b[0] - a[0]) * t; n.y = a[1] + (b[1] - a[1]) * t; n.r = a[2] + (b[2] - a[2]) * t; });
        var f = state.focus; apply([f.x, f.y, f.r * 2 + (f === root ? 0 : 34)]);
      };
    });
  }
  styleDots(); apply(view);

  // ---------- controls ----------
  function seg(id, key, after) {
    document.querySelectorAll('#' + id + ' button').forEach(function (b) {
      b.addEventListener('click', function () {
        state[key] = b.dataset.m;
        document.querySelectorAll('#' + id + ' button').forEach(function (x) { x.setAttribute('aria-pressed', String(x === b)); });
        after();
      });
    });
  }
  seg('colorMode', 'color', function () { if (state.color === 'ulp' && state.day !== LAST) setDay(LAST); styleDots(); styleLadder(); legend(); });
  seg('sizeMode', 'size', function () { relayout(state.size); });
  var counts = d3.rollup(data, function (v) { return v.length; }, function (d) { return d.cls; });
  renderChartMarkup(document.getElementById('chips'), ['exact', 'last_bit', 'numeric', 'structural', 'excluded'].filter(function (c) { return counts.get(c); }).map(function (c) {
    return '<button type="button" class="chip" data-c="' + c + '" aria-pressed="true"><i data-background="' + CLASS_COLOR[c] + '"></i>' + CLASS_NAME[c] + ' ' + counts.get(c) + '</button>';
  }).join(''));
  document.querySelectorAll('#chips .chip').forEach(function (b) {
    b.addEventListener('click', function () { var c = b.dataset.c; state.hidden[c] = !state.hidden[c]; b.setAttribute('aria-pressed', String(!state.hidden[c])); styleDots(); styleLadder(); });
  });
  document.getElementById('names').innerHTML = data.map(function (d) { return '<option value="' + d.name + '">'; }).join('');
  document.getElementById('search').addEventListener('input', function () {
    var q = this.value.trim().toUpperCase(); if (!q) { hover(null); return; }
    var hit = data.find(function (d) { return d.name === q; }) || data.find(function (d) { return d.name.indexOf(q) === 0; });
    if (hit) { select(hit); hover(hit.name); var f = famByName[hit.fam]; if (state.focus !== f) zoomTo(f); }
  });

  // ---------- time scrubber ----------
  var dayInput = document.getElementById('day'), playBtn = document.getElementById('play'), timer = null;
  dayInput.max = LAST; dayInput.value = LAST;
  function setDay(i) {
    state.day = i; dayInput.value = i;
    if (i !== LAST && state.color === 'ulp') { state.color = 'match'; document.querySelectorAll('#colorMode button').forEach(function (x) { x.setAttribute('aria-pressed', String(x.dataset.m === 'match')); }); legend(); }
    var t = HIST.totals[i];
    document.getElementById('when').textContent = DAYS[i] + (i === LAST ? ' (today)' : '');
    document.getElementById('what').textContent = t[2] + ' functions · ' + fmt(t[0]) + ' answers · ' + (t[1] / t[0] * 100).toFixed(2) + '% matched';
    tcursor.attr('x', tx(i) - 3).attr('y', tm.t - 2).attr('width', tx.bandwidth() + 6).attr('height', TH - tm.b - tm.t + 4);
    styleDots(); if (state.selected) select(state.selected, true);
  }
  dayInput.addEventListener('input', function () { stop(); setDay(Number(this.value)); });
  function stop() { if (timer) { clearInterval(timer); timer = null; playBtn.textContent = '▶ Play'; } }
  playBtn.addEventListener('click', function () {
    if (timer) return stop();
    var i = state.day >= LAST ? 0 : state.day; setDay(i); playBtn.textContent = '❚❚ Pause';
    timer = setInterval(function () { if (state.day >= LAST) return stop(); setDay(state.day + 1); }, 650);
  });

  // ---------- detail card ----------
  var detail = document.getElementById('detail');
  function sparkline(d) {
    var pts = DAYS.map(function (day, i) { var dv = dayValue(d, i); return dv ? { i: i, rows: dv[0], rate: rate(dv[0], dv[1]) } : null; });
    var have = pts.filter(Boolean); if (have.length < 2) return '<div class="spark"><div class="cap"><span>History</span><span>in the bank since ' + DAYS[have.length ? have[0].i : LAST] + '</span></div></div>';
    var SW = 300, SH = 86, x = d3.scaleLinear().domain([0, LAST]).range([4, SW - 4]);
    var minR = d3.min(have, function (p) { return p.rate; }); var lo = Math.min(.9, Math.floor(minR * 20) / 20);
    var y = d3.scaleLinear().domain([lo, 1]).range([SH - 18, 6]);
    var yr = d3.scaleLinear().domain([0, d3.max(have, function (p) { return p.rows; })]).range([SH - 18, 40]);
    var bars = have.map(function (p) { return '<rect x="' + (x(p.i) - 4) + '" y="' + yr(p.rows) + '" width="8" height="' + (SH - 18 - yr(p.rows)) + '" fill="' + C.line2 + '"/>'; }).join('');
    var line = d3.line().x(function (p) { return x(p.i); }).y(function (p) { return y(p.rate); })(have);
    var dots = have.map(function (p) { return '<circle cx="' + x(p.i) + '" cy="' + y(p.rate) + '" r="' + (p.i === state.day ? 4.5 : 2.6) + '" fill="' + ramp(closenessOf(1e6, p.rate * 1e6)) + '" stroke="' + (p.i === state.day ? C.ink : 'none') + '"/>'; }).join('');
    return '<div class="spark"><svg viewBox="0 0 ' + SW + ' ' + SH + '" role="img" aria-label="Share matched per day">' + bars +
      '<line x1="4" x2="' + (SW - 4) + '" y1="' + y(1) + '" y2="' + y(1) + '" stroke="' + C.exact + '" stroke-dasharray="3 3" stroke-width=".8"/>' +
      '<path d="' + line + '" fill="none" stroke="' + C.ink + '" stroke-width="1.6"/>' + dots +
      '<text x="4" y="' + (SH - 4) + '" fill="' + C.faint + '" font-size="10" font-family="IBM Plex Mono, monospace">' + DAYS[have[0].i].slice(5) + '</text>' +
      '<text x="' + (SW - 4) + '" y="' + (SH - 4) + '" text-anchor="end" fill="' + C.faint + '" font-size="10" font-family="IBM Plex Mono, monospace">' + DAYS[LAST].slice(5) + '</text>' +
      '<text x="' + (SW - 4) + '" y="' + (y(1) - 3) + '" text-anchor="end" fill="' + C.exact + '" font-size="10" font-family="IBM Plex Mono, monospace">100%</text>' +
      '<text x="' + (SW - 4) + '" y="' + (y(lo) + 11) + '" text-anchor="end" fill="' + C.faint + '" font-size="10" font-family="IBM Plex Mono, monospace">' + Math.round(lo * 100) + '%</text></svg>' +
      '<div class="cap"><span>share matched (line) · answers (bars)</span><span>' + have.length + ' days</span></div></div>';
  }
  function esc(s) { return String(s).replace(/&/g, '&amp;').replace(/</g, '&lt;'); }
  function select(d, keepScroll) {
    state.selected = d;
    var dv = dayValue(d, state.day), rows = dv ? dv[0] : 0, m = dv ? dv[1] : 0;
    var h = '<h2>Function' + (state.day !== LAST ? ' · ' + DAYS[state.day] : '') + '</h2><div class="fn">' + esc(d.name) + '</div><div class="fam">' + esc(d.fam) + '</div>';
    if (d.desc) h += '<p class="desc">' + esc(d.desc) + '</p>';
    h += dv ? '<div class="kv"><span>Excel answers</span><b>' + fmt(rows) + '</b><span>matched</span><b>' + fmt(m) + '</b><span>missed</span><b>' + fmt(rows - m) + '</b><span>share matched</span><b>' + pct(rows, m) + '</b>' + (state.day === LAST ? '<span>worst miss</span><b>' + ulpText(d.ulp) + '</b>' : '') + '</div>'
      : '<p class="desc chart-muted">Not yet in the bank on ' + DAYS[state.day] + '.</p>';
    if (state.day === LAST) h += '<p class="state">' + esc(CLASS_TEXT[d.cls]) + '</p>';
    if (d.state) h += '<p class="state">Handbook: <b>' + esc(d.state) + '</b></p>';
    h += '<h2 class="chart-subheading">Fuzz coverage</h2>';
    if (d.fuzz && d.fuzz.swept > 0) {
      h += '<div class="kv"><span>cases swept</span><b>' + fmt(d.fuzz.swept) + '</b><span>matched / failed</span><b>' + fmt(d.fuzz.matched) + ' / ' + fmt(d.fuzz.failed) + '</b><span>harness faults</span><b>' + fmt(d.fuzz.harnessFaults) + '</b><span>regions covered</span><b>' + fmt(d.fuzz.regionsCovered) + ' / ' + fmt(d.fuzz.regionsTotal) + '</b><span>frontiers mapped</span><b>' + fmt(d.fuzz.frontiers) + '</b><span>open clusters</span><b>' + fmt(d.fuzz.openClusters) + '</b><span>last sweep</span><b>' + esc(d.fuzz.lastDate) + '</b></div>';
    } else {
      h += '<p class="desc chart-muted">No live fuzz sweep recorded yet.</p>';
    }
    h += sparkline(d);
    if (d.notes.length) h += '<h2 class="chart-subheading">Measured notes</h2><ul class="list">' + d.notes.map(function (n) { return '<li><code>' + esc(n[0]) + '</code>' + esc(n[1]) + '</li>'; }).join('') + '</ul>';
    if (d.beads.length) h += '<h2 class="chart-subheading">Open work</h2><ul class="list beads">' + d.beads.map(function (b) { return '<li><code>' + esc(b[0]) + '</code>' + esc(b[1]) + '</li>'; }).join('') + '</ul>';
    h += '<div class="links"><a href="https://handbook.dnacalc.com/functions/' + encodeURIComponent(d.name) + '.html" target="_blank" rel="noopener">Handbook page</a></div>';
    detail.innerHTML = h;
    styleDots(); styleLadder();
    drawLand(d);
    if (!keepScroll && location.hash.slice(1) !== hashName(d.name)) { try { history.replaceState(null, '', '#' + hashName(d.name)); } catch (e) {} }
  }
  function hashName(n) { return n.replace(/[^A-Za-z0-9]+/g, '-'); }

  // ---------- landscapes: individual answers placed by their arguments ----------
  var LS = d3.select('#land'), LSW = 1200, LSH = 460, lm = { l: 70, r: 24, t: 20, b: 48 };
  function pcolor(c) { return c === 0 ? C.exact : c === -1 ? C.kind : c <= .05 ? C.near : ramp(.7 - .7 * Math.min(1, c / 4.5)); }
  // A symmetric log axis over the middle 96% of the arguments; the few beyond it are pinned to the edges.
  function symlog(values, lo, hi) {
    var sorted = values.filter(function (x) { return x !== null && isFinite(x); }).sort(d3.ascending);
    if (!sorted.length) sorted = [0, 1];
    var a = d3.quantile(sorted, .02), b = d3.quantile(sorted, .98);
    if (a === b) { a = sorted[0]; b = sorted[sorted.length - 1]; }
    if (a === b) { a -= 1; b += 1; }
    var abs = sorted.map(Math.abs).filter(function (x) { return x > 0; }).sort(d3.ascending);
    var span = Math.max(Math.abs(a), Math.abs(b));
    var k = abs.length ? Math.max(span * 1e-6, Math.min(span / 10, d3.quantile(abs, .1))) : 1;
    return d3.scaleSymlog().constant(k || 1).domain([a, b]).range([lo, hi]).clamp(true);
  }
  function statusText(c) { return c === 0 ? 'matches Excel exactly' : c === -1 ? 'differs in kind (error, type or shape)' : c <= .05 ? '1 ulp from Excel' : '≈' + fmt(Math.round(Math.pow(10, c))) + ' ulp from Excel'; }
  function axisTicks(sc) {
    var dom = sc.domain(), r = sc.range(), px = Math.abs(r[1] - r[0]);
    var cand = [dom[0], dom[1], 0].concat(sc.ticks ? sc.ticks(9) : []);
    [-1e12, -1e9, -1e6, -1e3, -100, -10, -1, -.1, -.01, -1e-3, -1e-6, 1e-6, 1e-3, .01, .1, .5, 1, 10, 100, 1e3, 1e6, 1e9, 1e12].forEach(function (t) { cand.push(t); });
    cand = cand.filter(function (t) { return t >= dom[0] && t <= dom[1]; }).sort(d3.ascending);
    var out = [], lastPx = -1e9;
    cand.forEach(function (t) { var p = sc(t); if (Math.abs(p - lastPx) > px / 12) { out.push(t); lastPx = p; } });
    return out;
  }
  function shortNum(x) { var a = Math.abs(x); return a !== 0 && (a < 1e-3 || a >= 1e6) ? x.toExponential(1).replace('e+', 'e') : d3.format('~g')(+x.toPrecision(4)); }
  var landAll = false;
  document.getElementById('landAll').addEventListener('click', function () { landAll = !landAll; if (state.selected) drawLand(state.selected); });
  function drawLand(d) {
    var L = LAND[d.name], sig = SIG[d.name] || [];
    var title = document.getElementById('landTitle'), text = document.getElementById('landText');
    LS.selectAll('*').remove();
    document.querySelectorAll('#gallery .mini').forEach(function (b) { b.setAttribute('aria-current', String(b.dataset.n === d.name)); });
    title.innerHTML = esc(d.name) + '<small>(' + esc(sig.join(', ')) + ')</small>';
    if (!L || !L.p.length) {
      text.textContent = 'This function\'s answers have no numeric argument to place them by (text, arrays or references), so there is no landscape to draw.';
      LS.append('text').attr('x', LSW / 2).attr('y', LSH / 2).attr('text-anchor', 'middle').attr('fill', C.faint).style('font', '400 15px "IBM Plex Sans", sans-serif').text('No numeric arguments to plot.');
      return;
    }
    var two = L.a[1] !== null && L.a[1] !== undefined && L.p[0].length === 3;
    var a1 = sig[L.a[0]] || ('argument ' + (L.a[0] + 1)), a2 = two ? (sig[L.a[1]] || ('argument ' + (L.a[1] + 1))) : null;
    var shown = L.p.length, missShown = L.p.filter(function (p) { return p[p.length - 1] !== 0; }).length;
    text.textContent = 'A sample of ' + fmt(shown) + ' of its ' + fmt(L.n) + ' recorded answers (' + fmt(L.miss) + ' misses in all), misses first, placed by ' + (two ? a1 + ' across and ' + a2 + ' up' : a1 + ' across and the size of the miss up') + '. The argument axes stretch near zero (a symmetric log scale) and span the middle 96% of the arguments; the rest sit on the edges. Hover a dot for its exact arguments.';
    // By default the axes zoom to where the misses are: fuzzed arguments far outside a function's domain would
    // otherwise squeeze every interesting answer into one column.
    var missPts = L.p.filter(function (p) { return p[p.length - 1] !== 0; });
    var zoomed = !landAll && missPts.length >= 5;
    var basis = zoomed ? missPts : L.p;
    var xs = basis.map(function (p) { return p[0]; });
    var x = symlog(xs, lm.l, LSW - lm.r);
    var btn = document.getElementById('landAll');
    btn.hidden = missPts.length < 5; btn.textContent = landAll ? 'Zoom to the misses' : 'Show all arguments';
    var outside = L.p.filter(function (p) { var dom = x.domain(); return p[0] < dom[0] || p[0] > dom[1]; }).length;
    if (zoomed && outside) text.textContent += ' Zoomed to the misses: ' + fmt(outside) + ' answers lie outside this range and sit on the edges.';
    var g = LS.append('g');
    function xAxis() {
      axisTicks(x).forEach(function (t) { g.append('line').attr('x1', x(t)).attr('x2', x(t)).attr('y1', lm.t).attr('y2', LSH - lm.b).attr('stroke', C.line).attr('stroke-width', .6); g.append('text').attr('x', x(t)).attr('y', LSH - lm.b + 16).attr('text-anchor', 'middle').attr('fill', C.faint).style('font', '400 11px "IBM Plex Mono", monospace').text(shortNum(t)); });
      g.append('text').attr('x', (lm.l + LSW - lm.r) / 2).attr('y', LSH - 10).attr('text-anchor', 'middle').attr('fill', C.muted).style('font', '600 12px "IBM Plex Sans", sans-serif').text(a1);
    }
    xAxis();
    var y, yOf;
    if (two) {
      var ys = basis.map(function (p) { return p[1]; }).filter(function (v) { return v !== null; });
      y = symlog(ys.length ? ys : [0, 1], LSH - lm.b, lm.t);
      axisTicks(y).forEach(function (t) { g.append('line').attr('x1', lm.l).attr('x2', LSW - lm.r).attr('y1', y(t)).attr('y2', y(t)).attr('stroke', C.line).attr('stroke-width', .6); g.append('text').attr('x', lm.l - 8).attr('y', y(t) + 4).attr('text-anchor', 'end').attr('fill', C.faint).style('font', '400 11px "IBM Plex Mono", monospace').text(shortNum(t)); });
      g.append('text').attr('transform', 'translate(16,' + ((lm.t + LSH - lm.b) / 2) + ') rotate(-90)').attr('text-anchor', 'middle').attr('fill', C.muted).style('font', '600 12px "IBM Plex Sans", sans-serif').text(a2);
      yOf = function (p) { return p[1] === null ? LSH - lm.b + 4 : y(p[1]); };
    } else {
      var top = lm.t, kindY = top + 14, ulpTop = top + 40, ulpBot = LSH - lm.b - 46, exactY = LSH - lm.b - 16;
      var maxC = Math.max(1, d3.max(L.p, function (p) { return p[1]; }) || 1);
      y = d3.scaleLinear().domain([0, Math.ceil(maxC)]).range([ulpBot, ulpTop]);
      g.append('rect').attr('x', lm.l).attr('width', LSW - lm.r - lm.l).attr('y', kindY - 12).attr('height', 24).attr('fill', 'rgba(185,156,255,.07)');
      g.append('rect').attr('x', lm.l).attr('width', LSW - lm.r - lm.l).attr('y', exactY - 12).attr('height', 24).attr('fill', 'rgba(99,190,123,.07)');
      g.append('text').attr('x', lm.l - 8).attr('y', kindY + 4).attr('text-anchor', 'end').attr('fill', C.kind).style('font', '600 11px "IBM Plex Mono", monospace').text('in kind');
      g.append('text').attr('x', lm.l - 8).attr('y', exactY + 4).attr('text-anchor', 'end').attr('fill', C.exact).style('font', '600 11px "IBM Plex Mono", monospace').text('exact');
      d3.range(0, Math.ceil(maxC) + 1).forEach(function (e) { if (Math.ceil(maxC) > 8 && e % 2) return; g.append('line').attr('x1', lm.l).attr('x2', LSW - lm.r).attr('y1', y(e)).attr('y2', y(e)).attr('stroke', C.line).attr('stroke-width', .6); g.append('text').attr('x', lm.l - 8).attr('y', y(e) + 4).attr('text-anchor', 'end').attr('fill', C.faint).style('font', '400 11px "IBM Plex Mono", monospace').text(e === 0 ? '1 ulp' : '10^' + e); });
      if (maxC >= 4) g.append('line').attr('x1', lm.l).attr('x2', LSW - lm.r).attr('y1', y(4)).attr('y2', y(4)).attr('stroke', C.bad).attr('stroke-dasharray', '4 3');
      var jit = 0;
      yOf = function (p) { jit = (jit * 9301 + 49297) % 233280; var j = (jit / 233280 - .5) * 16; return p[1] === 0 ? exactY + j : p[1] === -1 ? kindY + j : y(Math.max(0, p[1])); };
    }
    var pts = L.p.slice().sort(function (a, b) { return (a[a.length - 1] === 0 ? 0 : 1) - (b[b.length - 1] === 0 ? 0 : 1); });
    g.append('g').selectAll('circle').data(pts).join('circle')
      .attr('cx', function (p) { return x(p[0]); }).attr('cy', yOf)
      .attr('r', function (p) { return p[p.length - 1] === 0 ? 3 : 4.2; })
      .attr('fill', function (p) { return pcolor(p[p.length - 1]); })
      .attr('fill-opacity', function (p) { return p[p.length - 1] === 0 ? .55 : .92; })
      .attr('stroke', C.night).attr('stroke-width', .6)
      .on('mouseenter', function (e, p) {
        var c = p[p.length - 1];
        tipEl.innerHTML = '<b>' + esc(d.name) + '</b><br>' + esc(a1) + ' = ' + p[0] + (two ? '<br>' + esc(a2) + ' = ' + (p[1] === null ? '(not a number)' : p[1]) : '') + '<br>' + statusText(c);
        tipEl.hidden = false; moveTip(e);
      })
      .on('mousemove', moveTip).on('mouseleave', function () { tipEl.hidden = true; });
    renderChartMarkup(document.getElementById('landKey'), [[C.exact, 'exact'], [C.near, '1 ulp'], [ramp(.4), 'tens to hundreds of ulp'], [C.bad, '10,000+ ulp'], [C.kind, 'differs in kind']].map(function (k) { return '<span><i data-background="' + k[0] + '"></i>' + k[1] + '</span>'; }).join(''));
  }
  function miniLand(name) {
    var L = LAND[name]; if (!L) return '';
    var MW = 176, MH = 92, two = L.p[0] && L.p[0].length === 3;
    var mp = L.p.filter(function (p) { return p[p.length - 1] !== 0; }), basis = mp.length >= 5 ? mp : L.p;
    var x = symlog(basis.map(function (p) { return p[0]; }), 4, MW - 4), y;
    if (two) { var ys = basis.map(function (p) { return p[1]; }).filter(function (v) { return v !== null; }); y = symlog(ys.length ? ys : [0, 1], MH - 4, 4); }
    else { var maxC = Math.max(1, d3.max(L.p, function (p) { return p[1]; }) || 1); y = d3.scaleLinear().domain([0, maxC]).range([MH - 18, 14]); }
    var dots = L.p.map(function (p) {
      var c = p[p.length - 1], cy = two ? (p[1] === null ? MH - 3 : y(p[1])) : (c === 0 ? MH - 6 : c === -1 ? 5 : y(Math.max(0, c)));
      return '<circle cx="' + x(p[0]).toFixed(1) + '" cy="' + cy.toFixed(1) + '" r="' + (c === 0 ? 1.4 : 2) + '" fill="' + pcolor(c) + '" fill-opacity="' + (c === 0 ? .45 : .9) + '"/>';
    }).join('');
    return '<svg viewBox="0 0 ' + MW + ' ' + MH + '" aria-hidden="true">' + dots + '</svg>';
  }
  // Numeric misses make the telling pictures; refusals and differences in kind make flat bands.
  function numericMisses(n) { return LAND[n].p.filter(function (p) { return p[p.length - 1] > 0; }).length; }
  var galleryNames = Object.keys(LAND).filter(function (n) { return LAND[n].p.length > 40 && numericMisses(n) >= 12; })
    .sort(function (a, b) { return numericMisses(b) - numericMisses(a) || LAND[b].miss - LAND[a].miss; }).slice(0, 18);
  document.getElementById('gallery').innerHTML = galleryNames.map(function (n) {
    var L = LAND[n];
    return '<button type="button" class="mini" data-n="' + esc(n) + '"><b>' + esc(n) + '</b><span>' + fmt(L.miss) + ' misses of ' + fmt(L.n) + '</span>' + miniLand(n) + '</button>';
  }).join('');
  document.querySelectorAll('#gallery .mini').forEach(function (b) {
    b.addEventListener('click', function () { var d = data.find(function (x) { return x.name === b.dataset.n; }); if (!d) return; select(d); var f = famByName[d.fam]; if (state.focus !== f) zoomTo(f); });
  });

  // ---------- legend ----------
  function legend() {
    var el = document.getElementById('legend');
    var scale = state.color === 'match'
      ? '<div class="scale" data-background="linear-gradient(90deg,' + C.bad + ',' + C.far + ' 25%,' + C.mid + ' 50%,' + C.near + ' 75%,' + C.exact + ')"></div><div class="scale-l"><span>under 90%</span><span>99%</span><span>99.9%</span><span>all exact</span></div><div>Colour: share of the function\'s Excel answers that DNA Calc reproduces bit for bit.</div>'
      : '<div class="scale" data-background="linear-gradient(90deg,' + C.exact + ',' + C.near + ' 22%,' + C.mid + ' 45%,' + C.far + ' 70%,' + C.bad + ')"></div><div class="scale-l"><span>exact</span><span>1 ulp</span><span>100</span><span>30,000+ ulp</span></div><div>Colour: the function\'s worst numeric miss. <span data-color="' + C.kind + '">Violet</span>: misses only in kind (an error, a type, a shape), no numeric miss.</div>';
    renderChartMarkup(el, '<h2>Reading the map</h2>' + scale +
      '<div class="rings"><div><i class="x"></i>every answer bit-exact</div><div><i class="l"></i>misses only in the last bit</div><div><i class="s"></i>some answers differ in kind</div><div><i class="g"></i>not yet in the bank on the chosen day</div></div>' +
      '<div>Circle area follows the square root of the answer count, so small functions stay visible next to the giants. Use Size: Equal to give every function the same weight.</div>');
  }
  legend();

  // ---------- family ledger ----------
  var fams = famNames.map(function (f) {
    var fs = data.filter(function (d) { return d.fam === f; });
    var c = {}; fs.forEach(function (d) { c[d.cls] = (c[d.cls] || 0) + 1; });
    return { name: f, n: fs.length, rows: d3.sum(fs, function (d) { return d.rows; }), matched: d3.sum(fs, function (d) { return d.matched; }), c: c, worst: d3.max(fs, function (d) { return d.ulp || 0; }) };
  }).sort(function (a, b) { return b.rows - a.rows; });
  renderChartMarkup(document.getElementById('ledger'), '<table><thead><tr><th>Family</th><th class="numeric">Functions</th><th class="numeric">Excel answers</th><th class="numeric">Matched</th><th>Functions by standing</th><th class="numeric">Worst miss</th></tr></thead><tbody>' +
    fams.map(function (f) {
      var stack = ['exact', 'last_bit', 'numeric', 'structural', 'excluded'].map(function (c) { return f.c[c] ? '<i title="' + CLASS_NAME[c] + ': ' + f.c[c] + '" data-width="' + (f.c[c] / f.n * 100) + '%" data-background="' + CLASS_COLOR[c] + '"></i>' : ''; }).join('');
      return '<tr data-f="' + esc(f.name) + '"><td>' + esc(f.name) + '</td><td class="n">' + f.n + '</td><td class="n">' + fmt(f.rows) + '</td><td class="n" data-color="' + ramp(closenessOf(f.rows, f.matched)) + '">' + pct(f.rows, f.matched) + '</td><td><div class="stack">' + stack + '</div></td><td class="n">' + ulpText(f.worst) + '</td></tr>';
    }).join('') + '</tbody></table>');
  document.querySelectorAll('#ledger tr[data-f]').forEach(function (tr) { tr.addEventListener('click', function () { zoomTo(famByName[tr.dataset.f]); document.getElementById('map').scrollIntoView({ behavior: 'smooth', block: 'start' }); }); });

  // ---------- ulp ladder ----------
  var L = d3.select('#ladder'), LW = 1240, LH = 270, x0 = 176, x1 = LW - 24, maxExp = 19;
  var lx = d3.scaleLinear().domain([0, maxExp]).range([x0, x1]);
  var axis = L.append('g');
  for (var e = 0; e <= maxExp; e++) {
    axis.append('line').attr('x1', lx(e)).attr('x2', lx(e)).attr('y1', 24).attr('y2', LH - 34).attr('stroke', C.line).attr('stroke-width', e % 3 === 0 ? 1 : .5);
    if (e % 3 === 0) axis.append('text').attr('x', lx(e)).attr('y', LH - 14).attr('text-anchor', 'middle').attr('fill', C.faint).style('font', '400 12px "IBM Plex Mono", monospace').text(e === 0 ? '1 ulp' : '10^' + e);
  }
  axis.append('line').attr('x1', x0 - 22).attr('x2', x0 - 22).attr('y1', 16).attr('y2', LH - 30).attr('stroke', C.muted).attr('stroke-dasharray', '3 3');
  axis.append('text').attr('x', 14).attr('y', LH - 14).attr('fill', C.muted).style('font', '600 12px "IBM Plex Sans", sans-serif').text('no numeric miss');
  axis.append('line').attr('x1', lx(4)).attr('x2', lx(4)).attr('y1', 24).attr('y2', LH - 34).attr('stroke', C.bad).attr('stroke-width', 1.3).attr('stroke-dasharray', '4 3');
  axis.append('text').attr('x', lx(4) + 6).attr('y', 17).attr('fill', C.bad).style('font', '400 11px "IBM Plex Mono", monospace').text('10,000 ulp');
  var seed = 11; function rnd() { seed = (seed * 16807) % 2147483647; return seed / 2147483647; }
  var pts = data.map(function (d) { var ex = d.ulp ? Math.min(maxExp, Math.log10(Math.max(1, d.ulp))) : null; return { d: d, x: ex === null ? 22 + rnd() * 124 : lx(ex), y: 0 }; });
  var lanes = {}, mid = (24 + LH - 34) / 2;
  pts.sort(function (a, b) { return a.x - b.x; }).forEach(function (p) {
    var key = Math.round(p.x / 8), n = lanes[key] = (lanes[key] || 0) + 1, side = n % 2 ? 1 : -1, step = Math.floor(n / 2);
    p.y = mid + side * step * 7.5; if (p.y < 30 || p.y > LH - 40) p.y = 30 + rnd() * (LH - 74);
  });
  var lad = L.append('g').selectAll('circle').data(pts).join('circle')
    .attr('cx', function (p) { return p.x; }).attr('cy', function (p) { return p.y; })
    .attr('r', function (p) { return Math.max(2, Math.min(6.5, 1 + Math.log10(Math.max(10, p.d.rows)))); })
    .style('cursor', 'pointer')
    .on('mouseenter', function (e, p) { tip(e, p.d); hover(p.d.name); }).on('mousemove', moveTip)
    .on('mouseleave', function () { tipEl.hidden = true; hover(null); })
    .on('click', function (e, p) { select(p.d); var f = famByName[p.d.fam]; if (state.focus !== f) zoomTo(f); });
  function styleLadder() {
    lad.attr('fill', function (p) { return state.color === 'ulp' ? fillOf(p.d) : ramp(closenessOf(p.d.rows, p.d.matched)); })
      .attr('stroke', function (p) { return p.d === state.selected ? C.accent : C.night; }).attr('stroke-width', function (p) { return p.d === state.selected ? 3 : .8; })
      .attr('opacity', function (p) { return visible(p.d) ? 1 : .08; });
  }

  // ---------- hover and tooltip ----------
  var tipEl = document.getElementById('tip');
  function tip(e, d) {
    var dv = dayValue(d, state.day);
    tipEl.innerHTML = '<b>' + esc(d.name) + '</b><br>' + esc(d.fam) + '<br>' + (dv ? fmt(dv[1]) + ' / ' + fmt(dv[0]) + ' · ' + pct(dv[0], dv[1]) : 'not in the bank on ' + DAYS[state.day]) + (state.day === LAST && d.ulp ? '<br>worst ' + ulpText(d.ulp) : '') + (d.fuzz ? '<br>fuzz: ' + fmt(d.fuzz.swept) + ' swept · ' + fmt(d.fuzz.regionsCovered) + '/' + fmt(d.fuzz.regionsTotal) + ' regions · ' + fmt(d.fuzz.openClusters) + ' open clusters' : '');
    tipEl.hidden = false; moveTip(e);
  }
  function moveTip(e) { tipEl.style.left = Math.min(innerWidth - 290, e.clientX + 14) + 'px'; tipEl.style.top = Math.min(innerHeight - 110, e.clientY + 14) + 'px'; }
  function hover(name) {
    dot.attr('opacity', function (d) { return !name || d.data.name === name ? 1 : .35; });
    lad.attr('fill-opacity', function (p) { return !name || p.d.name === name ? 1 : .3; });
  }

  document.getElementById('foot').innerHTML = 'Data: <code>crates/calc-func/witness-status.tsv</code> at ' + esc(BANK_AT) + ' (full bank, judged against Excel 16.0 answers) and its git history, one snapshot per day from ' + DAYS[0] + '. Fuzz coverage: <code>crates/calc-func/fuzz-status.tsv</code> (live Excel sweeps, regions, frontiers, open clusters, last sweep date). Descriptions, families, notes and handbook verdicts from the Excel Functions Handbook build; the handbook verdict can lag the bank. Open work: open beads in the DNA Calc tracker whose title names the function.';
  function fromHash() {
    var h = decodeURIComponent(location.hash.slice(1));
    return h ? data.find(function (d) { return hashName(d.name) === h || d.name === h.toUpperCase(); }) : null;
  }
  setDay(LAST);
  var start = fromHash();
  select(start || data.find(function (d) { return d.name === 'GAMMALN'; }) || data[0], !start);
  if (start) zoomTo(famByName[start.fam]);
  addEventListener('hashchange', function () { var d = fromHash(); if (d && d !== state.selected) { select(d, true); var f = famByName[d.fam]; if (state.focus !== f) zoomTo(f); } });
  document.getElementById('foot').innerHTML += ' Landscapes: a sample of the individual answers per function (misses first), ' + esc(LAND_AT) + '.';
})();

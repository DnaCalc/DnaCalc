(function () {
  var cs = getComputedStyle(document.documentElement); function v(n) { return cs.getPropertyValue(n).trim(); }
  var K = { ink: v('--ink'), muted: v('--muted'), faint: v('--faint'), rule: v('--rule'), rule2: v('--rule2'), teal: v('--teal'), coral: v('--coral'), gold: v('--gold'), violet: v('--violet'), exact: v('--exact'), mid: v('--mid'), bad: v('--bad'), card: v('--card') };
  var fmt = d3.format(','), A = ATLAS;
  var tipEl = document.getElementById('tip');
  function tip(e, html) { tipEl.innerHTML = html; tipEl.hidden = false; move(e); }
  function move(e) { tipEl.style.left = Math.min(innerWidth - 310, e.clientX + 14) + 'px'; tipEl.style.top = Math.min(innerHeight - 120, e.clientY + 14) + 'px'; }
  function hide() { tipEl.hidden = true; }

  var FIX = { COUNT: 'Statistical', SUM: 'Math & trigonometry', IF: 'Logical', FILTER: 'Lookup & reference', UNIQUE: 'Lookup & reference', XLOOKUP: 'Lookup & reference', TEXTBEFORE: 'Text' };
  var fns = A.fns.map(function (r) { return { name: r[0], files: r[1], calls: r[2], rows: r[3], matched: r[4], ulp: r[5], cls: r[6], fam: FIX[r[0]] || r[7] || 'Other' }; });
  var judged = fns.filter(function (f) { return f.rows; });
  var calls = d3.sum(fns, function (f) { return f.calls; });
  document.getElementById('counts').innerHTML = [
    [fmt(A.files), 'unique workbooks'], [fmt(A.with_formulas), 'with formulas'], [d3.format('.3s')(A.formula_cells).replace('M', ' million'), 'formula cells'],
    [fmt(fns.length), 'catalogue functions in use'], [fmt(A.udf_names), 'other names called (add-ins, VBA)']
  ].map(function (c) { return '<div><b>' + c[0] + '</b><span>' + c[1] + '</span></div>'; }).join('');

  // ---------- usage against exactness ----------
  var FAMS = ['Math & trigonometry', 'Statistical', 'Lookup & reference', 'Logical', 'Text', 'Date & time', 'Financial', 'Information', 'Engineering', 'Database', 'Compatibility', 'Other'];
  var famColor = d3.scaleOrdinal().domain(FAMS).range([K.teal, K.violet, K.coral, K.gold, '#3b82c4', '#2f8f5b', '#b4562a', '#8a6d3b', '#5b6b7a', '#a04c8c', '#7d8b55', K.faint]);
  var clsColor = { exact: K.exact, last_bit: '#8fbf5a', numeric: K.mid, structural: K.bad, excluded: K.faint };
  var colorBy = 'family';
  var Q = d3.select('#quad'), QW = 860, QH = 560, m = { l: 64, r: 20, t: 20, b: 50 };
  var x = d3.scaleLog().domain([1, d3.max(judged, function (f) { return f.files; }) * 1.3]).range([m.l, QW - m.r]);
  // 1 = every answer exact; below that a log scale of the miss share: 0.95 = 99.999%, 0.76 = 99.99%, 0.57 = 99.9%, 0.38 = 99%, 0.19 = 90%, 0 = none.
  function closeness(f) { var miss = 1 - f.matched / f.rows; if (miss <= 0) return 1; return .95 - .95 * Math.max(0, Math.min(1, (Math.log10(miss) + 5) / 5)); }
  var y = d3.scaleLinear().domain([0, 1.02]).range([QH - m.b, m.t]);
  var r = d3.scaleSqrt().domain([0, d3.max(judged, function (f) { return f.calls; })]).range([2.5, 34]);
  var cutX = 50, cutY = .999;
  Q.append('rect').attr('x', x(cutX)).attr('y', y(closeness({ rows: 1000, matched: 999 })) ).attr('width', QW - m.r - x(cutX)).attr('height', QH - m.b - y(closeness({ rows: 1000, matched: 999 }))).attr('fill', K.coral).attr('opacity', .07);
  Q.append('text').attr('x', QW - m.r - 8).attr('y', QH - m.b - 10).attr('text-anchor', 'end').attr('fill', K.coral).style('font', '600 12px "IBM Plex Mono",monospace').text('used widely, not yet exact everywhere');
  [1, 10, 100, 1000, 10000].forEach(function (t) { if (t > x.domain()[1]) return; Q.append('line').attr('x1', x(t)).attr('x2', x(t)).attr('y1', m.t).attr('y2', QH - m.b).attr('stroke', K.rule); Q.append('text').attr('x', x(t)).attr('y', QH - m.b + 18).attr('text-anchor', 'middle').attr('fill', K.faint).style('font', '400 11px "IBM Plex Mono",monospace').text(fmt(t)); });
  [[0, '0%'], [.19, '90%'], [.38, '99%'], [.57, '99.9%'], [.76, '99.99%'], [.95, '99.999%'], [1, 'all exact']].forEach(function (t) { Q.append('line').attr('x1', m.l).attr('x2', QW - m.r).attr('y1', y(t[0])).attr('y2', y(t[0])).attr('stroke', K.rule); Q.append('text').attr('x', m.l - 8).attr('y', y(t[0]) + 4).attr('text-anchor', 'end').attr('fill', t[0] === 1 ? K.exact : K.faint).style('font', '400 10.5px "IBM Plex Mono",monospace').text(t[1]); });
  Q.append('text').attr('x', (m.l + QW - m.r) / 2).attr('y', QH - 8).attr('text-anchor', 'middle').attr('fill', K.muted).style('font', '600 12px "IBM Plex Sans",sans-serif').text('workbooks that call the function');
  Q.append('text').attr('transform', 'translate(14,' + (QH / 2) + ') rotate(-90)').attr('text-anchor', 'middle').attr('fill', K.muted).style('font', '600 12px "IBM Plex Sans",sans-serif').text('share of Excel answers matched');
  var seed = 3; function jit() { seed = (seed * 16807) % 2147483647; return (seed / 2147483647 - .5) * 8; }
  var dots = Q.append('g').selectAll('circle').data(judged.slice().sort(function (a, b) { return b.calls - a.calls; })).join('circle')
    .attr('cx', function (f) { return x(f.files); }).attr('cy', function (f) { var c = closeness(f); return y(c) + (c === 1 ? jit() : 0); })
    .attr('r', function (f) { return r(f.calls); }).attr('fill-opacity', .72).attr('stroke', K.card).attr('stroke-width', 1).style('cursor', 'pointer')
    .on('mouseenter', function (e, f) { tip(e, '<b>' + f.name + '</b><br>' + f.fam + '<br>' + fmt(f.files) + ' workbooks · ' + fmt(f.calls) + ' calls<br>bank: ' + fmt(f.matched) + ' / ' + fmt(f.rows) + ' matched (' + (f.matched / f.rows * 100).toFixed(2) + '%)'); d3.select(this).attr('stroke', K.ink).attr('stroke-width', 2); })
    .on('mousemove', move).on('mouseleave', function () { hide(); d3.select(this).attr('stroke', K.card).attr('stroke-width', 1); });
  var labels = Q.append('g').selectAll('text').data(judged.filter(function (f) { return f.files >= 250 || (f.files >= cutX && closeness(f) < .57); })).join('text')
    .attr('x', function (f) { return x(f.files) + r(f.calls) + 3; }).attr('y', function (f) { return y(closeness(f)) + 4; })
    .attr('fill', K.ink).style('font', '600 11px "IBM Plex Mono",monospace').attr('paint-order', 'stroke').attr('stroke', K.card).attr('stroke-width', 3).text(function (f) { return f.name; });
  function recolor() {
    dots.attr('fill', function (f) { return colorBy === 'family' ? famColor(f.fam) : clsColor[f.cls] || K.faint; });
    var items = colorBy === 'family' ? FAMS.filter(function (fm) { return judged.some(function (f) { return f.fam === fm; }); }).map(function (fm) { return [famColor(fm), fm]; })
      : [['exact', 'every answer exact'], ['last_bit', 'last-bit misses'], ['numeric', 'numeric misses'], ['structural', 'misses in kind']].map(function (c) { return [clsColor[c[0]], c[1]]; });
    renderChartMarkup(document.getElementById('qlegend'), items.map(function (it) { return '<span><i data-background="' + it[0] + '"></i>' + it[1] + '</span>'; }).join(''));
  }
  document.querySelectorAll('.ctl button').forEach(function (b) { b.addEventListener('click', function () { colorBy = b.dataset.c; document.querySelectorAll('.ctl button').forEach(function (x2) { x2.setAttribute('aria-pressed', String(x2 === b)); }); recolor(); }); });
  recolor();
  // Priority: most-used functions not exact everywhere, weighted by how many workbooks call them.
  var prio = judged.filter(function (f) { return f.matched < f.rows; }).sort(function (a, b) { return b.files - a.files; }).slice(0, 14);
  document.getElementById('prio').innerHTML = prio.map(function (f, i) {
    var miss = f.rows - f.matched;
    return '<li data-n="' + f.name + '"><i>' + (i + 1) + '</i><b>' + f.name + '</b><span>' + fmt(f.files) + ' files</span><small>' + fmt(miss) + ' of ' + fmt(f.rows) + ' answers differ · ' + ({ structural: 'in kind', numeric: 'numeric, worst ' + (f.ulp ? fmt(f.ulp) + ' ulp' : '?'), last_bit: 'last bit' })[f.cls] + '</small></li>';
  }).join('');
  document.querySelectorAll('#prio li').forEach(function (li) {
    li.addEventListener('mouseenter', function () { var n = li.dataset.n; dots.attr('fill-opacity', function (f) { return f.name === n ? 1 : .15; }); });
    li.addEventListener('mouseleave', function () { dots.attr('fill-opacity', .72); });
  });

  // ---------- long tail ----------
  var ranked = fns.slice().sort(function (a, b) { return b.calls - a.calls; });
  var top10 = d3.sum(ranked.slice(0, 10), function (f) { return f.calls; }) / calls;
  var cum = 0, half = 0; ranked.forEach(function (f, i) { cum += f.calls; if (!half && cum >= calls / 2) half = i + 1; });
  document.getElementById('tailnote').textContent = 'Functions ranked by how many calls the workbooks make. The top ten make ' + Math.round(top10 * 100) + '% of all ' + fmt(calls) + ' calls; ' + half + ' functions make half of them. The rest of the catalogue lives in the long tail, where each function still has to be exactly right.';
  var T = d3.select('#tail'), TW = 1200, TH = 300, tm = { l: 64, r: 16, t: 14, b: 70 };
  var tx = d3.scaleBand().domain(ranked.map(function (f) { return f.name; })).range([tm.l, TW - tm.r]).padding(.1);
  var ty = d3.scaleLog().domain([1, ranked[0].calls]).range([TH - tm.b, tm.t]);
  [1, 100, 1e4, 1e6].forEach(function (t) { T.append('line').attr('x1', tm.l).attr('x2', TW - tm.r).attr('y1', ty(t)).attr('y2', ty(t)).attr('stroke', K.rule); T.append('text').attr('x', tm.l - 6).attr('y', ty(t) + 4).attr('text-anchor', 'end').attr('fill', K.faint).style('font', '400 11px "IBM Plex Mono",monospace').text(d3.format('~s')(t)); });
  T.append('g').selectAll('rect').data(ranked).join('rect').attr('x', function (f) { return tx(f.name); }).attr('width', tx.bandwidth()).attr('y', function (f) { return ty(Math.max(1, f.calls)); }).attr('height', function (f) { return TH - tm.b - ty(Math.max(1, f.calls)); })
    .attr('fill', function (f) { return !f.rows ? K.faint : f.matched === f.rows ? K.exact : f.cls === 'structural' ? K.bad : K.mid; })
    .on('mouseenter', function (e, f) { tip(e, '<b>' + f.name + '</b><br>' + fmt(f.calls) + ' calls in ' + fmt(f.files) + ' workbooks' + (f.rows ? '<br>bank ' + (f.matched / f.rows * 100).toFixed(2) + '% matched' : '<br>not in the bank (volatile or host)')); }).on('mousemove', move).on('mouseleave', hide);
  ranked.slice(0, 22).forEach(function (f) { T.append('text').attr('transform', 'translate(' + (tx(f.name) + tx.bandwidth() / 2) + ',' + (TH - tm.b + 8) + ') rotate(60)').attr('fill', K.muted).style('font', '400 10.5px "IBM Plex Mono",monospace').text(f.name); });
  T.append('text').attr('x', TW - tm.r).attr('y', TH - 12).attr('text-anchor', 'end').attr('fill', K.faint).style('font', '400 11px "IBM Plex Mono",monospace').text(ranked.length + ' functions · green: every bank answer exact · amber: numeric misses · red: misses in kind · grey: not in the bank');

  // ---------- features and shapes ----------
  var FEAT = [['xl/calcChain', 'a calculation chain'], ['xl/drawings', 'drawings'], ['xl/externalLinks', 'links to other workbooks'], ['volatile', 'volatile functions (NOW, OFFSET, INDIRECT…)', 'f'], ['xl/charts', 'charts'], ['external_reference', 'formulas reading other workbooks', 'f'], ['xl/comments', 'comments'], ['xl/pivotCache', 'pivot caches'], ['xl/pivotTables', 'pivot tables'], ['xl/vbaProject', 'a VBA project'], ['xl/tables', 'Excel tables'], ['array_formula', 'array formulas (CSE)', 'f'], ['xl/connections', 'data connections'], ['structured_reference', 'structured references', 'f'], ['add_in_call', 'add-in function calls', 'f'], ['spill_reference', 'spill references (#)', 'f'], ['let', 'LET', 'f'], ['lambda', 'LAMBDA', 'f']];
  var fe = FEAT.map(function (f) { return [f[1], (f[2] ? A.features[f[0]] : A.parts[f[0]]) || 0]; }).filter(function (f) { return f[1] > 0; }).sort(function (a, b) { return b[1] - a[1]; });
  var fmax = fe[0][1];
  renderChartMarkup(document.getElementById('feat'), fe.map(function (f) { return '<div class="bar"><span class="t" title="' + f[0] + '">' + f[0] + '</span><div class="track"><div class="fill" data-width="' + Math.max(.6, f[1] / fmax * 100) + '%" data-background="var(--teal)"></div></div><span class="n">' + (f[1] / A.files * 100 >= 1 ? Math.round(f[1] / A.files * 100) + '%' : (f[1] / A.files * 100).toFixed(2) + '%') + '</span></div>'; }).join(''));
  var sh = A.shapes.slice(0, 18), smax = sh[0][1];
  renderChartMarkup(document.getElementById('shapes'), sh.map(function (s) { return '<div class="bar"><span class="t chart-mono">' + s[0].replace(/</g, '&lt;') + '</span><div class="track"><div class="fill" data-width="' + Math.max(.6, s[1] / smax * 100) + '%" data-background="var(--coral)"></div></div><span class="n">' + d3.format('.3~s')(s[1]) + '</span></div>'; }).join(''));

  // ---------- sizes ----------
  var Z = d3.select('#size'), ZW = 1200, ZH = 380, zm = { l: 60, r: 16, t: 14, b: 44 };
  var SETS = { enron: ['Enron', 'var(--enron)'], batch1: ['Enron (capture batch)', 'var(--enron)'], libs: ['library test suites', 'var(--libs)'], 'new-github': ['GitHub', 'var(--github)'], 'new-zenodo': ['Zenodo', 'var(--zenodo)'] };
  var pts = A.points.filter(function (p) { return p[1] > 0; });
  var zx = d3.scaleLog().domain([1, d3.max(pts, function (p) { return p[1]; }) * 1.2]).range([zm.l, ZW - zm.r]);
  var zy = d3.scaleSymlog().constant(1).domain([0, d3.max(pts, function (p) { return p[2]; }) * 1.2]).range([ZH - zm.b, zm.t]);
  [10, 100, 1e3, 1e4, 1e5, 1e6].forEach(function (t) { if (t > zx.domain()[1]) return; Z.append('line').attr('x1', zx(t)).attr('x2', zx(t)).attr('y1', zm.t).attr('y2', ZH - zm.b).attr('stroke', K.rule); Z.append('text').attr('x', zx(t)).attr('y', ZH - zm.b + 16).attr('text-anchor', 'middle').attr('fill', K.faint).style('font', '400 11px "IBM Plex Mono",monospace').text(d3.format('~s')(t)); });
  [0, 10, 100, 1e3, 1e4, 1e5].forEach(function (t) { if (t > zy.domain()[1]) return; Z.append('line').attr('x1', zm.l).attr('x2', ZW - zm.r).attr('y1', zy(t)).attr('y2', zy(t)).attr('stroke', K.rule); Z.append('text').attr('x', zm.l - 6).attr('y', zy(t) + 4).attr('text-anchor', 'end').attr('fill', K.faint).style('font', '400 11px "IBM Plex Mono",monospace').text(d3.format('~s')(t)); });
  Z.append('text').attr('x', (zm.l + ZW) / 2).attr('y', ZH - 6).attr('text-anchor', 'middle').attr('fill', K.muted).style('font', '600 12px "IBM Plex Sans",sans-serif').text('cells in the workbook');
  Z.append('text').attr('transform', 'translate(14,' + (ZH / 2) + ') rotate(-90)').attr('text-anchor', 'middle').attr('fill', K.muted).style('font', '600 12px "IBM Plex Sans",sans-serif').text('formula cells');
  Z.append('g').selectAll('circle').data(pts).join('circle').attr('cx', function (p) { return zx(p[1]); }).attr('cy', function (p) { return zy(p[2]); }).attr('r', function (p) { return 1.8 + Math.min(4, p[3] / 6); })
    .attr('fill', function (p) { return (SETS[p[4]] || ['', K.faint])[1]; }).attr('fill-opacity', .55)
    .on('mouseenter', function (e, p) { tip(e, '<b>' + (SETS[p[4]] || [p[4]])[0] + '</b><br>' + fmt(p[1]) + ' cells · ' + fmt(p[2]) + ' formulas<br>' + p[3] + ' distinct functions · ' + p[5] + ' sheets · ' + d3.format('.3~s')(p[0]) + 'B'); }).on('mousemove', move).on('mouseleave', hide);
  renderChartMarkup(document.getElementById('slegend'), [['enron', 'Enron'], ['libs', 'library test suites'], ['new-github', 'GitHub'], ['new-zenodo', 'Zenodo']].map(function (s) { return '<span><i data-background="' + SETS[s[0]][1] + '"></i>' + s[1] + ' (' + fmt(A.sets[s[0]] + (s[0] === 'enron' ? (A.sets.batch1 || 0) : 0)) + ' files)</span>'; }).join(''));

  document.getElementById('foot').innerHTML = 'Sources: the corpus classifier (<code>cargo xtask classify</code>) over the public sets on dna-koderbot and kode-fi on 2026-11-12, unique by SHA-256 and opened by our reader; .xls files are counted only once our own reader exists. Exactness from the full function bank (crates/calc-func/witness-status.tsv). Volatile functions such as NOW and TODAY are not in the bank and show grey. "Other names" are calls to functions outside the catalogue: VBA functions, add-ins and future functions.';
})();

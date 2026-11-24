(function () {
  var COUNT_LABELS = ['Exact', 'Last bit', 'Numeric', 'Gross', 'Structural', 'Refused', 'Excluded'];
  var O = ['structural', 'gross', 'numeric', 'last_bit', 'excluded', 'exact'];
  var LBL = { structural: 'Structural: a different kind of answer (error, type, shape)', gross: 'Gross: over 1,024 ulp or the wrong sign', numeric: 'Numeric: 5 to 1,024 ulp', last_bit: 'Last bit: 1 to 4 ulp', excluded: 'Excluded rows only', exact: 'Exact on every call' };
  var SHORT = { structural: 'structural', gross: 'gross', numeric: 'numeric', last_bit: 'last bit', excluded: 'excluded', exact: 'exact' };
  var COL = { structural: 'var(--kind)', gross: 'var(--bad)', numeric: 'var(--far)', last_bit: 'var(--near)', excluded: 'var(--faint)', exact: 'var(--exact)' };
  function f(n) { return n.toLocaleString('en-US'); }
  function e(t) { return String(t).replace(/[&<>"]/g, function (c) { return { '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' }[c]; }); }
  function ulp(u) { return u == null ? '' : u < 1e6 ? f(u) : u.toExponential(1).replace('e+', 'e'); }
  var FUZZ_MAP = (typeof FUZZ !== 'undefined' && FUZZ) ? FUZZ : {};
  var rows = BANK.map(function (b) { var c = O.indexOf(b[4]) < 0 ? 'structural' : b[4]; return { n: b[0], t: b[1], m: b[2], u: b[3], c: c, fam: b[5] || '', beads: b[9] || [], counts: b[10], fuzz: FUZZ_MAP[b[0]] || null }; });
  var by = {}; O.forEach(function (k) { by[k] = rows.filter(function (r) { return r.c === k; }); });
  var T = rows.reduce(function (a, r) { return a + r.t; }, 0), M = rows.reduce(function (a, r) { return a + r.m; }, 0);
  var fuzzedRows = rows.filter(function (r) { return !!r.fuzz; });
  var fuzzSweptTotal = fuzzedRows.reduce(function (a, r) { return a + r.fuzz[0]; }, 0);
  document.getElementById('confLede').innerHTML = rows.length + ' functions, ' + f(T) + ' recorded Excel calls, ' + f(M) + ' matched (' + (100 * M / T).toFixed(2) + '%). Each function is classed by its worst call: ' + O.map(function (k) { return '<b class="chart-ink">' + by[k].length + '</b> ' + SHORT[k]; }).join(', ') + '. Live fuzz coverage: <b class="chart-ink">' + f(fuzzedRows.length) + '</b> functions swept (' + f(fuzzSweptTotal) + ' cases). Misses counts every call that differs; the worst miss is in units in the last place. A dash marks counts unavailable in this snapshot. Status at ' + e(BANK_AT) + '.';
  renderChartMarkup(document.getElementById('confKey'), O.map(function (k) { var mis = by[k].reduce(function (a, r) { return a + r.t - r.m; }, 0); return '<span><i data-background="' + COL[k] + '"></i>' + LBL[k] + ': ' + by[k].length + ' functions' + (mis ? ', ' + f(mis) + ' misses' : '') + '</span>'; }).join(''));
  var st = { show: { structural: 1, gross: 1, numeric: 1, last_bit: 1, excluded: 1, exact: 0 }, sort: 'sev', q: '' };
  var chips = document.getElementById('confChips');
  renderChartMarkup(chips, O.map(function (k) { return '<button type="button" class="chip" data-k="' + k + '" aria-pressed="' + (st.show[k] ? 'true' : 'false') + '"><i data-background="' + COL[k] + '"></i>' + SHORT[k] + ' ' + by[k].length + '</button>'; }).join(''));
  chips.querySelectorAll('button').forEach(function (b) { b.addEventListener('click', function () { st.show[b.dataset.k] = st.show[b.dataset.k] ? 0 : 1; b.setAttribute('aria-pressed', st.show[b.dataset.k] ? 'true' : 'false'); render(); }); });
  function render() {
    var q = st.q.toLowerCase();
    var list = rows.filter(function (r) { return st.show[r.c] && (!q || r.n.toLowerCase().indexOf(q) >= 0 || r.fam.toLowerCase().indexOf(q) >= 0); });
    var key = { sev: function (r) { return O.indexOf(r.c) * 1e12 - (r.t - r.m); }, miss: function (r) { return -(r.t - r.m); }, share: function (r) { return r.m / r.t; }, fuzz: function (r) { return -(r.fuzz ? r.fuzz[0] : -1); }, name: null }[st.sort];
    list.sort(function (a, b) { return key ? (key(a) - key(b)) || (a.n < b.n ? -1 : 1) : (a.n < b.n ? -1 : 1); });
    renderChartMarkup(document.getElementById('conf'), '<table><thead><tr><th>Function</th><th>Family</th><th>Worst call</th><th class="numeric">Calls</th><th class="numeric">Misses</th><th class="numeric">Matched</th><th>Share matched</th><th class="numeric">Worst ulp</th>' + COUNT_LABELS.map(function (label) { return '<th class="numeric">' + label + '</th>'; }).join('') + '<th class="numeric">Fuzz swept</th><th class="numeric">Regions</th><th class="numeric">Frontiers</th><th class="numeric">Open clusters</th><th>Last sweep</th><th>Open beads</th></tr></thead><tbody>' + list.map(function (r) {
      var mis = r.t - r.m, share = r.m / r.t, fz = r.fuzz;
      var fuzzCells = fz
        ? '<td class="n">' + f(fz[0]) + '</td><td class="n">' + f(fz[3]) + '/' + f(fz[4]) + '</td><td class="n">' + f(fz[5]) + '</td><td class="n">' + f(fz[6]) + '</td><td>' + e(fz[7]) + '</td>'
        : '<td class="n">—</td><td class="n">—</td><td class="n">—</td><td class="n">—</td><td>—</td>';
      return '<tr data-n="' + e(r.n) + '"><td><b>' + e(r.n) + '</b></td><td>' + e(r.fam) + '</td><td><span class="conf-standing"><i class="conf-marker" data-background="' + COL[r.c] + '"></i>' + SHORT[r.c] + '</span></td><td class="n">' + f(r.t) + '</td><td class="n">' + (mis ? f(mis) : '') + '</td><td class="n">' + (100 * share).toFixed(mis ? 2 : 0) + '%</td><td><div class="stack"><i data-width="' + (100 * share) + '%" data-background="var(--exact)"></i><i data-width="' + (100 - 100 * share) + '%" data-background="' + COL[r.c] + '"></i></div></td><td class="n">' + ulp(r.u) + '</td>' + COUNT_LABELS.map(function (_, i) { return '<td class="n">' + (r.counts ? f(r.counts[i]) : '—') + '</td>'; }).join('') + fuzzCells + '<td class="chart-beads">' + r.beads.map(function (x) { return e(x[0]); }).join(' ') + '</td></tr>';
    }).join('') + '</tbody></table>' + (list.length ? '' : '<p>No function matches.</p>'));
    document.querySelectorAll('#conf tr[data-n]').forEach(function (tr) { tr.addEventListener('click', function () { location.hash = encodeURIComponent(tr.dataset.n); document.getElementById('map').scrollIntoView({ behavior: 'smooth', block: 'start' }); }); });
  }
  document.querySelectorAll('#confSort button').forEach(function (b) { b.addEventListener('click', function () { st.sort = b.dataset.k; document.querySelectorAll('#confSort button').forEach(function (x) { x.setAttribute('aria-pressed', x === b ? 'true' : 'false'); }); render(); }); });
  document.getElementById('confFind').addEventListener('input', function (ev) { st.q = ev.target.value; render(); });
  render();
})();

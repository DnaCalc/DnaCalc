(function () {
  var W = 10, H = 20, HEADW = 30, HEADH = 22;
  var PIECES = [
    { f: '=SEQUENCE(1,4)', c: '--p3', m: [[1,1,1,1]] },
    { f: '=SEQUENCE(2,2)', c: '--p1', m: [[1,1],[1,1]] },
    { f: '=VSTACK({1,2,3},{"",4,""})', c: '--p4', m: [[1,1,1],[0,1,0]] },
    { f: '=HSTACK(SEQUENCE(3),{"";"";4})', c: '--p5', m: [[1,0],[1,0],[1,1]] },
    { f: '=HSTACK({"";"";1},SEQUENCE(3,1,2))', c: '--p3', m: [[0,1],[0,1],[1,1]] },
    { f: '=VSTACK({"",1,2},{3,4,""})', c: '--p0', m: [[0,1,1],[1,1,0]] },
    { f: '=VSTACK({1,2,""},{"",3,4})', c: '--p2', m: [[1,1,0],[0,1,1]] }
  ];
  var cv = document.getElementById('cv'), ctx = cv.getContext('2d');
  var ncv = document.getElementById('nextcv'), nctx = ncv.getContext('2d');
  var colors = {};
  function readColors() { var cs = getComputedStyle(document.documentElement); ['--paper','--ink','--muted','--grid','--head','--cell','--select','--p0','--p1','--p2','--p3','--p4','--p5','--p6','--spill','--err','--errtint'].forEach(function (k) { colors[k] = cs.getPropertyValue(k).trim(); }); }
  readColors();
  try { matchMedia('(prefers-color-scheme: dark)').addEventListener('change', function () { readColors(); draw(); }); } catch (e) {}
  new MutationObserver(function () { readColors(); draw(); }).observe(document.documentElement, { attributes: true, attributeFilter: ['data-theme'] });
  var buf = new DataView(new ArrayBuffer(8));
  function hex(d) { buf.setFloat64(0, d); var s = ''; for (var i = 0; i < 8; i++) s += buf.getUint8(i).toString(16).padStart(2, '0'); return s; }
  function col(c) { return String.fromCharCode(65 + c); }

  var board, cur, nxt, score, rows, level, timer, running, over, clearing = false, flashRows = [], seq = 1, bag = [];
  var outlines = true;
  try { outlines = localStorage.getItem('spill-outlines') !== 'off'; } catch (e) {}
  var box = document.getElementById('outlines');
  box.checked = outlines;
  function setOutlines(on) { outlines = on; box.checked = on; try { localStorage.setItem('spill-outlines', on ? 'on' : 'off'); } catch (e) {} draw(); }
  box.addEventListener('change', function () { setOutlines(box.checked); box.blur(); });
  function newBoard() { board = []; for (var y = 0; y < H; y++) { var r = []; for (var x = 0; x < W; x++) r.push(null); board.push(r); } }
  function draw7() { if (!bag.length) { bag = [0,1,2,3,4,5,6]; for (var i = bag.length - 1; i > 0; i--) { var j = Math.floor(Math.random() * (i + 1)); var t = bag[i]; bag[i] = bag[j]; bag[j] = t; } } return bag.pop(); }
  function make(k) {
    var p = PIECES[k], m = p.m.map(function (r) { return r.slice(); });
    var vals = m.map(function (r) { return r.map(function (v) { return v ? seq++ : 0; }); });
    // The formula names the numbers its cells really hold: SEQUENCE's third argument is the start.
    var solid = m.every(function (r) { return r.every(Boolean); });
    var f = solid ? '=SEQUENCE(' + m.length + ',' + m[0].length + ',' + vals[0][0] + ')'
      : '=VSTACK(' + vals.map(function (r, y) { return '{' + r.map(function (v, x) { return m[y][x] ? v : '""'; }).join(',') + '}'; }).join(',') + ')';
    return { k: k, f: f, c: p.c, m: m, vals: vals, x: Math.floor((W - m[0].length) / 2), y: 0, t: false };
  }
  function fits(p, dx, dy, m) {
    m = m || p.m;
    for (var y = 0; y < m.length; y++) for (var x = 0; x < m[y].length; x++) {
      if (!m[y][x]) continue;
      var bx = p.x + x + dx, by = p.y + y + dy;
      if (bx < 0 || bx >= W || by >= H) return false;
      if (by >= 0 && board[by][bx]) return false;
    }
    return true;
  }
  function transpose(m) { var r = []; for (var x = 0; x < m[0].length; x++) { var row = []; for (var y = m.length - 1; y >= 0; y--) row.push(m[y][x]); r.push(row); } return r; }
  function rotate() {
    var m = transpose(cur.m), v = transpose(cur.vals);
    var kicks = [0, -1, 1, -2, 2];
    for (var i = 0; i < kicks.length; i++) if (fits(cur, kicks[i], 0, m)) { cur.m = m; cur.vals = v; cur.x += kicks[i]; cur.t = !cur.t; bars(); draw(); return; }
  }
  function lock() {
    for (var y = 0; y < cur.m.length; y++) for (var x = 0; x < cur.m[y].length; x++) if (cur.m[y][x]) {
      var by = cur.y + y; if (by < 0) return gameOver();
      board[by][cur.x + x] = { v: cur.vals[y][x], c: cur.c };
    }
    var full = [];
    for (y = 0; y < H; y++) if (board[y].every(Boolean)) full.push(y);
    if (full.length) {
      var total = 0;
      full.forEach(function (r) { board[r].forEach(function (c) { total += c.v; }); });
      var bonus = [0, 1, 3, 5, 8][full.length];
      score += total * level * bonus;
      rows += full.length;
      document.getElementById('lastSum').textContent = 'last: =SUM(A' + (full[0] + 1) + ':J' + (full[full.length - 1] + 1) + ') = ' + total + (full.length > 1 ? ' ×' + bonus : '');
      // The locked piece is now part of the board: nothing may move or lock it again while the rows flash.
      cur = null; clearing = true;
      flashRows = full.slice(); draw();
      setTimeout(function () {
        full.forEach(function (r) { board.splice(r, 1); var e = []; for (var x = 0; x < W; x++) e.push(null); board.unshift(e); });
        flashRows = []; clearing = false; level = 1 + Math.floor(rows / 8); restart(); spawn();
      }, 180);
      bars();
      return;
    }
    spawn();
  }
  function spawn() {
    cur = nxt || make(draw7()); nxt = make(draw7());
    if (!fits(cur, 0, 0)) return gameOver();
    bars(); drawNext(); draw();
  }
  function fall() { if (!running || clearing || !cur) return; if (fits(cur, 0, 1)) { cur.y++; draw(); } else lock(); }
  function hard() { if (!running || clearing || !cur) return; var d = 0; while (fits(cur, 0, 1)) { cur.y++; d++; } score += d; lock(); bars(); }
  function restart() { clearInterval(timer); if (running) timer = setInterval(fall, Math.max(90, 700 - (level - 1) * 60)); }
  function bars() {
    var x = cur ? cur.x : 4, y = cur ? cur.y : 0;
    document.getElementById('nameBox').textContent = col(Math.max(0, Math.min(W - 1, x))) + (Math.max(0, y) + 1);
    document.getElementById('formula').textContent = cur ? (cur.t ? '=TRANSPOSE(' + cur.f.slice(1) + ')' : cur.f) : '';
    document.getElementById('nextF').textContent = nxt ? nxt.f : '';
    document.getElementById('score').textContent = score.toLocaleString('en-US');
    document.getElementById('scoreBits').textContent = 'bits ' + hex(score);
    document.getElementById('rows').textContent = rows;
    document.getElementById('level').textContent = level;
  }
  function gameOver() {
    running = false; over = true; clearInterval(timer); draw();
    document.getElementById('dlgBar').textContent = 'Microsoft Excel would show';
    var t = document.getElementById('dlgTitle'); t.textContent = '#SPILL!'; t.className = 'err';
    document.getElementById('dlgText').textContent = 'Spill range isn\'t blank. ' + (cur ? cur.f : 'The next formula') + ' at ' + document.getElementById('nameBox').textContent + ' wanted cells that were already full.';
    document.getElementById('dlgBits').textContent = 'Score ' + score.toLocaleString('en-US') + ' (bits ' + hex(score) + ') · ' + rows + ' rows summed · level ' + level;
    document.getElementById('startBtn').textContent = 'New sheet';
    document.getElementById('overlay').hidden = false;
    document.getElementById('startBtn').focus();
  }
  function start() {
    newBoard(); score = 0; rows = 0; level = 1; seq = 1; bag = []; nxt = null; over = false; clearing = false; flashRows = []; running = true;
    document.getElementById('lastSum').textContent = 'last row: none yet';
    document.getElementById('overlay').hidden = true; spawn(); restart();
  }

  function act(a) {
    if (!running || !cur || clearing) return;
    if (a === 'left' && fits(cur, -1, 0)) cur.x--;
    else if (a === 'right' && fits(cur, 1, 0)) cur.x++;
    else if (a === 'soft') { if (fits(cur, 0, 1)) { cur.y++; score++; } else return lock(); }
    else if (a === 'rot') return rotate();
    else if (a === 'hard') return hard();
    bars(); draw();
  }
  addEventListener('keydown', function (e) {
    var map = { ArrowLeft: 'left', ArrowRight: 'right', ArrowDown: 'soft', ArrowUp: 'rot', x: 'rot', X: 'rot', ' ': 'hard' };
    if (e.key === 'F9' || (e.key === 'Enter' && !running)) { e.preventDefault(); if (!running) start(); return; }
    if (e.key === 'o' || e.key === 'O') { setOutlines(!outlines); return; }
    if (e.key === 'p' || e.key === 'P') { if (over || !cur) return; running = !running; restart(); draw(); return; }
    if (map[e.key]) { e.preventDefault(); if (!running && !document.getElementById('overlay').hidden && e.key === ' ') return start(); act(map[e.key]); }
  });
  document.getElementById('startBtn').addEventListener('click', start);
  document.querySelectorAll('.pad button').forEach(function (b) { b.addEventListener('click', function () { act(b.dataset.a); }); });
  var t0 = null, well = document.getElementById('well');
  well.addEventListener('pointerdown', function (e) { t0 = { x: e.clientX, y: e.clientY, t: Date.now ? 0 : 0 }; });
  well.addEventListener('pointerup', function (e) {
    if (!t0) return; var dx = e.clientX - t0.x, dy = e.clientY - t0.y; t0 = null;
    if (Math.abs(dx) < 16 && Math.abs(dy) < 16) return act('rot');
    if (Math.abs(dx) > Math.abs(dy)) act(dx > 0 ? 'right' : 'left'); else act(dy > 0 ? 'hard' : 'rot');
  });

  var cw, chh;
  function layout() {
    var w = well.clientWidth || 440, dpr = Math.min(devicePixelRatio || 1, 2);
    cw = (w - HEADW) / W; chh = cw * 0.78;
    var h = HEADH + chh * H;
    cv.width = Math.round(w * dpr); cv.height = Math.round(h * dpr); cv.style.height = h + 'px';
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0); lastFont = '';
  }
  function cellBox(x, y) { return [HEADW + x * cw, HEADH + y * chh, cw, chh]; }
  var lastFont = '';
  function fillCell(x, y, color, val, strong) {
    var r = cellBox(x, y);
    ctx.fillStyle = color; ctx.fillRect(r[0] + 1, r[1] + 1, r[2] - 1, r[3] - 1);
    if (val !== undefined) {
      var font = (strong ? '600 ' : '400 ') + Math.min(13, chh * .5) + 'px "IBM Plex Mono", Consolas, monospace';
      ctx.fillStyle = colors['--ink']; if (font !== lastFont) { ctx.font = font; lastFont = font; }
      ctx.textAlign = 'right'; ctx.textBaseline = 'middle'; ctx.fillText(String(val), r[0] + r[2] - 4, r[1] + r[3] / 2 + 1);
    }
  }
  // Redraws are batched: many moves in one frame cost one paint.
  var pending = false;
  function draw() { if (pending) return; pending = true; requestAnimationFrame(function () { pending = false; paint(); }); }
  function paint() {
    if (!board) return;
    var Wpx = HEADW + W * cw, Hpx = HEADH + H * chh;
    ctx.fillStyle = colors['--cell']; ctx.fillRect(0, 0, Wpx, Hpx);
    ctx.fillStyle = colors['--head']; ctx.fillRect(0, 0, Wpx, HEADH); ctx.fillRect(0, 0, HEADW, Hpx);
    for (var y = 0; y < H; y++) for (var x = 0; x < W; x++) { var c = board[y][x]; if (c) fillCell(x, y, flashRows.indexOf(y) >= 0 ? colors['--select'] : colors[c.c], c.v); }
    if (cur) {
      var gy = 0; while (outlines && fits(cur, 0, gy + 1)) gy++;
      if (outlines) for (y = 0; y < cur.m.length; y++) for (x = 0; x < cur.m[y].length; x++) if (cur.m[y][x] && cur.y + y + gy >= 0) {
        var g = cellBox(cur.x + x, cur.y + y + gy); ctx.setLineDash([3, 3]); ctx.strokeStyle = colors['--muted']; ctx.lineWidth = 1; ctx.strokeRect(g[0] + 2.5, g[1] + 2.5, g[2] - 5, g[3] - 5); ctx.setLineDash([]);
      }
      for (y = 0; y < cur.m.length; y++) for (x = 0; x < cur.m[y].length; x++) if (cur.m[y][x] && cur.y + y >= 0) fillCell(cur.x + x, cur.y + y, colors[cur.c], cur.vals[y][x], true);
      var minx = 99, miny = 99, maxx = -1, maxy = -1;
      for (y = 0; y < cur.m.length; y++) for (x = 0; x < cur.m[y].length; x++) if (cur.m[y][x]) { minx = Math.min(minx, x); maxx = Math.max(maxx, x); miny = Math.min(miny, y); maxy = Math.max(maxy, y); }
      var a = cellBox(cur.x + minx, Math.max(0, cur.y + miny)), b = cellBox(cur.x + maxx, cur.y + maxy);
      if (outlines) ctx.strokeStyle = colors['--spill']; else ctx.strokeStyle = 'transparent'; ctx.lineWidth = 1.5; ctx.setLineDash([5, 3]); ctx.strokeRect(a[0] + 1, a[1] + 1, b[0] + b[2] - a[0] - 2, b[1] + b[3] - a[1] - 2); ctx.setLineDash([]);
    }
    ctx.strokeStyle = colors['--grid']; ctx.lineWidth = 1; ctx.beginPath();
    for (x = 0; x <= W; x++) { var px = Math.round(HEADW + x * cw) + .5; ctx.moveTo(px, 0); ctx.lineTo(px, Hpx); }
    for (y = 0; y <= H; y++) { var py = Math.round(HEADH + y * chh) + .5; ctx.moveTo(0, py); ctx.lineTo(Wpx, py); }
    ctx.moveTo(.5, 0); ctx.lineTo(.5, Hpx); ctx.stroke();
    ctx.font = '600 11px "IBM Plex Mono", Consolas, monospace'; ctx.textAlign = 'center'; ctx.textBaseline = 'middle';
    for (x = 0; x < W; x++) { ctx.fillStyle = cur && x >= cur.x && x < cur.x + cur.m[0].length ? colors['--select'] : colors['--muted']; ctx.fillText(col(x), HEADW + x * cw + cw / 2, HEADH / 2 + 1); }
    for (y = 0; y < H; y++) { ctx.fillStyle = colors['--muted']; ctx.fillText(String(y + 1), HEADW / 2, HEADH + y * chh + chh / 2 + 1); }
    if (!running && !over && cur) { ctx.fillStyle = colors['--ink']; ctx.font = '600 15px "IBM Plex Sans", system-ui'; ctx.fillText('Calculation: Manual (P to resume)', Wpx / 2 + HEADW / 2, HEADH + chh * 3); }
  }
  function drawNext() {
    var p = nxt; nctx.clearRect(0, 0, ncv.width, ncv.height); if (!p) return;
    var s = 26, ox = (ncv.width - p.m[0].length * s) / 2, oy = (ncv.height - p.m.length * s) / 2;
    for (var y = 0; y < p.m.length; y++) for (var x = 0; x < p.m[y].length; x++) {
      nctx.strokeStyle = colors['--grid']; nctx.strokeRect(ox + x * s + .5, oy + y * s + .5, s, s);
      if (p.m[y][x]) { nctx.fillStyle = colors[p.c]; nctx.fillRect(ox + x * s + 1, oy + y * s + 1, s - 1, s - 1); }
    }
  }
  addEventListener('resize', function () { layout(); draw(); });
  layout(); newBoard(); running = false; draw();
})();

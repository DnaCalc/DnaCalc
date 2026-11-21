(function () {
  var COLS = 24, ROWS = 14, HEADW = 38, HEADH = 22;
  var cv = document.getElementById('cv'), ctx = cv.getContext('2d');
  var cw, ch;
  var css = getComputedStyle(document.documentElement);
  function tok(n) { return css.getPropertyValue(n).trim(); }
  var colors = {};
  function readColors() { css = getComputedStyle(document.documentElement); ['--paper','--ink','--muted','--grid','--head','--headink','--select','--snake','--snakeink','--food','--pi','--err','--errink'].forEach(function (k) { colors[k] = tok(k); }); }
  readColors();
  try { matchMedia('(prefers-color-scheme: dark)').addEventListener('change', function () { readColors(); draw(); }); } catch (e) {}
  new MutationObserver(function () { readColors(); draw(); }).observe(document.documentElement, { attributes: true, attributeFilter: ['data-theme'] });

  function colName(c) { var s = ''; c += 1; while (c > 0) { var r = (c - 1) % 26; s = String.fromCharCode(65 + r) + s; c = Math.floor((c - 1) / 26); } return s; }
  function addr(p) { return colName(p.x) + (p.y + 1); }

  var buf = new DataView(new ArrayBuffer(8));
  function hex(d) { buf.setFloat64(0, d); var s = ''; for (var i = 0; i < 8; i++) s += buf.getUint8(i).toString(16).padStart(2, '0'); return s; }
  function excel15(d) { if (d === 0) return '0'; var s = Number(d.toPrecision(15)); return Math.abs(s) >= 1e15 ? s.toExponential(14).replace(/\.?0+e/, 'E') : String(s); }

  var FOODS = [
    { label: '1', v: 1 }, { label: '2', v: 2 }, { label: '3', v: 3 }, { label: '5', v: 5 }, { label: '7', v: 7 },
    { label: '0.1', v: 0.1 }, { label: '0.2', v: 0.2 }, { label: '0.7', v: 0.7 }, { label: '-1', v: -1 }, { label: '1E-16', v: 1e-16 }
  ];
  var ERRORS = ['#DIV/0!', '#N/A', '#VALUE!', '#REF!', '#NAME?', '#NUM!', '#NULL!', '#SPILL!', '#CALC!'];

  var snake, dir, nextDir, foods, errs, sum, terms, alive, running, tickMs, timer, eaten, best = 0;
  try { best = Number(localStorage.getItem('circref-best')) || 0; } catch (e) {}
  document.getElementById('best').textContent = best;

  function free(p) {
    if (snake.some(function (s) { return s.x === p.x && s.y === p.y; })) return false;
    if (foods.some(function (s) { return s.x === p.x && s.y === p.y; })) return false;
    if (errs.some(function (s) { return s.x === p.x && s.y === p.y; })) return false;
    return true;
  }
  function spot() { for (var i = 0; i < 500; i++) { var p = { x: Math.floor(Math.random() * COLS), y: Math.floor(Math.random() * ROWS) }; if (free(p)) return p; } return null; }
  function addFood() {
    var p = spot(); if (!p) return;
    var gold = Math.random() < 0.12;
    var f = gold ? { label: '=PI()', v: Math.PI, gold: true } : FOODS[Math.floor(Math.random() * FOODS.length)];
    foods.push({ x: p.x, y: p.y, label: f.label, v: f.v, gold: !!f.gold });
  }
  function addErr() { var p = spot(); if (!p) return; var h = snake[0]; if (Math.abs(p.x - h.x) + Math.abs(p.y - h.y) < 4) return; errs.push({ x: p.x, y: p.y, label: ERRORS[Math.floor(Math.random() * ERRORS.length)] }); }

  function reset() {
    snake = [{ x: 4, y: 6 }, { x: 3, y: 6 }, { x: 2, y: 6 }];
    dir = { x: 1, y: 0 }; nextDir = dir; foods = []; errs = []; sum = 0; terms = []; eaten = 0; alive = true; tickMs = 150;
    for (var i = 0; i < 4; i++) addFood();
    for (var j = 0; j < 2; j++) addErr();
    updateBars();
  }

  function updateBars() {
    var h = snake[0];
    document.getElementById('nameBox').textContent = addr(h);
    var tail = snake[snake.length - 1];
    var shown = terms.length ? terms.slice(-6).join('+') : '0';
    document.getElementById('formula').textContent = '=SUM(' + addr(tail) + ':' + addr(h) + ')  → ' + excel15(sum) + '    ' + (terms.length > 6 ? '…+' : '') + shown;
    document.getElementById('len').textContent = snake.length;
    document.getElementById('mode').textContent = running ? 'Automatic' : 'Manual';
  }

  function step() {
    dir = nextDir;
    var h = snake[0], n = { x: h.x + dir.x, y: h.y + dir.y };
    if (n.x < 0 || n.y < 0 || n.x >= COLS || n.y >= ROWS) return over('ref', n);
    if (snake.slice(0, -1).some(function (s) { return s.x === n.x && s.y === n.y; })) return over('circ', n);
    var e = errs.find(function (s) { return s.x === n.x && s.y === n.y; });
    if (e) return over('err', n, e.label);
    snake.unshift(n);
    var fi = foods.findIndex(function (s) { return s.x === n.x && s.y === n.y; });
    if (fi >= 0) {
      var f = foods.splice(fi, 1)[0];
      sum = sum + f.v; terms.push(f.label); eaten++;
      addFood();
      if (eaten % 3 === 0) addErr();
      if (eaten % 4 === 0 && tickMs > 70) { tickMs -= 8; restartTimer(); }
      flash = { x: n.x, y: n.y, t: 6 };
    } else snake.pop();
    updateBars(); draw();
  }

  var flash = null;
  function over(kind, n, label) {
    alive = false; running = false; clearInterval(timer); updateBars(); draw(kind === 'ref' ? null : n);
    var score = snake.length;
    if (score > best) { best = score; try { localStorage.setItem('circref-best', String(best)); } catch (e) {} document.getElementById('best').textContent = best; }
    var t = document.getElementById('dlgTitle'), x = document.getElementById('dlgText'), b = document.getElementById('dlgBits'), bar = document.getElementById('dlgBar');
    document.getElementById('dlgIcon').className = 'icon'; document.getElementById('dlgIcon').textContent = '!';
    if (kind === 'circ') { bar.textContent = 'Microsoft Excel would say'; t.textContent = 'Circular reference'; x.textContent = 'Your formula at ' + addr(n) + ' refers to its own cell, so it can never finish calculating. DNA Calc found the cycle, as Excel does.'; }
    else if (kind === 'err') { bar.textContent = 'Error'; t.textContent = label; x.textContent = 'You summed an error cell at ' + addr(n) + '. In a SUM, one error wins over every number, so the whole formula is now ' + label + '.'; }
    else { bar.textContent = 'Error'; t.textContent = '#REF!'; x.textContent = 'Your formula walked off the edge of the sheet. A reference outside the grid becomes #REF!.'; }
    var exact = String(sum), shown = excel15(sum);
    b.textContent = 'Final SUM ' + shown + (exact !== shown ? ' (exactly ' + exact + ')' : '') + ' · bits ' + hex(sum) + ' · ' + score + ' cells' + (score >= best ? ' · new best' : '');
    document.getElementById('startBtn').textContent = 'New sheet';
    document.getElementById('overlay').hidden = false;
    document.getElementById('startBtn').focus();
  }

  function restartTimer() { clearInterval(timer); if (running) timer = setInterval(step, tickMs); }
  function start() { if (!alive) reset(); document.getElementById('overlay').hidden = true; running = true; updateBars(); restartTimer(); draw(); }
  function toggle() { if (!alive) return; running = !running; restartTimer(); updateBars(); draw(); }

  function turn(d) {
    var m = { up: { x: 0, y: -1 }, down: { x: 0, y: 1 }, left: { x: -1, y: 0 }, right: { x: 1, y: 0 } }[d];
    if (!m || (m.x === -dir.x && m.y === -dir.y)) return;
    nextDir = m;
    if (!running && alive && document.getElementById('overlay').hidden) { running = true; restartTimer(); updateBars(); }
  }
  addEventListener('keydown', function (ev) {
    var k = ev.key;
    var map = { ArrowUp: 'up', ArrowDown: 'down', ArrowLeft: 'left', ArrowRight: 'right', w: 'up', s: 'down', a: 'left', d: 'right', W: 'up', S: 'down', A: 'left', D: 'right' };
    if (map[k]) { ev.preventDefault(); turn(map[k]); return; }
    if (k === ' ') { ev.preventDefault(); if (!document.getElementById('overlay').hidden) start(); else toggle(); }
    if (k === 'F9' || k === 'Enter') { ev.preventDefault(); if (!running) start(); }
  });
  document.getElementById('startBtn').addEventListener('click', start);
  document.querySelectorAll('.pad button').forEach(function (b) { b.addEventListener('click', function () { turn(b.dataset.d); }); });
  var touch = null, board = document.getElementById('board');
  board.addEventListener('pointerdown', function (e) { touch = { x: e.clientX, y: e.clientY }; });
  board.addEventListener('pointerup', function (e) {
    if (!touch) return; var dx = e.clientX - touch.x, dy = e.clientY - touch.y; touch = null;
    if (Math.max(Math.abs(dx), Math.abs(dy)) < 24) return;
    turn(Math.abs(dx) > Math.abs(dy) ? (dx > 0 ? 'right' : 'left') : (dy > 0 ? 'down' : 'up'));
  });

  function layout() {
    var w = board.clientWidth || 960;
    COLS = w < 520 ? 12 : w < 760 ? 18 : 24;
    ROWS = w < 520 ? 16 : 14;
    var dpr = Math.min(window.devicePixelRatio || 1, 2);
    cw = (w - HEADW) / COLS; ch = Math.max(24, Math.min(34, cw * 0.62));
    var h = HEADH + ch * ROWS;
    cv.width = Math.round(w * dpr); cv.height = Math.round(h * dpr);
    cv.style.height = h + 'px';
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  }

  function cellRect(p) { return [HEADW + p.x * cw, HEADH + p.y * ch, cw, ch]; }
  function text(s, p, color, bold) {
    var r = cellRect(p); ctx.fillStyle = color; ctx.font = (bold ? '600 ' : '400 ') + Math.min(13, ch * 0.45) + 'px "IBM Plex Mono", Consolas, monospace';
    ctx.textAlign = 'center'; ctx.textBaseline = 'middle';
    var t = s; while (ctx.measureText(t).width > r[2] - 4 && t.length > 2) t = t.slice(0, -2) + '…';
    ctx.fillText(t, r[0] + r[2] / 2, r[1] + r[3] / 2 + 1);
  }
  function draw(hit) {
    var W = HEADW + COLS * cw, H = HEADH + ROWS * ch;
    ctx.fillStyle = colors['--paper']; ctx.fillRect(0, 0, W, H);
    ctx.fillStyle = colors['--head']; ctx.fillRect(0, 0, W, HEADH); ctx.fillRect(0, 0, HEADW, H);
    var h = snake[0];
    ctx.fillStyle = colors['--snake'];
    ctx.fillRect(HEADW + h.x * cw, 0, cw, HEADH); ctx.fillRect(0, HEADH + h.y * ch, HEADW, ch);
    ctx.strokeStyle = colors['--grid']; ctx.lineWidth = 1; ctx.beginPath();
    for (var c = 0; c <= COLS; c++) { var x = Math.round(HEADW + c * cw) + .5; ctx.moveTo(x, 0); ctx.lineTo(x, H); }
    for (var r = 0; r <= ROWS; r++) { var y = Math.round(HEADH + r * ch) + .5; ctx.moveTo(0, y); ctx.lineTo(W, y); }
    ctx.moveTo(.5, 0); ctx.lineTo(.5, H); ctx.stroke();
    ctx.font = '600 11px "IBM Plex Mono", Consolas, monospace'; ctx.textAlign = 'center'; ctx.textBaseline = 'middle';
    for (c = 0; c < COLS; c++) { ctx.fillStyle = c === h.x ? colors['--select'] : colors['--headink']; ctx.fillText(colName(c), HEADW + c * cw + cw / 2, HEADH / 2 + 1); }
    for (r = 0; r < ROWS; r++) { ctx.fillStyle = r === h.y ? colors['--select'] : colors['--headink']; ctx.fillText(String(r + 1), HEADW / 2, HEADH + r * ch + ch / 2 + 1); }

    foods.forEach(function (f) { var R = cellRect(f); ctx.fillStyle = f.gold ? colors['--pi'] : colors['--food']; ctx.fillRect(R[0] + 1, R[1] + 1, R[2] - 1, R[3] - 1); text(f.label, f, colors['--ink'], f.gold); });
    errs.forEach(function (e) { var R = cellRect(e); ctx.fillStyle = colors['--err']; ctx.fillRect(R[0] + 1, R[1] + 1, R[2] - 1, R[3] - 1); text(e.label, e, colors['--errink'], true); });
    for (var i = snake.length - 1; i >= 0; i--) {
      var s = snake[i], R = cellRect(s);
      ctx.fillStyle = colors['--snake']; ctx.fillRect(R[0] + 1, R[1] + 1, R[2] - 1, R[3] - 1);
      if (i > 0) { var nx = snake[i - 1]; text(nx.x > s.x ? '→' : nx.x < s.x ? '←' : nx.y > s.y ? '↓' : '↑', s, colors['--snakeink'], false); }
    }
    var HR = cellRect(h);
    ctx.fillStyle = colors['--paper']; ctx.fillRect(HR[0] + 1, HR[1] + 1, HR[2] - 1, HR[3] - 1);
    text(excel15(sum), h, colors['--ink'], true);
    ctx.strokeStyle = colors['--select']; ctx.lineWidth = 2.5; ctx.strokeRect(HR[0] + 1, HR[1] + 1, HR[2] - 2, HR[3] - 2);
    ctx.fillStyle = colors['--select']; ctx.fillRect(HR[0] + HR[2] - 4, HR[1] + HR[3] - 4, 6, 6);
    if (flash && flash.t > 0) { var F = cellRect(flash); ctx.strokeStyle = colors['--pi']; ctx.lineWidth = 2; ctx.strokeRect(F[0] - flash.t, F[1] - flash.t, F[2] + 2 * flash.t, F[3] + 2 * flash.t); flash.t--; }
    if (hit) { var X = cellRect(hit); ctx.strokeStyle = colors['--errink']; ctx.lineWidth = 3; ctx.setLineDash([5, 3]); ctx.strokeRect(X[0] + 2, X[1] + 2, X[2] - 4, X[3] - 4); ctx.setLineDash([]); }
    if (!running && alive && document.getElementById('overlay').hidden) {
      ctx.fillStyle = colors['--muted']; ctx.font = '600 15px "IBM Plex Sans", system-ui, sans-serif'; ctx.textAlign = 'center';
      ctx.fillText('Calculation: Manual. Press Space or steer to recalculate.', HEADW + COLS * cw / 2, HEADH + 18);
    }
  }

  addEventListener('resize', function () { var wasRunning = running; layout(); if (snake) { snake = snake.filter(function (s) { return s.x < COLS && s.y < ROWS; }); if (!snake.length) reset(); foods = foods.filter(function (s) { return s.x < COLS && s.y < ROWS; }); errs = errs.filter(function (s) { return s.x < COLS && s.y < ROWS; }); while (foods.length < 4) addFood(); draw(); } running = wasRunning; });
  layout(); reset(); running = false; draw();
})();

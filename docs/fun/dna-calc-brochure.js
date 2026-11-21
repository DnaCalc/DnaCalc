(function () {
  var input = document.getElementById('num');
  var buf = new DataView(new ArrayBuffer(8));
  function hex(d) { buf.setFloat64(0, d); var s = ''; for (var i = 0; i < 8; i++) s += buf.getUint8(i).toString(16).padStart(2, '0'); return s; }
  function bin(d) { buf.setFloat64(0, d); var s = ''; for (var i = 0; i < 8; i++) s += buf.getUint8(i).toString(2).padStart(8, '0'); return s; }
  function nextUp(d) {
    if (Number.isNaN(d) || d === Infinity) return d;
    if (d === 0) return Number.MIN_VALUE;
    buf.setFloat64(0, d); var hi = buf.getUint32(0), lo = buf.getUint32(4);
    if (d > 0) { lo = (lo + 1) >>> 0; if (lo === 0) hi = (hi + 1) >>> 0; }
    else { if (lo === 0) hi = (hi - 1) >>> 0; lo = (lo - 1) >>> 0; }
    buf.setUint32(0, hi); buf.setUint32(4, lo); return buf.getFloat64(0);
  }
  function excel15(d) {
    if (!Number.isFinite(d)) return '#NUM!';
    if (d === 0) return '0';
    var s = Number(d.toPrecision(15));
    var a = Math.abs(s);
    if (a >= 1e15 || a < 1e-9) return s.toExponential(14).replace(/\.?0+e/, 'E').replace('e', 'E');
    return String(s);
  }
  function render() {
    var raw = input.value.trim().replace(',', '.');
    var d = Number(raw);
    var chips = document.getElementById('chips'), b = document.getElementById('binary');
    if (raw === '' || Number.isNaN(d)) {
      chips.innerHTML = '<span class="s">#VALUE!</span>';
      b.textContent = 'Enter a number such as 0.1, -2.5 or 1e308.';
      ['up', 'ulp', 'shown'].forEach(function (id) { document.getElementById(id).textContent = '-'; });
      return;
    }
    var h = hex(d), bits = bin(d);
    var sign = bits[0], exp = bits.slice(1, 12), man = bits.slice(12);
    chips.innerHTML = '<span class="e" title="sign and exponent, 12 bits">' + h.slice(0, 3) + '</span><span class="m" title="fraction, 52 bits">' + h.slice(3) + '</span>';
    b.innerHTML = '<span class="s">' + sign + '</span> <span class="e">' + exp + '</span> <span class="m">' + man + '</span>';
    var up = nextUp(d);
    document.getElementById('up').textContent = up + '  (' + hex(up) + ')';
    document.getElementById('ulp').textContent = Number.isFinite(up - d) ? String(up - d) : 'n/a';
    document.getElementById('shown').textContent = excel15(d);
  }
  input.addEventListener('input', render);
  render();
})();

(function () {
  var Q = [
    { f: '=-2^2', o: ['-4', '4', '#VALUE!'], a: 1,
      t: 'Negation binds tighter than ^ in Excel.', d: 'Excel reads it as (-2)^2. Most programming languages, and mathematicians, say -4. Write =-(2^2) to get -4.' },
    { f: '=DATE(1900,2,29)', o: ['#NUM!', '1 March 1900', '29 February 1900'], a: 2,
      t: 'Excel thinks 1900 was a leap year.', d: 'Serial number 60 is a day that never happened, kept on purpose so old Lotus 1-2-3 files kept their dates. DNA Calc keeps day 60 too.' },
    { f: '=0.5-0.4-0.1', o: ['0', '-2.77555756156289E-17', '#NUM!'], a: 0,
      t: 'Excel snaps a tiny result of the last subtraction to zero.', d: 'In binary64 the exact result is -2.77555756156289E-17, and =1*(0.5-0.4-0.1) shows exactly that. Only the final + or - in a formula gets the snap.' },
    { f: '=SUM(A1:A2)', s: 'A1 and A2 both hold TRUE.', o: ['2', '0', '#VALUE!'], a: 1,
      t: 'SUM skips booleans that come from cells.', d: 'But =SUM(TRUE,TRUE) is 2: booleans typed straight into the arguments count. The same function, two rules, depending on where the value came from.' },
    { f: '="1"+"1"', o: ['"11"', '2', '#VALUE!'], a: 1,
      t: 'Arithmetic converts text that looks like a number.', d: 'The + operator coerces both texts to numbers. Use & to join text: ="1"&"1" gives "11".' },
    { f: '=MOD(-1,3)', o: ['-1', '2', '#NUM!'], a: 1,
      t: 'MOD takes the sign of the divisor.', d: 'Excel computes n - d*INT(n/d), so the result has the divisor\'s sign. C, Java and JavaScript give -1 here.' },
    { f: '=LEN(1/3)', o: ['17', '19', '#VALUE!'], a: 0,
      t: 'Numbers turn into text with 15 significant digits.', d: 'LEN sees "0.333333333333333": a zero, a point and fifteen 3s. The value in the cell keeps all 17 digits; its text does not.' },
    { f: '=ROUND(-2.5,0)', o: ['-2', '-3', '-2.5'], a: 1,
      t: 'Excel rounds halves away from zero.', d: 'No banker\'s rounding here: 2.5 goes to 3 and -2.5 to -3. Many languages round half to even and give -2.' },
    { f: '=WEEKDAY(1)', o: ['1 (Sunday)', '2 (Monday)', '7 (Saturday)'], a: 0,
      t: 'Excel says 1 January 1900 was a Sunday.', d: 'It was a Monday. Because of the phantom 29 February, every weekday before March 1900 is off by one, and DNA Calc is off by exactly the same one.' },
    { f: '=FACT(171)', o: ['1.24101807021766E+309', '#NUM!', 'Infinity'], a: 1,
      t: 'Past the largest binary64 number, Excel gives #NUM!.', d: 'FACT(170) is 7.25741561530799E+306, just under the limit of about 1.8E+308. Excel never shows infinity; a result that overflows is an error.' }
  ];
  var i = 0, score = 0, marks = [];
  var card = document.getElementById('card');
  function pips() {
    document.getElementById('pips').innerHTML = Q.map(function (_, k) { return '<i class="' + (marks[k] || '') + (k === i && marks[k] === undefined ? ' now' : '') + '"></i>'; }).join('');
    document.getElementById('score').textContent = 'Score ' + score + ' / ' + marks.filter(Boolean).length;
  }
  function esc(s) { return s.replace(/&/g, '&amp;').replace(/</g, '&lt;'); }
  function show() {
    pips();
    if (i >= Q.length) return end();
    var q = Q[i];
    card.innerHTML = '<div class="fx"><div class="name">B' + (i + 2) + '</div><div class="f">fx</div><div class="formula">' + esc(q.f) + '</div></div>' +
      '<div class="q">' + (q.s ? '<p class="setup">' + esc(q.s) + '</p>' : '') +
      '<div class="opts" role="group" aria-label="Possible answers">' + q.o.map(function (o, k) { return '<button type="button" data-k="' + k + '">' + esc(o) + '</button>'; }).join('') + '</div>' +
      '<div id="after"></div></div>';
    card.querySelectorAll('.opts button').forEach(function (b) { b.addEventListener('click', function () { pick(Number(b.dataset.k)); }); });
    card.querySelector('.opts button').focus({ preventScroll: true });
  }
  function pick(k) {
    var q = Q[i], ok = k === q.a;
    if (ok) score++;
    marks[i] = ok ? 'r' : 'w';
    card.querySelectorAll('.opts button').forEach(function (b, n) { b.disabled = true; if (n === q.a) b.classList.add('right'); else if (n === k) b.classList.add('wrong'); });
    var after = document.getElementById('after');
    after.innerHTML = '<p class="why' + (ok ? '' : ' miss') + '"><b>' + (ok ? 'Right. ' : 'Excel says ' + esc(q.o[q.a]) + '. ') + esc(q.t) + '</b><span>' + esc(q.d) + '</span></p>' +
      '<button class="next" type="button" id="next">' + (i + 1 < Q.length ? 'Next formula' : 'See the total') + '</button>';
    pips();
    var n = document.getElementById('next'); n.addEventListener('click', function () { i++; show(); }); n.focus({ preventScroll: true });
  }
  function end() {
    var verdict = score === 10 ? 'You think like Excel. That is not entirely a compliment.' : score >= 7 ? 'Close to bit-exact. A few ulps off.' : score >= 4 ? 'You think like a programmer, and Excel does not.' : 'You think like a sensible person. Excel had other plans.';
    card.innerHTML = '<div class="fx"><div class="name">B12</div><div class="f">fx</div><div class="formula">=SUM(B2:B11)</div></div><div class="q end"><h2>' + score + ' out of 10</h2><p>' + verdict + '</p><p class="setup">DNA Calc has to give Excel\'s answer to every one of these, including the ones that look like mistakes.</p><button class="next" type="button" id="again">Play again</button></div>';
    document.getElementById('again').addEventListener('click', function () { i = 0; score = 0; marks = []; show(); });
  }
  show();
})();

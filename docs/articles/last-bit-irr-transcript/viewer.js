const O = window.OVERLAY;
const $ = s => document.querySelector(s);
const esc = s => String(s ?? '').replace(/[&<>"]/g, c => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' }[c]));
let DATA = null, STEPS = [], MOM = new Map(O.moments.map(m => [m.i, m]));
const phaseOf = i => O.phases.find(p => i >= p.from && i <= p.to);

function highlight(text, q) {
  const e = esc(text);
  if (!q) return e;
  const rx = new RegExp(q.replace(/[.*+?^${}()|[\]\\]/g, '\\$&'), 'gi');
  return e.replace(rx, m => `<mark>${m}</mark>`);
}

function render() {
  const showThink = $('#f-think').checked, showCalls = $('#f-calls').checked, showOut = $('#f-out').checked,
        showSys = $('#f-sys').checked, keyOnly = $('#f-key').checked, thread = $('#f-thread').value,
        q = $('#f-q').value.trim();
  const ql = q.toLowerCase();
  let html = '', shown = 0, lastPhase = null;
  for (const s of STEPS) {
    const m = MOM.get(s.i);
    if (keyOnly && !m) continue;
    if (thread && !s.tags.includes(thread)) continue;
    if (!showSys && (s.kind === 'system' || s.kind === 'checkpoint') && !m) continue;
    if (s.kind === 'result' && !showOut && !(q && (s.out || '').toLowerCase().includes(ql))) continue;
    if (s.kind === 'think' && !showThink && !showCalls && !m) continue;
    if (q) {
      const hay = ((s.think || '') + ' ' + (s.calls || []).map(c => c.main + ' ' + c.args).join(' ') + ' ' + (s.out || '')).toLowerCase();
      if (!hay.includes(ql)) continue;
    }
    const ph = phaseOf(s.i);
    if (ph && ph !== lastPhase) {
      lastPhase = ph;
      const t0 = STEPS.find(x => x.i >= ph.from)?.t || '', t1 = [...STEPS].reverse().find(x => x.i <= ph.to)?.t || '';
      html += `<div class="phase" id="${ph.id}"><div class="when">${t0.slice(0, 5)}–${t1.slice(0, 5)} SAST · steps ${ph.from}–${ph.to}</div><h2>${esc(ph.title)}</h2><p>${esc(ph.note)}</p></div>`;
    }
    shown++;
    let body = '';
    if (m) body += `<div class="note"><div class="lab">Overlay · ${esc(m.thread)}</div><h4>${esc(m.title)}</h4><p>${esc(m.note)}</p></div>`;
    body += `<div class="chips"><span class="chip">${s.kind === 'think' ? 'reasoning' : s.kind === 'result' ? 'tool output' : s.kind}</span>${s.tags.map(t => `<span class="chip ${t}">${t}</span>`).join('')}${s.tok ? `<span class="chip">${(s.tok.out || 0).toLocaleString()} out tokens</span>` : ''}</div>`;
    if (s.think && showThink) body += `<div class="think">${highlight(s.think, q)}</div>`;
    if (s.calls && showCalls) for (const c of s.calls) {
      const first = (c.main || c.args || '').split('\n')[0].slice(0, 110);
      body += `<details class="call"><summary><span class="nm">${esc(c.name)}</span> ${highlight(first, q)}</summary>${c.main ? `<pre>${highlight(c.main, q)}</pre>` : ''}${c.args && c.args !== '{}' ? `<pre>${highlight(c.args, q)}</pre>` : ''}</details>`;
    }
    if (s.out && (s.kind !== 'result' || showOut || q)) body += `<details class="out"${q ? ' open' : ''}><summary>${s.kind === 'result' ? 'output' : s.kind} · ${s.out.length.toLocaleString()} chars</summary><pre>${highlight(s.out, q)}</pre></details>`;
    html += `<div class="step kind-${s.kind}" id="s${s.i}"><div class="gut"><b>${s.t.slice(0, 5)}</b>#${s.i}</div><div>${body}</div></div>`;
  }
  $('#stream').innerHTML = html || '<p class="empty">No steps match these filters.</p>';
  $('#count').textContent = `${shown} of ${STEPS.length} steps`;
}

function jump(i) {
  showView('transcript');
  let el = document.getElementById('s' + i);
  if (!el) { $('#f-key').checked = false; $('#f-thread').value = ''; $('#f-q').value = ''; render(); el = document.getElementById('s' + i); }
  if (!el) return;
  el.querySelectorAll('details').forEach(d => d.open = true);
  // One scroll, to the step's top minus the sticky header, with no smooth animation to fight.
  const head = document.querySelector('header').getBoundingClientRect().height;
  const y = el.getBoundingClientRect().top + window.scrollY - head - 12;
  window.scrollTo({ top: Math.max(0, y), behavior: 'auto' });
  document.querySelectorAll('.step.here').forEach(x => x.classList.remove('here'));
  el.classList.add('here');
}

function strip() {
  const svg = $('#strip'), W = svg.clientWidth || 1000, H = 46, n = STEPS.length, max = STEPS.at(-1).i || 1;
  const x = i => 6 + (W - 12) * i / max;
  let g = '';
  O.phases.forEach((p, k) => g += `<rect x="${x(p.from)}" y="0" width="${Math.max(1, x(p.to) - x(p.from))}" height="12" fill="var(--${k % 2 ? 'code' : 'line'})"><title>${esc(p.title)}</title></rect>`);
  const outMax = Math.max(...STEPS.map(s => s.tok?.out || 0));
  for (const s of STEPS) {
    if (s.kind !== 'think') continue;
    const h = 4 + 18 * Math.sqrt((s.tok?.out || 0) / outMax);
    const col = s.tags.includes('IRR') && !s.tags.includes('RATE') ? 'var(--irr)' : s.tags.includes('RATE') && !s.tags.includes('IRR') ? 'var(--rate)' : 'var(--muted)';
    g += `<rect x="${x(s.i) - 1}" y="${40 - h}" width="2" height="${h}" fill="${col}" opacity=".85"></rect>`;
  }
  for (const m of O.moments) g += `<g data-i="${m.i}"><circle cx="${x(m.i)}" cy="16" r="4.5" fill="var(--panel)" stroke="var(--${m.thread === 'RATE' ? 'rate' : m.thread === 'Excel' ? 'xl' : 'irr'})" stroke-width="2"></circle><title>#${m.i} ${esc(m.title)}</title></g>`;
  svg.setAttribute('viewBox', `0 0 ${W} ${H}`);
  svg.innerHTML = g;
  svg.onclick = e => { const r = svg.getBoundingClientRect(); const i = Math.round((e.clientX - r.left - 6) / (r.width - 12) * max); const near = STEPS.reduce((a, s) => Math.abs(s.i - i) < Math.abs(a.i - i) ? s : a); jump(near.i); };
}

function sidebar() {
  $('#phases').innerHTML = O.phases.map(p => `<a href="#" data-j="${p.from}"><span class="pt">${(STEPS.find(s => s.i >= p.from)?.t || '').slice(0, 5)}</span>${esc(p.title)}</a>`).join('');
  $('#moments').innerHTML = O.moments.map(m => `<a href="#" data-j="${m.i}"><span class="dot t-${m.thread}"></span><span class="pt">#${m.i}</span>${esc(m.title)}</a>`).join('');
  document.querySelectorAll('aside a').forEach(a => a.onclick = e => { e.preventDefault(); jump(+a.dataset.j); });
}

function argument() {
  const st = new Map(STEPS.map(s => [s.i, s]));
  const t = i => (st.get(i)?.t || '').slice(0, 5);
  let h = `<h2>How cb25 found IRR's exact solver</h2>
  <p class="lede">The smart fuzzer had left four IRR clusters with 7,129 rows where DNA Calc and Excel disagreed, most of them a number against an error. In two and a half hours, Carbon rebuilt master's IRR exactly, read off the shape from Excel's extreme answers, and arrived at five rules that match all 22,749 sweep rows. It then proved them on 96 unseen probes with rival rules registered in advance: 96 of 96, where master scores 35.</p>
  <h2>The five rules</h2>`;
  for (const f of O.findings) h += `<div class="finding"><div class="n">${f.n}</div><div><h3>${esc(f.title)} <span class="chip IRR">${esc(f.bead)}</span></h3><p>${esc(f.rule)}</p><p class="ev">${esc(f.evidence)}</p><p class="ev">First seen <button class="jump" data-j="${f.first}">#${f.first} ${t(f.first)}</button> · settled <button class="jump" data-j="${f.settled}">#${f.settled} ${t(f.settled)}</button></p></div></div>`;
  h += `<h2>Unseen probes on live Excel</h2><p class="ev">Frozen and hashed before any question was asked (SHA-256 7440deab…). Bars show matches out of the block size.</p><div class="score"><b>Block</b><b>cb25</b><b>master</b>`;
  for (const r of O.scorecard) h += `<span>${esc(r.block)} <span class="ev">(${r.n})</span></span><span><div class="bar" title="${r.cb25}/${r.n}"><i class="bar-cb25" data-pct="${100 * r.cb25 / r.n}"></i></div>${r.cb25}/${r.n}</span><span><div class="bar" title="${r.master}/${r.n}"><i class="bar-master" data-pct="${100 * r.master / r.n}"></i></div>${r.master}/${r.n}</span>`;
  h += `</div><h2>How the argument evolved</h2><ol class="evol">`;
  for (const m of O.moments) h += `<li class="${m.thread}"><span class="tm">${t(m.i)} · #${m.i} · ${esc(m.thread)}</span><br><b>${esc(m.title)}</b> — ${esc(m.note)} <button class="jump" data-j="${m.i}">open step</button></li>`;
  h += `</ol>`;
  $('#arg').innerHTML = h;
  $('#arg').querySelectorAll('.bar i[data-pct]').forEach(el => { el.style.width = el.dataset.pct + '%'; });
  $('#arg').querySelectorAll('.jump').forEach(b => b.onclick = () => jump(+b.dataset.j));
}

function md(src) {
  const lines = src.split('\n'); let out = '', i = 0;
  const inl = s => esc(s).replace(/`([^`]+)`/g, '<code>$1</code>').replace(/\*\*([^*]+)\*\*/g, '<b>$1</b>').replace(/\$([^$]+)\$/g, '<code>$1</code>');
  while (i < lines.length) {
    const l = lines[i];
    if (/^#{1,4} /.test(l)) { const n = l.match(/^#+/)[0].length; out += `<h${n + 1}>${inl(l.slice(n + 1))}</h${n + 1}>`; i++; continue; }
    if (/^\|/.test(l)) { let rows = []; while (i < lines.length && /^\|/.test(lines[i])) rows.push(lines[i++]); rows = rows.filter(r => !/^\|\s*:?-/.test(r)); out += '<div class="tw"><table>' + rows.map((r, k) => '<tr>' + r.replace(/^\||\|$/g, '').split('|').map(c => `<${k ? 'td' : 'th'}>${inl(c.trim())}</${k ? 'td' : 'th'}>`).join('') + '</tr>').join('') + '</table></div>'; continue; }
    if (/^\s*[-*] /.test(l)) { out += '<ul>'; while (i < lines.length && /^\s*[-*] /.test(lines[i])) out += `<li>${inl(lines[i++].replace(/^\s*[-*] /, ''))}</li>`; out += '</ul>'; continue; }
    if (/^\d+\. /.test(l)) { out += '<ol>'; while (i < lines.length && /^\s*\d+\. /.test(lines[i])) out += `<li>${inl(lines[i++].replace(/^\s*\d+\. /, ''))}</li>`; out += '</ol>'; continue; }
    if (l.trim()) out += `<p>${inl(l)}</p>`;
    i++;
  }
  return out;
}

function showView(v) {
  document.querySelectorAll('nav.tabs button').forEach(b => b.setAttribute('aria-selected', b.dataset.v === v));
  document.querySelectorAll('section.view').forEach(s => s.classList.toggle('on', s.id === 'v-' + v));
  $('#strip').style.display = v === 'transcript' ? '' : 'none';
}
document.querySelectorAll('nav.tabs button').forEach(b => b.onclick = () => showView(b.dataset.v));
['#f-think', '#f-calls', '#f-out', '#f-sys', '#f-key', '#f-thread'].forEach(s => $(s).onchange = render);
let qt; $('#f-q').oninput = () => { clearTimeout(qt); qt = setTimeout(render, 220); };
function headSize() { document.documentElement.style.setProperty('--head', (document.querySelector('header').getBoundingClientRect().height + 12) + 'px'); }
addEventListener('resize', () => { headSize(); STEPS.length && strip(); });
headSize();

fetch('data.json').then(r => r.json()).then(d => {
  DATA = d; STEPS = d.steps; const m = d.meta;
  $('#meta').textContent = `${m.model} · ${m.host} · ${m.start.slice(0, 5)}–${m.end.slice(0, 5)} SAST, 24 Nov 2026 · ${m.steps} steps · ${m.tool_calls} tool calls · ${(m.out_tokens / 1000).toFixed(0)}k output tokens`;
  sidebar(); strip(); render(); argument();
}).catch(() => { $('#stream').innerHTML = '<p class="empty">The transcript data did not load. Reload the page to try again.</p>'; });
fetch('review.json').then(r => r.json()).then(d => { $('#review').innerHTML = md(d.review); }).catch(() => { $('#review').innerHTML = '<p class="empty">The review did not load.</p>'; });

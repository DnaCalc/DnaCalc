// Captures the Formula Explorer (skin 0) as embedded at dnacalc.com/onecalc/, opened from a fragment link.
import { browser } from "./cdp.mjs";
const out = `${process.env.HOME}/codex-runs/sg1/shots`;
const b = await browser(1100, 1000, 2);
await b.goto("https://dnacalc.com/onecalc/#f=%3DPMT%286.5%25%2F12%2C360%2C-250000%29", 11000);
const frame = '[data-skin0="frame"]';
await b.eval(`document.querySelector('${frame}').scrollIntoView({block:'start'})`);
await b.sleep(500);
const press = async (sel) => b.eval(`(() => { const e = document.querySelector('${sel}'); if (!e) return 'missing ${sel}'; e.click(); return 'clicked'; })()`);
console.log(await press('[data-skin0="evaluate"]'));
await b.sleep(1500);
console.log(await b.eval(`JSON.stringify([...document.querySelectorAll('${frame} button')].map(e => e.innerText + '|' + (e.getAttribute('data-skin0')||'')).slice(0,40))`));
// show the steps of the formula if a steps button exists
console.log(await b.eval(`(() => { const e = [...document.querySelectorAll('${frame} button')].find(e => /step/i.test(e.innerText)); if (e) { e.click(); return 'steps: ' + e.innerText; } return 'no steps button'; })()`));
await b.sleep(1500);
const r = JSON.parse(await b.eval(`(() => { const r = document.querySelector('${frame}').getBoundingClientRect(); return JSON.stringify({x: r.x + scrollX, y: r.y + scrollY, width: r.width, height: r.height}); })()`));
console.log(r);
await b.shot(`${out}/explorer-frame.png`, r);
console.log(await b.eval(`document.querySelector('${frame}').innerText.slice(0, 1500)`));
console.log(await b.eval(`JSON.stringify([...document.querySelectorAll('${frame} [data-skin0], ${frame} section, ${frame} header, ${frame} footer')].map(e => { const q = e.getBoundingClientRect(), f = document.querySelector('${frame}').getBoundingClientRect(); return [(e.getAttribute('data-skin0') || e.className || e.tagName), Math.round(q.x - f.x), Math.round(q.y - f.y), Math.round(q.width), Math.round(q.height)]; }))`));
await b.close();

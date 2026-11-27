// Captures the Classic preview as deployed at dnacalc.com/app/preview/ with a small loan model typed in.
import { browser } from "./cdp.mjs";
const out = `${process.env.HOME}/codex-runs/sg1/shots`;
const dark = process.argv[2] === "dark";
const b = await browser(1000, 520, 2, dark);
await b.goto("https://dnacalc.com/app/preview/", 12000);
const probe = async () => b.eval("JSON.stringify(window.__dnaProbe()).slice(0, 300)");
const box = async (sel) => JSON.parse(await b.eval(`(() => { const r = document.querySelector('${sel}').getBoundingClientRect(); return JSON.stringify([r.x, r.y, r.width, r.height]); })()`));
const rows = [["Loan", "250000"], ["Rate", "0.065"], ["Years", "30"], ["Payment", "=PMT(B2/12,B3*12,-B1)"], ["Total paid", "=B4*B3*12"], ["Interest", "=B5-B1"]];
const x = [77, 136], y0 = 143, dy = 22.6;
for (let r = 0; r < rows.length; r++) for (let c = 0; c < 2; c++) {
  await b.click(x[c], y0 + r * dy);
  await b.type(rows[r][c]); await b.key("Enter"); await b.sleep(350);
}
// leave B4 selected and open the edit so the Bridge shows the seam label
await b.click(x[1], y0 + 3 * dy); await b.sleep(400);
console.log(await probe());
console.log(await b.eval("document.querySelector('[data-dnacalc=\"strip\"]').innerText"));
await b.shot(`${out}/classic-ready${dark ? "-dark" : ""}.png`);
await b.key("F2"); await b.sleep(500);
console.log(await b.eval("document.querySelector('.dna-book-bridge').innerText"));
await b.shot(`${out}/classic-edit${dark ? "-dark" : ""}.png`);
await b.key("Escape");
await b.key("q", { alt: true }); await b.sleep(400); await b.type("calc"); await b.sleep(500);
await b.shot(`${out}/classic-search${dark ? "-dark" : ""}.png`);
for (const s of [".dna-book-mast", ".dna-book-deck", ".dna-book-bridge", ".dna-book-stage", ".dna-book-sheets", ".dna-book-strip"]) console.log(s, await box(s));
await b.close();

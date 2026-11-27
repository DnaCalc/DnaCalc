// Minimal CDP driver for bun: launches headless Chromium, runs steps, saves PNG screenshots.
import { spawn } from "node:child_process";
import { writeFileSync } from "node:fs";
const CHROME = process.env.CHROME;
export async function browser(width = 1280, height = 800, scale = 1, dark = false) {
  const port = 9300 + Math.floor(Math.random() * 500);
  const proc = spawn(CHROME, ["--headless=new", `--remote-debugging-port=${port}`, "--no-sandbox", "--hide-scrollbars",
    `--window-size=${width},${height}`, `--user-data-dir=${process.env.HOME}/codex-runs/sg1/shots/profile-${port}`, "about:blank"], { stdio: "ignore" });
  let target;
  for (let i = 0; i < 50; i++) {
    try { const list = await (await fetch(`http://127.0.0.1:${port}/json`)).json(); target = list.find(t => t.type === "page"); if (target) break; } catch {}
    await new Promise(r => setTimeout(r, 200));
  }
  const ws = new WebSocket(target.webSocketDebuggerUrl);
  await new Promise(r => ws.addEventListener("open", r));
  let id = 0; const waiting = new Map();
  ws.addEventListener("message", e => { const m = JSON.parse(e.data); if (m.id && waiting.has(m.id)) { waiting.get(m.id)(m); waiting.delete(m.id); } });
  const send = (method, params = {}) => new Promise(res => { const i = ++id; waiting.set(i, m => res(m.result ?? m)); ws.send(JSON.stringify({ id: i, method, params })); });
  await send("Page.enable"); await send("Runtime.enable");
  await send("Emulation.setDeviceMetricsOverride", { width, height, deviceScaleFactor: scale, mobile: false });
  if (dark) await send("Emulation.setEmulatedMedia", { features: [{ name: "prefers-color-scheme", value: "dark" }] });
  const sleep = ms => new Promise(r => setTimeout(r, ms));
  const api = {
    send, sleep,
    goto: async (url, wait = 4000) => { await send("Page.navigate", { url }); await sleep(wait); },
    eval: async (expression) => { const r = await send("Runtime.evaluate", { expression, awaitPromise: true, returnByValue: true }); return r.result?.value ?? JSON.stringify(r); },
    shot: async (file, clip) => { const r = await send("Page.captureScreenshot", { format: "png", captureBeyondViewport: !!clip, ...(clip ? { clip: { ...clip, scale: 1 } } : {}) }); writeFileSync(file, Buffer.from(r.data, "base64")); },
    key: async (key, opts = {}) => {
      const mods = (opts.alt ? 1 : 0) | (opts.ctrl ? 2 : 0) | (opts.shift ? 8 : 0);
      const codes = { Enter: 13, Escape: 27, Tab: 9, ArrowDown: 40, ArrowUp: 38, ArrowLeft: 37, ArrowRight: 39, F2: 113, F9: 120, F10: 121, Backspace: 8 };
      const vk = codes[key] ?? key.toUpperCase().charCodeAt(0);
      const code = codes[key] ? key : (/[0-9]/.test(key) ? `Digit${key}` : `Key${key.toUpperCase()}`);
      const base = { key, code, windowsVirtualKeyCode: vk, nativeVirtualKeyCode: vk, modifiers: mods };
      await send("Input.dispatchKeyEvent", { type: "rawKeyDown", ...base });
      if (key === "Enter") await send("Input.dispatchKeyEvent", { type: "char", text: "\r", ...base });
      await send("Input.dispatchKeyEvent", { type: "keyUp", ...base }); await sleep(150);
    },
    type: async (text) => { for (const ch of text) { await send("Input.dispatchKeyEvent", { type: "keyDown", key: ch, text: ch }); await send("Input.dispatchKeyEvent", { type: "keyUp", key: ch }); await sleep(20); } },
    click: async (x, y) => { for (const type of ["mousePressed", "mouseReleased"]) await send("Input.dispatchMouseEvent", { type, x, y, button: "left", clickCount: 1 }); await sleep(200); },
    close: async () => { ws.close(); proc.kill(); },
  };
  return api;
}

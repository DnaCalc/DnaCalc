#!/usr/bin/env node
import { execFile, spawn } from "node:child_process";
import { readFile, writeFile } from "node:fs/promises";
import { basename, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { parseArgs, promisify } from "node:util";
import { setTimeout as sleep } from "node:timers/promises";

const { values: args } = parseArgs({ options: {
  port: { type: "string", default: "9222" },
  scenario: { type: "string", default: fileURLToPath(new URL("./scenarios.json", import.meta.url)) },
  case: { type: "string" },
  workbook: { type: "string" },
  url: { type: "string" },
  match: { type: "string" },
  out: { type: "string" },
  "webview-exe": { type: "string" },
  "webview-data": { type: "string" },
  "width": { type: "string" },
  "height": { type: "string" },
  "painter": { type: "string", default: "dom" },
} });

const GRID = "[data-dnacalc=grid][data-grid-active=true]";
const READY = GRID + "[data-surface-ready=true]";
const EDITOR = "[data-dnacalc=cell-editor], .dc-cell-editor textarea, .dc-cell-editor [contenteditable=true]";
const NAME = "[data-dnacalc=name-box]";
const OPEN = "[data-dnacalc=open-file]";
const KEY_CODES = new Map(Object.entries({
  End: 35, Home: 36, ArrowDown: 40, ArrowRight: 39, PageDown: 34, Enter: 13, Escape: 27, F2: 113, a: 65,
}));
const PENDING_TIMEOUT = 30000;
const endpoint = `http://127.0.0.1:${parseInt(args.port, 10)}`;

class Cdp {
  constructor(socket) {
    this.socket = socket;
    this.next = 0;
    this.pending = new Map();
    socket.addEventListener("message", ({ data }) => {
      const msg = JSON.parse(data);
      if (!this.pending.has(msg.id)) return;
      const { resolve, reject, timer } = this.pending.get(msg.id);
      this.pending.delete(msg.id);
      clearTimeout(timer);
      if (msg.error) reject(new Error(msg.error.message));
      else resolve(msg.result);
    });
    socket.addEventListener("close", () => {
      for (const { reject, timer } of this.pending.values()) {
        clearTimeout(timer);
        reject(new Error("CDP connection closed"));
      }
      this.pending.clear();
    });
  }

  static async open(url) {
    const address = new URL(url);
    if (!["localhost", "127.0.0.1", "[::1]"].includes(address.hostname)) {
      throw new Error("Expected a loopback CDP websocket");
    }
    const socket = new WebSocket(address);
    const timer = setTimeout(() => socket.close(), PENDING_TIMEOUT);
    try {
      await new Promise((resolve, reject) => {
        socket.addEventListener("open", resolve, { once: true });
        socket.addEventListener("error", () => reject(new Error("CDP connection failed")), { once: true });
        socket.addEventListener("close", () => reject(new Error("CDP connection closed")), { once: true });
      });
    } finally {
      clearTimeout(timer);
    }
    return new Cdp(socket);
  }

  send(method, params = {}) {
    const id = ++this.next;
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => {
        this.pending.delete(id);
        reject(new Error(`CDP timed out in ${method}`));
      }, PENDING_TIMEOUT);
      this.pending.set(id, { resolve, reject, timer });
      this.socket.send(JSON.stringify({ id, method, params }));
    });
  }

  async evaluate(source) {
    const { result, exceptionDetails } = await this.send("Runtime.evaluate", {
      expression: source, awaitPromise: true, returnByValue: true,
    });
    if (exceptionDetails) throw new Error(result.description ?? exceptionDetails.text);
    return result.value;
  }

  close() { this.socket.close(); }
}

async function waitFor(probe, description, timeout = PENDING_TIMEOUT) {
  const expires = performance.now() + timeout;
  while (performance.now() < expires) {
    const result = await probe();
    if (result) return result;
    await sleep(20);
  }
  throw new Error(`Timed out waiting for ${description}`);
}

function distribution(values) {
  const sorted = [...values].sort((a, b) => a - b);
  const percentile = p => sorted[Math.ceil(p * sorted.length) - 1] ?? null;
  return {
    samples: sorted.length,
    p50_ms: percentile(0.5), p95_ms: percentile(0.95), p99_ms: percentile(0.99),
    over_16_7_ms: sorted.filter(x => x > 16.7).length,
    over_33_ms: sorted.filter(x => x > 33).length,
  };
}

// The injected recorder runs during scripted activity only. Two rAF callbacks
// bracket a rendering opportunity. Neither callback proves display scanout.
function installRecorder(gridSelector, editorSelector) {
  const state = window.__ferro_eval = { frames: [], keys: [], handle: 0, firstGrid: null };
  const afterPaint = callback => requestAnimationFrame(() => requestAnimationFrame(callback));
  const observeFirstGrid = () => {
    if (state.firstGrid !== null || !document.querySelector(gridSelector)?.getClientRects().length) return;
    state.firstGrid = false;
    afterPaint(() => { state.firstGrid = performance.now(); });
  };
  const observer = new MutationObserver(() => {
    observeFirstGrid();
    if (state.firstGrid !== null) observer.disconnect();
  });
  observer.observe(document, { childList: true, subtree: true, attributes: true, attributeFilter: ["data-surface-ready"] });
  observeFirstGrid();
  state.start = () => {
    state.frames = [];
    state.keys = [];
    const record = time => {
      state.frames.push(time);
      state.handle = requestAnimationFrame(record);
    };
    state.handle = requestAnimationFrame(record);
  };
  state.stop = () => {
    cancelAnimationFrame(state.handle);
    state.handle = 0;
    return { frames: state.frames, keys: state.keys };
  };
  window.addEventListener("input", event => {
    if (!state.handle || !event.target.matches(editorSelector)) return;
    const input = performance.now();
    const length = (event.target.value ?? event.target.textContent).length;
    afterPaint(paint => state.keys.push({ input, paint, length }));
  }, true);
}

const recorder = `;
(${installRecorder.toString()})(${JSON.stringify(READY)}, ${JSON.stringify(EDITOR)})`;

async function key(cdp, value, modifiers = 0, text) {
  const code = KEY_CODES.get(value) ?? value.toUpperCase().charCodeAt(0);
  const physical = value.length === 1
    ? (/\d/.test(value) ? `Digit${value}` : `Key${value.toUpperCase()}`)
    : value;
  const data = { key: value, code: physical, modifiers, windowsVirtualKeyCode: code };
  await cdp.send("Input.dispatchKeyEvent", {
    ...data, type: text === undefined ? "rawKeyDown" : "keyDown", ...(text === undefined ? {} : { text, unmodifiedText: text }),
  });
  await cdp.send("Input.dispatchKeyEvent", { ...data, type: "keyUp" });
}

async function typeField(cdp, selector, text) {
  if (!await cdp.evaluate(`Boolean(document.querySelector(${JSON.stringify(selector)}))`)) {
    throw new Error(`Field missing: ${selector}`);
  }
  await cdp.evaluate(`document.querySelector(${JSON.stringify(selector)}).focus()`);
  await key(cdp, "a", 2);
  await cdp.send("Input.insertText", { text });
  await key(cdp, "Enter");
}

const snapshot = async cdp => cdp.evaluate(`(() => {
  const el = document.querySelector(${JSON.stringify(GRID)});
  return {
    width: el.clientWidth, height: el.clientHeight, x: el.scrollLeft, y: el.scrollTop,
    window: [innerWidth, innerHeight, devicePixelRatio],
    frozen: [Number(el.dataset.frozenRows), Number(el.dataset.frozenColumns)],
    rows: [...new Set([...el.querySelectorAll('[data-row][data-dnacalc=row-header]')].map(h => Number(h.dataset.row)))],
    columns: [...new Set([...el.querySelectorAll('[data-column][data-dnacalc=column-header]')].map(h => h.dataset.column))],
    area: el.querySelector('[data-dnacalc=selection]')?.dataset.area ?? null,
    name: document.querySelector(${JSON.stringify(NAME)}).value,
  };
})()`);

async function upload(cdp, filename, selector = OPEN) {
  const { root } = await cdp.send("DOM.getDocument");
  const { nodeId } = await cdp.send("DOM.querySelector", { nodeId: root.nodeId, selector });
  if (!nodeId) throw new Error("Workbook file input missing");
  await cdp.send("DOM.setFileInputFiles", { nodeId, files: [resolve(filename)] });
}

async function workbookReady(cdp, filename) {
  const name = JSON.stringify(basename(filename));
  await waitFor(() => cdp.evaluate(`(() => {
    const doc = document.querySelector('[data-dnacalc=document]');
    const grid = document.querySelector(${JSON.stringify(READY)});
    return Boolean(grid?.getClientRects().length && doc?.textContent.includes(${name}));
  })()`), `loaded ${basename(filename)}`, 120000);
}

async function raf(cdp) {
  return cdp.evaluate("new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve)))");
}

async function paced(items, interval, action, startAt = performance.now()) {
  let index = 0;
  for (const item of items) {
    const wait = startAt + index++ * interval - performance.now();
    if (wait > 0) await sleep(wait);
    await action(item);
  }
}

function steps(count, units) {
  if (!Number.isInteger(units) || units <= 0) throw new Error("Invalid wheel unit count");
  const result = [];
  for (let left = Math.abs(count); left > 0; left -= units) {
    result.push(Math.sign(count) * Math.min(left, units));
  }
  return result;
}

async function wheel(cdp, scenario, checkpoints) {
  const axis = scenario.axis === "rows" ? "Y" : "X";
  const metric = await cdp.evaluate(`(() => {
    const grid = document.querySelector(${JSON.stringify(GRID)});
    const target = grid.getBoundingClientRect();
    const header = grid.querySelector('[data-dnacalc=${axis === "Y" ? "row" : "column"}-header]').getBoundingClientRect();
    return { x: target.left + target.width / 2, y: target.top + target.height / 2,
      unit: header.${axis === "Y" ? "height" : "width"} };
  })()`);
  if (!metric.unit) throw new Error("Grid header has zero extent");
  for (const count of scenario.counts) {
    await paced(steps(count, scenario.units_per_event), scenario.interval_ms, amount =>
      cdp.send("Input.dispatchMouseEvent", { type: "mouseWheel", x: metric.x, y: metric.y,
        deltaX: axis === "X" ? amount * metric.unit : 0, deltaY: axis === "Y" ? amount * metric.unit : 0 }));
    await raf(cdp);
    checkpoints.push(await snapshot(cdp));
  }
  const columnNumber = name => [...name].reduce((sum, ch) => sum * 26 + ch.charCodeAt(0) - 64, 0);
  const offset = axis === "Y"
    ? info => Math.min(...info.rows.filter(row => row > info.frozen[0])) - 2
    : info => Math.min(...info.columns.map(columnNumber).filter(col => col > info.frozen[1])) - 2;
  let target = 0;
  for (const [index, checkpoint] of checkpoints.entries()) {
    target += scenario.counts[index];
    // Rendered headers may include one overscan row or column adjacent to the viewport.
    if (Math.abs(offset(checkpoint) - target) > 2) {
      throw new Error(`Wheel checkpoint ${index + 1}: expected offset ${target}, saw ${offset(checkpoint)}`);
    }
  }
}

async function viewSize(cdp, width, height) {
  await cdp.send("Emulation.setDeviceMetricsOverride", { width, height, deviceScaleFactor: 1, mobile: false });
}

const ferroSnapshot = cdp => cdp.evaluate("window.ferroEval?.state() ?? null");

async function executeFerro(cdp, scenario, size, report) {
  // Focus the grid through a native pointer event first; FerroUI owns the focus
  // for the canvas and its text input. Selection needs to start at A1.
  const x = 90, y = 70;
  for (const type of ["mousePressed", "mouseReleased"]) {
    await cdp.send("Input.dispatchMouseEvent", { type, x, y, button: "left", clickCount: 1 });
  }
  const text = ch => key(cdp, ch, 0, ch);
  const observed = () => ferroSnapshot(cdp);
  const paints = [];
  const waitPaint = async (before, condition, description) => {
    const result = await waitFor(async () => {
      const state = await observed();
      return state && state.frames > before && condition(state) ? state : null;
    }, description);
    await raf(cdp);
    return result;
  };
  switch (scenario.action) {
    case "wheel": {
      const axes = scenario.axis === "rows";
      let target = 2;
      for (const count of scenario.counts) {
        const before = (await observed()).frames;
        // Browser input maps 50 CSS pixels to one wheel unit; SheetGrid maps one wheel
        // unit to three grid rows or columns. Pass the same signed logical step.
        await paced(steps(count, scenario.units_per_event), scenario.interval_ms, amount =>
          cdp.send("Input.dispatchMouseEvent", { type: "mouseWheel", x: size[0] / 2, y: size[1] / 2,
            deltaX: axes ? 0 : amount * 50 / 3, deltaY: axes ? amount * 50 / 3 : 0 }));
        target += count;
        report.checkpoints.push(await waitPaint(before, state => state.first[axes ? 0 : 1] === target, "FerroUI wheel destination"));
      }
      break;
    }
    case "keys": {
      const keys = scenario.keys.flatMap(step => Array.from({ length: step.count }, () => step));
      const before = (await observed()).frames;
      await paced(keys, scenario.interval_ms, step => key(cdp, step.key, step.shift ? 8 : 0));
      const result = await waitPaint(before, state => state.selected[0] > 1, "FerroUI keyboard navigation");
      if (scenario.id === "S6" && (result.selected.join() !== "500,50" || result.anchor.join() !== "1,1")) {
        throw new Error("FerroUI 500×50 selection did not reach AX500");
      }
      break;
    }
    case "jump":
      await paced(scenario.addresses, scenario.interval_ms, async address => {
        const before = (await observed()).frames;
        const end = address === "XFD1048576";
        await key(cdp, end ? "End" : "Home", 2);
        report.checkpoints.push(await waitPaint(before,
          state => state.selected.join() === (end ? "1048576,16384" : "1,1"), "FerroUI far address"));
      });
      break;
    case "typing": {
      const characters = [...scenario.text];
      const keys = Array.from({ length: scenario.count }, (_, index) => characters[index % characters.length]);
      let draft = "";
      await paced(keys, scenario.interval_ms, async ch => {
        const before = (await observed()).frames;
        const start = performance.now();
        draft += ch;
        await text(ch);
        await waitPaint(before, state => state.editor === draft, "FerroUI typed glyph");
        paints.push(performance.now() - start);
      });
      report.ferro_latencies = paints;
      break;
    }
    case "recalc": {
      await text(scenario.text);
      const before = (await observed()).frames;
      const start = performance.now();
      await key(cdp, "Enter");
      await waitPaint(before, state => state.editor === null, "FerroUI edit commit");
      report.recalc_paint_ms = performance.now() - start;
      if (!await cdp.evaluate(`ferroEval.verifyDependents(${JSON.stringify(scenario.dependent_range)}, ${scenario.dependent_count})`)) {
        throw new Error("FerroUI did not calculate all W1 dependents");
      }
      break;
    }
    case "resize_zoom":
      report.resize_method = "CDP device metrics override";
      await paced(scenario.resize_scales, scenario.interval_ms, async scale => {
        await viewSize(cdp, Math.round(size[0] * scale), Math.round(size[1] * scale));
        await raf(cdp);
        report.checkpoints.push(await observed());
      });
      await paced(scenario.zoom_percents, scenario.interval_ms, async percent => {
        const before = (await observed()).frames;
        const x = size[0] - 45;
        for (const type of ["mousePressed", "mouseReleased"]) {
          await cdp.send("Input.dispatchMouseEvent", { type, x, y: 15, button: "left", clickCount: 1 });
        }
        await key(cdp, "a", 2);
        await cdp.send("Input.insertText", { text: String(percent) });
        await key(cdp, "Enter");
        if ((await observed()).zoom !== percent) {
          await waitPaint(before, state => state.zoom === percent, "FerroUI worksheet zoom");
        }
      });
      break;
    default:
      throw new Error(`Unknown FerroUI scenario ${scenario.action}`);
  }
}

async function execute(cdp, scenario, size, report) {
  const focusGrid = () => cdp.evaluate(`document.querySelector(${JSON.stringify(GRID)}).focus()`);
  if (scenario.action !== "typing") await focusGrid();
  switch (scenario.action) {
    case "wheel":
      await wheel(cdp, scenario, report.checkpoints);
      break;
    case "keys": {
      const keys = scenario.keys.flatMap(step => Array.from({ length: step.count }, () => step));
      await paced(keys, scenario.interval_ms, step => key(cdp, step.key, step.shift ? 8 : 0));
      break;
    }
    case "jump":
      await paced(scenario.addresses, scenario.interval_ms, async address => {
        await typeField(cdp, NAME, address);
        await raf(cdp);
        const observed = await snapshot(cdp);
        report.checkpoints.push(observed);
        if (observed.name !== address) throw new Error(`Expected name box ${address}, saw ${observed.name}`);
      });
      break;
    case "typing": {
      const characters = [...scenario.text];
      const keys = Array.from({ length: scenario.count }, (_, index) => characters[index % characters.length]);
      await paced(keys, scenario.interval_ms, ch => key(cdp, ch, 0, ch));
      const draft = await cdp.evaluate(`(() => {
        const el = document.querySelector(${JSON.stringify(EDITOR)});
        return el?.value ?? el?.textContent ?? "";
      })()`);
      if (draft !== keys.join("")) throw new Error(`Expected ${keys.length} typed characters, saw ${draft.length}`);
      break;
    }
    case "recalc": {
      await key(cdp, scenario.text, 0, scenario.text);
      await waitFor(() => cdp.evaluate(`Boolean(document.querySelector(${JSON.stringify(EDITOR)}))`), "cell edit field");
      const before = await cdp.evaluate("performance.now()");
      await key(cdp, "Enter");
      const visible = await waitFor(() => cdp.evaluate(`(() => {
        if (document.querySelector(${JSON.stringify(EDITOR)})) return false;
        const grid = document.querySelector(${JSON.stringify(GRID)});
        const view = grid.getBoundingClientRect();
        const verified = [];
        for (const el of grid.querySelectorAll('[data-dnacalc=cell][data-formula=true]')) {
          const match = el.dataset.cell.match(/^([A-U])(\\d+)$/);
          if (!match) continue;
          const col = match[1].charCodeAt(0) - 65, row = Number(match[2]);
          if (col < 1 || row < 2 || row > 501) continue;
          const rect = el.getBoundingClientRect();
          if (rect.right <= view.left || rect.left >= view.right || rect.bottom <= view.top || rect.top >= view.bottom) continue;
          const expected = 2 + (row - 2) * 20 + col;
          if (el.dataset.provenance !== "calculated" || Number(el.querySelector(".dc-cell-text").textContent.trim()) !== expected) return false;
          verified.push(el.dataset.cell);
        }
        return verified.includes("B2") ? verified : false;
      })()`), "all visible calculated W1 dependents");
      report.visible_dependents = visible.length;
      report.recalc_paint_ms = (await raf(cdp)) - before;
      break;
    }
    case "resize_zoom":
      report.resize_method = "CDP device metrics override";
      await paced(scenario.resize_scales, scenario.interval_ms, async scale => {
        await viewSize(cdp, Math.round(size[0] * scale), Math.round(size[1] * scale));
        report.checkpoints.push(await snapshot(cdp));
      });
      await paced(scenario.zoom_percents, scenario.interval_ms, async percent => {
        await typeField(cdp, "[data-dnacalc=zoom-percent]", String(percent));
        const actual = await cdp.evaluate(`Number.parseInt(document.querySelector('[data-dnacalc=zoom]').textContent)`);
        if (actual !== percent) throw new Error(`Expected zoom ${percent}%, saw ${actual}%`);
      });
      break;
    default:
      throw new Error(`Unknown active scenario ${scenario.action}`);
  }
}

async function processSample(browser, hostPid) {
  if (!browser) return null;
  const { processInfo } = await browser.send("SystemInfo.getProcessInfo");
  const pids = processInfo.map(process => process.id).filter(id => Number.isInteger(id) && id > 0);
  if (hostPid) pids.push(hostPid);
  const cpuMs = processInfo.reduce((sum, process) => sum + process.cpuTime * 1000, 0);
  let bytes, hostCpuMs = 0;
  if (process.platform === "win32") {
    const code = `Get-Process -Id ${[...new Set(pids)].join(",")} -ErrorAction SilentlyContinue | ForEach-Object { "$($_.Id),$($_.WorkingSet64),$([long](1000 * $_.CPU))" }`;
    const { stdout } = await promisify(execFile)("powershell.exe", ["-NoProfile", "-NonInteractive", "-Command", code]);
    const counts = stdout.trim().split(/\r?\n/).filter(Boolean).map(line => line.split(",").map(Number));
    bytes = counts.reduce((sum, parts) => sum + parts[1], 0);
    if (hostPid && !processInfo.some(process => process.id === hostPid)) {
      hostCpuMs = counts.find(parts => parts[0] === hostPid)?.[2] ?? 0;
    }
  } else {
    const counts = await Promise.all([...new Set(pids)].map(async pid => {
      const status = await readFile(`/proc/${pid}/status`, "utf8").catch(() => "");
      return Number(status.match(/^VmRSS:\s+(\d+)/m)?.[1] ?? 0) * 1024;
    }));
    bytes = counts.reduce((sum, count) => sum + count, 0);
  }
  return { bytes, cpu_ms: cpuMs + hostCpuMs, pids: [...new Set(pids)].sort((a, b) => a - b) };
}

async function main() {
  if (!args.case || !args.out) throw new Error("Use --case ID --out FILE [--url PAGE | --match SUBSTRING] [--workbook FILE]");
  const plan = JSON.parse(await readFile(args.scenario, "utf8"));
  const scenario = plan.scenarios.find(item => item.id === args.case);
  if (plan.schema !== 1 || !scenario) throw new Error("Unknown scenario schema or case");
  if ((scenario.workbook === "empty") !== !args.workbook) throw new Error(`Workbook required setting: ${scenario.workbook}`);

  const stack = args.ferro ? "ferroui"
    : args["webview-exe"] ? "webview2" : "leptos";
  const report = { id: args.case, workbook: scenario.workbook, stack, checkpoints: [], errors: [], passed: false };
  let host, page, browser, interval, pending;
  const samples = [];
  const sampling = async () => {
    try {
      const sample = await processSample(browser, host?.pid);
      if (sample) samples.push(sample);
    } catch (error) {
      report.process_sampling_error = error.message;
    }
  };
  const started = performance.now();
  try {
    if (args["webview-exe"]) {
      const launchArgs = ["--app", "--macros", "off"];
      if (args.workbook) launchArgs.push("--open", resolve(args.workbook));
      const profile = args["webview-data"];
      if (!profile) throw new Error("--webview-exe requires a dedicated --webview-data directory");
      host = spawn(resolve(args["webview-exe"]), launchArgs, { stdio: "ignore", env: {
        ...process.env, WEBVIEW2_USER_DATA_FOLDER: resolve(profile),
        WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${args.port}`,
      } });
      host.on("error", error => report.errors.push(error.message));
    }
    const fetchJson = async path => (await fetch(endpoint + path, { signal: AbortSignal.timeout(2000) })).json();
    const target = await waitFor(async () => {
      const targets = await fetchJson("/json/list").catch(() => []);
      return targets.find(item => item.type === "page" && (!args.match || item.url.includes(args.match)));
    }, "matching CDP page", scenario.workbook === "W2" ? 120000 : PENDING_TIMEOUT);
    const version = await fetchJson("/json/version").catch(() => null);
    report.browser = version?.Browser ?? null;
    if (version?.webSocketDebuggerUrl) {
      browser = await Cdp.open(version.webSocketDebuggerUrl).catch(error => {
        report.process_sampling_error = error.message;
        return null;
      });
    }
    page = await Cdp.open(target.webSocketDebuggerUrl);
    await page.send("Page.enable");
    await page.send("Page.bringToFront");
    await page.send("Page.addScriptToEvaluateOnNewDocument", { source: recorder });
    if (args.width && args.height) await viewSize(page, Number(args.width), Number(args.height));
    if (browser) {
      await sampling();
      interval = setInterval(() => {
        if (!pending) pending = sampling().finally(() => { pending = null; });
      }, 1000);
    }
    const navigation = performance.now();
    if (args.url) {
      await page.send("Network.enable");
      await page.send("Network.setCacheDisabled", { cacheDisabled: true });
      const result = await page.send("Page.navigate", { url: args.url });
      if (result.errorText) throw new Error(result.errorText);
    } else {
      await page.evaluate(recorder);
    }
    if (args.ferro) {
      await waitFor(async () => (await ferroSnapshot(page).catch(() => null))?.frames > 0, "first FerroUI grid", 120000);
      await raf(page);
    } else {
      await waitFor(() => page.evaluate(`typeof window.__ferro_eval?.firstGrid === "number"`).catch(() => false), "first painted grid", 120000);
    }

    const firstEmpty = await page.evaluate(args.ferro ? "performance.now()" : "window.__ferro_eval.firstGrid");
    const load = performance.now();
    if (args.workbook) {
      if (args.ferro) {
        await upload(page, args.workbook, "#workbook");
        const name = JSON.stringify(basename(args.workbook));
        await waitFor(() => page.evaluate(`(() => {
          const { workbook, openError } = document.body.dataset;
          if (openError) throw new Error(openError);
          return workbook === ${name} && ferroEval.state()?.frames > 0;
        })()`), "painted FerroUI workbook", 120000);
      } else {
        if (!host) await upload(page, args.workbook);
        await workbookReady(page, args.workbook);
      }
      await raf(page);
    }
    const ready = performance.now();
    report.first_empty_grid_ms = args.url ? firstEmpty : null;
    report.upload_to_grid_ms = args.workbook && !host ? ready - load : null;
    report.launch_to_observed_grid_ms = host ? ready - started : null;
    report.actual_viewport = await page.evaluate("[innerWidth,innerHeight,devicePixelRatio]");

    if (scenario.action === "startup") {
      report.wall_ms = host ? ready - started : ready - navigation;
      report.frames = distribution([]);
      report.input_to_paint = distribution([]);
      await sleep(plan.settle_ms);
    } else {
      const before = args.ferro ? await ferroSnapshot(page) : await snapshot(page);
      report.initial = before;
      if (!args.ferro) {
        if (before.frozen.join() !== "1,1") throw new Error("W1 must start with first row and column frozen");
        if (scenario.address && before.name !== scenario.address) await typeField(page, NAME, scenario.address);
      }
      const size = [Number(args.width ?? report.actual_viewport[0]), Number(args.height ?? report.actual_viewport[1])];
      if (scenario.action === "idle") {
        const idleBefore = await processSample(browser, host?.pid).catch(() => null);
        const idleStart = performance.now();
        await sleep(scenario.duration_ms);
        const idleElapsed = performance.now() - idleStart;
        const idleAfter = await processSample(browser, host?.pid).catch(() => null);
        report.wall_ms = idleElapsed;
        report.idle_core_percent = idleBefore && idleAfter && idleBefore.pids.join() === idleAfter.pids.join()
          ? (idleAfter.cpu_ms - idleBefore.cpu_ms) / idleElapsed * 100 : null;
        report.frames = distribution([]);
        report.input_to_paint = distribution([]);
      } else {
        if (scenario.action === "typing" && !args.ferro) {
          await page.evaluate(`document.querySelector(${JSON.stringify(GRID)}).focus()`);
          await key(page, "F2");
          await waitFor(() => page.evaluate(`Boolean(document.querySelector(${JSON.stringify(EDITOR)}))`), "cell text editor");
          await key(page, "a", 2);
        }
        await sleep(plan.settle_ms);
        const start = performance.now();
        await page.evaluate("window.__ferro_eval.start()");
        if (args.ferro) await executeFerro(page, scenario, size, report);
        else await execute(page, scenario, size, report);
        await raf(page);
        report.wall_ms = performance.now() - start;
        const { frames, keys } = await page.evaluate("window.__ferro_eval.stop()");
        report.frames = distribution(frames.slice(1).map((time, index) => time - frames[index]));
        report.input_to_paint = distribution(
          report.recalc_paint_ms === undefined
            ? (report.ferro_latencies ?? keys.map(sample => sample.paint - sample.input)) : [report.recalc_paint_ms]);
        delete report.ferro_latencies;
        report.final = args.ferro ? await ferroSnapshot(page) : await snapshot(page);
        if (!args.ferro && scenario.id === "S3" && Number(report.final.name.match(/\d+$/)?.[0]) < 201) throw new Error("PageDown did not advance selection");
        if (!args.ferro && scenario.id === "S6" && !report.final.area?.endsWith("A1:AX500")) throw new Error("Expected A1:AX500 selection");
        if (scenario.id === "S7" && report.input_to_paint.samples !== scenario.count) throw new Error("Some typed keys lack a paint witness");
        await sleep(plan.settle_ms);
      }
    }
    report.passed = !report.errors.length;
  } catch (error) {
    report.errors.push(error.message);
  } finally {
    if (interval) clearInterval(interval);
    await pending;
    if (browser) await sampling();
    report.peak_working_set_bytes = samples.length ? Math.max(...samples.map(sample => sample.bytes)) : null;
    report.settled_working_set_bytes = samples.at(-1)?.bytes ?? null;
    report.process_samples = samples.length;
    page?.close();
    browser?.close();
    if (host?.pid && host.exitCode === null) host.kill();
    await writeFile(args.out, JSON.stringify(report, null, 2) + "\n");
  }
  if (!report.passed) throw new Error(report.errors.join("; "));
}

await main().catch(error => { console.error(error.message); process.exitCode = 1; });

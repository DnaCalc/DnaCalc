#!/usr/bin/env node
import { spawn } from "node:child_process";
import { mkdir, mkdtemp, readdir, readFile, writeFile } from "node:fs/promises";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { parseArgs } from "node:util";
import { gzipSync, brotliCompressSync, constants } from "node:zlib";

const here = dirname(fileURLToPath(import.meta.url));
const { positionals: [command], values: arg } = parseArgs({ allowPositionals: true, options: {
  stack: { type: "string" },
  exe: { type: "string" },
  edge: { type: "string" },
  url: { type: "string" },
  out: { type: "string" },
  w1: { type: "string" },
  w2: { type: "string" },
  size: { type: "string", default: "1920x1080" },
  runs: { type: "string", default: "3" },
  port: { type: "string", default: "9222" },
  scenario: { type: "string", default: join(here, "scenarios.json") },
  case: { type: "string", multiple: true },
  raster: { type: "boolean" },
  dir: { type: "string" },
} });

function run(program, args) {
  const child = spawn(program, args, { stdio: "inherit" });
  return new Promise((resolve, reject) => {
    child.on("error", reject);
    child.on("exit", (code, signal) => resolve(code ?? signal));
  });
}

async function sizes(directory) {
  const files = (await readdir(directory, { recursive: true, withFileTypes: true }))
    .filter(entry => entry.isFile() && !entry.name.endsWith(".map"));
  const parts = [];
  for (const item of files) {
    const name = join(item.parentPath, item.name);
    const data = await readFile(name);
    parts.push({
      file: name.slice(directory.length + 1),
      raw: data.length,
      gzip: gzipSync(data, { level: 9 }).length,
      brotli: brotliCompressSync(data, { params: { [constants.BROTLI_PARAM_QUALITY]: 11 } }).length,
    });
  }
  parts.sort((a, b) => a.file.localeCompare(b.file));
  const total = Object.fromEntries(["raw", "gzip", "brotli"].map(key =>
    [key, parts.reduce((sum, item) => sum + item[key], 0)]));
  return { files: parts, total };
}

function median(values) {
  const sorted = values.filter(Number.isFinite).sort((a, b) => a - b);
  if (!sorted.length) return null;
  const middle = Math.floor(sorted.length / 2);
  return (sorted[middle] + sorted[Math.floor((sorted.length - 1) / 2)]) / 2;
}

function summary(reports) {
  const result = {};
  for (const group of reports) {
    const good = group.filter(report => report.passed);
    const id = group[0].id;
    const fields = [
      "wall_ms", "first_paint_ms", "first_empty_grid_ms", "upload_to_grid_ms", "launch_to_observed_grid_ms",
      "peak_working_set_bytes", "settled_working_set_bytes", "idle_core_percent",
    ];
    const distributions = ["frames", "input_to_paint"];
    result[id] = {
      passed: good.length, runs: group.length,
      errors: group.flatMap(report => report.errors ?? []),
      ...Object.fromEntries(fields.map(field => [field, median(good.map(report => report[field]))])),
      ...Object.fromEntries(distributions.map(name => [
        name, Object.fromEntries(["samples", "p50_ms", "p95_ms", "p99_ms", "over_16_7_ms", "over_33_ms"]
          .map(field => [field, median(good.map(report => report[name]?.[field]))])),
      ])),
    };
  }
  return result;
}

async function matrix() {
  if (!arg.out || !["native", "webview2", "leptos", "ferroui"].includes(arg.stack)) {
    throw new Error("Use run --stack native|webview2|leptos|ferroui --out DIR");
  }
  const desktop = ["native", "webview2"].includes(arg.stack);
  if (desktop ? !arg.exe : (!arg.edge || !arg.url)) {
    throw new Error("Desktop stacks need --exe. Browser stacks need --edge and --url.");
  }
  const match = /^(\d+)x(\d+)$/.exec(arg.size);
  if (!match || !/^\d+$/.test(arg.runs) || Number(arg.runs) < 1) throw new Error("Invalid --size or --runs");
  const [, width, height] = match;
  const directory = resolve(arg.out);
  await mkdir(directory, { recursive: true });
  const plan = JSON.parse(await readFile(arg.scenario, "utf8"));
  const selected = plan.scenarios.filter(item => !arg.case || arg.case.includes(item.id));
  if (!selected.length) throw new Error("No scenarios selected");
  const reports = [];
  let port = Number(arg.port);
  for (const scenario of selected) {
    const group = [];
    for (let index = 1; index <= Number(arg.runs); index++) {
      const path = join(directory, `${scenario.id}-${index}.json`);
      const workbook = arg[scenario.workbook.toLowerCase()];
      const fileArg = workbook ? ["--workbook", resolve(workbook)] : [];
      const standard = ["--scenario", resolve(arg.scenario), "--case", scenario.id,
        "--width", width, "--height", height, "--out", path, ...fileArg];
      let report;
      let edge;
      const trialPort = String(port++);
      try {
        if (scenario.workbook !== "empty" && !workbook) throw new Error(`Missing ${scenario.workbook}`);
        console.log(`${arg.stack} ${arg.size} ${scenario.id} trial ${index}`);
        let status;
        if (arg.stack === "native") {
          const exe = resolve(arg.exe);
          if (arg.raster && process.platform !== "linux") throw new Error("Raster smoke is for X11");
          status = arg.raster
            ? await run("xvfb-run", ["-a", "-s", `-screen 0 ${width}x${height}x24`, exe, "--raster", ...standard])
            : await run(exe, standard);
        } else {
          const profile = await mkdtemp(join(directory, `profile-${scenario.id}-${index}-`));
          const cdp = [join(here, "cdp.mjs"), "--port", trialPort, ...standard];
          if (arg.stack === "webview2") {
            cdp.push("--webview-exe", resolve(arg.exe), "--webview-data", profile);
          } else {
            edge = spawn(resolve(arg.edge), [
              `--user-data-dir=${profile}`, `--remote-debugging-port=${trialPort}`,
              "--no-first-run", "--no-default-browser-check", "--start-fullscreen", `--window-size=${width},${height}`, "about:blank",
            ], { stdio: "ignore" });
            edge.on("error", error => console.error(error.message));
            cdp.push("--url", arg.url, "--match", "about:blank");
            if (arg.stack === "ferroui") cdp.push("--ferro");
          }
          status = await run(process.execPath, cdp);
        }
        report = JSON.parse(await readFile(path, "utf8"));
        if (status !== 0 && report.passed) {
          report.passed = false;
          report.errors.push(`Process exit ${status}`);
        }
      } catch (error) {
        report = { id: scenario.id, passed: false, errors: [error.message] };
        await writeFile(path, JSON.stringify(report, null, 2) + "\n");
      } finally {
        if (edge?.pid && edge.exitCode === null) {
          const exited = new Promise(resolve => edge.once("exit", resolve));
          edge.kill();
          await exited;
        }
      }
      group.push(report);
    }
    reports.push(group);
    const aggregate = { stack: arg.stack, requested_size: arg.size, scenarios: summary(reports) };
    await writeFile(join(directory, "medians.json"), JSON.stringify(aggregate, null, 2) + "\n");
  }
  if (reports.some(group => group.some(report => !report.passed))) process.exitCode = 1;
}

try {
  if (command === "run") {
    await matrix();
  } else if (command === "sizes" && arg.dir) {
    const report = JSON.stringify(await sizes(resolve(arg.dir)), null, 2) + "\n";
    if (arg.out) await writeFile(arg.out, report);
    else process.stdout.write(report);
  } else {
    throw new Error("Use run --stack ... or sizes --dir BUNDLE [--out FILE]");
  }
} catch (error) {
  console.error(error.message);
  process.exitCode = 1;
}

#!/usr/bin/env node
// Visual QC for the WebAssembly debug viewer (shells/viewer-web).
//
//   node tools/visual_qc.mjs           compare fresh renders with qc/visual-goldens/*.png
//   node tools/visual_qc.mjs --bless   regenerate the goldens
//
// For each case the page runs the toy scenario in WebAssembly, the canvas is screenshotted in
// headless Chromium, and (1) the state hash shown by the page must equal the hash printed by the
// native `hd-sim hash` for the same parameters, (2) the canvas must match the golden PNG within
// tolerance. Failing renders are written to target/visual-qc/ next to a full-page screenshot.
import { createRequire } from "node:module";
import { execFileSync } from "node:child_process";
import { createServer } from "node:http";
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, extname, join, normalize, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const require = createRequire(import.meta.url);
const { chromium } = require("playwright");

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const WWW = join(ROOT, "shells/viewer-web/www");
const GOLDEN_DIR = join(ROOT, "qc/visual-goldens");
const OUT_DIR = join(ROOT, "target/visual-qc");

// A pixel differs when any channel differs by more than CHANNEL_TOLERANCE (0..255); the case
// fails when more than MAX_DIFF_FRACTION of the pixels differ.
const CHANNEL_TOLERANCE = 8;
const MAX_DIFF_FRACTION = 0.001;

// Fixed scenario: screenshotted cases, then hash-only cases (no golden image).
const SHOT_CASES = [
  { name: "seed1-n400-t0", seed: 1, agents: 400, tick: 0 },
  { name: "seed1-n400-t300", seed: 1, agents: 400, tick: 300 },
  { name: "seed1-n400-t1200", seed: 1, agents: 400, tick: 1200 },
  { name: "seed7-n1500-t600", seed: 7, agents: 1500, tick: 600 },
];
const HASH_CASES = [
  { seed: 42, agents: 1000, tick: 250 },
  { seed: 9, agents: 64, tick: 2000 },
  { seed: 1, agents: 1, tick: 5 },
];

const bless = process.argv.includes("--bless");
const MIME = { ".html": "text/html", ".js": "text/javascript", ".wasm": "application/wasm" };

function fail(msg) {
  console.error(`visual_qc: FAIL ${msg}`);
  process.exitCode = 1;
}

function nativeHashBinary() {
  const exe = join(ROOT, "target/release", process.platform === "win32" ? "hd-sim.exe" : "hd-sim");
  execFileSync("cargo", ["build", "--release", "--locked", "-p", "hd-sim"], { cwd: ROOT, stdio: "ignore" });
  return exe;
}

function nativeHash(exe, c) {
  return execFileSync(
    exe,
    ["hash", "--agents", String(c.agents), "--ticks", String(c.tick), "--seed", String(c.seed)],
    { encoding: "utf8" },
  ).trim();
}

function serve() {
  const server = createServer((req, res) => {
    const rel = normalize(decodeURIComponent(new URL(req.url, "http://x").pathname)).replace(/^[/\\]+/, "");
    const file = join(WWW, rel === "" ? "index.html" : rel);
    if (!file.startsWith(WWW) || !existsSync(file)) {
      res.writeHead(404).end("not found");
      return;
    }
    res.writeHead(200, { "content-type": MIME[extname(file)] ?? "application/octet-stream" });
    res.end(readFileSync(file));
  });
  return new Promise((ok) => server.listen(0, "127.0.0.1", () => ok(server)));
}

async function render(page, base, c) {
  const url = `${base}/index.html?seed=${c.seed}&agents=${c.agents}&tick=${c.tick}`;
  await page.goto(url);
  await page.waitForFunction(() => window.__ready === true || window.__error, null, { timeout: 30000 });
  const err = await page.evaluate(() => window.__error ?? null);
  if (err) throw new Error(`page error for ${url}: ${err}`);
  return page.evaluate(() => ({
    snapshot: window.__snapshot,
    hashText: document.getElementById("hash").textContent,
    tickText: document.getElementById("tick").textContent,
  }));
}

// Decodes both PNGs in the browser and counts differing pixels.
async function comparePngs(page, actual, golden) {
  return page.evaluate(
    async ([a, g, tol]) => {
      const load = (b64) =>
        new Promise((ok, no) => {
          const img = new Image();
          img.onload = () => ok(img);
          img.onerror = () => no(new Error("png decode failed"));
          img.src = `data:image/png;base64,${b64}`;
        });
      const pix = (img) => {
        const c = document.createElement("canvas");
        c.width = img.width;
        c.height = img.height;
        const x = c.getContext("2d");
        x.drawImage(img, 0, 0);
        return x.getImageData(0, 0, img.width, img.height).data;
      };
      const [ia, ig] = await Promise.all([load(a), load(g)]);
      if (ia.width !== ig.width || ia.height !== ig.height) {
        return { sizeMismatch: [ia.width, ia.height, ig.width, ig.height] };
      }
      const da = pix(ia), dg = pix(ig);
      let diff = 0, worst = 0;
      for (let i = 0; i < da.length; i += 4) {
        let m = 0;
        for (let k = 0; k < 4; k++) m = Math.max(m, Math.abs(da[i + k] - dg[i + k]));
        worst = Math.max(worst, m);
        if (m > tol) diff++;
      }
      return { diff, total: da.length / 4, worst };
    },
    [actual.toString("base64"), golden.toString("base64"), CHANNEL_TOLERANCE],
  );
}

async function main() {
  if (!existsSync(join(WWW, "pkg/viewer_web.js"))) {
    console.error("visual_qc: shells/viewer-web/www/pkg is missing; run: bash tools/build_viewer_web.sh");
    process.exit(2);
  }
  const exe = nativeHashBinary();
  mkdirSync(GOLDEN_DIR, { recursive: true });
  mkdirSync(OUT_DIR, { recursive: true });
  const server = await serve();
  const base = `http://127.0.0.1:${server.address().port}`;
  const browser = await chromium.launch();
  try {
    const page = await browser.newPage({ viewport: { width: 860, height: 900 }, deviceScaleFactor: 1 });
    page.on("pageerror", (e) => fail(`uncaught page error: ${e.message}`));

    let checked = 0;
    for (const c of [...SHOT_CASES, ...HASH_CASES]) {
      const label = c.name ?? `seed${c.seed}-n${c.agents}-t${c.tick}`;
      const r = await render(page, base, c);
      const want = nativeHash(exe, c);
      if (r.hashText !== want || r.snapshot.hash !== want || r.tickText !== String(c.tick)) {
        fail(`${label}: wasm hash ${r.hashText} (tick ${r.tickText}) != native hd-sim hash ${want}`);
      } else {
        console.log(`hash ok   ${label}: ${want}`);
        checked++;
      }
      if (!c.name) continue;

      const canvasPng = await page.locator("#stage").screenshot();
      const goldenPath = join(GOLDEN_DIR, `${c.name}.png`);
      if (bless) {
        writeFileSync(goldenPath, canvasPng);
        console.log(`blessed   ${goldenPath}`);
        continue;
      }
      if (!existsSync(goldenPath)) {
        fail(`${label}: missing golden ${goldenPath} (run with --bless)`);
        continue;
      }
      const res = await comparePngs(page, canvasPng, readFileSync(goldenPath));
      if (res.sizeMismatch) {
        fail(`${label}: size ${res.sizeMismatch.slice(0, 2)} != golden ${res.sizeMismatch.slice(2)}`);
      } else if (res.diff / res.total > MAX_DIFF_FRACTION) {
        writeFileSync(join(OUT_DIR, `${c.name}.actual.png`), canvasPng);
        fail(`${label}: ${res.diff}/${res.total} pixels differ (worst channel delta ${res.worst})`);
      } else {
        console.log(`pixels ok ${label}: ${res.diff} differ, worst delta ${res.worst}`);
      }
      // Full-page capture for humans and agents to inspect; never compared.
      await page.screenshot({ path: join(OUT_DIR, `${c.name}.page.png`) });
    }
    console.log(`visual_qc: ${checked} hash checks passed${bless ? ", goldens blessed" : ""}`);
  } finally {
    await browser.close();
    server.close();
  }
}

main().catch((e) => {
  console.error(`visual_qc: ${e.stack ?? e}`);
  process.exit(2);
});

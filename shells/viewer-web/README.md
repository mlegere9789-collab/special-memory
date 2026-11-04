# viewer-web: WebAssembly debug viewer

A spike that lets an agent (or a person) check simulation results by looking at them, with no GPU and no
engine: the toy scenario of `hd-sim` runs inside a WebAssembly module, a static page draws a snapshot on a
canvas, and headless Chromium screenshots the page.

- `src/lib.rs`: `Session`/`Snapshot` (plain Rust, tested natively) and the `Viewer` wasm-bindgen wrapper.
  The page can read agent positions (whole centimetres, `i32`), team, hp, tick, arena size and the state hash,
  and can only advance the run forward. No floating point is used in the Rust code.
- `www/index.html`: loads `pkg/viewer_web.js`, reads `?seed=&agents=&tick=`, draws team colours (blue, orange),
  fades agents with falling hp (red outline at or below a quarter hp), and prints tick, hash, agent count and seed.
- `../../tools/visual_qc.mjs`: serves `www/`, renders fixed cases, checks hashes and compares canvas pixels.
- `../../qc/visual-goldens/`: committed golden PNGs of the canvas.

## Prerequisites (once)

```
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version =0.2.130 --locked   # must equal the pinned wasm-bindgen crate
```

Node 22 with Playwright resolvable by `require('playwright')` and a Chromium it can launch with default options.

## Build and run (from the repository root)

```
bash tools/build_viewer_web.sh            # writes shells/viewer-web/www/pkg/ (git-ignored); set WASM_BINDGEN=/path if not on PATH
node tools/visual_qc.mjs                  # hash equality + pixel comparison against the goldens
node tools/visual_qc.mjs --bless          # regenerate the goldens after an intended visual change
(cd shells/viewer-web/www && python3 -m http.server 8080)   # then open http://127.0.0.1:8080/?seed=1&agents=400&tick=300
```

Failing renders are written to `target/visual-qc/<case>.actual.png`; a full-page capture of every case is written
to `target/visual-qc/<case>.page.png` for inspection.

Native tests: `cargo test -p viewer-web` (snapshot hash equals `hd_sim::scenario::final_hash`, incremental
advance equals a direct run, bounds, error cases). Wasm build check: `cargo build -p viewer-web --target wasm32-unknown-unknown`.

## Tolerance

A pixel counts as different when any channel differs by more than 8 (of 255); a case fails when more than 0.1%
of the canvas pixels differ. The canvas holds only filled arcs and no text, so it does not depend on installed
fonts; the hash text is checked through the DOM, not through pixels.

## Lint scope

The workspace sets `unsafe_code = "forbid"`. The `wasm-bindgen` macro expands to unsafe glue, which `forbid`
rejects, so this crate alone sets `unsafe_code = "deny"` in its own `[lints.rust]`. The hand-written code has no
`unsafe`; any new `unsafe` in this crate would still fail the build.

## What the spike proves

- The simulation core compiles unchanged to `wasm32-unknown-unknown` and produces the same state hash in the
  browser as the native `hd-sim hash` for the same seed, agent count and tick (asserted in `visual_qc.mjs` for
  seven parameter sets, and natively in the crate tests). Fixed-point integer arithmetic makes this hold.
- An agent can render a scenario and look at the PNG, with no GPU, in about five seconds, and a regression in the
  drawn state shows up as a failing pixel comparison (a swapped golden is rejected: 29998 of 640800 pixels differ).

## What it does not prove

- Nothing about the final renderer, engine host, frame rate, or GPU behaviour; the canvas is a 2D debug view.
- Cross-machine pixel stability: the goldens were produced on one Linux container; another Chromium build or
  platform may anti-alias differently and need a wider tolerance or its own goldens. The hash check is exact
  everywhere.
- Wasm performance at scale (only up to 1500 agents are drawn here), cross-platform wasm hashes beyond Linux
  hosts, or any game rule (the toy scenario is not a rule set).

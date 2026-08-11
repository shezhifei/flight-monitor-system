# Official OR-Tools wasm build

This directory contains the pinned upstream metadata, bridge source and release scripts for the browser-side CP-SAT solver.

Current production solver contract:

- `model_version`: `dispatch_wasm_pdf_full_model_v2`
- `solver_version`: `dispatch_solver_ortools_wasm_strict_pdf_v3`
- source/local artifact version: `v9.14-bridge.4`
- published artifact version: `v9.14-bridge.2` (tracked by `active-manifest.json` until bridge.4 is released)
- primary bridge: `tools/ortools_wasm/bridge/dispatch_replan_solver.cc`
- runtime loader: `frontend/vue-app/src/workers/dispatchReplanWorker.ts`
- loaded artifact is resolved from `frontend/vendor/ortools/runtime-manifest.json`

## Inputs

- Upstream source: `google/or-tools` `v9.14`
- Source archive SHA256: `6af83f7d373084c1221c875a3000f53ecdd1f253336648ef87e0aeef408facd7`
- Toolchain: `emsdk 3.1.74`, `cmake 3.30.5`, `ninja 1.12.1`

## Outputs

The release build emits a versioned directory containing:

- `dispatch_replan_solver.js`
- `dispatch_replan_solver.wasm`
- `manifest.json`
- `SHA256SUMS`
- `LICENSES/`
- `LICENSES.tar.gz`

## Build

There is no dedicated CI workflow for this artifact; the build is run locally on
Linux (or WSL) with Emscripten installed and the resulting release is installed
into `frontend/vendor/ortools/<artifact_version>/`. That directory is
git-ignored (only `active-manifest.json` is tracked there) -- the install
script writes `runtime-manifest.json` beside the artifacts so the app and the
fixture harness load what is actually installed.

```bash
tools/ortools_wasm/build_release.sh --output-dir dist/ortools --artifact-version v9.14-bridge.4
```

A full first build compiles the whole OR-Tools dependency tree and takes a long
time. Cap parallelism if the machine is memory-constrained (protobuf and abseil
are the peaks) — roughly 1GB per compile job:

```bash
export CMAKE_BUILD_PARALLEL_LEVEL=8
```

Once the dependency tree is built, bridge-only edits relink in seconds via
`cmake --build <build-dir> --target dispatch_replan_solver`.

The build script automatically patches the pinned upstream `cmake/host.cmake`
to avoid the `or-tools/host_tools` target collision seen with newer
CMake/Ninja combinations during cross-compilation.

Install the locally built release into the app's static vendor directory:

```bash
python legacy-backend/scripts/ortools/install_local_release.py \
  --release-dir dist/ortools/v9.14-bridge.4 --project-root . --force
```

`--project-root` matters: the script derives its default root from its own
location, which resolves inside `legacy-backend/` and installs to the wrong
vendor directory.

Verify the installed artifact against the fixture assertions and the golden
baseline:

```bash
node tools/ortools_wasm/run_fixture_tests.mjs \
  --js frontend/vendor/ortools/v9.14-bridge.4/dispatch_replan_solver.js \
  --wasm frontend/vendor/ortools/v9.14-bridge.4/dispatch_replan_solver.wasm
node tools/ortools_wasm/capture_golden.mjs --check tools/ortools_wasm/golden/baseline.json
```

`run_fixture_tests.mjs` is silent on success and throws on the first mismatch.
`capture_golden.mjs --check` re-solves every fixture and diffs against the
committed baseline, exiting non-zero on any change; without `--check` it
*rewrites* the baseline, so only run the bare form when you intend to accept
new output. The comparison covers `captures[*].result` only — per-fixture
timings live under `volatile_info` precisely so host speed cannot fail the gate.

Current bridge semantics are strict-PDF oriented:

- explicit order timing uses `earliest_start_time` / `latest_start_time` / `duration_minutes`; completion-anchored work also carries an immutable `completion_target_time`
- staged lexicographic objective order is `slot_gap -> continuity_break -> baseline_change -> travel_cost -> scarcity_cost -> load_deviation`; planned completion is a forecast and is not treated as an SLA objective
- fixed anchors participate in turnaround continuity truth modeling
- travel cost is derived from selected path-flow edges rather than all-pair overcounting
- per-resource sequencing uses one `AddCircuit` per (resource, free window), with
  node 0 as the window boundary and one node per candidate order. Windows stay
  per-window rather than collapsed into a single circuit per resource, because
  the resource performs fixed anchor work between windows -- a cross-window
  circuit would admit a direct order-to-order arc spanning that fixed work.
- a redundant `AddNoOverlap` per resource layers the disjunctive and timetable
  propagators over the same start variables. Fixed anchors need no interval of
  their own: free windows are already cut around them.
- when a lexicographic stage exhausts its budget without proving optimality, the
  stage bound is applied as an inequality rather than pinning the unproven
  incumbent, and `solver_run_metadata.lexicographic_degraded` /
  `.degraded_stages` report which stages were only approximated

## Fixtures

`fixtures/` holds the committed assertion fixtures used by `run_fixture_tests.mjs`.
They are small (at most three orders on one resource) and deliberately check
objective semantics rather than search behavior.

`make_scale_fixture.mjs` generates larger sequencing-heavy cases into
`fixtures-scale/` -- many orders sharing one window with asymmetric travel, where
the ordering itself carries cost. These are what distinguish sequencing
encodings; the committed fixtures are all solved identically by any of them.

```bash
node tools/ortools_wasm/make_scale_fixture.mjs --orders 16 --resources 2
node tools/ortools_wasm/capture_golden.mjs \
  --fixtures-dir tools/ortools_wasm/fixtures-scale \
  --output tools/ortools_wasm/golden/scale-capture.json
```

Use a repo-relative `--output`: these scripts run under Node on Windows, where a
`/tmp/...` argument resolves to `C:\tmp\...` rather than a temp directory.
Scale captures are scratch output and are not committed as goldens.

## Publish flow

1. Rebuild from pinned upstream source (see Build above).
2. Fixture tests run against the generated JS/WASM pair.
3. Upload individual assets and the bundle to an immutable GitHub Release tag.
4. `frontend/vendor/ortools/active-manifest.json` is then updated to the published
   artifact digests. It tracks the *published* release, so it lags a locally
   installed artifact until a release is actually cut --
   `runtime-manifest.json` is what the app loads.

## Runtime flow

Development and production both consume prebuilt artifacts through:

- `frontend/vendor/ortools/active-manifest.json`
- `scripts/ortools/fetch_prebuilt.py`
- `scripts/ortools/install_local_release.py`
- `frontend/vendor/ortools/runtime-manifest.json`

The application never builds OR-Tools at runtime.

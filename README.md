# Raycast Extension: Pixels to Viewport Width or Height

**Native Rust 0.2.0 for Tezbar:** [release](https://github.com/almatkai/convert-px-to-vw-vh/releases/tag/v0.2.0), [macOS Apple Silicon ZIP](https://github.com/almatkai/convert-px-to-vw-vh/releases/download/v0.2.0/convert-px-to-vw-vh-aarch64-apple-darwin.zip), [Windows x64 ZIP](https://github.com/almatkai/convert-px-to-vw-vh/releases/download/v0.2.0/convert-px-to-vw-vh-x86_64-pc-windows-msvc.zip), [trusted install reference](https://github.com/almatkai/convert-px-to-vw-vh/releases/download/v0.2.0/extension-release.json).

Requires a Tezbar build with schema-3 process extensions and the import CLI. Download the trusted JSON and your platform ZIP, then import:

```sh
tezbar --extensions-v3-import extension-release.json convert-px-to-vw-vh-<target>.zip
```

Both native platform builds, extracted-package protocol checks and actual Tezbar CLI installs into isolated profiles passed. [Validation and exact hashes](https://github.com/almatkai/convert-px-to-vw-vh/releases/download/v0.2.0/validation.json) record the tested source. Actual desktop effects remain unverified; Store discovery still needs a Tezbar catalog update.

![Extension Logo](assets/logo.png)

A Raycast extension that converts pixel values to viewport units (vw/vh) based on the chosen viewport size.

## Features

- Precise conversion of pixels to vw (viewport width) or vh (viewport height) based on the selected viewport size.
- Converted values are directly copied to the clipboard for quick and convenient use.

## Screenshots

![Screenshot 1](metadata/convert-px-to-vw-or-vh-1.png)
![Screenshot 2](metadata/convert-px-to-vw-or-vh-2.png)
![Screenshot 3](metadata/convert-px-to-vw-or-vh-3.png)
![Screenshot 4](metadata/pixels-viewport-width-1.png)
![Screenshot 5](metadata/pixels-viewport-width-2.png)
![Screenshot 6](metadata/pixels-viewport-height.png)
_Screenshots of the extension in action_

## Installation

1. Make sure you have [Raycast](https://www.raycast.com) installed on your system.
2. Open Raycast and navigate to the "Extensions" tab.
3. Click on the "Add Extension" button.
4. Search for "Pixels to Viewport Width or Height" and install the extension.

## Usage

1. Launch Raycast.
2. Search and select "Convert PX to VW or VH".
3. Choose between viewport width or height.
4. Then choose your desired currency.
5. Enter the pixel value you want to convert.
6. The converted value in vw/vh will be automatically copied to the clipboard.

## Contribution

Contributions are welcome! If you want to improve this extension or add new features, feel free to open a pull request.

## License

This project is licensed under the MIT License. Please see the [LICENSE](LICENSE) file for more details.

---

**Note:** This extension was developed by Atiipik and ThoPel.

## Tezbar native Rust extension (installable)

This repository also builds an installable **standalone Rust Tezbar extension**
next to the preserved Raycast TypeScript source (`src/`, `package.json`,
`assets/`, which are unchanged). No Node, Bun or JavaScript runs at runtime.

| | |
| --- | --- |
| Manifest | [`extension.json`](extension.json): schema 3, `runtime: rust`, `execution: process`, `protocolVersion: 1` |
| Canonical ID | `com.tezbar.native.convert-px-to-vw-vh`, replaces `raycast.convert-px-to-vw-vh` |
| Version | `0.2.0` (first executable package; `0.1.0` was the unreleased compiled-in Native V2 library) |
| Executable | `bin/tezbar-convert-px-to-vw-vh` (Cargo root crate, `native/main.rs`) |
| SDK | `tezbar-extension-sdk` git rev `aaec571673ab64a85a136cf6bd054ef660c4d93c` (locked in `Cargo.lock`) |
| Host grants | `clipboard.write`, `window.visibility`, `tray.set`, `tray.remove`, `background.retain`, `background.release`, `settings.open` |

### Commands

| Command | Original mode | Rust package |
| --- | --- | --- |
| `index` Convert PX to VW or VH | view | Form: Width/Height, the original 22 device presets, pixel field (draft restored like `storeValue`), Convert and Copy |
| `height` Pixels Viewport Height | no-view, args `height`, `pixels` | Converts, records history, copies, hides the launcher |
| `width` Pixels Viewport Width | no-view, args `width`, `pixels` | Same, for vw |
| `menu-bar` Conversion History | menu-bar | Creates a persistent native tray and opens a searchable management List. Both provide Copy, Remove and Clear History |

The executable compiles the reviewed Native V2 engine in [`v2/`](v2/README.md)
(numeric JavaScript `parseInt`/`Number`/`toFixed(3)` compatibility, presets,
validated history storage) and its adapter `v2/src/tezbar_adapter.rs`
**unchanged**. A small shim in `native/main.rs` maps the adapter's host calls onto
the SDK: clipboard writes become `host.clipboard.write {text}`, launcher hiding
becomes `host.window.visibility {visible:false}`, and storage uses the SDK's
private atomic store at `<dataDir>/convert-px-to-vw-vh/storage.json`. The SDK's
`serve()` handles the JSONL lifecycle (hello, initialize, session open/call/dispose,
shutdown, cancellation). It stages session state and discards it when a call fails.
`native/effects.rs` keeps a per-session record of finished side effects (history
saved, copied, hidden), so retrying a failed action does only the step that is
left, as in V2. The record is cleared on dispose and on relaunch.
If the host cancels an action while an effect is in flight, that effect's
progress may be lost, and a retry can repeat it. V2 has the same limit: effects
are not exactly-once.

`native/boundary.rs` returns a complete Form for field actions because the
current renderer consumes action `unchanged` replies as views. It omits the
pixel value after mounting to preserve queued local typing. Clear History is
included in every history row's action IDs, matching the current overlay filter.

`native/tray.rs` uses the current generic `host.tray.set/remove` lifecycle and
`background.event` callbacks. The tray survives List disposal, updates after
conversion/removal/clear, and is restored through a durable background lease
on host startup. Stop Conversion History releases the lease and removes the
tray; shutdown removes the tray without erasing the user's startup choice.
Explicit Remove items replace secondary-click deletion: the host callback has
no click-type field. Configure Command opens Tezbar Extension Settings, because
there is no command-specific preference target (the original declares none).
Tray Copy writes the clipboard without hiding or stealing focus. Callback
errors return successful diagnostic envelopes, because the current host terminates
the process on every background error. Invalid or obsolete IDs execute no action.
Failed tray refreshes preserve completed conversion state and mark the snapshot
dirty. A later operation or Refresh retries once; the recovered tray displays a
tray-specific notice. If tray publication remains unavailable, that notice cannot
be displayed until recovery. Actual desktop behavior still
needs QA. Refresh History provides an explicit retry of the tray snapshot.

Before commands become available, `native/migration.rs` imports the original
Tezbar JavaScript Cache's JSON-string `history` into an empty native store.
Only six explicit cache/package manifest file grants are read: `extensions/`,
`extensions/packages/`, then `extension-registry/packages/`, each under this extension's
`raycast.convert-px-to-vw-vh/.tezbar-support/cache/shared.json`. That priority
matches current host discovery: the first parseable object package.json owns
the cache, even when that cache is absent. Orphan caches and invalid manifests
cannot win; caches are never merged. Entries retain
ordering, timestamps and duplicate values, with stable descending IDs and the
original ten-item limit. Validation covers the complete input. Source files
are never modified; an atomic migration marker prevents reimport after clear.
Any nonempty native store is authoritative and left byte-identical. Malformed
input and read/write errors fail initialization without replacing native data;
a new process can retry. Raycast application-private data, `storeValue`, custom
external install paths, and the never-registered V2 store are not imported.

### Build, test and package (macOS arm64)

```sh
export CARGO_TARGET_DIR="$PWD/target"
cargo test -j2 --locked                     # effects unit test, manifest mirror, real-pipe protocol tests
cargo clippy -j2 --locked --all-targets -- -D warnings
cargo +1.85.0 check -j2 --locked --all-targets
python3 scripts/check-msrv.py
python3 scripts/vendor-licenses.py --check
(cd v2 && CARGO_TARGET_DIR="$PWD/../target/v2" cargo test -j2 --locked)   # preserved V2 suite
cargo build -j2 --locked --release --target aarch64-apple-darwin
python3 scripts/package.py --target aarch64-apple-darwin \
  --binary target/aarch64-apple-darwin/release/tezbar-convert-px-to-vw-vh
python3 scripts/smoke-package.py dist/convert-px-to-vw-vh-aarch64-apple-darwin.zip --protocol-tests
```

The real-process suite includes source-extracted current host renderer callback
and selected-row overlay checks (`scripts/renderer-consumer.cjs`). Those checks
need Node and the specified host checkout's TypeScript compiler only during
testing; use `TEZBAR_HOST_SOURCE` to select its path. Runtime stays Rust-only.
The default test host uses the actual per-method reply envelopes and rejects
unknown methods; synthetic in-band failure probes are labeled separately.

Exact original dependency notices for both release targets are recorded in
`licenses/dependency-license-index.json` and linked by `licenses/THIRD_PARTY.md`.
`vendor-licenses.py` includes the locked normal/build closure, SDK LICENSE/NOTICE
and all supplied alternative notices; packaging checks it before writing a ZIP.
`check-msrv.py` checks both target closures' declared requirements against 1.85;
the native `cargo +1.85.0 check` also compiles the locked dependency graph.
Native Windows compilation, extracted-package smoke and isolated CLI import passed for the published source.

`package.py` writes `dist/<slug>-<target>.zip` with `extension.json`, `bin/`,
`resources/`, `licenses/`, `LICENSE` and `NOTICE`. It also writes these sidecars:
`.zip.sha256`, `.executable.sha256`, `.artifact.json` (archive and executable
digests for the trusted catalog) and `.files.json` (the exact inventory). The
smoke script checks the ZIP inventory and paths, runs the extracted binary
offline, and with `--protocol-tests` runs `tests/protocol.rs` against that binary.
The test host answers reverse requests with recorded stubs, so the real
clipboard and windows are never touched.

Both platform packages are published after independent review, native tests/builds,
extracted protocol checks and isolated CLI imports. Real clipboard/window/tray
desktop QA remains unverified; both manifest `verification` entries stay `not-run`.

[Tezbar Native V2 Rust implementation](v2/README.md)

Older migration markers remain authoritative. Without an import provenance snapshot,
an incorrectly imported old history cannot safely be distinguished from subsequent
user edits or intentional clear. Automatic repair never overwrites it; recovery
requires user-reviewed export/merge outside this migration.

The accepted renderer compatibility baseline is `d77f03b12a93d72cd2a56a73faa1a04fae94a2cd`. Run `TEZBAR_HOST_SOURCE=/absolute/frozen-host-source cargo test -j2 --locked` and use the same environment for `scripts/smoke-package.py … --protocol-tests`; its source fixture must contain that revision and a TypeScript compiler. The helper extracts the callback and its real notice helpers from the selected source, and only loads `extensionFeedback.ts` when that callback uses it. Testing a mutable sibling checkout alone is not baseline evidence. Candidate host `9cac530f0cb3012eedb6bea410817764dffe914b` is an additional compatibility target, pending its own review.

# Pixels to Viewport Width or Height — Native V2

A standalone Rust library and concrete compiled-in Tezbar adapter for the original
Raycast extension by **Atiipik**. No Node, Bun, JavaScript worker, external Rust
loader, or dynamic plugin loading is needed at runtime. The adapter must be
registered in Tezbar and the application rebuilt before it can run there.

`manifest.json` uses the beta schema version 2, canonical ID
`com.tezbar.native.convert-px-to-vw-vh`, replacement ID
`raycast.convert-px-to-vw-vh`, and runtime `rust`. All four command handlers compile
in the standalone contract harness on macOS and for Windows GNU. This package
has not been registered in a host application. Both manifest verification entries
remain **not-run**: real macOS and Windows clipboard, window, focus, persistence,
launcher, and UI QA have not run.

## Commands and numeric compatibility

- `index`: Form with Property Width/Height, the original Desktop/Tablet/Mobile
  groups and eleven presets per property, Size in pixels, validation, and
  Convert and Copy. The pixel draft is persisted on change and on valid submit;
  only this field is restored, as in the original `storeValue` declaration.
- `width` and `height`: no-view commands. Actual beta launch payloads must use
  `argumentValues: { width: "1440", pixels: "16" }` or
  `argumentValues: { height: "1024", pixels: "16" }`. Argument definitions, order,
  names, placeholders, titles, and required flags match the original manifest.
- `menu-bar`: Conversion History, implemented as a supported List with Copy
  Value, Remove from History, and Clear History. Newest first, at most ten entries;
  equal conversions remain separate entries.

The no-view commands preserve the original parseInt validation (`> 0`), then
compute `(parseInt(pixels) / Number(originalViewportString)) * 100`. Decimal
viewports, hexadecimal prefixes, scientific notation, whitespace, trailing junk,
overflow, Infinity, and NaN keep their original JavaScript behavior. For example,
`width=1e2,pixels=16` produces `16.000vw`, whereas `width=20px,pixels=16` records
`NaNvw` and copies nothing. Viewport `1e-9999` validates as 1 but divides by zero,
producing `Infinityvw`. There is no invented maximum viewport/pixel bound.

The form parses pixels and the selected viewport before calling the helper;
the helper then parses its numeric pixel argument a second time using JavaScript
number-to-string semantics. This includes the surprising `1e21`-to-`1` behavior.
The form accepts zero and negative pixels. Labels retain the original input
strings for no-view commands and JavaScript number strings for form conversions.

`numeric.rs` implements ECMAScript whitespace, parseInt, Number coercion,
shortest binary64 number strings, and exact `toFixed(3)`. Rounding uses the exact
binary64 significand and integer arithmetic, including ties toward larger
magnitude, `-0` versus tiny negative numbers, subnormals, and the switch to
JavaScript shortest notation at absolute values of at least `1e21`. Nonfinite
conversion results are stored as strings, keeping JSON valid.

## Intentional changes and beta UI limitations

1. Invalid form input returns a visible field error without copying the original
   bug's `undefinedvw`/`undefinedvh`. An invalid device/property or stale submit
   also fails without conversion. Errors clear on the next accepted text change.
2. Removal really removes the selected persisted entry. Original `remove`
   compares a history object with `item.data.value`, a string, so it is a no-op.
3. Native SDK storage is namespaced separately from V1 and is validated before
   mutation. No legacy storage is read or migrated. The pinned beta
   `createCacheShim` defaults to `.tezbar-support/cache/shared.json`, and its
   `useCachedState` uses that same default Cache. Thus the source's Cache writer
   and hook reader already share a namespace **in this beta**. One shared native
   history preserves that relationship; it is not claimed as a beta namespace fix
   or as proof of Raycast's private utils implementation.
4. `menu-bar` has manifest mode `view`. The beta renderer has a supported List
   surface; this port supplies no OS menu-bar/status popup, secondary-click
   deletion, command configuration UI, or invented Configure Command RPC.
   Copy/removal are explicit List actions; Clear History is a global action.
5. The original group nodes and preset text are preserved, including the source's
   missing `px` in `iPhone 14 Pro Max - 932`. The pinned Form parser flattens
   dropdown sections; it does not display native option groups. `autoFocus` and
   `storeValue` props are retained in the text-field node, but the beta Form
   renderer does not forward autofocus or onBlur. Persistence is implemented by
   the adapter; initial focus and blur-time validation are not claimed. Validation
   runs on submit. Property changes explicitly reset Device to the first preset.
6. Instead of the original unawaited hide/copy/HUD/toast sequence, conversion
   stores history, awaits clipboard success, and then awaits `window:hide`.
   Source history-before-copy ordering is retained. Native response messages
   report success only after these calls succeed; no separate HUD/toast event is
   fabricated. History copying also hides the launcher after copying.

## Storage, actions, and failure handling

The adapter calls the real beta `v2_sdk::{read_store,update_store,copy,view,
no_view,unchanged}` and `CoreContext::request("window:hide", [])`. Clipboard routes
through `clipboard:write`. Host transport errors and error envelopes (`ok:false`,
`success:false`, or a non-null `error`) fail the operation.

The SDK's process-wide transaction lane and atomic file replacement are used for
all writes under `native-extensions-v2/convert-px-to-vw-vh/storage.json` in the
host's data directory. History entries contain monotonic IDs, ISO millisecond UTC
dates, string values, and labels. `historyRevision`, `history`, and `textfield`
are the owned root keys. Unknown root keys survive. Invalid JSON, invalid owned
fields, missing revisions for existing history, duplicate/out-of-order IDs,
invalid dates, and histories larger than ten fail without overwriting the file.
The SDK's in-process serialization is not an inter-process transaction guarantee.

Each launch gets new process-monotonic action IDs. No IDs are derived only from a
row index. Text and property field IDs stay stable while typing; device IDs
change when the property changes, and submit IDs expire on device/property
transitions. Every action is checked against the active command/session and
current allowed values. Subsequent Form views omit the pixel `value`, so the
actual beta reconciliation keeps queued local typing. Most text updates return
`unchanged`; a validation error clearing returns a new Form without replacing
the local draft. The beta host serializes session operations.

History actions validate the current persisted revision before acting. Removal
and clearing check that revision again **inside** the atomic transaction.
Concurrent additions cannot be lost. A changed history rejects stale actions;
`extension:refresh-session` recomputes rows and action IDs from storage. Copy
validates the selected entry against its snapshot, but clipboard I/O is not
atomic with storage: another session can change history immediately after that
check. Disposal clears state; relaunch never revives old IDs.

An active form retains an in-memory conversion with saved/copied/hidden stages.
Retrying its same submit action resumes the incomplete stage and ignores edits
to that pending conversion. A failed history save prevents clipboard/hide; a
failed clipboard call leaves the already-recorded entry and retries clipboard;
a failed hide retries hide without repeating a confirmed copy or history write.
After confirmed history copy, other actions are blocked until hide succeeds or
history is reopened/refreshed after a storage change. Completed action IDs expire.

These guarantees apply to acknowledged operations in one live, serialized
session. There is no cross-restart or crash-safe exactly-once effect guarantee:
a blocking SDK write can finish after host cancellation, and an OS operation
could succeed before returning an error. Retrying an unacknowledged effect may
repeat it. In particular, the beta discards failed **no-view launch** state;
relaunching width/height is a new invocation and can add another history entry.
Neither history nor clipboard is rolled back after a later failure. Draft writes
and history writes are separate SDK transactions. History removal/clear failures
can be retried with the same action while its revision is still current.

## Host integration

Reference API: Tezbar beta commit
`fd300e276356587726566026f072563eb9c34cb1`. This repository supplies code and metadata;
JSON installation alone cannot register executable Rust.

1. Vendor the **contents of this `v2/` directory** at the stable host path
   `src-tauri/native-extensions/convert-px-to-vw-vh/`. Add a dependency in
   `src-tauri/Cargo.toml`:

   ```toml
   tezbar-convert-px-to-vw-vh = { path = "native-extensions/convert-px-to-vw-vh" }
   ```

2. Include `src/tezbar_adapter.rs` unchanged as a module in the host's
   `native_core::extensions`, beside the real `v2_sdk`. For example, from
   `src-tauri/src/native_core/extensions/mod.rs`:

   ```rust
   #[path = "../../../native-extensions/convert-px-to-vw-vh/src/tezbar_adapter.rs"]
   mod v2_convert_px_to_vw_vh;
   ```

   The host already provides `serde_json`, `CoreContext`, and `CoreResult`.
   The library owns its numeric, serde, and timestamp dependencies. Test fixture
   modules and the Tauri boundary shim must not replace host modules.

3. Add the slug to `v2_runtime::slug`, its four-command `commands()` branch, and
   the `handle` dispatch branch calling this adapter. Add the supplied manifest
   to the bundled `v2_catalog.json` data and run the host's real compiled coverage
   checks. The adapter exposes all four handlers; host coverage remains absent
   until this registration and rebuild are completed. Preserve platform checks
   and the installed-package gate in the beta host. Its native session manager
   handles actual disposal; the adapter also accepts disposal for contract tests.

4. Copy the exact PNG files to the real renderer asset directory:

   | Package asset | Host destination under `src/renderer/public/extensions/icons/` |
   | --- | --- |
   | `icons/logo.png` | `convert-px-to-vw-vh.png` |
   | `icons/logo-vw.png` | `convert-px-to-vw-vh-vw.png` |
   | `icons/logo-vh.png` | `convert-px-to-vw-vh-vh.png` |

   The root icon path is `/extensions/icons/convert-px-to-vw-vh.png`. The beta
   schema permits command-specific icon paths through its command metadata.
   Do not convert, resize, or re-encode these images. The correct asset root is
   `src/renderer/public`, not a repository-root `public` directory.

5. Rebuild and install through the real host flow. Test all commands, launch
   arguments, clipboard/hide failures, field focus, history persistence across
   relaunch, stale actions, and List usability on macOS and Windows. Only actual
   desktop QA justifies changing the manifest verification states.

## Validation and reproducibility

From this directory:

```sh
cargo test --locked
cargo +1.85.0 test --locked
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo +1.85.0 clippy --locked --all-targets -- -D warnings
cargo check --locked --all-targets --target x86_64-pc-windows-gnu
cargo clippy --locked --all-targets --target x86_64-pc-windows-gnu -- -D warnings
python3 scripts/verify-provenance.py
python3 scripts/high-precision-oracle.py
# Requires Python jsonschema (tested with 4.25.1):
python3 scripts/verify-schema.py
cargo package --allow-dirty --locked
```

`tests/beta_contract.rs` compiles the production adapter unchanged next to the
**byte-exact** beta `common.rs` and `v2_sdk.rs`. Only Tauri AppHandle/Emitter and
the router's OS operations are stubbed. Tests exercise real temporary SDK storage,
concurrent updates, ten-entry duplicate retention, corruption, error envelopes,
retries, stale actions, real removal/clearing, and disposal/relaunch.

`source-oracle.cjs` transpiles and executes the seven pinned original TS/TSX
sources with deterministic API/React/Cache boundaries. It executes the real
commands, form callbacks, device declarations, and history functions, not a
second JavaScript conversion implementation. The checked-in oracle contains
962 conversion cases and 22 preset records. Its formatting vector contains
35,022 binary64 values; `high-precision-oracle.py` independently verifies 25,763
finite fixed-format results using exact Decimal arithmetic. All oracle JSON is
packaged, so normal Rust tests require no JavaScript runtime.

To regenerate source oracles, provide TypeScript 5.9.3 installed outside tracked
files (the optional default is `target/v1-validation/node_modules/typescript`):

```sh
node scripts/source-oracle.cjs /path/to/typescript
TEZBAR_RENDERER_SAMPLES="$PWD/target/renderer-samples.json" cargo test --test beta_contract renderer_samples_and_all_preset_options_have_supported_submit_kind
node scripts/renderer-contract.cjs /path/to/typescript
```

The renderer script executes actual pinned Form/List parser and reconciliation
helpers with imported UI components stubbed. This checks wire shapes and draft
handling; it is not desktop interaction QA. V1's smoke test and typecheck are also
run from an isolated copy under `target/`, leaving its files and lock unchanged.
Package validation includes tests from Cargo's extracted package, not only the
working source tree.

## Provenance and licensing

Original: `raycast/extensions` commit
`6734d85992290066fd31a7f5bee3cca3d3545a1a`, path
`extensions/convert-px-to-vw-vh`, author Atiipik. Tezbar API fixture commit:
`fd300e276356587726566026f072563eb9c34cb1`. `provenance.json` records paths and
SHA-256 for all pinned sources, API fixtures, and assets. Add `--remote` to
`verify-provenance.py` to compare the original sources/assets with pinned GitHub
bytes again. The PNGs are byte-identical to the original and existing V1 assets:

| PNG | Bytes | SHA-256 |
| --- | ---: | --- |
| `logo.png` | 182084 | `8fbce00428eac4cae1c1294f10de6b7ff829b5e57c5e4cb63ca3b72c303300bd` |
| `logo-vh.png` | 183156 | `6d88d7027fbc863cb0d8032c1aa1143274a55e4ae09251d3532214c3d86172db` |
| `logo-vw.png` | 182030 | `ca37b3cb68c58297b3ed50597e496688638fcbc88d397930db003362049bd978` |

See `NOTICE`, `LICENSE-MIT`, and `LICENSE-APACHE-2.0`. Package SPDX:
`MIT AND Apache-2.0`.

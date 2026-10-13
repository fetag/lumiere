# Native workspace preparation (2026-10-13)

This change implements the structural preparation in `~/workspace/cross-platform.md`.
It does not implement Windows, Linux or web support. The existing work was saved
first in commit `a89ad20`; all restructuring is in the following commit, as
requested. Revert the restructuring commit to return to that saved baseline.

## Responsibility and movement

```
apps/lumiere ──> crates/lumiere-desktop ──> crates/lumiere-core
     └───────────────────────────────────────────> core DTOs
```

| Previous location | Current location / responsibility |
| --- | --- |
| `src/main.rs`, `app*`, `browser`, `viewer`, `theme` | `apps/lumiere/src/`: unchanged UI and texture ownership |
| `app::Action`, `app::Mode` | `lumiere-core::command`; re-exported by app |
| imaging pixel DTO, scaling, portable image/PCX decode, orientation, dimension limit | `lumiere-core::imaging` |
| state fields, defaults, parse and serialize | `lumiere-core::state` |
| `state::State` load/save, private atomic write, config path | `lumiere-desktop::state`; field-compatible wrapper, explicit conversions |
| `fsmodel`, `file_ops`, `loader` | `lumiere-desktop`; same queues, locks, backpressure and filesystem semantics |
| imaging decode policy, metadata, disk cache | `lumiere-desktop::imaging`; core portable functions + ImageIO fallback |
| `imaging/macos` | `lumiere-desktop/src/platform/image_io.rs` |
| menu description and shortcut DTOs | `lumiere-desktop::menu_model`; UI facade re-exports them |
| `panels`, `alerts`, display geometry | `lumiere-desktop::platform`; service-local native/fallback cfg retained |
| native menu, open-event repaint, drag session | `apps/lumiere/src/platform_ui` |
| `alerts::trash_names` | shared desktop helper; file operations no longer depend on alerts |

`application_icon` now returns the existing raw `Decoded` DTO from desktop;
the UI facade performs the same unmultiplied RGBA conversion. No native decoder
trait was needed for this extraction: desktop chooses the existing ImageIO
function at compile time. Platform services retain their small local cfg branches
rather than duplicate entire files into empty per-OS implementations. No OS
feature switches or speculative service contracts were introduced.

The desktop `test-support` feature is enabled only by the app's dev dependency.
It preserves the original headless test behavior of the native chooser, alerts,
Open With catalog and fixtures when those modules are compiled across a crate
boundary. Production builds retain the original native paths. Menu glyph display
is now available independently of the target, for model tests and future adapters.

The root is a virtual workspace with app as default member, a single lockfile and
the original release profile. Package/binary `lumiere`, version 0.1.2, bundle ID,
assets, bundle output, scripts and persistence paths are retained. Bundle builds
explicitly select `-p lumiere --locked`. Third-party lockfile versions are unchanged.
The desktop package rejects wasm32 explicitly; web work must select a future web
package rather than compile native desktop as web.

## Validation

Environment: macOS / Apple Silicon; rustc 1.91.1
(`ed61e7d7e`, 2025-11-07), cargo 1.91.1 (`ea2d97820`, 2025-10-10).
Commands ran with `$HOME/.cargo/bin` on PATH. Logs are in [validation](validation/).

| Check | Baseline | Workspace |
| --- | --- | --- |
| fmt | pass | `cargo fmt --all --check`: pass |
| clippy, all targets, warnings denied | pass | `--locked --workspace --all-targets`: pass |
| default parallel tests | 88 pass, 2 fail | 87 pass, 3 fail |
| serial tests (`-- --test-threads=1`) | 90 pass | 90 pass: app 73, core 1, desktop 16 |
| release + signed bundle | pass | pass |
| locked metadata | original single package | three local packages, pass |
| core / desktop dependency trees | coupled binary | no prohibited dependencies, pass |

Parallel baseline failures: `open_with_submenu_lists_applications`,
`scroll_pans_an_overflowing_image`. Parallel workspace failures:
`oversized_images_are_clamped_to_the_texture_limit`,
`pinch_gesture_zooms_about_the_pointer`, `scroll_pans_an_overflowing_image`.
The latter failures time out waiting for background image decoding. Serial
workspace execution passes all original tests without changing test assertions
or worker scheduling. CI and the documented test command use one test thread to
avoid contention on the process-wide pixel-work gate. This records the parallel
suite limitation rather than claiming the default parallel suite passed.

Baseline and refactored bundles are retained at
`work-baseline-bundle/Lumiere.app` and `work-refactor-bundle/Lumiere.app` (ignored
build artifacts). No installed application was replaced and no user state or
cache was deleted. Existing real-photo tests return early when their local sample
is absent; their test harness result alone is not a format capability guarantee.

## Remaining native validation and platform backlog

Manual GUI acceptance was not performed in this change. The specification's
native menu/shortcut dispatch, Finder launch and multi-file events, chooser,
drag-in/out, Reveal, Open With icons, Share, Trash cancel/confirm, native Space
fullscreen, geometry, gestures, appearance and font matrix still require an
interactive macOS pass. HEIC/AVIF/RAW, orientation, alpha and reduced-image pixel
comparisons across identical files also require the full format fixture matrix.
Headless tests and a signed bundle do not certify those native interactions.

Windows/Linux remain phase two: native services and UX, atomic no-replace rename,
codec capabilities, native runners and packaging. Existing non-macOS Unsupported
and empty/fallback results are preserved and do not imply platform support.
WASM remains phase three only after an explicit browser requirement, including
byte-based APIs, browser storage and worker/cancellation design.

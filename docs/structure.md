# Native workspace preparation (2026-10-13; updated for Linux support)

This change implements the structural preparation in `~/workspace/cross-platform.md`.
At the time of the restructuring it did not implement Windows, Linux or web
support. Linux x86_64 support and portable packaging were added subsequently;
Windows and web support remain future work. The existing work was saved first in
commit `a89ad20`; all restructuring is in the following commit, as requested.
Revert the restructuring commit to return to that saved baseline.

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
the original release profile. Package/binary `lumiere`, currently version 0.1.3,
retains its bundle ID and persistence paths. macOS uses `bundle.sh`; Linux x86_64
uses `bundle-linux.sh` to produce a portable archive with a per-user installer,
desktop launcher and icon-theme assets. Bundle builds explicitly select
`-p lumiere --locked`. The desktop package rejects wasm32 explicitly; web work
must select a future web package rather than compile native desktop as web.

## Validation

Environment: macOS / Apple Silicon; rustc 1.91.1
(`ed61e7d7e`, 2025-11-07), cargo 1.91.1 (`ea2d97820`, 2025-10-10).
Commands ran with `$HOME/.cargo/bin` on PATH. Logs are in [validation](validation/).

| Check | Baseline | Workspace |
| --- | --- | --- |
| fmt | pass | `cargo fmt --all --check`: pass |
| clippy, all targets, warnings denied | pass | `--locked --workspace --all-targets`: pass |
| default parallel tests | 88 pass, 2 fail | pass: app 73, core 29, desktop 20 |
| serial tests (`-- --test-threads=1`) | 90 pass | 122 pass: app 73, core 29, desktop 20 |
| release + signed bundle | pass | pass |
| locked metadata | original single package | three local packages, pass |
| core / desktop dependency trees | coupled binary | no prohibited dependencies, pass |

The refactored workspace initially failed more tests under default parallelism
(`oversized_images_are_clamped_to_the_texture_limit`,
`pinch_gesture_zooms_about_the_pointer`, `scroll_pans_an_overflowing_image`)
than the single-package baseline (`open_with_submenu_lists_applications`,
`scroll_pans_an_overflowing_image`). Both failure groups were timing issues,
now fixed: the decode pipeline serialized every loader's pixel work behind a
process-wide gate, so parallel tests queued behind each other's decodes and
timed out; and the Open With prefetch thread performed real LaunchServices and
icon lookups even in test builds. The pixel-work gate is now per-loader (the
production app runs a single loader, so its behavior is unchanged) and test
builds skip native Open With lookups. The default parallel workspace suite
passes on macOS without changing test assertions or worker scheduling, and CI
runs it with the default parallelism.

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

Linux x86_64 now has a native runner, portable packaging, a desktop launcher and
fallback UI paths. Some macOS-only integrations remain intentionally platform
specific, including ImageIO codecs, Finder/LaunchServices, AppKit panels and the
native menu bar. Windows remains the next desktop-platform phase; it needs its
own native services, packaging and validation. WASM remains future work after an
explicit browser requirement, including byte-based APIs, browser storage and
worker/cancellation design.

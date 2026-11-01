# Third-party notices

## Bootstrap Icons

`src/theme.rs` embeds SVG path data derived from
[Bootstrap Icons](https://icons.getbootstrap.com/), Copyright 2019–2024 The
Bootstrap Authors. Bootstrap Icons is licensed under the MIT License:

> The MIT License (MIT)
>
> Copyright (c) 2019-2024 The Bootstrap Authors
>
> Permission is hereby granted, free of charge, to any person obtaining a copy
> of this software and associated documentation files (the "Software"), to deal
> in the Software without restriction, including without limitation the rights
> to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
> copies of the Software, and to permit persons to whom the Software is
> furnished to do so, subject to the following conditions:
>
> The above copyright notice and this permission notice shall be included in
> all copies or substantial portions of the Software.
>
> THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
> IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
> FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
> AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
> LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
> OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN
> THE SOFTWARE.

## Rust dependencies

Lumiere uses Rust crates with their respective licenses. The current dependency
set is primarily MIT and/or Apache-2.0 licensed; Cargo.lock records the exact
resolved versions used for a build. Some bundled egui font assets are subject to
the SIL Open Font License 1.1 and the Ubuntu Font Licence 1.0. Their notices
remain applicable when those assets are distributed.

## Video playback libraries in the 0.1.6 release bundles

Video playback uses [mpv](https://mpv.io/) and FFmpeg. The bundled libraries
include GPL-licensed components; their licenses and corresponding sources also
apply when redistributing the binaries. Lumiere's own source remains available
under its stated license.

- **Windows:** the libmpv DLL is from the mpv project's
  [official GPL build](https://github.com/mpv-player/mpv/releases/tag/git-release),
  built by [run 41103006513](https://github.com/mpv-player/mpv/actions/runs/41103006513)
  from [mpv commit 9e2ed65](https://github.com/mpv-player/mpv/tree/9e2ed65eb0c02f99a352dfa1ad3c85ecceefe005).
  The build log identifies FFmpeg revision
  [a3c5339](https://github.com/FFmpeg/FFmpeg/commit/a3c5339) and the toolchain
  image `ghcr.io/btbn/ffmpeg-builds/win64-gpl@sha256:572cc7bc0974ebe2e5fd5f82c49e68c07942c6858098fd04f8daa23e3dc4dd2c`.
  See the [mpv build script](https://github.com/mpv-player/mpv/blob/9e2ed65eb0c02f99a352dfa1ad3c85ecceefe005/ci/build-mingw64-full.sh)
  for its dependency build procedure. The exact corresponding sources of all
  statically linked dependencies have not been verified; the recorded toolchain
  image digest no longer resolves on GHCR. Do not redistribute that DLL until
  the corresponding sources and license obligations have been confirmed.
- **macOS:** the libmpv dylibs are from [IINA 1.5.0](https://github.com/iina/iina/releases/tag/v1.5.0).
  Its [dependency build scripts](https://github.com/iina/deps-buildscripts/tree/iina-1.5.0-release),
  [source versions and URLs](https://github.com/iina/deps-buildscripts/blob/iina-1.5.0-release/versions.env),
  [credits](https://github.com/iina/iina/blob/v1.5.0/iina/Credits.rtf), and
  [license](https://github.com/iina/iina/blob/v1.5.0/LICENSE) are published by IINA.
- **Linux:** the portable bundles contain Ubuntu 22.04 packages. Their copyright
  files and `build-packages.tsv` version/source list are included in the bundle.
  Corresponding Ubuntu package sources are available from
  [Ubuntu's archive](https://archive.ubuntu.com/ubuntu/pool/).

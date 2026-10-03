# Runtime dependency bundle

oca release artifacts are self-contained. The application never downloads models, media
engines, Python packages, fonts or codec plugins while it is running. Network access during
normal use is limited to features whose purpose is network access: checking GitHub Releases,
applying an update, and downloading media from a URL supplied by the user.

`packaging/bundle-manifest.json` is the machine-readable source of truth for versions, pinned
download URLs, SHA-256 checksums, licenses and bundle placement. `packaging/fetch_models.py`
downloads model files only while assembling a release. `packaging/validate_bundle.py` blocks
publication when any required runtime component is missing.

## Included components

| Component | Included form | Used by |
|---|---|---|
| FFmpeg (pinned 8.1 LGPL build on Windows/Linux; GPL-enabled Homebrew runner build on macOS) | `ffmpeg` CLI, libav shared libraries | Native bridge, yt-dlp merge/extraction |
| ONNX Runtime 1.28 | Statically linked by `ort-sys` | Auto-reframe and background removal |
| whisper.cpp | Statically linked by `whisper-rs` | Automatic subtitles |
| Whisper Base | `resources/models/ggml-base.bin` | Automatic subtitles |
| UltraFace | `resources/models/version-RFB-320_simplified.onnx` | Auto-reframe |
| MODNet | `resources/models/modnet_photographic.onnx` | Background removal |
| Piper Faber pt-BR | ONNX model plus JSON sidecar | Text-to-speech |
| eSpeak NG | Statically linked engine plus `espeak-ng-data/` | Piper phonemization |
| CPython 3.10.21 | Private runtime beside `ytbridge` | YouTube downloader helper |
| yt-dlp 2026.07.04 | Verified release package copied into private Python `site-packages` | YouTube downloader helper |
| yt-dlp EJS 0.8.0 | Verified wheel extracted into private Python `site-packages` | YouTube signature extraction |
| Deno 2.9.5 | Private executable beside `ytbridge` | JavaScript runtime required by current yt-dlp YouTube support |
| Application fonts | Compiled into the Rust binary with `include_bytes!` | Titles, subtitles and text metrics |
| Inno Setup 6.7.1 | Build-time compiler, not an installed runtime | Configurable Windows installer |
| dpkg-deb | Build-time compiler, not an installed runtime | Debian package |
| Apple codesign and hdiutil | macOS runner tools, not installed runtimes | Signed `.app` and compressed DMG |

Each portable Windows/Linux package also carries `resources/DEPENDENCIES.json`, generated directly from `Cargo.lock`
and the bundle manifest. It inventories every Rust package with its exact version, registry
source and checksum as well as every native/model dependency. It also records the path, size
and SHA-256 of every actual file in the assembled payload, including shared libraries,
private Python files; validation rejects missing, additional or changed
files. `packaging/generate_third_party_notices.py` runs offline during every assembly. It uses
locked Cargo metadata and the bundle manifest to generate `resources/licenses/THIRD_PARTY_NOTICES.txt`,
copies package-specific `LICENSE`, `COPYING`, `COPYRIGHT`, `UNLICENSE`, and `NOTICE` files, and
includes checksum-verified full SPDX texts vendored from the pinned revision recorded in
`packaging/licenses/spdx-manifest.json`. Large upstream notices that are not published inside a
Cargo crate, such as ONNX Runtime's third-party notice, are pinned and checksum-verified by
`packaging/licenses/components-manifest.json`. Font OFL notices and the oca license are also included
under `resources/licenses/`. Bundle validation requires every distributed native/model component
to appear in the generated notice. `resources/RUNTIME_VERSIONS.txt` records the
actual FFmpeg and Python versions reported by the assembled payload.
It also records yt-dlp, yt-dlp EJS and Deno versions. Bundle validation recalculates every
model's hash and rejects a runtime-version mismatch, so an accidental script/manifest drift
cannot be published.
The macOS DMG carries the same inventory as `DEPENDENCIES.json` beside `Oca.app`. Keeping it
outside the signed app avoids changing the code signature after file hashes are generated.

The Windows ZIP contains the DLLs beside `ui.exe`, where the Windows loader can resolve them.
The Inno Setup installer contains that same validated tree and adds a configurable installation
directory, Start menu/optional desktop shortcuts and uninstallation; setup performs no network
downloads. Inno Setup itself is needed only on the release runner that compiles the installer
and is not installed on end-user machines. The installer is currently unsigned; release signing
requires configuring a Windows code-signing certificate in CI.
The Linux portable tree uses a launcher that sets private library and tool paths before
starting `ui-bin`; the AppImage and `.deb` wrap that exact validated tree. The Debian package
installs it under `/opt/oca`, exposes `/usr/bin/oca`, registers an English/pt-BR desktop entry,
and performs no network access in its maintainer scripts. `OCA_RESOURCE_DIR` can override
the default `resources/`-beside-the-executable lookup for development and diagnostics.
The macOS release has separate native Apple Silicon and Intel DMGs. `Oca.app` uses the standard
`Contents/MacOS`, `Contents/Resources` and `Contents/Frameworks` layout. The assembler walks all
Mach-O executables, libraries and Python extensions recursively, copies every
non-system dependency into the app, rewrites load paths to `@loader_path`, rejects architecture
or external-path leaks, and signs the resulting bundle. Releases use Developer ID signing and
Apple notarization when the corresponding CI secrets are configured; otherwise CI produces a
validated ad-hoc-signed artifact suitable for testing but subject to Gatekeeper warnings.

The preview decodes and composites in Bush (`crates/bush`), which uses the FFmpeg libraries above;
there is no separate media framework or plugin set to bundle. Audio output goes through the
operating system's own audio API (WASAPI, CoreAudio, ALSA); on Linux the ALSA library and its
configuration stay system-provided and are not copied into the bundle.

## Deliberate system contract

Operating-system kernels, the Windows Universal C Runtime, Linux glibc, graphics/display stacks
and GPU vendor drivers are platform contracts rather than application payloads. Linux VAAPI
encode discovers DRM render nodes under `/dev/dri` (or uses `OCA_VAAPI_DEVICE`) and therefore
requires the matching Intel/AMD VAAPI driver and device permissions from the host. Hardware
acceleration remains optional and falls back to CPU paths when a compatible vendor driver is not
available. Explorer on Windows and `xdg-open` on Linux are optional desktop integrations used
only by "open folder" actions; failure to find them does not affect editing or export. The exact
minimum contracts are recorded in the manifest.

Music and SFX are user media, not application dependencies, and are therefore not shipped.

## Release process

Pushing a `v*` tag runs `.github/workflows/release.yml`. It builds Windows, Linux, Apple Silicon
macOS and Intel macOS from the same source revision. It verifies downloaded archives, assembles
and validates complete payloads, then publishes the Windows portable ZIP and bilingual Inno
Setup installer, Linux tarball/AppImage/`.deb`, and both macOS DMGs. A manually dispatched
workflow builds and retains the artifacts without publishing a release.

To update a dependency, update its URL, version and SHA-256 in the manifest together. For
FFmpeg, mirror the same pin in `.github/workflows/ci.yml`; see `.github/FFMPEG_UPDATE.md`.

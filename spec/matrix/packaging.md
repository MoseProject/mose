# Packaging & Distribution

Detail: `matrix/changelog.md` Fase 8, plus the "Custom title bar" and "YouTube download modal"
ad-hoc entries.

- [x] Windows/Linux/macOS release bundles — Windows Inno Setup installer (bilingual EN/pt-BR)
      + portable ZIP, Linux AppImage + root-owned `.deb`, macOS Apple Silicon + Intel DMGs
      (Developer ID signing/notarization when CI credentials configured).
- [x] Everything embedded: FFmpeg (LGPL, static), Whisper
      Base, UltraFace, MODNet, Piper pt-BR, eSpeak data, private CPython/yt-dlp/yt-dlp-ejs/Deno,
      Linux AppImage runtime — no runtime model downloader anywhere, `packaging/
      validate_bundle.py` blocks publication if anything required is missing or a hash drifts.
- [x] Auto-update — `avcore::update_check` hits GitHub Releases API, `self_update` on a worker
      thread, atomic replacement, retry/manual-download fallback on failure, restart required
      after install (so normal `eframe` shutdown still persists prefs). Linux AppImage uses its
      own `$APPIMAGE`-based replace/restart path.
- [x] `.github/workflows/release.yml` builds + publishes tagged releases; bundle manifest
      (`packaging/bundle-manifest.json`) pins every model/engine URL + SHA-256.
- [x] Custom title bar (no OS window chrome) — drag-to-move, hand-painted min/max/close,
      resize-border handling (south/west/east/bottom-corners only — top edge deliberately
      excluded, conflicts with the title-bar drag region).
- [x] YouTube download modal — MP4 (height picker)/MP3 (bitrate picker), progress + cancel,
      imports result through the normal media-import pipeline on completion.

## Verification caveats (real, not silently assumed)

- GitHub Releases API call itself unexercised in this dev sandbox (outbound proxy blocks
  `api.github.com` directly) — JSON shape/headers checked against GitHub's documented API,
  not against a live response.
- `ytbridge`'s Linux Python-runtime bundling is written and syntax-checked only — no Linux
  build/toolchain available in the dev sandbox that wrote it.
- Custom title bar's actual OS-level drag/resize behavior (especially Wayland, best-effort in
  winit) is unverified beyond API cross-checking against vendored `egui`/`eframe` source.

---

[← back to spec/INDEX.md](../INDEX.md)

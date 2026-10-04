# Webplayer integration

Source: https://github.com/zzzwannasleep/webplayer (main, commit d3bbbfcf7717a5107e1a6552453ee5b8f915d7d1, vendored on 2026-10-04).
Upstream JavaScript is distributed under its accompanying MIT LICENSE. FBZ uses the Player and Subtitles APIs; it does not embed the upstream Emby application.

Generated browser assets are built from the versions pinned in pnpm-lock.yaml. `@ffmpeg/core` is GPL-2.0-or-later (corresponding upstream source/build recipes: https://github.com/ffmpegwasm/ffmpeg.wasm/tree/v0.12.10); JASSUB includes libass/font libraries with the licenses declared in its package (https://github.com/ThaUnknown/jassub). libpgs and the ffmpeg JavaScript wrapper retain their package licenses. License files available in installed packages are copied with the generated assets.

Local integration adjustment: software audio asset URLs resolve relative to the player module, not the SPA document route. Disposal also terminates the dedicated player/audio workers when leaving the Vue overlay. These adjustments are applied by scripts/build-webplayer.mjs; vendored source remains unchanged.

The engine remuxes supported containers in the browser; video codecs still depend on browser/platform support. Range and CORS are required for remote remux playback. FFmpeg WASM for unsupported audio is loaded lazily. No source stream or account token is sent to a third-party playback service.

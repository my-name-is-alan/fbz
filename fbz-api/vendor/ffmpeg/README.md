# Bundled FFmpeg Policy

FBZ distributions should include platform-specific `ffmpeg` and `ffprobe` binaries here when a packaged build is produced.

Runtime resolution order:

1. Use `FFMPEG_PATH` and `FFPROBE_PATH` when they are explicitly configured.
2. Try the default command names `ffmpeg` and `ffprobe` from `PATH`.
3. Fall back to this bundled directory when `FBZ_ENABLE_BUNDLED_FFMPEG=true`.
4. Fail startup with a clear diagnostic if neither external nor bundled binaries are executable.

Expected files:

- Windows: `ffmpeg.exe`, `ffprobe.exe`
- Linux/NAS/Docker: `ffmpeg`, `ffprobe`

Before distributing bundled binaries, record the source URL, version, build flags, and license mode. FFmpeg builds are generally LGPL when built without GPL components, but enabling GPL libraries or `--enable-gpl` changes redistribution obligations. Do not ship a bundled build until its license mode and codec flags have been reviewed.

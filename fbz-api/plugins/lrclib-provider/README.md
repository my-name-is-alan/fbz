# fbz.lyrics.lrclib — 远程歌词（LRCLIB，免 key）

- 扩展点：`lyrics.provider.query`（sync，迁移 0109 加入 allow-list）。
- core 接线（`routes/remote_lyrics.rs` + `routes/lyrics.rs`）：
  - `GET /Audio/{id}/RemoteSearch/Lyrics[?SearchTerm=&ProviderName=]` → 插件 `{action:"search", title, artist, album, durationSeconds, searchTerm}`；结果 id 为 `fbz.lyrics.lrclib~{lrclibId}`。
  - `GET /Providers/Lyrics/{lyricId}` → 插件 `{action:"download", lyricId}`，返回 JF `LyricDto`（不落盘）。
  - `POST /Audio/{id}/RemoteSearch/Lyrics/{lyricId}`（管理员）→ 下载并写 sidecar（同步 `.lrc`，纯文本 `.txt`）。
  - 未启用任何歌词插件时，core 回退到内置 LRCLIB 搜索（id `lrclib-{n}`，同样支持上面两个下载端点）。
- 配置：`base_url`（默认 `https://lrclib.net`，可指向自建镜像）、`max_results`（默认 10）。
- 运行：`PORT=19109 node server.mjs`；测试：`node --test provider.test.mjs`。

# OpenSubtitles Provider（远程字幕）

可装包身份：`fbz.subtitle.opensubtitles` · `runtime: http` · 订阅 `subtitle.provider.query`（迁移 0108）。

Core 接线（`compat/emby/routes/remote_subtitles.rs`）：

- `GET /Items/{id}/RemoteSearch/Subtitles/{lang}` → `{action:"search", title, year, imdbId, seriesName, season, episode, fileName, language}` → JF `RemoteSubtitleInfo[]`，Id = `fbz.subtitle.opensubtitles~<base64url>`。
- `POST /Items/{id}/RemoteSearch/Subtitles/{subtitleId}`（管理员）→ `{action:"download"}` → core 写 sidecar `{stem}.{lang}.{fmt}` 并入队 probe。
- `GET /Providers/Subtitles/Subtitles/{subtitleId}` → 直接返回字幕内容。
- 未启用任何订阅插件：搜索 `[]`、下载 409（不造假）。

## 密钥（可选）

| 配置 | 说明 |
| --- | --- |
| `api_key`（secret，可选；或 env `OPENSUBTITLES_API_KEY`） | 有：走 api.opensubtitles.com v1（搜索 + `/download` 取链接）；无：走免 key 的旧版 rest.opensubtitles.org |
| `user_agent` | 默认 com=`fbz v0.1`，legacy=`TemporaryUserAgent` |
| `max_results` | 默认 30 |

## 运行 / 测试

```bash
node plugins/opensubtitles-provider/server.mjs      # 127.0.0.1:19107/fbz-plugin
node --test plugins/opensubtitles-provider/provider.test.mjs
```

## 已知限制

- 旧版 rest.opensubtitles.org 为官方弃用接口，可能随时下线；生产建议配置 api_key。
- moviehash：core 对本地可读、≥64KiB 的文件按 OpenSubtitles 定义计算 `movieHash`/`movieByteSize` 随搜索请求下发；有 key 时作为 `moviehash` 参数，无 key 时先走 legacy 哈希搜索再合并文本搜索（哈希命中排前、`isHashMatch=true`）。strm/远端存储文件不算哈希，退回 IMDb id / 标题搜索。
- 广告行：core 在下载后统一剔除（srt 整条 cue 去掉并重编号；其它格式去行），插件不处理。
- legacy 下载仅允许 `dl.opensubtitles.org` 主机；非 UTF-8 字幕按 `SubEncoding` 转码为 UTF-8。

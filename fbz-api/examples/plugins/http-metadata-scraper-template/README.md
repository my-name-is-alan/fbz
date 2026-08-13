# Demo Metadata Scraper (sync provider template)

第一方示例：**同步元数据刮削器插件**。订阅 `metadata.provider.query` hook 后，
FBZ 在每次元数据刷新任务内同步调用本插件（见 `docs/plugin-system.md` 的
"Synchronous Provider Invocation"），把识别层的 lookup 上下文发过来，插件返回
一个元数据贡献。

合并优先级由宿主控制（`metadata-scraper-design.md` §9）：

- 内置 provider（TMDB/TVDB/…）优先；插件只**填补空字段**；
- 当内置 provider 全部未命中时，插件作为**兜底 base match**；
- artwork 落在 `plugin:{id}` 命名空间，绝不覆盖内置图片。

本模板从本地 `scraper-fixture.json`（按规范化标题索引）作答；真实刮削器把
`handleProviderQuery` 换成对上游（豆瓣 / Bangumi / 私有 NFO 服务…）的调用即可。

## 请求 / 响应契约

请求（`POST {entrypoint}`，带 `x-fbz-plugin-invocation: sync` 头）：

```json
{
  "invocation": "sync",
  "hookEvent": "metadata.provider.query",
  "handler": "providers.query",
  "request": {
    "lookup": {
      "itemType": "movie",
      "title": "Sintel",
      "originalTitle": null,
      "productionYear": 2010,
      "season": null,
      "episode": null,
      "tmdbId": null,
      "imdbId": null,
      "tvdbId": null,
      "language": "zh-CN",
      "country": "CN"
    },
    "current": null
  }
}
```

`current` 非空时表示内置 provider 已命中，插件处于补空 enrichment 角色。

响应（SDK 会包一层 `{ok, result}` 信封，宿主两种形态都接受）：

```json
{
  "metadata": {
    "title": "Sintel",
    "overview": "…",
    "productionYear": 2010,
    "premiereDate": "2010-09-27",
    "communityRating": 7.4,
    "externalIds": [{ "provider": "imdb", "externalId": "tt1727587" }],
    "artwork": [{ "artworkType": "poster", "remoteUrl": "https://…", "isPrimary": false }],
    "genres": ["动画", "奇幻"],
    "studios": ["Blender Foundation"],
    "people": [{ "name": "Colin Levy", "roleType": "Director" }]
  }
}
```

`{ "metadata": null }` 表示无贡献。列表有宿主侧上限（externalIds 32 / artwork 16 /
genres、studios 32 / people 100），artwork 类型限 poster/backdrop/logo/thumb/banner。

## 本地运行

```bash
node examples/plugins/http-metadata-scraper-template/server.mjs
# PORT=19093 PLUGIN_PATH=/fbz-plugin SCRAPER_FIXTURE_PATH=... 可覆盖
```

打包安装（PowerShell）：

```powershell
./scripts/package-plugin.ps1 -PluginDir examples/plugins/http-metadata-scraper-template -Force
```

安装 → 审批 → 启用后，对任意条目触发元数据刷新即可在
`provider_attempts` 里看到 `plugin:dev.fbz.scraper.demo` 的命中记录。
失败调用自动落 `plugin_sync_invocations` 审计，连续失败触发熔断
（`PLUGIN_SYNC_*` 环境变量调节预算与冷却）。

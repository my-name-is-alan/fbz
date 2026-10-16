# fbz.provider.books — 书籍元数据（Open Library / Google Books）

- 扩展点：`metadata.provider.query`（sync）。**只回答 `lookup.itemType === "book"`**，其他类型返回 `metadata: null`。
- core 侧（`metadata::service::refresh_book`）只接受带 `isbn` / `openlibrary` / `googlebooks` 外部 id 的贡献，避免影视 provider 误匹配书籍。
- 请求：`{lookup:{itemType:"book", title, authors[], isbn, productionYear, language}, current:null}`。
- 来源（`sources`，默认 `openlibrary,googlebooks`，前者优先、后者补空字段）：
  - Open Library（免 key）：有 ISBN → `/isbn/{isbn}.json` + work/author；否则 `search.json?title=&author=`；封面 `covers.openlibrary.org/b/id/{id}-L.jpg`。
  - Google Books：`volumes?q=isbn:` 或 `intitle:+inauthor:`；`google_books_api_key` 可选（无 key 走匿名配额）。
- 返回：title / overview / productionYear / people(writer) / externalIds / artwork(poster)。core 仅补空字段，本地已有封面时不覆盖。

运行：`PORT=19108 node server.mjs`；测试：`node --test provider.test.mjs`。

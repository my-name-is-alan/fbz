/**
 * 一次性抓取脚本：从 TMDB 拉取大量真实数据，烤成两个文件：
 *   src/service/mock/tmdb-catalog.json  —— 轻量目录（数百条，用于首页/媒体库网格），随包加载
 *   src/service/mock/tmdb-details.json  —— 完整详情（演职员/相似/系列/季集），详情页懒加载
 *
 *   node scripts/fetch-tmdb.mjs
 *
 * token 只在本脚本运行时使用，绝不进入前端构建包。
 * 图片只存 TMDB 相对路径，前端用公开 CDN 拼完整地址，无需 token。
 *
 * 说明：
 * - libraryId（movie/series/anime/documentary）= 归属媒体库；type（movie/tv）= 详情页类型路由。
 *   动漫多为 tv、纪录片多为 movie，两者与媒体库解耦。
 * - 版本/规格标签（4K/DTS-HD…）、字幕、剧集标题等 TMDB 不提供，由前端按需合成，这里不抓。
 */
import { readFileSync, writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, resolve } from "node:path";

const __dirname = dirname(fileURLToPath(import.meta.url));
const root = resolve(__dirname, "..");

/* ---------- 读取 .env 里的 TMDB 凭据（只取引号内的值，忽略行尾注释） ---------- */
const env = {};
for (const line of readFileSync(resolve(root, ".env"), "utf8").split("\n")) {
  const m = line.match(/^\s*([\w-]+)\s*=\s*"([^"]*)"/);
  if (m) env[m[1]] = m[2];
}
const TOKEN = env.api_token;
const BASE = env.api_base_url ?? "https://api.themoviedb.org/3";
if (!TOKEN) throw new Error("缺少 api_token，请检查 .env");

const LANG = "zh-CN";

async function tmdb(path, params = {}) {
  const url = new URL(BASE + path);
  url.searchParams.set("language", LANG);
  for (const [k, v] of Object.entries(params)) url.searchParams.set(k, v);
  for (let attempt = 0; attempt < 3; attempt++) {
    try {
      const res = await fetch(url, {
        headers: { Authorization: `Bearer ${TOKEN}`, accept: "application/json" },
      });
      if (res.status === 429) {
        await sleep(1500);
        continue;
      }
      if (!res.ok) throw new Error(`${res.status} ${res.statusText} @ ${path}`);
      return res.json();
    } catch (e) {
      if (attempt === 2) throw e;
      await sleep(800);
    }
  }
}

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

/** 简易并发池 */
async function pool(items, size, fn) {
  const out = [];
  let i = 0;
  const workers = Array.from({ length: size }, async () => {
    while (i < items.length) {
      const idx = i++;
      try {
        out[idx] = await fn(items[idx], idx);
      } catch (e) {
        out[idx] = null;
        process.stdout.write(`  ✗ ${items[idx]}: ${e.message}\n`);
      }
    }
  });
  await Promise.all(workers);
  return out.filter(Boolean);
}

/* ---------- 题材字典 ---------- */
const genreMap = new Map();
async function loadGenres() {
  for (const kind of ["movie", "tv"]) {
    const { genres } = await tmdb(`/genre/${kind}/list`);
    for (const g of genres) genreMap.set(`${kind}:${g.id}`, g.name);
  }
}
const genreNames = (kind, ids = []) =>
  ids.map((id) => genreMap.get(`${kind}:${id}`)).filter(Boolean);

/* ---------- discover 抓多页目录 ---------- */
async function discover(kind, params, pages, libraryId) {
  const items = [];
  for (let page = 1; page <= pages; page++) {
    const data = await tmdb(`/discover/${kind}`, {
      ...params,
      sort_by: "popularity.desc",
      include_adult: "false",
      "vote_count.gte": "150",
      page: String(page),
    });
    for (const r of data.results ?? []) {
      if (!r.poster_path) continue;
      const isTv = kind === "tv";
      items.push({
        id: r.id,
        type: isTv ? "tv" : "movie",
        libraryId,
        title: isTv ? r.name : r.title,
        overview: r.overview,
        poster_path: r.poster_path,
        backdrop_path: r.backdrop_path,
        year: Number((isTv ? r.first_air_date : r.release_date)?.slice(0, 4)) || null,
        rating: r.vote_average ? Number(r.vote_average.toFixed(1)) : null,
        genres: genreNames(kind, r.genre_ids),
      });
    }
  }
  return items;
}

const trimCast = (credits, n = 14) =>
  (credits?.cast ?? []).slice(0, n).map((c) => ({
    id: c.id,
    name: c.name,
    character: c.roles?.[0]?.character ?? c.character ?? "",
    profile_path: c.profile_path ?? null,
    order: c.order ?? 0,
  }));

const similarItems = (data, libraryId, type, n = 12) =>
  (data?.results ?? [])
    .filter((r) => r.poster_path)
    .slice(0, n)
    .map((r) => ({
      id: r.id,
      type,
      libraryId,
      title: type === "tv" ? r.name : r.title,
      poster_path: r.poster_path,
      year: Number((type === "tv" ? r.first_air_date : r.release_date)?.slice(0, 4)) || null,
      rating: r.vote_average ? Number(r.vote_average.toFixed(1)) : null,
    }));

async function movieDetail(item) {
  const m = await tmdb(`/movie/${item.id}`, { append_to_response: "credits,similar" });
  return {
    key: `movie:${m.id}`,
    runtime: m.runtime,
    tagline: m.tagline,
    original_title: m.original_title,
    collection_id: m.belongs_to_collection?.id ?? null,
    collection_name: m.belongs_to_collection?.name ?? null,
    cast: trimCast(m.credits),
    directors: (m.credits?.crew ?? [])
      .filter((c) => c.job === "Director")
      .slice(0, 3)
      .map((c) => ({ id: c.id, name: c.name, job: c.job })),
    similar: similarItems(m.similar, item.libraryId, "movie"),
  };
}

async function tvDetail(item) {
  const t = await tmdb(`/tv/${item.id}`, { append_to_response: "aggregate_credits,similar" });
  return {
    key: `tv:${t.id}`,
    tagline: t.tagline,
    original_title: t.original_name,
    seasons_count: t.number_of_seasons,
    episodes_count: t.number_of_episodes,
    creators: (t.created_by ?? []).slice(0, 3).map((c) => ({ id: c.id, name: c.name })),
    // 季概要（真实），剧集标题前端按 episode_count 合成
    seasons: (t.seasons ?? [])
      .filter((s) => s.season_number > 0 && s.episode_count > 0)
      .map((s) => ({
        season_number: s.season_number,
        name: s.name,
        episode_count: s.episode_count,
        air_date: s.air_date,
        poster_path: s.poster_path,
        overview: s.overview,
      })),
    cast: trimCast(t.aggregate_credits),
    similar: similarItems(t.similar, item.libraryId, "tv"),
  };
}

async function collectionDetail(id) {
  const c = await tmdb(`/collection/${id}`);
  return {
    key: `collection:${c.id}`,
    id: c.id,
    type: "collection",
    title: c.name,
    overview: c.overview,
    poster_path: c.poster_path,
    backdrop_path: c.backdrop_path,
    parts: (c.parts ?? [])
      .filter((p) => p.poster_path)
      .sort((a, b) => (a.release_date ?? "").localeCompare(b.release_date ?? ""))
      .map((p) => ({
        id: p.id,
        title: p.title,
        poster_path: p.poster_path,
        year: Number(p.release_date?.slice(0, 4)) || null,
        rating: p.vote_average ? Number(p.vote_average.toFixed(1)) : null,
      })),
  };
}

async function personDetail(id) {
  const p = await tmdb(`/person/${id}`, { append_to_response: "combined_credits" });
  return {
    key: `person:${p.id}`,
    id: p.id,
    type: "person",
    name: p.name,
    biography: p.biography,
    profile_path: p.profile_path,
    birthday: p.birthday,
    place_of_birth: p.place_of_birth,
    known_for_department: p.known_for_department,
    known_for: (p.combined_credits?.cast ?? [])
      .filter((c) => c.poster_path)
      .sort((a, b) => (b.popularity ?? 0) - (a.popularity ?? 0))
      .slice(0, 12)
      .map((c) => ({
        id: c.id,
        type: c.media_type,
        libraryId: c.media_type === "tv" ? "series" : "movie",
        title: c.title ?? c.name,
        character: c.character,
        poster_path: c.poster_path,
        year: Number((c.release_date ?? c.first_air_date)?.slice(0, 4)) || null,
        rating: c.vote_average ? Number(c.vote_average.toFixed(1)) : null,
      })),
  };
}

async function main() {
  console.log("加载题材字典...");
  await loadGenres();

  console.log("抓取目录（discover 多页）...");
  const movie = await discover("movie", {}, 7, "movie"); // ~140
  const series = await discover("tv", {}, 6, "series"); // ~120
  // 动漫：tv + 动画题材 + 日本
  const anime = await discover("tv", { with_genres: "16", with_origin_country: "JP" }, 4, "anime"); // ~80
  // 纪录片：movie + 纪录题材
  const documentary = await discover("movie", { with_genres: "99" }, 3, "documentary"); // ~60

  // 目录去重（同一作品可能跨库出现，按 type:id 保留首次）
  const seen = new Set();
  const catalogRaw = [...movie, ...series, ...anime, ...documentary].filter((it) => {
    const k = `${it.type}:${it.id}`;
    if (seen.has(k)) return false;
    seen.add(k);
    return true;
  });
  console.log(
    `目录 ${catalogRaw.length}：电影 ${movie.length} · 剧集 ${series.length} · 动漫 ${anime.length} · 纪录片 ${documentary.length}（去重后）`,
  );

  console.log("抓取详情（演职员 + 相似 + 季）...");
  const detailList = await pool(catalogRaw, 8, (it) =>
    it.type === "tv" ? tvDetail(it) : movieDetail(it),
  );
  const details = Object.fromEntries(detailList.map((d) => [d.key, d]));

  // 系列：从电影详情收集
  const collectionIds = [...new Set(detailList.map((d) => d.collection_id).filter(Boolean))];
  console.log(`抓取系列 ${collectionIds.length}...`);
  const collectionList = await pool(collectionIds, 6, (id) => collectionDetail(id));
  for (const c of collectionList) details[c.key] = c;

  // 演员：取演职表里高频出现的人
  const freq = new Map();
  for (const d of detailList)
    for (const c of d.cast ?? []) freq.set(c.id, (freq.get(c.id) ?? 0) + 1);
  const personIds = [...freq.entries()]
    .sort((a, b) => b[1] - a[1])
    .slice(0, 60)
    .map(([id]) => id);
  console.log(`抓取演员 ${personIds.length}...`);
  const personList = await pool(personIds, 8, (id) => personDetail(id));
  for (const p of personList) details[p.key] = p;

  const catalog = {
    generated_at: new Date().toISOString(),
    image_base: "https://image.tmdb.org/t/p",
    items: catalogRaw,
  };

  writeFileSync(resolve(root, "src/service/mock/tmdb-catalog.json"), JSON.stringify(catalog));
  writeFileSync(resolve(root, "src/service/mock/tmdb-details.json"), JSON.stringify(details));

  console.log(
    `\n完成：目录 ${catalogRaw.length} 条 · 详情 ${Object.keys(details).length} 条（含系列 ${collectionList.length}、演员 ${personList.length}）`,
  );
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});

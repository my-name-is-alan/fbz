/**
 * 书籍元数据 provider：Open Library（免 key）+ Google Books（key 可选）。
 * 只处理 lookup.itemType === 'book'；其他类型返回 null，避免与影视 provider 抢匹配。
 * 结果必须带书籍外部 id（isbn/openlibrary/googlebooks），core 只接受这类贡献。
 */
import { buildUrl, fetchJson, pushExternalId, safeHttpUrl, trimText } from './utils.mjs'

const OL = 'https://openlibrary.org'
const GB = 'https://www.googleapis.com/books/v1'
const UA = 'fbz-book-provider/0.1 (+https://github.com/fbz)'

export function normalizeIsbn(value) {
  const raw = String(value ?? '').replace(/^(urn:)?isbn:/i, '').replace(/[\s-]/g, '').toUpperCase()
  if (/^\d{13}$/.test(raw)) {
    const sum = [...raw.slice(0, 12)].reduce((s, d, i) => s + Number(d) * (i % 2 ? 3 : 1), 0)
    return (10 - (sum % 10)) % 10 === Number(raw[12]) ? raw : null
  }
  if (/^\d{9}[\dX]$/.test(raw)) {
    const sum = [...raw].reduce((s, d, i) => s + (d === 'X' ? 10 : Number(d)) * (10 - i), 0)
    return sum % 11 === 0 ? raw : null
  }
  return null
}

const yearOf = (value) => {
  const m = String(value ?? '').match(/\b(1[5-9]\d\d|20\d\d)\b/)
  return m ? Number(m[1]) : null
}
const textOf = (value) => trimText(typeof value === 'object' && value ? value.value : value, 8000)
const stripHtml = (s) => (s ? s.replace(/<[^>]+>/g, ' ').replace(/\s+/g, ' ').trim() : s)

const norm = (s) => String(s ?? '').toLowerCase().normalize('NFKD').replace(/[\u0300-\u036f]/g, '')
  .replace(/^(the|a|an)\s+/, '').replace(/[^\p{L}\p{N}]+/gu, ' ').trim()

/** 标题搜索的防误配：标题需相等或一方为另一方前缀（副标题）；双方都有作者时需姓氏重叠。 */
export function plausibleMatch(lookup, title, authors = []) {
  const want = norm(lookup.title)
  const got = norm(title)
  if (!want || !got) return false
  if (!(want === got || got.startsWith(`${want} `) || want.startsWith(`${got} `))) return false
  const wantAuthors = (Array.isArray(lookup.authors) ? lookup.authors : [lookup.author]).filter(Boolean)
  if (!wantAuthors.length || !authors.length) return true
  const last = (n) => norm(n).split(' ').pop()
  const gotLast = new Set(authors.map(last))
  return wantAuthors.some((a) => gotLast.has(last(a)))
}

async function getJson(fetcher, url) {
  try {
    return await fetcher(url, { userAgent: UA })
  } catch (err) {
    if (err?.status === 404) return null
    throw err
  }
}

/** Open Library：ISBN 走 /isbn/{isbn}.json，否则 search.json（title + author）。 */
export async function queryOpenLibrary(lookup, fetcher = fetchJson) {
  const isbn = normalizeIsbn(lookup.isbn)
  let edition = null
  let workKey = null
  let doc = null
  if (isbn) {
    edition = await getJson(fetcher, `${OL}/isbn/${isbn}.json`)
    workKey = edition?.works?.[0]?.key ?? null
  }
  if (!edition) {
    const title = trimText(lookup.title, 512)
    if (!title) return null
    const author = Array.isArray(lookup.authors) ? lookup.authors[0] : lookup.author
    const search = await getJson(fetcher, buildUrl(OL, '/search.json', {
      title, author, limit: 5,
      fields: 'key,title,author_name,first_publish_year,cover_i,isbn,edition_key',
    }))
    const docs = search?.docs ?? []
    const candidates = docs.filter((d) => plausibleMatch(lookup, d.title, d.author_name ?? []))
    doc = candidates.find((d) => !lookup.productionYear || d.first_publish_year === lookup.productionYear) ?? candidates[0]
    if (!doc) return null
    workKey = doc.key ?? null
  }
  const work = workKey ? await getJson(fetcher, `${OL}${workKey}.json`) : null
  let authors = doc?.author_name ?? []
  if (!authors.length) {
    const keys = (edition?.authors ?? work?.authors?.map((a) => a.author) ?? []).map((a) => a?.key).filter(Boolean).slice(0, 5)
    for (const key of keys) {
      const a = await getJson(fetcher, `${OL}${key}.json`)
      if (a?.name) authors.push(a.name)
    }
  }
  const coverId = edition?.covers?.find((c) => c > 0) ?? doc?.cover_i ?? work?.covers?.find((c) => c > 0)
  const externalIds = []
  const isbnOut = isbn ?? (doc?.isbn ?? []).map(normalizeIsbn).find((x) => x && x.length === 13) ?? null
  if (isbnOut) pushExternalId(externalIds, 'isbn', isbnOut)
  const olid = (edition?.key ?? workKey ?? '').split('/').pop()
  if (olid) pushExternalId(externalIds, 'openlibrary', olid)
  return {
    title: trimText(edition?.title ?? doc?.title ?? work?.title, 512),
    overview: textOf(edition?.description) ?? textOf(work?.description),
    productionYear: yearOf(edition?.publish_date) ?? doc?.first_publish_year ?? yearOf(work?.first_publish_date),
    people: authors.slice(0, 20).map((name) => ({ name, roleType: 'writer' })),
    externalIds,
    artwork: coverId ? [{ artworkType: 'poster', remoteUrl: `https://covers.openlibrary.org/b/id/${coverId}-L.jpg`, isPrimary: true }] : [],
  }
}

/** Google Books：isbn: 优先，否则 intitle:+inauthor:。key 可选。 */
export async function queryGoogleBooks(lookup, apiKey, fetcher = fetchJson) {
  const isbn = normalizeIsbn(lookup.isbn)
  const author = Array.isArray(lookup.authors) ? lookup.authors[0] : lookup.author
  const title = trimText(lookup.title, 512)
  const q = isbn ? `isbn:${isbn}` : title ? `intitle:${title}${author ? ` inauthor:${author}` : ''}` : null
  if (!q) return null
  const data = await getJson(fetcher, buildUrl(GB, '/volumes', { q, maxResults: 5, key: apiKey || undefined, langRestrict: lookup.language || undefined }))
  const items = data?.items ?? []
  const item = isbn ? items[0] : items.find((x) => plausibleMatch(lookup, x?.volumeInfo?.title, x?.volumeInfo?.authors ?? []))
  const info = item?.volumeInfo
  if (!info) return null
  const externalIds = []
  const ids = (info.industryIdentifiers ?? []).map((x) => normalizeIsbn(x.identifier)).filter(Boolean)
  const isbnOut = isbn ?? ids.find((x) => x.length === 13) ?? ids[0]
  if (isbnOut) pushExternalId(externalIds, 'isbn', isbnOut)
  if (item.id) pushExternalId(externalIds, 'googlebooks', item.id)
  const thumb = safeHttpUrl(String(info.imageLinks?.thumbnail ?? info.imageLinks?.smallThumbnail ?? '').replace(/^http:/, 'https:'))
  return {
    title: trimText([info.title, info.subtitle].filter(Boolean).join(': '), 512),
    overview: stripHtml(trimText(info.description, 8000)),
    productionYear: yearOf(info.publishedDate),
    people: (info.authors ?? []).slice(0, 20).map((name) => ({ name, roleType: 'writer' })),
    externalIds,
    artwork: thumb ? [{ artworkType: 'poster', remoteUrl: thumb, isPrimary: true }] : [],
  }
}

/** 按 sources 顺序合并：先到者优先，空字段由后者补齐；外部 id 与封面并集。 */
export function mergeBook(results) {
  const list = results.filter(Boolean)
  if (!list.length) return null
  const out = { title: null, overview: null, productionYear: null, people: [], externalIds: [], artwork: [] }
  for (const r of list) {
    out.title ??= r.title ?? null
    out.overview ??= r.overview ?? null
    out.productionYear ??= r.productionYear ?? null
    if (!out.people.length) out.people = r.people ?? []
    for (const id of r.externalIds ?? []) pushExternalId(out.externalIds, id.provider, id.externalId)
    for (const a of r.artwork ?? []) if (!out.artwork.some((x) => x.remoteUrl === a.remoteUrl)) out.artwork.push({ ...a, isPrimary: out.artwork.length === 0 })
  }
  return out.externalIds.length ? out : null
}

export async function queryBooks(config, lookup, fetcher = fetchJson) {
  if (String(lookup?.itemType ?? '').toLowerCase() !== 'book') return null
  const sources = String(config.sources || 'openlibrary,googlebooks').split(',').map((s) => s.trim().toLowerCase())
  const results = []
  for (const source of sources) {
    try {
      if (source === 'openlibrary') results.push(await queryOpenLibrary(lookup, fetcher))
      else if (source === 'googlebooks') results.push(await queryGoogleBooks(lookup, config.google_books_api_key, fetcher))
    } catch (err) {
      console.error(JSON.stringify({ plugin: 'fbz.provider.books', source, error: String(err?.message ?? err) }))
    }
    const merged = mergeBook(results)
    if (merged?.overview && merged.artwork.length && merged.people.length) return merged
  }
  return mergeBook(results)
}

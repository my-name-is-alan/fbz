import assert from 'node:assert/strict'
import { gzipSync } from 'node:zlib'
import { test } from 'node:test'
import { comSearchUrl, decodeId, download, legacyHashSearchUrl, legacySearchUrl, mergeResults, mapLegacyResults, search, toThreeLetter, toTwoLetter } from './provider.mjs'

test('language mapping', () => {
  assert.equal(toTwoLetter('eng'), 'en')
  assert.equal(toThreeLetter('en'), 'eng')
  assert.equal(toThreeLetter('chi'), 'chi')
})

test('legacy url: movie by imdb, episode by series query (alphabetical segments)', () => {
  assert.equal(legacySearchUrl({ language: 'eng', imdbId: 'tt1727587', itemType: 'movie' }), 'https://rest.opensubtitles.org/search/imdbid-1727587/sublanguageid-eng')
  assert.equal(legacySearchUrl({ language: 'eng', title: 'Pilot', seriesName: 'The Show', itemType: 'episode', season: 1, episode: 2 }), 'https://rest.opensubtitles.org/search/episode-2/query-the%20show/season-1/sublanguageid-eng')
  assert.equal(legacySearchUrl({ language: 'eng' }), null)
})

test('com url uses two-letter language and parent imdb for episodes', () => {
  const url = new URL(comSearchUrl({ language: 'eng', imdbId: 'tt0944947', itemType: 'episode', season: 1, episode: 1 }))
  assert.equal(url.searchParams.get('languages'), 'en')
  assert.equal(url.searchParams.get('parent_imdb_id'), '944947')
  assert.equal(url.searchParams.get('season_number'), '1')
})

test('keyless search maps legacy rows and download gunzips', async () => {
  const rows = [{ SubDownloadLink: 'https://dl.opensubtitles.org/x.gz', SubFileName: 'a.srt', SubFormat: 'srt', SubLanguageID: 'eng', ISO639: 'en', SubDownloadsCnt: '5', MatchedBy: 'imdbid' }, { SubFileName: 'nolink' }]
  const calls = []
  const fakeFetch = async (url, opts) => {
    calls.push([url, opts])
    if (url.startsWith('https://rest.')) return { ok: true, json: async () => rows }
    return { ok: true, arrayBuffer: async () => gzipSync(Buffer.from('1\n00:00:00,000 --> 00:00:01,000\nhi\n')) }
  }
  const out = await search({ language: 'eng', title: 'Sintel' }, {}, fakeFetch)
  assert.equal(out.mode, 'legacy')
  assert.equal(out.results.length, 1)
  assert.equal(out.results[0].downloadCount, 5)
  assert.ok(!out.results[0].id.includes('/'))
  assert.equal(decodeId(out.results[0].id).m, 'legacy')
  assert.equal(calls[0][1].headers['User-Agent'], 'TemporaryUserAgent')
  const dl = await download({ subtitleId: out.results[0].id }, {}, fakeFetch)
  assert.equal(Buffer.from(dl.dataBase64, 'base64').toString(), '1\n00:00:00,000 --> 00:00:01,000\nhi\n')
  assert.equal(dl.language, 'eng')
})

test('api key switches to opensubtitles.com', async () => {
  const fakeFetch = async (url, opts) => {
    assert.equal(opts.headers['Api-Key'], 'K')
    return { ok: true, json: async () => ({ data: [{ attributes: { language: 'en', download_count: 3, files: [{ file_id: 42, file_name: 'x.srt' }] } }] }) }
  }
  const out = await search({ language: 'eng', title: 'Sintel' }, { api_key: 'K' }, fakeFetch)
  assert.equal(out.mode, 'com')
  assert.deepEqual(decodeId(out.results[0].id), { m: 'com', id: 42, f: 'srt', l: 'eng', hi: false })
})

test('download rejects foreign hosts', async () => {
  const id = Buffer.from(JSON.stringify({ m: 'legacy', u: 'https://evil.example/x' })).toString('base64url')
  await assert.rejects(download({ subtitleId: id }, {}, async () => ({ ok: true })), /unexpected legacy download host/)
})

test('mapLegacyResults tolerates non-array', () => {
  assert.deepEqual(mapLegacyResults(null), [])
})

test('moviehash: legacy hash url, com param, hash matches first', async () => {
  const req = { language: 'eng', title: 'Sintel', movieHash: '8E245D9679D31E12', movieByteSize: 12909756 }
  assert.equal(legacyHashSearchUrl(req), 'https://rest.opensubtitles.org/search/moviebytesize-12909756/moviehash-8e245d9679d31e12/sublanguageid-eng')
  assert.equal(legacyHashSearchUrl({ ...req, movieHash: 'nothex' }), null)
  assert.equal(new URL(comSearchUrl(req)).searchParams.get('moviehash'), '8e245d9679d31e12')
  assert.ok(comSearchUrl({ language: 'eng', movieHash: req.movieHash, movieByteSize: 1 }))
  assert.deepEqual(mergeResults([{ id: 'a', isHashMatch: false }], [{ id: 'b', isHashMatch: true }, { id: 'a', isHashMatch: false }]).map((r) => r.id), ['b', 'a'])
  const row = (n) => ({ SubDownloadLink: `https://dl.opensubtitles.org/x/${n}.gz`, SubFileName: n, SubLanguageID: 'eng' })
  const calls = []
  const fetchImpl = async (url) => {
    calls.push(url)
    return { ok: true, json: async () => (url.includes('moviehash') ? [row('h')] : [row('t'), row('h')]) }
  }
  const out = await search(req, {}, fetchImpl)
  assert.equal(calls.length, 2)
  assert.deepEqual(out.results.map((r) => [r.name, r.isHashMatch]), [['h', true], ['t', false]])
})

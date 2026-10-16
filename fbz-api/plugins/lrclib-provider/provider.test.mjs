import { test } from 'node:test'
import assert from 'node:assert/strict'
import { download, mapRecord, rank, search } from './provider.mjs'

const res = (status, body) => ({ status, ok: status < 400, json: async () => body })
const fake = (routes, seen = []) => async (url) => {
  seen.push(url)
  for (const [needle, status, body] of routes) if (url.includes(needle)) return res(status, body)
  return res(404, null)
}

test('mapRecord keeps synced/plain and drops instrumental or empty', () => {
  assert.equal(mapRecord({ id: 1, instrumental: true, plainLyrics: 'x' }), null)
  assert.equal(mapRecord({ id: 1, plainLyrics: '  ' }), null)
  assert.deepEqual(mapRecord({ id: 7, trackName: 'T', syncedLyrics: '[00:01.00]a' }).id, '7')
})

test('search uses exact get then search, dedups and ranks synced + duration first', async () => {
  const seen = []
  const out = await search({ title: 'Song', artist: 'Band', album: 'LP', durationSeconds: 200 }, {}, fake([
    ['/api/get?', 200, { id: 1, trackName: 'Song', artistName: 'Band', duration: 200, plainLyrics: 'p' }],
    ['/api/search?', 200, [{ id: 1, plainLyrics: 'p' }, { id: 2, trackName: 'Song', artistName: 'Band', duration: 201, syncedLyrics: '[00:01.00]s' }, { id: 3, instrumental: true }]],
  ], seen))
  assert.match(seen[0], /api\/get\?track_name=Song&artist_name=Band&album_name=LP&duration=200/)
  assert.match(seen[1], /api\/search\?track_name=Song&artist_name=Band/)
  assert.deepEqual(out.results.map((r) => r.id), ['2', '1'])
})

test('searchTerm uses q= and honours base_url/max_results', async () => {
  const seen = []
  const out = await search({ title: 'ignored', searchTerm: 'free text' }, { base_url: 'https://mirror.example/', max_results: 1 }, fake([
    ['/api/search?', 200, [{ id: 5, plainLyrics: 'a' }, { id: 6, plainLyrics: 'b' }]],
  ], seen))
  assert.equal(seen.length, 1)
  assert.equal(seen[0], 'https://mirror.example/api/search?q=free+text')
  assert.equal(out.results.length, 1)
})

test('download by numeric id, rejects bad ids', async () => {
  const ok = await download({ lyricId: '42' }, {}, fake([['/api/get/42', 200, { id: 42, syncedLyrics: '[00:02.00]x' }]]))
  assert.equal(ok.syncedLyrics, '[00:02.00]x')
  assert.deepEqual(await download({ lyricId: '../x' }, {}, fake([])), { error: 'invalid lyricId' })
  assert.deepEqual(await download({ lyricId: '9' }, {}, fake([])), { error: 'not found' })
})

test('rank is stable without hints', () => {
  assert.deepEqual(rank([{ id: 'a' }, { id: 'b', syncedLyrics: 's' }], {}).map((r) => r.id), ['b', 'a'])
})

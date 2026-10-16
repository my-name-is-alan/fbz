import { test } from 'node:test'
import assert from 'node:assert/strict'
import { plausibleMatch, mergeBook, normalizeIsbn, queryBooks, queryGoogleBooks, queryOpenLibrary } from './provider.mjs'

const fake = (routes) => async (url) => {
  for (const [needle, body] of routes) if (url.includes(needle)) return typeof body === 'function' ? body(url) : body
  const e = new Error('HTTP 404'); e.status = 404; throw e
}

test('isbn normalization validates checksums', () => {
  assert.equal(normalizeIsbn('978-0-306-40615-7'), '9780306406157')
  assert.equal(normalizeIsbn('urn:isbn:0306406152'), '0306406152')
  assert.equal(normalizeIsbn('9780306406158'), null)
  assert.equal(normalizeIsbn('abc'), null)
})

test('non-book lookups are ignored', async () => {
  assert.equal(await queryBooks({}, { itemType: 'movie', title: 'X' }, fake([])), null)
})

test('open library by isbn resolves work description, authors and cover', async () => {
  const r = await queryOpenLibrary({ itemType: 'book', isbn: '9780306406157' }, fake([
    ['/isbn/9780306406157.json', { key: '/books/OL1M', title: 'Hobbit', publish_date: 'Sep 21, 1937', covers: [42], works: [{ key: '/works/OL2W' }], authors: [{ key: '/authors/OL3A' }] }],
    ['/works/OL2W.json', { description: { value: 'There and back again.' } }],
    ['/authors/OL3A.json', { name: 'J.R.R. Tolkien' }],
  ]))
  assert.equal(r.title, 'Hobbit')
  assert.equal(r.productionYear, 1937)
  assert.equal(r.overview, 'There and back again.')
  assert.deepEqual(r.people, [{ name: 'J.R.R. Tolkien', roleType: 'writer' }])
  assert.deepEqual(r.externalIds, [{ provider: 'isbn', externalId: '9780306406157' }, { provider: 'openlibrary', externalId: 'OL1M' }])
  assert.equal(r.artwork[0].remoteUrl, 'https://covers.openlibrary.org/b/id/42-L.jpg')
})

test('open library title search prefers matching year', async () => {
  let seen = ''
  const r = await queryOpenLibrary({ itemType: 'book', title: 'Dune', authors: ['Frank Herbert'], productionYear: 1965 }, fake([
    ['/search.json', (u) => { seen = u; return { docs: [{ key: '/works/A', title: 'Dune (comic)', first_publish_year: 2020 }, { key: '/works/B', title: 'Dune', first_publish_year: 1965, author_name: ['Frank Herbert'], cover_i: 7, isbn: ['9780441013593'] }] } }],
    ['/works/B.json', { description: 'Spice.' }],
  ]))
  assert.match(seen, /title=Dune/)
  assert.match(seen, /author=Frank\+Herbert/)
  assert.equal(r.title, 'Dune')
  assert.equal(r.overview, 'Spice.')
  assert.deepEqual(r.externalIds.map((x) => x.provider), ['isbn', 'openlibrary'])
})

test('google books works keyless and passes key when configured', async () => {
  let seen = ''
  const body = { items: [{ id: 'gb1', volumeInfo: { title: 'T', authors: ['A'], publishedDate: '2001-02', description: '<b>Bold</b> text', imageLinks: { thumbnail: 'http://books.google.com/x.jpg' }, industryIdentifiers: [{ identifier: '0306406152' }] } }] }
  const r = await queryGoogleBooks({ title: 'T' }, '', fake([['/volumes', (u) => { seen = u; return body }]]))
  assert.doesNotMatch(seen, /key=/)
  assert.equal(r.overview, 'Bold text')
  assert.equal(r.productionYear, 2001)
  assert.equal(r.artwork[0].remoteUrl, 'https://books.google.com/x.jpg')
  assert.deepEqual(r.externalIds, [{ provider: 'isbn', externalId: '0306406152' }, { provider: 'googlebooks', externalId: 'gb1' }])
  await queryGoogleBooks({ isbn: '9780306406157' }, 'K', fake([['/volumes', (u) => { seen = u; return body }]]))
  assert.match(seen, /q=isbn%3A9780306406157/)
  assert.match(seen, /key=K/)
})

test('merge fills gaps from the second source and requires an external id', async () => {
  const m = mergeBook([{ title: 'A', overview: null, people: [], externalIds: [{ provider: 'openlibrary', externalId: 'OL1M' }], artwork: [] },
    { title: 'B', overview: 'O', people: [{ name: 'W', roleType: 'writer' }], externalIds: [{ provider: 'googlebooks', externalId: 'g' }], artwork: [{ artworkType: 'poster', remoteUrl: 'https://x/y.jpg' }] }])
  assert.equal(m.title, 'A')
  assert.equal(m.overview, 'O')
  assert.equal(m.externalIds.length, 2)
  assert.equal(m.artwork[0].isPrimary, true)
  assert.equal(mergeBook([{ title: 'x', externalIds: [] }]), null)
})

test('queryBooks falls back to google when open library errors', async () => {
  const r = await queryBooks({}, { itemType: 'book', title: 'T' }, fake([
    ['openlibrary.org', () => { throw new Error('HTTP 500') }],
    ['/volumes', { items: [{ id: 'gb', volumeInfo: { title: 'T', description: 'D', authors: ['A'] } }] }],
  ]))
  assert.equal(r.overview, 'D')
  assert.deepEqual(r.externalIds, [{ provider: 'googlebooks', externalId: 'gb' }])
})

test('title search rejects implausible matches', async () => {
  assert.equal(plausibleMatch({ title: 'The Hobbit' }, 'Hobbit, or There and Back Again'), true)
  assert.equal(plausibleMatch({ title: 'Smoke Paper' }, 'Paper Smoke Signals'), false)
  assert.equal(plausibleMatch({ title: 'Dune', authors: ['Frank Herbert'] }, 'Dune', ['Brian Herbert']), true)
  assert.equal(plausibleMatch({ title: 'Dune', authors: ['Frank Herbert'] }, 'Dune', ['Someone Else']), false)
  const r = await queryOpenLibrary({ itemType: 'book', title: 'Smoke Paper' }, fake([
    ['/search.json', { docs: [{ key: '/works/X', title: 'Data Science on Smoke', author_name: ['Z'] }] }],
  ]))
  assert.equal(r, null)
  const g = await queryGoogleBooks({ title: 'Smoke Paper' }, '', fake([['/volumes', { items: [{ id: 'g', volumeInfo: { title: 'Other' } }] }]]))
  assert.equal(g, null)
})

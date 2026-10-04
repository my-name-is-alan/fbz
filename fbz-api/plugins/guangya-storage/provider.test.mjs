import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createProvider, expiry, checkedUrl, publicIp, normalizeEntry, Scheduler } from './provider.mjs';

test('expired signed URLs are never extended by a minimum cache TTL', () => {
  const now = 1800000000000;
  assert.ok(expiry(`https://cdn.example/video?Expires=${now / 1000 - 1}`, now) < now);
  assert.ok(expiry(`https://cdn.example/video?Expires=${now / 1000 + 5}`, now) < now + 5000);
});
test('blocks private and credential-bearing downloads', () => {
  for (const url of ['http://cdn.example/a', 'https://127.0.0.1/a', 'https://user:pass@cdn.example/a', 'https://[::1]/a']) assert.throws(() => checkedUrl(url));
  assert.equal(publicIp('192.168.1.1'), false); assert.equal(publicIp('100.64.1.1'), false);
  assert.equal(publicIp('8.8.8.8'), true);
});
test('uses opaque cloud names and rejects malformed records', () => {
  assert.throws(() => normalizeEntry({fileId:'1',fileName:'bad\u0000name'}));
  assert.equal(normalizeEntry({id:'3',name:'Title / 4K',type:2}).name,'Title / 4K');
  assert.equal(normalizeEntry({fileId:'2',fileName:'Movie',resType:2}).directory,true);
});
test('account scheduler spaces concurrent requests', async () => {
  const scheduler = new Scheduler(); const times=[];
  await Promise.all([1,2,3].map(()=>scheduler.run('one',5,async()=>{times.push(Date.now());})));
  assert.ok(times[1]-times[0]>=180);assert.ok(times[2]-times[1]>=180);
});
test('refreshes exactly once and returns rotated credentials to host', async () => {
  const seen=[];
  const provider=createProvider({fetchImpl:async(url,options)=>{
    seen.push(url);
    if(url.endsWith('/v1/auth/token')) return Response.json({access_token:'new',refresh_token:'rotated'});
    if(options.headers.authorization==='Bearer old') return Response.json({code:110});
    return Response.json({code:0,data:{list:[{fileId:'1',fileName:'movie.mp4',resType:1,fileSize:123}],total:1}});
  }});
  const result=await provider.dispatch({op:'list',accountId:'one',deviceId:'test',qps:5,credentials:{access_token:'old',refresh_token:'refresh'}});
  assert.equal(result.credentials.refresh_token,'rotated');assert.equal(result.data.entries.length,1);assert.equal(result.data.nextPage,null);
  assert.equal(seen.filter(url=>url.endsWith('/v1/auth/token')).length,1);
});
test('partial page is not a complete directory and failures are not empty directories', async () => {
  const provider=createProvider({fetchImpl:async()=>Response.json({code:0,data:{list:Array.from({length:100},(_,i)=>({fileId:String(i),fileName:`file-${i}`,resType:1})),total:150}})});
  const r=await provider.dispatch({op:'list',accountId:'one',deviceId:'test',qps:5,credentials:{access_token:'t'}});assert.equal(r.data.nextPage,1);
  const broken=createProvider({fetchImpl:async()=>Response.json({code:0,data:{}})});
  assert.equal((await broken.dispatch({op:'list',accountId:'two',deviceId:'d',credentials:{access_token:'t'}})).error.code,'invalid_response');
});
test('identity failure does not persist unbound credentials', async () => {
  const provider=createProvider({fetchImpl:async url=>url.endsWith('/v1/auth/token')?Response.json({access_token:'new'}):Response.json({}, {status:500})});
  const r=await provider.dispatch({op:'auth.poll',accountId:'one',deviceId:'d',qps:5,deviceCode:'code'});
  assert.equal(r.error.code,'identity_failed');assert.equal(r.credentials,undefined);
});
test('download cache is account-scoped', async () => {
  let calls=0;
  const provider=createProvider({fetchImpl:async()=>{calls++;return Response.json({code:0,data:{signedURL:'https://cdn.example/video'}});}});
  const args={op:'resolve',deviceId:'d',qps:5,fileId:'same',credentials:{access_token:'t'}};
  await provider.dispatch({...args,accountId:'a'});await provider.dispatch({...args,accountId:'a'});await provider.dispatch({...args,accountId:'b'});
  assert.equal(calls,2);
});

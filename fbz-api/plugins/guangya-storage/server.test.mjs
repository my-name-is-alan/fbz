import test from 'node:test';
import assert from 'node:assert/strict';
import {createHash,createHmac} from 'node:crypto';
process.env.FBZ_GUANGYA_NO_LISTEN='1';
process.env.PLUGIN_SECRET_KEY='fixture-only-plugin-key-0123456789abcdef';
const {verifyPluginRequest,server}=await import('./server.mjs');
test('signed storage invocation binds package id and request body',()=>{
 const body=Buffer.from(JSON.stringify({invocation:'sync',handler:'storage.rpc',request:{op:'list'}}));
 const timestamp=String(Math.floor(Date.now()/1000));const id='org.fbz.guangya';const nonce='fixture-1';const digest=createHash('sha256').update(body).digest('hex');
 const signature='sha256='+createHmac('sha256',process.env.PLUGIN_SECRET_KEY).update(`v1\n${timestamp}\n${id}\n${nonce}\n${digest}`).digest('hex');
 const headers={'x-fbz-plugin-id':id,'x-fbz-plugin-signature-version':'v1','x-fbz-plugin-signature-timestamp':timestamp,'x-fbz-plugin-idempotency-key':nonce,'x-fbz-plugin-body-sha256':digest,'x-fbz-plugin-signature':signature};
 assert.equal(verifyPluginRequest(headers,body),true);
 assert.equal(verifyPluginRequest(headers,Buffer.from('{}')),false);
 assert.equal(verifyPluginRequest({...headers,'x-fbz-plugin-id':'org.other'},body),false);
 assert.equal(verifyPluginRequest({...headers,'x-fbz-plugin-signature-timestamp':'1'},body),false);
});

test('signed packaged HTTP endpoint answers an admin action',async()=>{
 await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));
 try{
  const body=JSON.stringify({invocation:'sync',pluginId:'org.fbz.guangya',handler:'admin.status',hookEvent:'admin.ui',request:{}});
  const ts=String(Math.floor(Date.now()/1000));const digest=createHash('sha256').update(body).digest('hex');const key='fixture-http';
  const signature='sha256='+createHmac('sha256',process.env.PLUGIN_SECRET_KEY).update(`v1\n${ts}\norg.fbz.guangya\n${key}\n${digest}`).digest('hex');
  const headers={'x-fbz-plugin-id':'org.fbz.guangya','x-fbz-plugin-signature-version':'v1','x-fbz-plugin-signature-timestamp':ts,'x-fbz-plugin-idempotency-key':key,'x-fbz-plugin-body-sha256':digest,'x-fbz-plugin-signature':signature};
  const url=`http://127.0.0.1:${server.address().port}/fbz-plugin`;
  const good=await fetch(url,{method:'POST',headers,body});assert.equal(good.status,200);assert.deepEqual(await good.json(),{ready:true});
  const denied=await fetch(url,{method:'POST',headers:{...headers,'x-fbz-plugin-signature':'sha256=invalid'},body});assert.equal(denied.status,401);
 }finally{await new Promise(resolve=>server.close(resolve));}
});

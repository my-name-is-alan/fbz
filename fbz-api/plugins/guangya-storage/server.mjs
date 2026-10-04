import http from 'node:http';
import { timingSafeEqual,createHash,createHmac } from 'node:crypto';
import { createProvider } from './provider.mjs';
const key = process.env.FBZ_STORAGE_PLUGIN_KEY ?? '';
const pluginSecret=process.env.PLUGIN_SECRET_KEY ?? '';
if(key.length<32 && pluginSecret.length<32)throw new Error('A storage or signed plugin key of at least 32 characters is required');
const provider = createProvider();
function equal(left,right){const a=Buffer.from(String(left??''));const b=Buffer.from(String(right??''));return a.length===b.length && timingSafeEqual(a,b);}
export function verifyPluginRequest(headers,body,secret=pluginSecret){
 if(secret.length<32 || headers['x-fbz-plugin-id']!=='org.fbz.guangya' || headers['x-fbz-plugin-signature-version']!=='v1')return false;
 const ts=Number(headers['x-fbz-plugin-signature-timestamp']);if(!Number.isInteger(ts)||Math.abs(Date.now()/1000-ts)>300)return false;
 const digest=createHash('sha256').update(body).digest('hex');if(!equal(headers['x-fbz-plugin-body-sha256'],digest))return false;
 const key=headers['x-fbz-plugin-idempotency-key'];if(typeof key!=='string'||key.length>256)return false;
 const canonical=`v1\n${ts}\norg.fbz.guangya\n${key}\n${digest}`;
 const expected='sha256='+createHmac('sha256',secret).update(canonical).digest('hex');
 return equal(headers['x-fbz-plugin-signature'],expected);
}
export const server = http.createServer(async (req, res) => {
 res.setHeader('Content-Type','application/json');res.setHeader('Cache-Control','no-store');
 if(req.method!=='POST' || !['/rpc','/fbz-plugin'].includes(req.url)){res.writeHead(404);res.end('{}');return;}
 try{
  const chunks=[];let size=0;for await(const chunk of req){size+=chunk.length;if(size>65536)throw new Error('large request');chunks.push(chunk);}
  const body=Buffer.concat(chunks);
  if(req.url==='/fbz-plugin' && !verifyPluginRequest(req.headers,body)){res.writeHead(401);res.end('{}');return;}
  if(req.url==='/rpc' && !equal(req.headers.authorization,`Bearer ${key}`)){res.writeHead(401);res.end('{}');return;}
  const envelope=JSON.parse(body);let input=envelope;
  if(req.url==='/fbz-plugin'){
   if(envelope.invocation!=='sync' || envelope.pluginId!=='org.fbz.guangya')throw new Error('invalid invocation');
   if(envelope.handler==='admin.status' && envelope.hookEvent==='admin.ui'){res.end(JSON.stringify({ready:true}));return;}
   if(envelope.handler!=='storage.rpc' || envelope.hookEvent!=='storage.provider.request')throw new Error('unsupported handler');
   input=envelope.request;
  }
  if(!input.accountId || !input.deviceId)throw new Error('missing account');
  res.end(JSON.stringify(await provider.dispatch(input)));
 }catch{res.writeHead(400);res.end(JSON.stringify({error:{code:'invalid_request',message:'插件请求无效'}}));}
});
server.requestTimeout = 60000; server.headersTimeout = 10000;
if (process.env.FBZ_GUANGYA_NO_LISTEN !== '1') server.listen(Number(process.env.PORT || 8098), process.env.HOST || '127.0.0.1', () => console.log('Guangya storage plugin ready'));

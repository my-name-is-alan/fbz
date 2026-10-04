// Explicitly launched offline fixture. Never loaded by server.mjs.
import http from 'node:http';
import { createProvider } from './provider.mjs';
const key=process.env.FBZ_STORAGE_PLUGIN_KEY;
if(!key || key.length<32) throw new Error('test RPC key required');
const entries={
  '':[{fileId:'movies',fileName:'已刮削电影',resType:2}],
  movies:[{fileId:'film-dir',fileName:'示例电影 (2026)',resType:2}],
  'film-dir':[
    {fileId:'video-1',fileName:'Example.mp4',resType:1,fileSize:123456},
    {fileId:'nfo-1',fileName:'movie.nfo',resType:1,fileSize:300},
    {fileId:'poster-1',fileName:'poster.png',resType:1,fileSize:68},
    {fileId:'sub-1',fileName:'Example.zh.srt',resType:1,fileSize:50},
  ],
};
let failNextList=false;
const provider=createProvider({
  fetchImpl:async(url,options)=>{
    const body=options.body?JSON.parse(options.body):{};
    if(url.endsWith('/v1/auth/device/code'))return Response.json({device_code:'fixture-device-code',verification_uri_complete:'https://fixture.invalid/authorize',user_code:'TEST',expires_in:300,interval:2});
    if(url.endsWith('/v1/auth/token'))return Response.json({access_token:'fixture-access-token',refresh_token:'fixture-refresh-token',expires_in:1200});
    if(url.endsWith('/v1/user/me'))return Response.json({id:'fixture-cloud-user',nickname:'离线测试账号'});
    if(url.endsWith('/userres/v1/file/get_file_list')){
      if(failNextList){failNextList=false;return Response.json({code:500},{status:503});}
      return Response.json({code:0,data:{list:entries[body.parentId]??[],total:(entries[body.parentId]??[]).length}});
    }
    if(url.endsWith('/userres/v1/get_res_download_url'))return Response.json({code:0,data:{signedURL:`https://fixture.invalid/${body.fileId}?Expires=${Math.floor(Date.now()/1000)+3600}`}});
    return Response.json({code:999});
  },
  smallReader:async(url)=>{
    const id=new URL(url).pathname.slice(1);
    if(id==='nfo-1')return Buffer.from('<movie><title>光鸭离线验证影片</title><plot>通过已有 NFO 导入，不调用刮削接口。</plot><year>2026</year><runtime>2</runtime><genre>测试</genre><uniqueid type="tmdb">987654321</uniqueid></movie>');
    if(id==='poster-1')return Buffer.from('iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+jWZkAAAAASUVORK5CYII=','base64');
    if(id==='sub-1')return Buffer.from('1\n00:00:01,000 --> 00:00:03,000\n光鸭外挂字幕\n');
    throw new Error('missing fixture');
  },
});
http.createServer(async(req,res)=>{
  if(req.headers.authorization!==`Bearer ${key}`){res.writeHead(401);res.end();return;}
  try{let body='';for await(const chunk of req){body+=chunk;if(body.length>65536)throw new Error('too large');}res.setHeader('Content-Type','application/json');const input=JSON.parse(body);if(input.op==='_test.failNextList'){failNextList=true;res.end('{}');return;}res.end(JSON.stringify(await provider.dispatch(input)));}
  catch{res.writeHead(400);res.end('{}');}
}).listen(8099,'127.0.0.1',()=>console.log('Offline storage fixture listening on 8099'));

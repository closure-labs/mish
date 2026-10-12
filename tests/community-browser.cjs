// Native CI browser qualification. Local smoke: MISH_BROWSER_CLIENTS=5 MISH_BROWSER_SECONDS=10.
// MISH_BROWSER_TOOLS must point to the pinned Chromium + playwright-driver Nix output.
const fs=require('fs'),os=require('os'),path=require('path'),cp=require('child_process'),https=require('https'),http=require('http'),assert=require('assert/strict');
const {chromium}=require(process.env.MISH_BROWSER_TOOLS);
const root=fs.mkdtempSync(path.join(os.tmpdir(),'mish-browser-'));
const exe=path.resolve('apps/mish/target/debug/mish');
const sleep=ms=>new Promise(r=>setTimeout(r,ms));
async function freePort(){const s=http.createServer();await new Promise(r=>s.listen(0,'127.0.0.1',r));const p=s.address().port;await new Promise(r=>s.close(r));return p;}
(async()=>{let runtime,proxy,browser;try{
 const port=await freePort();
 const enrollment=JSON.parse(cp.execFileSync(exe,['--state-dir',root,'commission','--name','Browser operator'],{encoding:'utf8'}));
 runtime=cp.spawn(exe,['--state-dir',root,'serve','--host','127.0.0.1','--port',String(port)],{stdio:'ignore'});
 const backend='http://127.0.0.1:'+port;
 for(let i=0;i<100;i++){try{if((await fetch(backend+'/health')).ok)break;}catch{}await sleep(100);}
 async function manage(body){const r=await fetch(backend+'/api/community/manage',{method:'POST',headers:{Authorization:'Bearer '+enrollment.token,'Content-Type':'application/json'},body:JSON.stringify(body)});assert(r.ok,await r.clone().text());return r.json();}
 await manage({action:'configure',enabled:true,enroll_per_hour:120,posts_per_minute:30,queue_limit:10});
 const room=await manage({action:'create',name:'Browser lobby',public:true,channel:null});
 cp.execFileSync('openssl',['req','-x509','-newkey','rsa:2048','-nodes','-keyout',root+'/key.pem','-out',root+'/cert.pem','-days','1','-subj','/CN=localhost'],{stdio:'ignore'});
 proxy=https.createServer({key:fs.readFileSync(root+'/key.pem'),cert:fs.readFileSync(root+'/cert.pem')},(req,res)=>{const upstream=http.request({hostname:'127.0.0.1',port,path:req.url,method:req.method,headers:req.headers},r=>{res.writeHead(r.statusCode,r.headers);r.pipe(res);});upstream.on('error',()=>{res.writeHead(502);res.end();});req.pipe(upstream);});
 await new Promise(r=>proxy.listen(0,'127.0.0.1',r));const base='https://127.0.0.1:'+proxy.address().port;
 browser=await chromium.launch({executablePath:process.env.MISH_BROWSER_TOOLS+'/bin/chromium',headless:true,args:['--no-sandbox']});
 const pages=[];const errors=[];
 const clients=Number(process.env.MISH_BROWSER_CLIENTS||50);
 for(let i=0;i<clients;i++){
  const context=await browser.newContext({ignoreHTTPSErrors:true});
  await context.route('**/*',route=>route.request().url().startsWith(base+'/')?route.continue():route.abort());
  const page=await context.newPage();page.on('pageerror',e=>errors.push(e.message));
  await page.goto(base+'/public');await page.locator('#nickname').fill('Attendee '+i);await page.getByRole('button',{name:'Join chat',exact:true}).click();
  await page.getByRole('button',{name:'Browser lobby · Wi-Fi only',exact:true}).click();await page.waitForFunction(()=>!document.getElementById('chat').hidden);pages.push(page);
 }
 const duration=Number(process.env.MISH_BROWSER_SECONDS||3600),sent=[],started=Date.now();let maxLag=0;
 for(let i=0;i<duration+5;i++){
  await sleep(Math.max(0,started+i*1000-Date.now()));
  if(i<duration){const page=pages[i%clients];await page.locator('#draft').fill('message '+i);const accepted=page.waitForResponse(r=>r.url().endsWith('/messages')&&r.request().method()==='POST');const began=Date.now();await page.getByRole('button',{name:'Send',exact:true}).click();const response=await accepted;assert(response.ok(),await response.text());assert((await response.json()).accepted);sent.push(began);}
  const observed=await Promise.all(pages.map(p=>p.locator('#messages li').count()));
  for(const count of observed){if(count<sent.length){const lag=Date.now()-sent[count];maxLag=Math.max(maxLag,lag);assert(lag<5000,'Browser exceeded five-second visibility: '+lag);}}
  if(i%60===0)console.log(JSON.stringify({seconds:i,accepted:sent.length,minVisible:Math.min(...observed),maxObservedLagMs:maxLag}));
  if(i===duration+4){assert(observed.every(n=>n===duration),'accepted message missing');for(const page of pages){const texts=await page.locator('#messages li').allTextContents();for(let n=0;n<duration;n++)assert(texts[n].includes(': message '+n+' [local]'),'missing or reordered message '+n);}}
 }
 assert(Date.now()-started < (duration+10)*1000,'did not sustain one message per second');
 assert.deepEqual(errors,[]);console.log('PASS: '+clients+' Chromium sessions, '+duration+' messages, one aggregate message per second, all visible within five seconds. Test TLS trust bypass; phone acceptance separate.');
 }finally{if(browser)await browser.close();if(proxy)await new Promise(r=>proxy.close(r));if(runtime){runtime.kill('SIGTERM');await sleep(500);}fs.rmSync(root,{recursive:true,force:true});}})().catch(e=>{console.error(e);process.exitCode=1;});

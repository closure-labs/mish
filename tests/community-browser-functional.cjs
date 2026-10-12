// Functional browser regression checks; uses the same pinned tools as the load test.
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
 const context=await browser.newContext({ignoreHTTPSErrors:true});const external=[];
 await context.route('**/*',route=>{if(route.request().url().startsWith(base+'/'))return route.continue();external.push(route.request().url());return route.abort();});
 const page=await context.newPage();const errors=[];page.on('pageerror',e=>errors.push(e.message));
 await page.goto(base+'/public');await page.locator('#nickname').fill('<script>x</script>');await page.getByRole('button',{name:'Join chat',exact:true}).click();
 await page.getByRole('button',{name:'Browser lobby · Wi-Fi only',exact:true}).click();
 const cookies=await context.cookies();const guest=cookies.find(c=>c.name==='__Host-mish-guest');assert(guest&&guest.secure&&guest.httpOnly&&guest.sameSite==='Strict');
 assert(!(await page.evaluate(()=>document.cookie)).includes('__Host-mish-guest'));
 async function send(text,count){await page.locator('#draft').fill(text);await page.getByRole('button',{name:'Send',exact:true}).click();await page.waitForFunction(n=>document.querySelectorAll('#messages li').length===n,count);}
 await send('hello',1);await send('<img src=x onerror="window.pwned=1">',2);
 assert.equal(await page.locator('#messages img').count(),0);assert.equal(await page.evaluate(()=>window.pwned),undefined);
 assert((await page.locator('#identity').textContent()).includes('<script>x</script>#'));
 await context.setOffline(true);await page.locator('#draft').fill('offline draft');await page.getByRole('button',{name:'Send',exact:true}).click();
 await page.waitForFunction(()=>document.getElementById('notice').textContent.includes('Draft saved'));
 await context.setOffline(false);await sleep(3000);
 assert.equal(await page.locator('#messages li').count(),2);assert.equal(await page.locator('#draft').inputValue(),'offline draft');
 await page.getByRole('button',{name:'Send',exact:true}).click();await page.waitForFunction(()=>document.querySelectorAll('#messages li').length===3);
 let releaseResponse,postReached=false;const responseGate=new Promise(resolve=>releaseResponse=resolve);
 await page.route('**/messages',async route=>{if(route.request().method()!=='POST')return route.continue();const response=await route.fetch();postReached=true;await responseGate;await route.fulfill({response});});
 await page.locator('#draft').fill('delayed acknowledgement');await page.getByRole('button',{name:'Send',exact:true}).click();
 for(let i=0;i<100&&!postReached;i++)await sleep(50);assert(postReached,'server did not accept delayed message');
 await page.locator('#draft').fill('saved across reload');releaseResponse();await page.waitForFunction(()=>!busy);
 assert.equal(await page.locator('#draft').inputValue(),'saved across reload');
 await page.reload();await page.getByRole('button',{name:'Browser lobby · Wi-Fi only',exact:true}).click();await page.waitForFunction(()=>document.getElementById('draft').value==='saved across reload');
 await manage({action:'hide',entry:1});await page.waitForFunction(()=>document.querySelector('#messages li')?.textContent==='Message hidden by operator');
 const guests=(await manage({action:'list'})).guests;await manage({action:'revoke',guest:guests[0].id});
 assert.equal(await page.evaluate(async()=> (await fetch('/api/community/session')).status),400);
 assert.deepEqual(errors,[]);assert.deepEqual(external,[]);
 console.log('Chromium attendee flow passed: local assets, secure HttpOnly session, safe rendering, polling, offline explicit retry, draft reload and edits during delayed responses, moderation and revocation. Test-only TLS trust bypass; phone/certificate acceptance remains separate.');
 }finally{if(browser)await browser.close();if(proxy)await new Promise(r=>proxy.close(r));if(runtime){runtime.kill('SIGTERM');await sleep(500);}fs.rmSync(root,{recursive:true,force:true});}})().catch(e=>{console.error(e);process.exitCode=1;});

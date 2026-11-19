import assert from 'node:assert/strict';
import {pathToFileURL} from 'node:url';
import {resolve} from 'node:path';
import {createPreviewServer} from './serve-runtime.mjs';

const [rom, executablePath, modulePath]=process.argv.slice(2);
const { chromium }=await import(pathToFileURL(resolve(modulePath)).href);
const server=createPreviewServer();
await new Promise(done=>server.listen(0,'127.0.0.1',done));
let browser;
try {
  browser=await chromium.launch({executablePath, headless:true});
  const page=await browser.newPage();
  const errors=[];
  page.on('pageerror',e=>errors.push(e.message));
  await page.clock.install();
  await page.goto('http://127.0.0.1:'+server.address().port);
  await page.waitForFunction(()=>!document.querySelector('#rom-file').disabled);
  await page.locator('#rom-file').setInputFiles(resolve(rom));
  await page.waitForFunction(()=>!document.querySelector('#pause').disabled);
  if(await page.locator('#pause').textContent()==='Resume') await page.locator('#pause').click();
  await page.locator('#stage').focus();

  const advance=ms=>page.clock.runFor(ms);
  const key=async k=>{await page.keyboard.down(k);await advance(48);await page.keyboard.up(k);await advance(48);};
  const action=()=>page.locator('#action-value').textContent();
  const view=()=>page.locator('#garden-view').textContent();
  await advance(5000);
  await key('Enter');await advance(400);await key('ArrowUp');await key('z');await advance(1500);
  assert.equal(await action(),'Garden');
  await key('z');assert.equal(await action(),'Memory');
  const rows=(await view()).split('\n').slice(0,5);
  const cards=new Map();
  rows.forEach((row,y)=>{for(let x=0;x<6;x++){const face=row[3*x+1];if(/[A-G]/.test(face))cards.set(face,[...(cards.get(face)||[]),[x,y]]);}});
  assert.equal(cards.size,7);

  await key('z');
  for(const pair of cards.values()){
    assert.equal(pair.length,2);
    for(const [tx,ty] of pair){
      for(let attempts=0;;attempts++) {
        const lines=(await view()).split('\n').slice(0,5);
        const y=lines.findIndex(line=>line.includes('['));
        assert.ok(y>=0);
        const x=lines[y].indexOf('[')/3;
        if(x===tx && y===ty) break;
        assert.ok(attempts<30, 'cursor failed to reach selected card');
        await key(x!==tx?'ArrowRight':'ArrowDown');
      }
      await key('z');
      for(let n=0;n<30;n++) {
        if(/Turn (First|Second)|Pairs 7\/7/.test(await view())) break;
        await advance(250);
      }
    }
  }
  assert.match(await view(),/Pairs 7\/7/);
  await key('z');assert.equal(await action(),'Garden');
  await key('ArrowRight');await key('z');assert.equal(await action(),'Rock-Paper-Scissors');
  await key('ArrowRight');await key('z');
  for(let i=0;i<12&&!/Round stopped/.test(await page.locator('#status').textContent());i++) await advance(10000);
  assert.match(await page.locator('#status').textContent(),/Round stopped/);
  await key('z');assert.equal(await action(),'Garden');await key('x');
  assert.equal(await page.locator('#stage-heading').textContent(),'Main menu');
  assert.deepEqual(errors,[]);
  console.log('Browser passed: title → Garden → seven Memory pairs → Garden → hands round → Garden → main menu.');
} finally {await browser?.close();await new Promise(done=>server.close(done));}

import { chromium } from 'playwright';
import assert from 'node:assert/strict';
import {mkdir} from 'node:fs/promises';

const base=process.env.PINBOARD_TEST_URL || 'http://127.0.0.1:4127';
const browser=await chromium.launch({headless:true,...(process.env.CHROMIUM_PATH?{executablePath:process.env.CHROMIUM_PATH}:{}),args:['--no-sandbox']});
const context=await browser.newContext({viewport:{width:1536,height:1000},permissions:['clipboard-read','clipboard-write']});
const page=await context.newPage(),errors=[];
page.on('pageerror',e=>errors.push(e.message));
const waitSaved=()=>page.waitForFunction(()=>document.querySelector('[role=status]')?.textContent==='All changes saved');
const state=()=>page.evaluate(async()=>{const id=new URL(location.href).searchParams.get('canvas');return (await fetch(`/api/canvases/${id}`)).json();});
await mkdir('artifacts',{recursive:true});
try {
  const library=await (await context.request.get(base+'/api/canvases')).json();const welcome=library.find(c=>c.title==='A little space to think big');await page.goto(base+(welcome?'/?canvas='+welcome.id:''));await page.waitForFunction(()=>document.querySelector('#board-title')?.value.length>0);
  await page.screenshot({path:'artifacts/canvas-desktop.png',fullPage:true});
  await page.getByRole('button',{name:'New canvas',exact:false}).click();
  await page.getByRole('textbox',{name:'Canvas title',exact:true}).waitFor();
  await page.waitForFunction(()=>document.querySelector('#board-title')?.value==='Untitled canvas');
  await page.getByRole('textbox',{name:'Canvas title',exact:true}).fill('Browser acceptance board');await page.getByRole('textbox',{name:'Canvas title',exact:true}).press('Tab');await waitSaved();
  await page.getByRole('textbox',{name:'Capture a thought',exact:true}).fill('A thought from the browser');await page.getByRole('textbox',{name:'Capture a thought',exact:true}).press('Enter');await waitSaved();
  await page.locator('.canvas-node').first().waitFor();assert.equal((await state()).nodes.length,1);
  await page.getByRole('textbox',{name:'Title',exact:true}).fill('First idea');await page.getByRole('textbox',{name:'Title',exact:true}).press('Tab');await waitSaved();
  assert.equal((await state()).nodes[0].title,'First idea');
  // Adding a process node and linking it exercises the same server mutation path.
  await page.getByRole('button',{name:'Process step',exact:true}).click();const stage=await page.locator('#stage').boundingBox();await page.mouse.click(stage.x+stage.width-150,stage.y+210);await waitSaved();assert.equal((await state()).nodes.length,2);
  await page.getByRole('button',{name:'Connect',exact:true}).click();await page.locator('.canvas-node').nth(0).click();await page.locator('.canvas-node').nth(1).click();await waitSaved();assert.equal((await state()).edges.length,1);
  const before=await state();const rect=await page.locator('.canvas-node').first().boundingBox();await page.mouse.move(rect.x+30,rect.y+40);await page.mouse.down();await page.mouse.move(rect.x+85,rect.y+80,{steps:8});await page.mouse.up();await waitSaved();const after=await state();assert(after.nodes[0].x>before.nodes[0].x);
  await page.locator('#stage').focus();await page.keyboard.press('Control+d');await waitSaved();assert.equal((await state()).nodes.length,3);
  await page.keyboard.press('Control+z');await waitSaved();assert.equal((await state()).nodes.length,2);
  // A second client must receive updates without reloading.
  const second=await context.newPage();second.on('pageerror',e=>errors.push(e.message));await second.goto(page.url());await second.locator('.canvas-node').first().waitFor();await page.locator('.canvas-node').first().click();
  await page.getByRole('textbox',{name:'Title',exact:true}).fill('Live idea');await page.getByRole('textbox',{name:'Title',exact:true}).press('Tab');await waitSaved();await second.getByRole('heading',{name:'Live idea',exact:true}).waitFor();
  await second.getByRole('button',{name:'Follow You',exact:true}).first().click();await page.getByRole('button',{name:'Zoom in',exact:true}).click();const targetZoom=await page.locator('.zoom-controls>span:not(.tool-separator)').innerText();await second.waitForFunction(value=>document.querySelector('.zoom-controls>span:not(.tool-separator)')?.textContent===value,targetZoom);await second.locator('.following-pill').click();
  await page.getByRole('button',{name:'Ask AI',exact:true}).click();await page.getByRole('textbox',{name:'What would you like to explore?',exact:true}).fill('Refine only this selection');await page.getByRole('button',{name:'Copy request for Claude or Codex',exact:true}).click();const clip=await page.evaluate(()=>navigator.clipboard.readText());assert(clip.includes('Scope: ONLY')&&clip.includes((await state()).id));
  await page.locator('.share-menu summary').click();const downloadPromise=page.waitForEvent('download');await page.getByRole('button',{name:'Export as PNG',exact:true}).click();const download=await downloadPromise;assert.equal(download.suggestedFilename(),'pinboard.png');await download.saveAs('artifacts/canvas-export.png');
  await page.reload();await page.getByRole('heading',{name:'Live idea',exact:true}).waitFor();assert.equal((await state()).nodes.length,2);
  const mobile=await context.newPage();await mobile.setViewportSize({width:390,height:844});await mobile.goto(base);await mobile.locator('.canvas-node').first().waitFor();await mobile.screenshot({path:'artifacts/canvas-mobile.png',fullPage:true});assert.equal(await mobile.evaluate(()=>document.documentElement.scrollWidth>innerWidth),false);await mobile.locator('.canvas-node').first().click();await mobile.getByRole('textbox',{name:'Title',exact:true}).waitFor({state:'visible'});await mobile.getByRole('button',{name:'Close panel',exact:true}).click();await mobile.getByRole('button',{name:'New canvas',exact:true}).click();await mobile.waitForFunction(()=>document.querySelector('#board-title')?.value==='Untitled canvas');
  assert.deepEqual(errors,[]);console.log('PASS: create, rename, capture, inspector edit, process node, connect, drag, duplicate, undo, two-client live edits and following, scoped AI prompt, PNG, reload, and mobile layout/selection/creation');
} finally {await browser.close();}

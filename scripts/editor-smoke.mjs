import {chromium} from 'playwright';
import assert from 'node:assert/strict';
import {mkdir,writeFile} from 'node:fs/promises';

const base=process.env.PINBOARD_TEST_URL||'http://127.0.0.1:4127';
const browser=await chromium.launch({headless:true,...(process.env.CHROMIUM_PATH?{executablePath:process.env.CHROMIUM_PATH}:{}),args:['--no-sandbox']});
const context=await browser.newContext({viewport:{width:1500,height:1000},permissions:['clipboard-read','clipboard-write'],reducedMotion:'reduce'});
await context.addInitScript(()=>{
  window.SpeechRecognition=class {
    constructor(){window.testSpeech=this;}
    start(){this.onstart?.();}
    stop(){this.onend?.();}
    abort(){window.speechAborted=true;this.onend?.();}
  };
});
const page=await context.newPage(),errors=[];page.on('pageerror',e=>errors.push(e.message));
let id;
const read=async()=>{const r=await context.request.get(`${base}/api/canvases/${id}`);assert(r.ok());return r.json();};
const saved=()=>page.waitForFunction(()=>document.querySelector('.status-bar [role=status]')?.textContent==='All changes saved');
async function change(action){const response=page.waitForResponse(r=>r.url()===`${base}/api/canvases/${id}/mutations`&&r.request().method()==='POST');const [r]=await Promise.all([response,action()]);assert(r.ok(),await r.text());await saved();return read();}
async function external(operations){const c=await read();const r=await context.request.post(`${base}/api/canvases/${id}/mutations`,{data:{base_revision:c.revision,operations}});assert(r.ok(),await r.text());return r.json();}
const focus=()=>page.locator('#stage').focus();
try {
  await mkdir('artifacts',{recursive:true});
  const response=await context.request.post(base+'/api/canvases',{data:{title:'Expanded editor acceptance',nodes:[],edges:[]}});assert.equal(response.status(),201);id=(await response.json()).id;
  await page.goto(base+'/?canvas='+id);await page.locator('#stage').waitFor();await page.waitForFunction(()=>document.querySelector('#board-title')?.value==='Expanded editor acceptance');
  const shapes=[['r','rectangle'],['Shift+r','rounded_rectangle'],['o','oval'],['d','decision']];
  for(const [key,kind] of shapes){await focus();await change(()=>page.keyboard.press(key));await page.getByRole('textbox',{name:'Edit object title',exact:true}).waitFor();await change(async()=>{await page.getByRole('textbox',{name:'Edit object title',exact:true}).fill('Shape '+kind);await page.keyboard.press('Enter');});assert.equal((await read()).nodes.at(-1).kind,kind);}
  const initial=await read();assert.equal(initial.nodes.length,4);
  // Native typing and IME must not invoke creation shortcuts.
  await page.getByRole('textbox',{name:'Capture a thought',exact:true}).fill('R O D L');await page.keyboard.press('r');assert.equal((await read()).nodes.length,4);
  await focus();await page.locator('#stage').dispatchEvent('keydown',{key:'r',isComposing:true,bubbles:true});assert.equal((await read()).nodes.length,4);
  await page.getByRole('textbox',{name:'Capture a thought',exact:true}).fill('');
  await focus();await page.keyboard.press('Control+a');
  await change(()=>page.getByRole('button',{name:'Bold',exact:true}).click());
  await change(()=>page.getByRole('combobox',{name:'Font size',exact:true}).selectOption('20'));
  await change(()=>page.getByRole('combobox',{name:'Selection color',exact:true}).selectOption('lavender'));
  await change(()=>page.getByRole('combobox',{name:'List style',exact:true}).selectOption('bullet'));
  assert((await read()).nodes.every(n=>n.text_style.bold&&n.text_style.font_size===20&&n.text_style.list==='bullet'&&n.color==='lavender'));
  await focus();await change(()=>page.keyboard.press('Control+z'));await change(()=>page.keyboard.press('Control+z'));
  assert((await read()).nodes.every(n=>n.text_style.bold&&n.text_style.font_size===20&&n.text_style.list==='none'&&n.color==='butter'));
  // External changes to disjoint fields survive local undo.
  await external([{op:'update_node',id:initial.nodes[0].id,changes:{body:'A collaborator wrote this'}}]);
  await page.waitForFunction(()=>document.querySelector('.canvas-node p')?.textContent==='A collaborator wrote this');
  await change(()=>page.keyboard.press('Control+z'));assert((await read()).nodes.every(n=>n.text_style.font_size===12));assert.equal((await read()).nodes[0].body,'A collaborator wrote this');
  // Arrange shapes using authoritative mutations, then exercise grouping in UI.
  await external(initial.nodes.map((n,i)=>({op:'update_node',id:n.id,changes:{x:100+(i%2)*300,y:100+Math.floor(i/2)*240,color:i===0?'mint':'butter'}})));
  await page.reload();await page.locator('.canvas-node').nth(3).waitFor();await focus();await page.keyboard.press('Control+a');
  const upper=await page.locator('#node-'+initial.nodes[0].id).boundingBox(),lower=await page.locator('#node-'+initial.nodes[2].id).boundingBox();
  await page.mouse.move(upper.x-15,upper.y-15);await page.mouse.down();await page.mouse.move(lower.x+lower.width+15,lower.y+lower.height+15,{steps:8});await page.mouse.up();
  assert.equal(await page.locator('.canvas-node.selected').count(),2,'marquee follows world coordinates');
  await page.getByRole('button',{name:'Ask AI',exact:true}).click();await page.getByRole('textbox',{name:'What would you like to explore?',exact:true}).fill('Review this selected region');await page.getByRole('button',{name:'Copy request for Claude or Codex',exact:true}).click();
  const scoped=await page.evaluate(()=>navigator.clipboard.readText());assert(scoped.includes(initial.nodes[0].id)&&scoped.includes(initial.nodes[2].id)&&scoped.includes(id));
  await focus();await page.keyboard.press('Control+a');
  assert.equal(await page.getByRole('combobox',{name:'Selection color'}).inputValue(),'','mixed color state');
  let board=await change(()=>page.getByRole('button',{name:'Group selection',exact:true}).click());
  const group=board.nodes.find(n=>n.kind==='frame');assert(group);assert.equal(board.nodes.filter(n=>n.parent_id===group.id).length,4);
  const groupElement=page.locator('#node-'+group.id);await groupElement.focus();await page.keyboard.press('Enter');
  await page.waitForFunction(parent=>document.activeElement?.getAttribute('aria-level')==='2'&&document.activeElement?.getAttribute('data-node-id')!==parent,group.id);
  const first=await page.evaluate(()=>document.activeElement.id);await page.keyboard.press('Tab');await page.waitForFunction(first=>document.activeElement.id!==first,first);
  await page.keyboard.press('Escape');await page.waitForFunction(id=>document.activeElement.id===id,'node-'+group.id);
  const before=await read();await change(()=>page.keyboard.press('Shift+ArrowRight'));board=await read();assert(board.nodes.every(n=>n.x===before.nodes.find(b=>b.id===n.id).x+1));
  await change(()=>page.keyboard.press('Control+d'));board=await read();assert.equal(board.nodes.length,10);const copied=board.nodes.find(n=>n.kind==='frame'&&n.id!==group.id);assert.equal(board.nodes.filter(n=>n.parent_id===copied.id).length,4);
  await change(()=>page.keyboard.press('Control+z'));assert.equal((await read()).nodes.length,5);
  await groupElement.focus();await page.keyboard.press('F6');await page.waitForFunction(()=>document.activeElement.id==='format-toolbar');await page.keyboard.press('Control+F6');await page.waitForFunction(()=>document.activeElement.id==='board-title');await page.keyboard.press('F6');await page.waitForFunction(()=>document.activeElement.id==='stage');
  // AX output validates nesting/names/state; manual spoken screen-reader testing remains separate.
  const cdp=await context.newCDPSession(page);const ax=await cdp.send('Accessibility.getFullAXTree');const items=ax.nodes.filter(n=>n.role?.value==='treeitem');assert.equal(items.length,5);assert(items.some(n=>n.properties?.some(p=>p.name==='level'&&p.value.value===2)));await writeFile('artifacts/canvas-accessibility.json',JSON.stringify(ax,null,2));
  await groupElement.focus();await change(()=>page.getByRole('button',{name:'Ungroup children'}).click());assert((await read()).nodes.every(n=>n.parent_id===''));
  // Anchored routing recalculates after nudge and persists its settings.
  const nodes=(await read()).nodes;await focus();await page.keyboard.press('Shift+l');assert.equal(await page.getByRole('button',{name:'Connect',exact:true}).getAttribute('aria-pressed'),'true');
  await page.locator('#node-'+nodes[0].id).click();await change(()=>page.locator('#node-'+nodes[1].id).click());
  const edge=(await read()).edges[0];await page.locator('.edge-hit').first().click({force:true});
  await change(()=>page.getByLabel('Routing',{exact:true}).selectOption('orthogonal'));await change(async()=>{await page.getByLabel('Connector label',{exact:true}).fill('Approved');await page.getByLabel('Connector label',{exact:true}).press('Tab');});
  const path=await page.locator('.connectors g>path:first-child').first().getAttribute('d');await page.locator('#node-'+nodes[1].id).focus();await change(()=>page.keyboard.press('ArrowRight'));assert.notEqual(await page.locator('.connectors g>path:first-child').first().getAttribute('d'),path);assert.equal((await read()).edges[0].routing,'orthogonal');
  // World-coordinate line/ink creation at zoom, cancellation, formatting, duplicate, undo.
  await focus();await page.keyboard.press('Control+-');await page.keyboard.press('Control+-');const stage=await page.locator('#stage').boundingBox();const a={x:stage.x+35,y:stage.y+stage.height-230},b={x:a.x+120,y:a.y+55};
  await page.keyboard.press('l');await page.mouse.move(a.x,a.y);await page.mouse.down();await page.mouse.move(b.x,b.y);await page.keyboard.press('Escape');await page.mouse.up();assert.equal((await read()).nodes.length,4);
  await focus();await page.keyboard.press('l');board=await change(async()=>{await page.mouse.move(a.x,a.y);await page.mouse.down();await page.mouse.move(b.x,b.y,{steps:5});await page.mouse.up();});assert.equal(board.nodes.at(-1).kind,'line');assert.equal(board.nodes.at(-1).points.length,2);
  await focus();await page.keyboard.press('p');board=await change(async()=>{await page.mouse.move(a.x+160,a.y);await page.mouse.down();await page.mouse.move(b.x+160,b.y,{steps:10});await page.mouse.up();});const ink=board.nodes.at(-1);assert.equal(ink.kind,'sketch');assert(ink.points.length>=5&&ink.width>120);
  await change(async()=>{await page.getByLabel('Stroke width',{exact:true}).fill('8');await page.getByLabel('Stroke width',{exact:true}).press('Tab');});await change(()=>page.getByRole('combobox',{name:'Selection color'}).selectOption('sky'));
  await focus();await change(()=>page.keyboard.press('Control+d'));assert.equal((await read()).nodes.at(-1).stroke_width,8);await change(()=>page.keyboard.press('Control+z'));
  await page.locator('#node-'+ink.id).focus();await change(()=>page.getByRole('button',{name:'Bring to front',exact:true}).click());const top=(await read()).nodes.find(n=>n.id===ink.id);assert(top.z_index>0);
  // Resizable context panel and fit after resizing keep all content visible.
  const divider=page.getByRole('separator',{name:'Resize context panel'});await divider.focus();const old=await page.locator('.side-panel').evaluate(e=>e.getBoundingClientRect().width);await page.keyboard.press('ArrowLeft');await page.waitForFunction(w=>document.querySelector('.side-panel').getBoundingClientRect().width>w,old);
  await page.setViewportSize({width:1280,height:900});await focus();await page.keyboard.press('Control+0');
  await page.waitForFunction(()=>{const s=document.querySelector('#stage').getBoundingClientRect();return [...document.querySelectorAll('.canvas-node')].every(e=>{const r=e.getBoundingClientRect();return r.left>=s.left&&r.right<=s.right&&r.top>=s.top&&r.bottom<=s.bottom;});});
  // Browser speech is mocked only at its API boundary; text still uses real Leptos state.
  const count=(await read()).nodes.length;await page.getByRole('button',{name:'Start microphone',exact:true}).click();await page.getByRole('button',{name:'Pause microphone',exact:true}).waitFor();
  await page.evaluate(()=>{const result=[{transcript:'A spoken idea'}];result.isFinal=true;window.testSpeech.onresult({resultIndex:0,results:[result]});});await page.waitForFunction(()=>document.querySelector('[aria-label="Capture a thought"]').value.includes('A spoken idea'));assert.equal((await read()).nodes.length,count);
  await page.keyboard.press('Escape');await page.getByText('Microphone paused.',{exact:false}).waitFor();await change(()=>page.getByRole('button',{name:'Add thought to canvas',exact:true}).click());assert.equal((await read()).nodes.length,count+1);
  await page.getByRole('button',{name:'Start microphone',exact:true}).click();await page.evaluate(()=>{window.testSpeech.onerror({error:'not-allowed'});window.testSpeech.onend();});await page.getByText('Voice capture unavailable.',{exact:false}).waitFor();
  await page.evaluate(()=>{delete window.SpeechRecognition;delete window.webkitSpeechRecognition;});await page.getByRole('button',{name:'Start microphone',exact:true}).click();await page.waitForFunction(()=>document.querySelector('.status-bar [role=status]').textContent.includes('unavailable'));
  await page.getByRole('button',{name:'Connect an assistant',exact:true}).click();await page.getByRole('dialog').waitFor();await page.getByRole('button',{name:'Close setup'}).focus();await page.keyboard.press('Shift+Tab');assert(await page.getByRole('dialog').evaluate(e=>e.contains(document.activeElement)));await page.keyboard.press('F6');assert(await page.getByRole('dialog').evaluate(e=>e.contains(document.activeElement)));await page.keyboard.press('Escape');await page.getByRole('dialog').waitFor({state:'detached'});
  await page.screenshot({path:'artifacts/editor-expanded.png',fullPage:true});await page.locator('.share-menu summary').click();const downloading=page.waitForEvent('download');await page.getByRole('button',{name:'Export as PNG',exact:true}).click();await (await downloading).saveAs('artifacts/editor-expanded.png-export.png');
  await page.reload();await page.locator('#node-'+ink.id).waitFor();assert.equal((await read()).nodes.find(n=>n.id===ink.id).stroke_width,8);assert.equal((await read()).edges.find(e=>e.id===edge.id).label,'Approved');assert.deepEqual(errors,[]);
  console.log('PASS: shape hotkeys/IME, inline typing, mixed formatting, multi-step/disjoint undo, groups/hierarchy/AX tree, routing, layers/nudge, zoomed line/ink, panel resizing/fit, speech states, modal focus, PNG, and persistence');
} catch(error) {console.error(await page.evaluate(()=>({active:document.activeElement?.outerHTML.slice(0,1500),nodes:[...document.querySelectorAll('.canvas-node')].map(n=>({id:n.id,level:n.getAttribute('aria-level'),owns:n.getAttribute('aria-owns'),tab:n.tabIndex})),status:document.querySelector('.status-bar')?.textContent})));await page.screenshot({path:'artifacts/editor-failure.png',fullPage:true});throw error;} finally {await browser.close();}

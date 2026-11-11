import {chromium} from 'playwright';
import assert from 'node:assert/strict';
import {mkdir} from 'node:fs/promises';
import {fixture} from './identity-fixture.mjs';

const browser=await chromium.launch({headless:true,...(process.env.CHROMIUM_PATH?{executablePath:process.env.CHROMIUM_PATH}:{}),args:['--no-sandbox']});
const app=await fixture();const contexts=[];
const context=async()=>{const c=await browser.newContext({ignoreHTTPSErrors:true});contexts.push(c);return c;};
async function login(c,provider,email='admin@example.test',mode='valid') {
  const page=await c.newPage();await page.goto(app.base+(provider==='saml'?'/auth/saml/login':'/auth/login'));
  await page.getByLabel('Email').fill(email);await page.getByLabel('Scenario').fill(mode);
  if(provider==='oidc') {
    const result=page.waitForResponse(r=>r.url().startsWith(app.base+'/auth/callback'));
    await page.getByRole('button',{name:'Continue',exact:true}).click();const response=await result;await page.waitForLoadState();return {page,status:response.status()};
  }
  await page.getByRole('button',{name:'Continue',exact:true}).click();
  const fields=await page.locator('form').evaluate(form=>Object.fromEntries(new FormData(form)));
  const cookies=await c.cookies(app.base+'/auth/saml/acs');
  const correlation=cookies.find(c=>c.name==='pinboard_saml_state');assert(correlation?.secure&&correlation.sameSite==='None');
  const result=page.waitForResponse(r=>r.url()===app.base+'/auth/saml/acs');
  await page.getByRole('button',{name:'Sign in to Pinboard'}).click();const response=await result;await page.waitForLoadState();return {page,status:response.status(),fields,correlation};
}
const json=async(c,path,method='GET',data,expected=200)=>{const r=await c.request.fetch(app.base+path,{method,data,headers:{Origin:app.base}});assert.equal(r.status(),expected,await r.text());return expected===204?undefined:expected>=400?r.text():r.json();};
try {
  await mkdir('artifacts',{recursive:true});
  const anonymous=await context();assert.equal((await anonymous.request.get(app.base+'/api/canvases')).status(),401);
  const loginPage=await anonymous.newPage();await loginPage.goto(app.base);await loginPage.getByRole('link',{name:'Sign in with Company SAML'}).waitFor();await loginPage.screenshot({path:'artifacts/sign-in-providers.png'});await loginPage.close();
  const providers=await json(anonymous,'/api/me');assert.equal(providers.providers.length,2);assert.equal(providers.user,null);
  const metadata=await anonymous.request.get(app.base+'/auth/saml/metadata');assert.match(await metadata.text(),/AuthnRequestsSigned="true"/);
  const owner=await context();const oidc=await login(owner,'oidc');assert.equal(oidc.status,303);await oidc.page.close();
  const identity=(await json(owner,'/api/me')).user;assert(identity.admin&&identity.internal);
  const saml=await context();const signed=await login(saml,'saml');assert.equal(signed.status,303);
  assert.equal((await json(saml,'/api/me')).user.id,identity.id,'SAML and OIDC link verified email to the same account');
  const replay=await saml.request.post(app.base+'/auth/saml/acs',{form:signed.fields,headers:{Cookie:'pinboard_saml_state='+signed.correlation.value}});assert.equal(replay.status(),401,'one-use SAML request');await signed.page.close();
  for(const mode of ['unsigned','tampered','audience','multiple-audience','issuer','recipient','destination','request','expired','sha1']) {
    const c=await context(),result=await login(c,'saml','admin@example.test',mode);assert.equal(result.status,401,'SAML '+mode);assert.equal((await json(c,'/api/me')).user,null);await c.close();
  }
  for(const mode of ['nonce','audience','issuer','expired','unverified','azp','multi-audience','multi-audience-bad-azp']) {
    const c=await context(),result=await login(c,'oidc','admin@example.test',mode);assert.equal(result.status,mode==='unverified'?403:401,'OIDC '+mode);await c.close();
  }
  const multiAudience=await context();assert.equal((await login(multiAudience,'oidc','admin@example.test','multi-audience-valid')).status,303);await multiAudience.close();
  // OIDC callback needs the initiating browser, even with a genuine code/state.
  const attempt=await context(),page=await attempt.newPage();await page.goto(app.base+'/auth/login');
  const authUrl=new URL(page.url());const missingState=await anonymous.request.get(app.base+'/auth/callback?code=fake&state='+authUrl.searchParams.get('state'));assert.equal(missingState.status(),401);await page.close();
  const board=await json(owner,'/api/canvases','POST',{title:'Governed team canvas',nodes:[{id:'review-step',kind:'step',title:'Review dependency',x:200,y:150,width:220,height:160}],edges:[]},201);
  const invite=async(email,role)=>{
    const summary=await json(owner,`/api/canvases/${board.id}/invitations`,'POST',{email,role});assert(summary.delivered);
    const message=app.mail.at(-1).replace(/=\r\n/g,'').replace(/=([A-Fa-f0-9]{2})/g,(_,h)=>String.fromCharCode(parseInt(h,16)));
    assert.match(message,new RegExp(email.replace(/[.]/g,'\\.')));const link=message.match(/https:\/\/localhost:\d+\/\?invite=([A-Za-z0-9_-]+)/);assert(link,'actual SMTP invitation body');return {summary,token:link[1]};
  };
  const viewer=await context();await login(viewer,'oidc','viewer@example.test');const viewerInfo=(await json(viewer,'/api/me')).user;
  const invitation=await invite('viewer@example.test','viewer');await json(viewer,'/api/invitations/accept','POST',{token:invitation.token});
  assert.equal((await json(viewer,`/api/canvases/${board.id}/access`)).role,'viewer');
  await json(viewer,`/api/canvases/${board.id}/mutations`,'POST',{base_revision:board.revision,operations:[{op:'rename',title:'Denied'}]},403);
  await json(viewer,`/api/canvases/${board.id}/comments`,'POST',{x:0,y:0,body:'Denied'},403);
  await json(viewer,'/api/settings/artifacts','PUT',{canvas_enabled:false,external_invites:true},403);
  const commenter=await context();await login(commenter,'saml','commenter@example.test');const commenterInfo=(await json(commenter,'/api/me')).user;
  const commentInvite=await invite('commenter@example.test','commenter');await json(viewer,'/api/invitations/accept','POST',{token:commentInvite.token},403);await json(commenter,'/api/invitations/accept','POST',{token:commentInvite.token});
  const thread=await json(commenter,`/api/canvases/${board.id}/comments`,'POST',{anchor_id:'review-step',x:100,y:100,body:'Please check this dependency'});assert.equal(thread.replies[0].author_id,commenterInfo.id);
  await json(commenter,`/api/canvases/${board.id}/comments/${thread.id}/replies`,'POST',{body:'Spoof',author_id:identity.id},422);
  const reply=await json(owner,`/api/canvases/${board.id}/comments/${thread.id}/replies`,'POST',{body:'Checked'});assert.equal(reply.replies.length,2);
  await json(commenter,`/api/canvases/${board.id}/comments/${thread.id}`,'PUT',{resolved:true});
  assert.equal((await json(viewer,`/api/canvases/${board.id}/comments`))[0].resolved,true);
  const ownerPage=await owner.newPage(),viewerPage=await viewer.newPage(),commentPage=await commenter.newPage();
  await ownerPage.setViewportSize({width:1536,height:1000});await viewerPage.setViewportSize({width:1000,height:820});
  await Promise.all([ownerPage,viewerPage,commentPage].map(async p=>{await p.goto(app.base+'/?canvas='+board.id);await p.locator('#node-review-step').waitFor();}));
  assert(await viewerPage.getByRole('textbox',{name:'Canvas title',exact:true}).isDisabled());
  await commentPage.getByRole('button',{name:'Comments 0',exact:true}).click();await commentPage.getByLabel('Show resolved threads').check();await commentPage.getByRole('button',{name:'Reopen thread',exact:true}).click();
  await ownerPage.getByRole('button',{name:'Comments 1',exact:true}).waitFor();await ownerPage.getByRole('button',{name:'Comments 1',exact:true}).click();await ownerPage.locator('#pin-'+thread.id).waitFor();
  await ownerPage.locator('#pin-'+thread.id).focus();await ownerPage.keyboard.press('Enter');await ownerPage.waitForFunction(id=>document.activeElement.id==='thread-'+id,thread.id);await ownerPage.keyboard.press('Escape');await ownerPage.waitForFunction(id=>document.activeElement.id==='pin-'+id,thread.id);
  await viewerPage.getByRole('button',{name:'Follow admin',exact:true}).first().click();await ownerPage.getByRole('button',{name:'Zoom in',exact:true}).click();const zoom=await ownerPage.locator('.zoom-controls>span:not(.tool-separator)').innerText();await viewerPage.waitForFunction(z=>document.querySelector('.zoom-controls>span:not(.tool-separator)').textContent===z,zoom);
  const center=p=>p.evaluate(()=>{const r=document.querySelector('#stage').getBoundingClientRect(),m=new DOMMatrix(getComputedStyle(document.querySelector('.world')).transform);return [(r.width/2-m.e)/m.a,(r.height/2-m.f)/m.a];});
  const a=await center(ownerPage),b=await center(viewerPage);assert(a.every((v,i)=>Math.abs(v-b[i])<1),'following preserves world center across viewport sizes');
  await viewerPage.locator('.following-pill').click();await ownerPage.screenshot({path:'artifacts/team-comments.png',fullPage:true});
  const token=await json(viewer,'/api/tokens','POST');
  const rpc=await anonymous.request.post(app.base+'/mcp',{data:{jsonrpc:'2.0',id:1,method:'initialize',params:{protocolVersion:'2025-06-18',capabilities:{},clientInfo:{name:'identity-fixture',version:'1'}}},headers:{Authorization:'Bearer '+token.token,Accept:'application/json, text/event-stream'}});assert.equal(rpc.status(),200);
  const session=rpc.headers()['mcp-session-id'];assert(session);
  const mcpHeaders={Authorization:'Bearer '+token.token,Accept:'application/json, text/event-stream','Mcp-Session-Id':session,'MCP-Protocol-Version':'2025-06-18'};
  const initialized=await anonymous.request.post(app.base+'/mcp',{data:{jsonrpc:'2.0',method:'notifications/initialized'},headers:mcpHeaders});assert(initialized.ok());
  const denied=await anonymous.request.post(app.base+'/mcp',{data:{jsonrpc:'2.0',id:2,method:'tools/call',params:{name:'edit_canvas',arguments:{canvas_id:board.id,base_revision:0,operations:[{op:'rename',title:'Denied MCP edit'}]}}},headers:mcpHeaders});assert(denied.ok());const text=await denied.text(),message=JSON.parse(text.startsWith('event:')||text.startsWith('data:')?text.split('\n').find(l=>l.startsWith('data: ')).slice(6):text);assert(message.error||message.result?.isError,'MCP must enforce the viewer role');
  await json(viewer,'/api/tokens/'+token.info.id,'DELETE',undefined,204);
  assert.equal((await anonymous.request.get(app.base+`/api/canvases/${board.id}`,{headers:{Authorization:'Bearer '+token.token}})).status(),401);
  await json(owner,`/api/canvases/${board.id}/members/${viewerInfo.id}`,'DELETE',undefined,204);
  await json(viewer,`/api/canvases/${board.id}`,'GET',undefined,403);
  await viewerPage.locator('#node-review-step').waitFor({state:'detached'});
  await viewerPage.getByText('Access to this canvas is no longer available',{exact:true}).waitFor();
  await json(viewer,'/api/invitations/accept','POST',{token:invitation.token},403);
  await json(owner,`/api/canvases/${board.id}/invitations`,'POST',{email:'external@partner.test',role:'editor'},403);
  await json(owner,'/api/settings/artifacts','PUT',{canvas_enabled:true,external_invites:true});const externalInvite=await invite('external@partner.test','editor');
  const external=await context();await login(external,'saml','external@partner.test');await json(external,'/api/invitations/accept','POST',{token:externalInvite.token});
  const latest=await json(owner,`/api/canvases/${board.id}`);await json(external,`/api/canvases/${board.id}/mutations`,'POST',{base_revision:latest.revision,operations:[{op:'rename',title:'External collaborator edit'}]});
  await json(owner,'/api/settings/artifacts','PUT',{canvas_enabled:false,external_invites:false});await json(external,`/api/canvases/${board.id}`,'GET',undefined,403);
  await json(owner,'/api/settings/artifacts','PUT',{canvas_enabled:true,external_invites:false});
  // Cross-browser/cross-provider sessions reopen persisted board membership.
  const returning=await context();await login(returning,'oidc','external@partner.test');assert.equal((await json(returning,`/api/canvases/${board.id}`)).title,'External collaborator edit');
  // A changed SSO email must not inherit an older MCP factory's admin identity.
  const privateBoard=await json(commenter,'/api/canvases','POST',{title:'Private account board',nodes:[],edges:[]},201);
  async function mcpRead(c,token,canvas) {
    const headers={Authorization:'Bearer '+token,Accept:'application/json, text/event-stream','MCP-Protocol-Version':'2025-06-18'};
    const init=await c.request.post(app.base+'/mcp',{data:{jsonrpc:'2.0',id:1,method:'initialize',params:{protocolVersion:'2025-06-18',capabilities:{},clientInfo:{name:'principal-regression',version:'1'}}},headers});assert(init.ok());headers['Mcp-Session-Id']=init.headers()['mcp-session-id'];
    await c.request.post(app.base+'/mcp',{data:{jsonrpc:'2.0',method:'notifications/initialized'},headers});
    const result=await c.request.post(app.base+'/mcp',{data:{jsonrpc:'2.0',id:2,method:'tools/call',params:{name:'read_canvas',arguments:{canvas_id:canvas}}},headers});assert(result.ok());const text=await result.text();return JSON.parse(text.startsWith('event:')||text.startsWith('data:')?text.split('\n').find(l=>l.startsWith('data: ')).slice(6):text);
  }
  const adminToken=await json(owner,'/api/tokens','POST');assert((await mcpRead(anonymous,adminToken.token,privateBoard.id)).result);
  const renamed=await context();await login(renamed,'oidc','former-admin@example.test','email-change');const renamedUser=(await json(renamed,'/api/me')).user;assert.equal(renamedUser.id,identity.id);assert.equal(renamedUser.admin,false);
  await json(renamed,`/api/canvases/${privateBoard.id}`,'GET',undefined,403);const renamedToken=await json(renamed,'/api/tokens','POST');const restricted=await mcpRead(anonymous,renamedToken.token,privateBoard.id);assert(restricted.error||restricted.result?.isError,'MCP must use the new non-admin principal');
  assert(app.counts().pkceExchanges>=8&&app.counts().requestSignatures>=13);
  console.log('PASS: signed SAML, replay/correlation/signature/audience/recipient/time checks, OIDC PKCE/nonce/claims, identity linking, SMTP invitations, roles, live comment pins, keyboard traversal, cross-size following, revocation, PATs, and organization policy');
} catch(error) {console.error(app.logs());throw error;}
finally {await browser.close();await app.stop();}

// SAML alone must never fall back to the trusted local principal.
const only=await fixture({oidc:false});
try {
  const {request}=await import('playwright');const c=await request.newContext({ignoreHTTPSErrors:true});
  const me=await (await c.get(only.base+'/api/me')).json();assert.equal(me.user,null);assert.equal(me.providers.length,1);assert.match(me.providers[0].login_url,/saml/);
  assert.equal((await c.get(only.base+'/api/canvases')).status(),401);await c.dispose();console.log('PASS: SAML-only deployment requires authentication');
} finally {await only.stop();}

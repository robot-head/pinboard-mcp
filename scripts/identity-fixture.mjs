// Local-only OIDC, SAML, SMTP, and HTTPS fixtures. No real accounts or mail.
import http from 'node:http';
import https from 'node:https';
import net from 'node:net';
import {mkdtemp, readFile, writeFile, rm} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import {join, resolve} from 'node:path';
import {spawn, execFileSync} from 'node:child_process';
import {createHash, sign, verify, randomUUID, createPublicKey} from 'node:crypto';
import {inflateRawSync} from 'node:zlib';
import assert from 'node:assert/strict';

const b64 = value => Buffer.from(typeof value === 'string' ? value : JSON.stringify(value)).toString('base64url');
const xml = value => String(value).replace(/[&<>"']/g, c => ({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&apos;'}[c]));
const listen = server => new Promise(resolve => server.listen(0, '127.0.0.1', () => resolve(server.address().port)));
const close = server => new Promise(resolve => {server.closeAllConnections?.(); server.close(resolve);});
const body = async request => {let text=''; for await (const chunk of request) text+=chunk; return new URLSearchParams(text);};
const reply = (response, value, type='application/json') => {response.setHeader('Content-Type', type); response.end(type==='application/json' ? JSON.stringify(value) : value);};

export async function fixture({oidc=true}={}) {
  const directory=await mkdtemp(join(tmpdir(),'pinboard-identity-'));
  const files=name=>join(directory,name);
  for (const name of ['idp','sp']) execFileSync('openssl',['req','-x509','-newkey','rsa:2048','-nodes','-keyout',files(name+'.key'),'-out',files(name+'.crt'),'-days','1','-subj','/CN=localhost','-addext','subjectAltName=DNS:localhost,IP:127.0.0.1'],{stdio:'ignore'});
  const key=await readFile(files('idp.key')), certificate=await readFile(files('idp.crt'));
  const spCertificate=await readFile(files('sp.crt'));
  const der=certificate.toString().replace(/-----[^-]+-----|\s/g,'');
  const jwk={...createPublicKey(key).export({format:'jwk'}),kid:'fixture',use:'sig',alg:'RS256'};
  const codes=new Map(), mail=[]; let issuer,base,logs=''; let requestSignatures=0,pkceExchanges=0;
  const loginForm=(response,url) => reply(response,`<!doctype html><form method="post" action="${xml(url.pathname+url.search)}"><label>Email <input name="email" value="admin@example.test"></label><label>Scenario <input name="mode" value="valid"></label><button>Continue</button></form>`,'text/html');
  const idp=http.createServer(async(request,response)=>{
    try {
      const url=new URL(request.url,issuer);
      if(url.pathname==='/.well-known/openid-configuration') return reply(response,{issuer,authorization_endpoint:issuer+'/oidc/authorize',token_endpoint:issuer+'/oidc/token',jwks_uri:issuer+'/jwks',userinfo_endpoint:issuer+'/userinfo'});
      if(url.pathname==='/jwks') return reply(response,{keys:[jwk]});
      if(url.pathname==='/oidc/authorize') {
        if(request.method==='GET') return loginForm(response,url);
        const input=await body(request),code=randomUUID(); codes.set(code,{parameters:url.searchParams,email:input.get('email'),mode:input.get('mode')});
        const callback=new URL(url.searchParams.get('redirect_uri')); callback.searchParams.set('code',code); callback.searchParams.set('state',url.searchParams.get('state'));
        response.writeHead(302,{location:callback.href});return response.end();
      }
      if(url.pathname==='/oidc/token') {
        const form=await body(request),entry=codes.get(form.get('code')); codes.delete(form.get('code')); assert(entry,'one-use authorization code');
        assert.equal(form.get('client_id'),'pinboard-fixture'); assert.equal(form.get('client_secret'),'local-fixture-secret');
        assert.equal(form.get('redirect_uri'),base+'/auth/callback');
        assert.equal(entry.parameters.get('code_challenge_method'),'S256');
        assert.equal(createHash('sha256').update(form.get('code_verifier')).digest('base64url'),entry.parameters.get('code_challenge'));pkceExchanges++;
        const {email,mode,parameters}=entry,now=Math.floor(Date.now()/1000);
        const claims={iss:mode==='issuer'?'https://wrong.test':issuer,sub:email,aud:mode==='audience'?'wrong-client':'pinboard-fixture',iat:now,exp:now+(mode==='expired'?-600:300),nonce:mode==='nonce'?'wrong':parameters.get('nonce'),email,email_verified:mode!=='unverified',name:email.split('@')[0]};
        const content=b64({alg:'RS256',kid:'fixture',typ:'JWT'})+'.'+b64(claims);
        return reply(response,{id_token:content+'.'+sign('RSA-SHA256',Buffer.from(content),key).toString('base64url'),access_token:'fixture-only',token_type:'Bearer'});
      }
      if(url.pathname==='/metadata') return reply(response,metadata(),'application/samlmetadata+xml');
      if(url.pathname==='/saml') {
        if(request.method==='GET') return loginForm(response,url);
        const input=await body(request),email=input.get('email'),mode=input.get('mode');
        const raw=request.url.split('?')[1].split('&').filter(p=>!p.startsWith('Signature=')).join('&');
        assert.equal(url.searchParams.get('SigAlg'),'http://www.w3.org/2001/04/xmldsig-more#rsa-sha256');
        assert(verify('RSA-SHA256',Buffer.from(raw),spCertificate,Buffer.from(url.searchParams.get('Signature'),'base64')),'signed SP request');requestSignatures++;
        const requestXml=inflateRawSync(Buffer.from(url.searchParams.get('SAMLRequest'),'base64')).toString();
        const requestId=requestXml.match(/\bID="([^"]+)"/)[1],id='_'+randomUUID();
        const requestRef=mode==='request'?'_unrequested':requestId;
        const acs=base+'/auth/saml/acs',recipient=mode==='recipient'?'https://wrong.test/acs':acs;
        const now=new Date().toISOString(),before=new Date(Date.now()-60000).toISOString(),after=new Date(Date.now()+(mode==='expired'?-600000:240000)).toISOString();
        const signature=`<ds:Signature xmlns:ds="http://www.w3.org/2000/09/xmldsig#"><ds:SignedInfo><ds:CanonicalizationMethod Algorithm="http://www.w3.org/2001/10/xml-exc-c14n#"/><ds:SignatureMethod Algorithm="${mode==='sha1'?'http://www.w3.org/2000/09/xmldsig#rsa-sha1':'http://www.w3.org/2001/04/xmldsig-more#rsa-sha256'}"/><ds:Reference URI="#${id}"><ds:Transforms><ds:Transform Algorithm="http://www.w3.org/2000/09/xmldsig#enveloped-signature"/><ds:Transform Algorithm="http://www.w3.org/2001/10/xml-exc-c14n#"/></ds:Transforms><ds:DigestMethod Algorithm="http://www.w3.org/2001/04/xmlenc#sha256"/><ds:DigestValue/></ds:Reference></ds:SignedInfo><ds:SignatureValue/><ds:KeyInfo><ds:X509Data><ds:X509Certificate>${der}</ds:X509Certificate></ds:X509Data></ds:KeyInfo></ds:Signature>`;
        let assertion=`<samlp:Response xmlns:samlp="urn:oasis:names:tc:SAML:2.0:protocol" xmlns:saml="urn:oasis:names:tc:SAML:2.0:assertion" ID="${id}" Version="2.0" IssueInstant="${now}" Destination="${xml(mode==='destination'?'https://wrong.test/acs':acs)}" InResponseTo="${requestRef}"><saml:Issuer>${issuer}/saml</saml:Issuer>${mode==='unsigned'?'':signature}<samlp:Status><samlp:StatusCode Value="urn:oasis:names:tc:SAML:2.0:status:Success"/></samlp:Status><saml:Assertion ID="_${randomUUID()}" Version="2.0" IssueInstant="${now}"><saml:Issuer>${mode==='issuer'?'https://wrong.test':issuer+'/saml'}</saml:Issuer><saml:Subject><saml:NameID Format="urn:oasis:names:tc:SAML:2.0:nameid-format:persistent">${xml(email)}</saml:NameID><saml:SubjectConfirmation Method="urn:oasis:names:tc:SAML:2.0:cm:bearer"><saml:SubjectConfirmationData InResponseTo="${requestRef}" Recipient="${xml(recipient)}" NotOnOrAfter="${after}"/></saml:SubjectConfirmation></saml:Subject><saml:Conditions NotBefore="${before}" NotOnOrAfter="${after}"><saml:AudienceRestriction><saml:Audience>${xml(mode==='audience'?'wrong-entity':base+'/auth/saml/metadata')}</saml:Audience></saml:AudienceRestriction>${mode==='multiple-audience'?'<saml:AudienceRestriction><saml:Audience>wrong-second-restriction</saml:Audience></saml:AudienceRestriction>':''}</saml:Conditions><saml:AuthnStatement AuthnInstant="${now}"><saml:AuthnContext><saml:AuthnContextClassRef>urn:oasis:names:tc:SAML:2.0:ac:classes:PasswordProtectedTransport</saml:AuthnContextClassRef></saml:AuthnContext></saml:AuthnStatement><saml:AttributeStatement><saml:Attribute Name="email"><saml:AttributeValue>${xml(email)}</saml:AttributeValue></saml:Attribute><saml:Attribute Name="displayName"><saml:AttributeValue>${xml(email.split('@')[0])}</saml:AttributeValue></saml:Attribute></saml:AttributeStatement></saml:Assertion></samlp:Response>`;
        if(mode!=='unsigned') {
          await writeFile(files('template.xml'),assertion);
          execFileSync(process.env.XMLSEC1_PATH||'xmlsec1',['--sign','--privkey-pem',files('idp.key')+','+files('idp.crt'),'--id-attr:ID','urn:oasis:names:tc:SAML:2.0:protocol:Response','--output',files('signed.xml'),files('template.xml')],{stdio:'pipe'});
          assertion=await readFile(files('signed.xml'),'utf8');
        }
        if(mode==='tampered') assertion=assertion.replace(xml(email),'attacker@example.test');
        return reply(response,`<!doctype html><form method="post" action="${acs}"><input type="hidden" name="SAMLResponse" value="${Buffer.from(assertion).toString('base64')}"><input type="hidden" name="RelayState" value="${xml(url.searchParams.get('RelayState'))}"><button>Sign in to Pinboard</button></form>`,'text/html');
      }
      response.writeHead(404);response.end();
    } catch(error) {logs+='\nFixture: '+error.stack;response.writeHead(500);response.end('Fixture failed');}
  });
  issuer='http://127.0.0.1:'+await listen(idp);
  const metadata=()=>`<EntityDescriptor xmlns="urn:oasis:names:tc:SAML:2.0:metadata" entityID="${issuer}/saml"><IDPSSODescriptor protocolSupportEnumeration="urn:oasis:names:tc:SAML:2.0:protocol"><KeyDescriptor use="signing"><ds:KeyInfo xmlns:ds="http://www.w3.org/2000/09/xmldsig#"><ds:X509Data><ds:X509Certificate>${der}</ds:X509Certificate></ds:X509Data></ds:KeyInfo></KeyDescriptor><SingleSignOnService Binding="urn:oasis:names:tc:SAML:2.0:bindings:HTTP-Redirect" Location="${issuer}/saml"/></IDPSSODescriptor></EntityDescriptor>`;
  await writeFile(files('metadata.xml'),metadata());
  const smtp=net.createServer(socket=>{
    socket.write('220 local fixture ESMTP\r\n');let buffer='',data=false,message='';
    socket.on('data',chunk=>{buffer+=chunk;let i;while((i=buffer.indexOf('\r\n'))>=0){const line=buffer.slice(0,i);buffer=buffer.slice(i+2);
      if(data){if(line==='.') {mail.push(message);message='';data=false;socket.write('250 queued locally\r\n');}else message+=line+'\r\n';}
      else if(/^EHLO|^HELO/.test(line))socket.write('250 localhost\r\n');
      else if(line==='DATA'){data=true;socket.write('354 continue\r\n');}
      else if(line==='QUIT'){socket.end('221 bye\r\n');}
      else socket.write('250 ok\r\n');
    }});
  });
  const smtpPort=await listen(smtp),reserve=net.createServer(),port=await listen(reserve);await close(reserve);
  const proxy=https.createServer({key,cert:certificate},(request,response)=>{
    const upstream=http.request({host:'127.0.0.1',port,path:request.url,method:request.method,headers:request.headers},r=>{response.writeHead(r.statusCode,r.headers);r.pipe(response);});
    upstream.on('error',()=>{response.writeHead(502);response.end();});request.pipe(upstream);
  });
  proxy.on('upgrade',(request,socket,head)=>{const upstream=net.connect(port,'127.0.0.1',()=>{upstream.write(`${request.method} ${request.url} HTTP/1.1\r\n${Object.entries(request.headers).map(([k,v])=>`${k}: ${v}`).join('\r\n')}\r\n\r\n`);upstream.write(head);socket.pipe(upstream).pipe(socket);});upstream.on('error',()=>socket.destroy());socket.on('error',()=>upstream.destroy());socket.on('close',()=>upstream.destroy());});
  base='https://localhost:'+await listen(proxy);
  const env=Object.fromEntries(Object.entries(process.env).filter(([k])=>!/^TURSO_|^PINBOARD_/.test(k)));
  Object.assign(env,{PINBOARD_PUBLIC_URL:base,PINBOARD_ADMIN_EMAILS:'admin@example.test',PINBOARD_INTERNAL_DOMAINS:'example.test',PINBOARD_SAML_IDP_METADATA_FILE:files('metadata.xml'),PINBOARD_SAML_SP_KEY_FILE:files('sp.key'),PINBOARD_SAML_SP_CERT_FILE:files('sp.crt'),PINBOARD_SAML_LABEL:'Company SAML',PINBOARD_SMTP_HOST:'127.0.0.1',PINBOARD_SMTP_PORT:String(smtpPort),PINBOARD_SMTP_TLS:'none',PINBOARD_SMTP_FROM:'Pinboard <canvas@example.test>'});
  if(oidc)Object.assign(env,{PINBOARD_OIDC_ISSUER:issuer,PINBOARD_OIDC_CLIENT_ID:'pinboard-fixture',PINBOARD_OIDC_CLIENT_SECRET:'local-fixture-secret',PINBOARD_OIDC_LABEL:'Company OIDC'});
  const server=spawn(resolve('target/debug/pinboard-mcp'),['--serve','--port',String(port),'--db',files('database.db'),'--dist',resolve('crates/web/dist'),'--no-demo'],{env,stdio:['ignore','ignore','pipe']});server.stderr.on('data',chunk=>logs+=chunk);
  for(let i=0;i<100;i++){try{if((await fetch('http://127.0.0.1:'+port+'/api/health')).ok)break;}catch{}if(server.exitCode!==null)throw Error(logs);if(i===99)throw Error('Server startup timed out\n'+logs);await new Promise(r=>setTimeout(r,100));}
  return {base,mail,logs:()=>logs,counts:()=>({requestSignatures,pkceExchanges}),async stop(){server.kill();await new Promise(r=>server.once('exit',r));await Promise.all([close(proxy),close(idp),close(smtp)]);await rm(directory,{recursive:true,force:true});}};
}

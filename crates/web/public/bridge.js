// Browser-only capabilities; canvas state and rendering live in Rust/Leptos.
let live, liveId, receiver, reconnect, camera = {name:'You', x:0,y:0,zoom:1,width:1000,height:750};
let cameraTimer;
export function openLive(id, callback) {
  liveId = id; receiver = callback; clearTimeout(reconnect);
  if (live) { live.onclose = null; live.close(); }
  const connect = () => {
    if (liveId !== id) return;
    live = new WebSocket(`${location.protocol === 'https:' ? 'wss' : 'ws'}://${location.host}/api/canvases/${encodeURIComponent(id)}/live`);
    live.onopen = () => { receiver(JSON.stringify({type:'connection',connected:true,canvas_id:id})); live.send(JSON.stringify(camera)); receiver(JSON.stringify({type:'refresh',canvas_id:id})); };
    live.onmessage = e => receiver(e.data);
    live.onclose = () => { receiver(JSON.stringify({type:'connection',connected:false,canvas_id:id})); reconnect=setTimeout(connect,1500); };
  };
  connect();
}
export function sendCamera(x,y,zoom,name) {
  const rect=document.getElementById('stage')?.getBoundingClientRect();camera = {x,y,zoom,name,width:rect?.width||1,height:rect?.height||1};
  clearTimeout(cameraTimer);
  cameraTimer = setTimeout(() => {if(live?.readyState===1)live.send(JSON.stringify(camera));},60);
}
export function capturePointer(id) { document.getElementById('stage')?.setPointerCapture(id); }
export function setLocation(id) { history.replaceState(null,'',`?canvas=${encodeURIComponent(id)}`); }
export async function copyText(text) { await navigator.clipboard.writeText(text); }
export function focusElement(id) { requestAnimationFrame(()=>document.getElementById(id)?.focus()); }

let recognition, voiceCallback, transcript='', listening=false, voiceError=false;
export function stopVoice() { recognition?.stop(); }
export function toggleVoice(callback,initial) {
  if(listening) { stopVoice(); return; }
  const Speech = window.SpeechRecognition || window.webkitSpeechRecognition;
  if(!Speech) { callback(JSON.stringify({state:'error',message:'Voice capture is unavailable in this browser. You can type or paste a transcript.'})); return; }
  transcript=initial; voiceCallback=callback; voiceError=false;
  recognition=new Speech(); recognition.continuous=true; recognition.interimResults=true;
  recognition.lang=navigator.language || 'en-US';
  recognition.onstart=()=>{listening=true;voiceCallback(JSON.stringify({state:'listening'}));};
  recognition.onresult=e=>{
    let final='', interim='';
    for(let i=e.resultIndex;i<e.results.length;i++) { if(e.results[i].isFinal)final+=e.results[i][0].transcript+' ';else interim+=e.results[i][0].transcript; }
    transcript+=(transcript && final ? ' ' : '')+final;
    voiceCallback(JSON.stringify({state:'listening',text:transcript+interim}));
  };
  recognition.onerror=e=>{voiceError=true;listening=false;voiceCallback(JSON.stringify({state:'error',message:`Microphone: ${e.error}. Your text has been kept.`}));};
  recognition.onend=()=>{listening=false;if(!voiceError)voiceCallback(JSON.stringify({state:'paused'}));};
  try{recognition.start();}catch(error){listening=false;callback(JSON.stringify({state:"error",message:"Voice capture could not start. Your text has been kept."}));}
}

const colors={butter:'#f4e9a8',lavender:'#e4ddf2',mint:'#d7e8d7',peach:'#f4dcc9',sky:'#dce9f1',paper:'#fffef9'};
function wrap(ctx,text,x,y,width,line,maxY) {
  for(const paragraph of text.split('\n')) {
    let row='';
    for(const word of paragraph.split(' ')) {
      if(ctx.measureText(row+word).width>width && row) {if(y>maxY)return y;ctx.fillText(row,x,y);row='';y+=line;}
      row+=word+' ';
    }
    if(y>maxY)return y;ctx.fillText(row,x,y);y+=line;
  }
  return y;
}
export async function exportPng(json) {
  const board=JSON.parse(json), nodes=board.nodes;
  const left=Math.min(0,...nodes.map(n=>n.x))-50,top=Math.min(0,...nodes.map(n=>n.y))-85;
  const width=Math.max(600,...nodes.map(n=>n.x+n.width))-left+50;
  const height=Math.max(350,...nodes.map(n=>n.y+n.height))-top+50;
  const scale=Math.min(2,8192/width,8192/height);
  const canvas=document.createElement('canvas');canvas.width=Math.ceil(width*scale);canvas.height=Math.ceil(height*scale);
  const ctx=canvas.getContext('2d');ctx.scale(scale,scale);ctx.translate(-left,-top);
  ctx.fillStyle='#f7f7f2';ctx.fillRect(left,top,width,height);
  ctx.fillStyle='#345341';ctx.font='600 22px system-ui';ctx.fillText(board.title,left+40,top+40);
  for(const edge of board.paths||[]) {
    ctx.strokeStyle='#909b8d';ctx.lineWidth=1.8;ctx.stroke(new Path2D(edge.path));
    if(edge.arrow){ctx.save();ctx.translate(edge.end.x,edge.end.y);ctx.rotate(edge.angle);ctx.fillStyle='#909b8d';ctx.beginPath();ctx.moveTo(0,0);ctx.lineTo(-9,-5);ctx.lineTo(-9,5);ctx.fill();ctx.restore();}
    if(edge.label){ctx.fillStyle='#6d7568';ctx.font='12px system-ui';ctx.fillText(edge.label,(edge.start.x+edge.end.x)/2,(edge.start.y+edge.end.y)/2-10);}
  }
  let omittedMedia=0;
  for(const n of [...nodes].sort((a,b)=>(a.z_index||0)-(b.z_index||0))) {
    if(n.kind==='sketch'||n.kind==='line') {
      const ink={butter:'#8b741c',lavender:'#795990',mint:'#3c7049',peach:'#ab603e',sky:'#3d7597',paper:'#345341'};
      ctx.strokeStyle=ink[n.color]||ink.paper;ctx.lineWidth=n.stroke_width||3;ctx.lineCap='round';ctx.lineJoin='round';ctx.beginPath();n.points.forEach((p,i)=>i?ctx.lineTo(n.x+p.x,n.y+p.y):ctx.moveTo(n.x+p.x,n.y+p.y));ctx.stroke();continue;
    }
    const frame=n.kind==='frame'||n.kind==='swimlane';
    ctx.save();ctx.fillStyle=colors[n.color]||colors.paper;ctx.strokeStyle=frame?'#adb8a6':'#d2d6c9';ctx.lineWidth=1;if(frame)ctx.setLineDash([8,5]);
    ctx.beginPath();
    if(n.kind==='decision'){ctx.moveTo(n.x+n.width/2,n.y);ctx.lineTo(n.x+n.width,n.y+n.height/2);ctx.lineTo(n.x+n.width/2,n.y+n.height);ctx.lineTo(n.x,n.y+n.height/2);ctx.closePath();}
    else if(n.kind==='oval')ctx.ellipse(n.x+n.width/2,n.y+n.height/2,n.width/2,n.height/2,0,0,Math.PI*2);
    else ctx.roundRect(n.x,n.y,n.width,n.height,['step','rounded_rectangle'].includes(n.kind)?16:n.kind==='rectangle'?0:3);
    if(!frame&&n.kind!=='text')ctx.fill();if(n.kind!=='text')ctx.stroke();ctx.setLineDash([]);ctx.clip();
    if(n.kind==='image' && n.url) {
      try {const img=new Image();img.crossOrigin='anonymous';img.referrerPolicy='no-referrer';img.src=n.url;await Promise.race([img.decode(),new Promise((_,reject)=>setTimeout(()=>reject(new Error('timeout')),2500))]);const ratio=Math.min((n.width-16)/img.naturalWidth,(n.height-60)/img.naturalHeight);ctx.drawImage(img,n.x+(n.width-img.naturalWidth*ratio)/2,n.y+8,img.naturalWidth*ratio,img.naturalHeight*ratio);}catch{omittedMedia++;ctx.fillStyle='#677061';ctx.font='14px system-ui';ctx.fillText('Image (CORS protected)',n.x+16,n.y+50);}
    }
    ctx.fillStyle='#6d7568';ctx.font='10px system-ui';ctx.fillText(n.group.toUpperCase(),n.x+22,n.y+28);
    ctx.fillStyle='#2e372c';ctx.font=`600 ${frame?14:20}px system-ui`;const y=wrap(ctx,n.title,n.x+22,n.y+58,n.width-44,26,n.y+n.height-20);
    if(n.kind!=='image') {const st=n.text_style||{},size=st.font_size||12;ctx.font=`${st.italic?'italic ':''}${st.bold?'700':'400'} ${size}px system-ui`;const body=n.body.split('\n').map((line,i)=>st.list==='bullet'?'• '+line:st.list==='numbered'?`${i+1}. ${line}`:line).join('\n');wrap(ctx,body,n.x+22,y+8,n.width-44,size*1.6,n.y+n.height-28);}
    if(n.kind==='video'){omittedMedia++;ctx.font='12px system-ui';ctx.fillText('Video: '+n.url,n.x+16,n.y+n.height-40,n.width-32);}
    if(n.owner||n.due){ctx.fillStyle='#53634f';ctx.font='11px system-ui';ctx.fillText([n.owner,n.due].filter(Boolean).join(' · '),n.x+22,n.y+n.height-12);}
    ctx.restore();
  }
  const blob=await new Promise(resolve=>canvas.toBlob(resolve,'image/png'));
  if(!blob)throw new Error('Image could not be exported');
  const url=URL.createObjectURL(blob),a=document.createElement('a');a.href=url;a.download='pinboard.png';a.click();setTimeout(()=>URL.revokeObjectURL(url),1000);
  return omittedMedia ? `PNG exported; ${omittedMedia} media item(s) shown as placeholders.` : 'PNG exported';
}

let canvasObserver,canvasWait;
export function observeCanvas(callback) {canvasObserver?.disconnect();canvasWait?.disconnect();const attach=()=>{const el=document.getElementById('stage');if(!el)return false;canvasObserver=new ResizeObserver(()=>callback());canvasObserver.observe(el);return true;};requestAnimationFrame(()=>{if(attach())return;canvasWait=new MutationObserver(()=>{if(attach())canvasWait.disconnect();});canvasWait.observe(document.body,{childList:true,subtree:true});});}
addEventListener('beforeunload',()=>{recognition?.abort();if(live){live.onclose=null;live.close();}});
export function stopLive(){clearTimeout(reconnect);liveId=null;if(live){live.onclose=null;live.close();live=null;}}
let dialogReturn;
export function startDialog(){dialogReturn=document.activeElement;requestAnimationFrame(()=>{const d=document.querySelector('[role=dialog]');d?.querySelector('button,a[href],input,textarea,select,[tabindex="0"]')?.focus();});}
export function endDialog(){if(dialogReturn?.isConnected)dialogReturn.focus();dialogReturn=null;}
addEventListener('keydown',event=>{if(event.key!=='Tab')return;const d=document.querySelector('[role=dialog]');if(!d)return;const items=[...d.querySelectorAll('button,a[href],input,textarea,select,[tabindex="0"]')].filter(el=>!el.disabled&&el.getClientRects().length);if(!items.length){event.preventDefault();return;}const first=items[0],last=items.at(-1);if(event.shiftKey&&(document.activeElement===first||!d.contains(document.activeElement))){event.preventDefault();last.focus();}else if(!event.shiftKey&&(document.activeElement===last||!d.contains(document.activeElement))){event.preventDefault();first.focus();}});

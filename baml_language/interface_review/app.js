(function () {
'use strict';
const {cases}=globalThis.BamlReview;
const examples=globalThis.BamlReviewExamples;
let preferredLanguage="Python";
const $=id=>document.getElementById(id);
const esc=value=>String(value).replace(/[&<>"']/g,c=>({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]));
const format=value=>typeof value==='string'?value:JSON.stringify(value);
const storageKey='baml-interface-review-v1';
let saved={},persist=true;
try {const x=JSON.parse(localStorage.getItem(storageKey)||'{}');if(x&&typeof x==='object'&&!Array.isArray(x))saved=x;}catch{persist=false;}
let selected=0,step=0;
const record=id=>saved[id]&&typeof saved[id]==='object'?saved[id]:{status:'unreviewed',notes:'',step:0};
const status=id=>['reviewed','discuss'].includes(record(id).status)?record(id).status:'unreviewed';
function save(){try{localStorage.setItem(storageKey,JSON.stringify(saved));}catch{persist=false;}$('savehint').textContent=persist?'Notes stay in this browser. Export them to keep or share a copy.':'Browser storage is unavailable. Notes remain in this open page; export before closing.';}
function updateRecord(values){const id=cases[selected].id;saved[id]={...record(id),...values};save();}
function nav(){
 const count=cases.filter(c=>status(c.id)!=='unreviewed').length;
 $('progress').textContent=`${count} of ${cases.length} cases assessed`;
 $('caselist').innerHTML=['Lifetime','Mutation'].map(group=>`<h3>${group} · ${cases.filter(c=>c.group===group).length}</h3>`+cases.filter(c=>c.group===group).map(c=>`<button class="navcase" data-case="${c.id}" aria-current="${c.id===cases[selected].id}"><span class="caseid">${c.id}</span><span class="navtitle">${esc(c.title)}</span><span class="navmark ${status(c.id)}" aria-label="${status(c.id)}">${status(c.id)==='reviewed'?'✓':status(c.id)==='discuss'?'?':c.decision?'◇':'·'}</span></button>`).join('')).join('');
 $('casepicker').innerHTML=['Lifetime','Mutation'].map(group=>`<optgroup label="${group}">`+cases.filter(c=>c.group===group).map(c=>`<option value="${c.id}" ${c.id===cases[selected].id?'selected':''}>${c.id} · ${esc(c.title)}${c.decision?' ◇':''}</option>`).join('')+'</optgroup>').join('');
}
function navigate(index,newstep,scroll=false){selected=Math.max(0,Math.min(cases.length-1,index));step=Math.max(0,Math.min(cases[selected].frames.length-1,newstep??(Number(record(cases[selected].id).step)||0)));render();if(scroll)$('main').scrollIntoView({block:'start',behavior:'smooth'});}
function nodeContent(o,edges){const n=edges.filter(e=>e.to===o.id).length;return `<div class="nodekind">${esc(o.id)} · ${esc(o.kind)}</div><div class="nodelabel">${esc(o.label)}</div><div class="nodevalue">${esc(format(o.value))}</div><div class="nodefoot">${o.status==='released'?'Released in model':`${n} ownership ${n===1?'edge':'edges'}`}</div>`;}
function draw(){
 const f=cases[selected].frames[step],g=$('graph'),objects=Object.values(f.objects),edges=Object.values(f.edges),outside=edges.filter(e=>!e.from),internal=edges.filter(e=>e.from),width=g.clientWidth;
 if(!width)return;
 g.innerHTML='';
 if(width<660){
  g.style.height='auto';g.innerHTML=objects.map(o=>`<div class="node ${esc(o.kind)} ${o.status==='released'?'released':''}" style="position:relative;width:100%;margin:8px 0">${nodeContent(o,edges)}</div>`).join('')+edges.map(e=>`<div class="mobile-edge"><strong>${esc(e.from?f.objects[e.from].label+' · '+e.label:e.label)}</strong><span aria-hidden="true">→</span><small>${esc(f.objects[e.to].label)}<br>${esc(e.kind==='lease'?'owned lease':e.kind==='reach'?'stored reference':e.kind==='pending'?'active operation':'native ownership')}</small></div>`).join('')+(edges.length?'':'<p class="modelnote">No live ownership edges remain in this scenario.</p>');return;
 }
 const three=internal.length>0,columns=three?3:2,gap=three?60:120,box=Math.min(245,(width-gap*(columns-1))/columns);
 const x=[0,three?(width-box)/2:width-box,width-box];
 const groups=[[],[],[]];
 outside.forEach(e=>groups[0].push({id:'owner:'+e.id,label:e.label,kind:e.kind,owner:true,edge:e}));
 objects.forEach((o,i)=>groups[three?(i===0?1:2):1].push({...o,owner:false}));
 const heights=groups.map(list=>list.length*118),contentHeight=Math.max(235,...heights)+22;
 const bypass=three?outside.filter(e=>e.to!==objects[0].id):[];
 const height=contentHeight+(bypass.length?30+15*bypass.length:0);g.style.height=height+'px';
 const coords={};
 groups.forEach((list,col)=>list.forEach((item,i)=>{const top=(contentHeight-heights[col])/2+i*118;const el=document.createElement('div');el.className='node '+(item.owner?'owner':item.kind)+(item.status==='released'?' released':'');el.style.cssText=`left:${x[col]}px;top:${top}px;width:${box}px`;el.dataset.node=item.id;
 el.innerHTML=item.owner?`<div class="nodekind">${esc(item.kind==='lease'?'owned bridge lease':item.kind==='pending'?'active operation':item.kind==='reach'?'VM reachability':'native ownership')}</div><div class="nodelabel">${esc(item.label)}</div>`:nodeContent(item,edges);g.appendChild(el);coords[item.id]={x:x[col],y:top,w:box,h:el.offsetHeight};}));
 const paths=[];edges.forEach((e,i)=>{const a=coords[e.from||'owner:'+e.id],b=coords[e.to];if(!a||!b)return;let d,lx,ly;
 if(three&&!e.from&&b.x===x[2]){const track=contentHeight+15*bypass.findIndex(v=>v.id===e.id),sx=a.x+a.w,sy=a.y+a.h/2,tx=b.x,ty=b.y+b.h/2;d=`M ${sx} ${sy} L ${sx+18} ${sy} L ${sx+18} ${track} L ${tx-18} ${track} L ${tx-18} ${ty} L ${tx} ${ty}`;lx=(sx+tx)/2;ly=track-6;}
 else if(a.x===b.x){const side=Math.min(width-8,a.x+a.w+28+i*8),sx=a.x+a.w,sy=a.y+a.h/2,ty=b.y+b.h/2;d=`M ${sx} ${sy} C ${side} ${sy}, ${side} ${ty}, ${sx} ${ty}`;lx=side;ly=(sy+ty)/2;}
 else {const forwards=a.x<b.x,peers=edges.filter(v=>v.from===e.from&&v.to===e.to),offset=(peers.findIndex(v=>v.id===e.id)-(peers.length-1)/2)*28+(e.from?(forwards?12:-12):0),sx=forwards?a.x+a.w:a.x,tx=forwards?b.x:b.x+b.w,sy=a.y+a.h/2+offset,ty=b.y+b.h/2+offset,mid=(sx+tx)/2;d=`M ${sx} ${sy} C ${mid} ${sy}, ${mid} ${ty}, ${tx} ${ty}`;lx=mid;ly=(sy+ty)/2-7;}
 const label=e.from?e.label:'';const labelw=Math.max(24,label.length*6.4+8);
 paths.push(`<path class="connection ${esc(e.kind)}" d="${d}" marker-end="url(#arrow)"><title>${esc(e.label)} → ${esc(f.objects[e.to].label)}</title></path>`+(label?`<rect class="edge-label-bg" x="${lx-labelw/2}" y="${ly-11}" width="${labelw}" height="17" rx="3"/><text class="edge-label" x="${lx}" y="${ly+1}" text-anchor="middle">${esc(label)}</text>`:''));
 });
 const svg=document.createElementNS('http://www.w3.org/2000/svg','svg');svg.setAttribute('class','connections');svg.setAttribute('viewBox',`0 0 ${width} ${height}`);svg.setAttribute('aria-hidden','true');svg.innerHTML=`<defs><marker id="arrow" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="6" markerHeight="6" orient="auto-start-reverse"><path d="M 0 0 L 10 5 L 0 10 z" fill="var(--edge)"/></marker></defs>${paths.join('')}`;g.prepend(svg);
}
function drawCode(){
 const options=examples[cases[selected].id],language=options[preferredLanguage]?preferredLanguage:Object.keys(options)[0],example=options[language];
 $('codelanguages').innerHTML=Object.keys(options).map(lang=>`<button class="langbutton" data-language="${esc(lang)}" aria-pressed="${lang===language}">${esc(lang)}</button>`).join('');
 $('sourcecode').innerHTML=example.lines.map((line,i)=>`<span class="sourceline ${line.step===step?'active':''}" ${line.step===step?'aria-current="step"':''}><span class="lineno" aria-hidden="true">${i+1}</span><span class="sourcecontent ${/^\s*(#|\/\/)/.test(line.text)?'sourcecomment':''}">${esc(line.text)||' '}</span></span>`).join('');
 $('codecaption').textContent=example.note+' The highlighted block corresponds to this diagram step.';
 const first=$('sourcecode').querySelector('.active');if(first)$('sourcecode').scrollTop=Math.max(0,first.offsetTop-$('sourcecode').offsetTop-35);
}
$('codelanguages').addEventListener('click',e=>{const b=e.target.closest('[data-language]');if(b){preferredLanguage=b.dataset.language;drawCode();}});
function render(){const c=cases[selected],f=c.frames[step];
 $('casekicker').innerHTML=`<span>${c.id} / ${c.group}</span><span class="badge">Design model</span>${c.decision?'<span class="badge warning">Policy to review</span>':''}${c.limit?'<span class="badge warning">Retention limit</span>':''}`;
 $('casetitle').textContent=c.title;$('summary').textContent=c.summary;$('evidence').textContent=c.evidence.label+' ↗';$('evidence').href=c.evidence.href;
 $('position').textContent=`Step ${step+1} of ${c.frames.length}`;$('back').disabled=step===0;$('next').disabled=step===c.frames.length-1;
 $('stepdots').innerHTML=c.frames.map((v,i)=>`<button class="stepdot ${i===step?'current':i<step?'done':''}" data-step="${i}" aria-label="Step ${i+1}: ${esc(v.title)}" ${i===step?'aria-current="step"':''}></button>`).join('');
 $('stagetitle').textContent=f.title;$('detail').textContent=f.detail;
 $('scope').innerHTML=`Origin scope: <strong>${f.scope==='open'?'open':f.scope==='closed'?'closed':'closing / operations revoked'}</strong>`;
 $('ledger').innerHTML=`<strong>${Object.keys(f.edges).length}</strong> active ownership ${Object.keys(f.edges).length===1?'edge':'edges'}`;
 $('locals').innerHTML=Object.entries(f.vars).map(([key,value])=>`<div class="local"><label>${esc(key)}</label><code>${esc(format(value))}</code></div>`).join('');
 $('warning').hidden=!f.warning;$('warning').textContent=f.warning;drawCode();
 $('checks').innerHTML=f.checks.map(check=>`<li><span class="tick">${check.pass?'✓':'✕'}</span><span>${esc(check.label)}</span></li>`).join('');$('gate').textContent=c.gate;
 $('reviewstatus').value=status(c.id);$('notes').value=typeof record(c.id).notes==='string'?record(c.id).notes:'';
 $('previouscase').disabled=selected===0;$('nextcase').disabled=selected===cases.length-1;
 updateRecord({step});try{history.replaceState(null,'',`#${c.id}/${step+1}`);}catch{}nav();draw();
}
$('caselist').addEventListener('click',e=>{const b=e.target.closest('[data-case]');if(b)navigate(cases.findIndex(c=>c.id===b.dataset.case),undefined,false);});
$('casepicker').addEventListener('change',e=>navigate(cases.findIndex(c=>c.id===e.target.value),undefined,false));
$('stepdots').addEventListener('click',e=>{const b=e.target.closest('[data-step]');if(b)navigate(selected,Number(b.dataset.step));});
$('back').addEventListener('click',()=>navigate(selected,step-1));$('next').addEventListener('click',()=>navigate(selected,step+1));
$('previouscase').addEventListener('click',()=>navigate(selected-1,undefined,true));$('nextcase').addEventListener('click',()=>navigate(selected+1,undefined,true));
$('reviewstatus').addEventListener('change',e=>{updateRecord({status:e.target.value});nav();});$('notes').addEventListener('input',e=>updateRecord({notes:e.target.value}));
$('overview').addEventListener('click',()=>{$('about').open=true;$('about').scrollIntoView({behavior:'smooth',block:'start'});});
$('export').addEventListener('click',()=>{const data={artifact:'BAML interface lifetime and mutation review',schema:1,exportedAt:new Date().toISOString(),scope:'Design assessments; not runtime test results',cases:cases.map(c=>({id:c.id,title:c.title,assessment:status(c.id),notes:record(c.id).notes||'',lastStep:Number(record(c.id).step||0)+1,policyToReview:!!c.decision}))};const json=JSON.stringify(data,null,2);if($('downloadjson').href.startsWith('blob:'))URL.revokeObjectURL($('downloadjson').href);$('downloadjson').href=URL.createObjectURL(new Blob([json],{type:'application/json'}));$('reviewjson').value=json;$('exportpanel').hidden=false;$('exportpanel').scrollIntoView({behavior:'smooth',block:'start'});});
$('selectjson').addEventListener('click',()=>{$('reviewjson').focus();$('reviewjson').select();});
$('closeexport').addEventListener('click',()=>{$('exportpanel').hidden=true;$('main').scrollIntoView({behavior:'smooth',block:'start'});});
function fromHash(){const match=location.hash.match(/^#([LM]\d{2})(?:\/(\d+))?$/);if(match){const index=cases.findIndex(c=>c.id===match[1]);if(index>=0){navigate(index,match[2]?Number(match[2])-1:undefined);return;}}navigate(0,0);}
window.addEventListener('hashchange',fromHash);let lastWidth=0;new ResizeObserver(entries=>{const w=entries[0].contentRect.width;if(Math.abs(w-lastWidth)>.5){lastWidth=w;draw();}}).observe($('graph'));
fromHash();
})();

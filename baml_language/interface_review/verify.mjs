import assert from 'node:assert/strict';
import { readFileSync, existsSync } from 'node:fs';
import { createRequire } from 'node:module';
import vm from 'node:vm';
const require=createRequire(import.meta.url);
const {Model,cases}=require('./cases.js');
const examples=require('./code.js');
const dir=new URL('./',import.meta.url);
let frames=0,checks=0;
assert.equal(new Set(cases.map(c=>c.id)).size,cases.length);
const expectedFinalEdges={L01:0,L02:0,L03:0,L04:0,L05:0,L06:0,L07:0,L08:0,L09:0,L10:0,L11:0,L12:0,L13:1,L14:0,L15:0,L16:0,L17:0,L18:0,L19:1,L20:0,M01:1,M02:1,M03:0,M04:0,M05:0,M06:0,M07:1,M08:1,M09:1,M10:0,M11:0,M12:2,M13:0};
for(const c of cases){
 assert.ok(c.frames.length>=3,c.id);
 assert.ok(examples[c.id],c.id+' missing code');
 for(const example of Object.values(examples[c.id])) for(let i=0;i<c.frames.length;i++) assert.ok(example.lines.some(l=>l.step===i),c.id+' missing highlighted code at step '+i);
 assert.ok(existsSync(new URL(c.evidence.href,dir)),c.id+' missing evidence file');
 for(const f of c.frames){
  frames++;
  for(const check of f.checks){checks++;assert.ok(check.pass,`${c.id}/${f.title}: ${check.label}`);}
  assert.equal(f.allocations-f.releases,Object.keys(f.edges).length);
  for(const e of Object.values(f.edges)){
   assert.equal(f.objects[e.to].status,'alive');
   if(e.from)assert.equal(f.objects[e.from].status,'alive');
  }
  if(f.scope==='closed')assert.ok(Object.values(f.edges).every(e=>e.kind!=='pending'),c.id+' closed with active work');
 }
 assert.equal(Object.keys(c.frames.at(-1).edges).length,expectedFinalEdges[c.id],c.id+' final ownership');
}
const frame=(id,index)=>cases.find(c=>c.id===id).frames[index];
assert.equal(frame('L11',1).objects.H.status,'alive','outside-unreachable cross-runtime cycle must remain');
assert.equal(frame('L19',2).scope,'closing','hung worker cannot silently close scope');
assert.equal(frame('M04',2).objects.B.value[0],99,'old alias must not mutate replacement');
assert.deepEqual(frame('M05',2).objects.B.value,[7,8],'host and VM must retain replacement identity');
assert.equal(frame('M08',3).objects.P.value.count,5,'cancellation must not undo committed mutation');
assert.equal(frame('M07',3).objects.P.value.count,1,'lost update must remain visible');
assert.equal(frame('L09',3).scope,'closed');
const m=new Model();m.object('x','x').own('a','a','x');assert.throws(()=>m.own('a','duplicate','x'));
m.drop('a').drop('a').collect();assert.equal(m.releases,1);assert.throws(()=>m.own('z','z','x'));
const busy=new Model();busy.object('x','x').own('work','work','x','pending').revoke();assert.throws(()=>busy.closeScope());
const html=readFileSync(new URL('index.html',dir),'utf8');
const expected=readFileSync(new URL('template.html',dir),'utf8').replace('/* CASE_DATA */',()=>readFileSync(new URL('cases.js',dir),'utf8')).replace('/* EXAMPLE_CODE */',()=>readFileSync(new URL('code.js',dir),'utf8')).replace('/* APP_CODE */',()=>readFileSync(new URL('app.js',dir),'utf8'));
assert.equal(html,expected,'rebuild the standalone HTML after changing source');
assert.ok(Buffer.byteLength(html)<1_000_000);
const scripts=[...html.matchAll(/<script>\s*([\s\S]*?)<\/script>/g)].map(m=>m[1]);
assert.equal(scripts.length,3);scripts.forEach(s=>new vm.Script(s));
assert.ok(!/<script[^>]+src=|<link[^>]+href=["']https?:|\bfetch\s*\(|\bXMLHttpRequest\b|\bWebSocket\b/.test(html),'artifact must stay standalone and offline');
for(const id of [...readFileSync(new URL('app.js',dir),'utf8').matchAll(/\$\('([^']+)'\)/g)].map(x=>x[1]))assert.ok(html.includes(`id="${id}"`),'missing element '+id);
console.log(`${cases.length} cases · ${frames} frames · ${checks} expected-state checks passed.`);
console.log('Also checked ownership failure guards, terminal states, source links, script syntax, offline bundle and generated-file freshness. These are model checks, not BAML ABI tests.');

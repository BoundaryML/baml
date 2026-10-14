// Same fixtures, operations, checksums, warm-up and sampling as node_compare.rs.
// BMP uses native JS indexing. Astral text uses an auxiliary UTF-16 offset
// index to implement BAML's codepoint semantics; it never splits a surrogate.
const N = 20000;
const verify = process.argv.includes('--verify');
const filter = process.argv.find(a => a.startsWith('--filter='))?.slice(9) ?? '';
let sink = 0;
const unit = kind => ({ascii:'abcd fghij',bmp:'abcd éghij',astral:'abcd 😀ghij'})[kind];
function fixture(kind,repr) {
  const text = unit(kind).repeat(N/10);
  if (repr === 'flat') return text;
  if (repr === 'slice') return ('前é🙂xyz'.repeat(17)+text+'tail').slice(119,119+text.length);
  let s = '';
  const part=unit(kind);
  for (let i=0;i<N/10;i++) s += part;
  return s;
}
function offsets(s) {
  const out = new Uint32Array(s.length+1);
  let i=0, b=0;
  for (const c of s) { out[i++]=b; b+=c.length; }
  out[i]=b;
  return out.subarray(0,i+1);
}
function prepare(s,kind) { return {s,idx:kind === 'astral' ? offsets(s) : null}; }
function charAt(f,i) { return f.idx ? f.s.slice(f.idx[i],f.idx[i+1]) : f.s.at(i); }
function sliceAt(f,i) { return f.idx ? f.s.slice(f.idx[i],f.idx[i+1]) : f.s.slice(i,i+1); }
function codepoint(f,i) { return f.s.codePointAt(f.idx ? f.idx[i] : i); }
function findFrom(f,pos) {
  const b=f.s.indexOf('a',f.idx ? f.idx[pos] : pos);
  if (b<0 || !f.idx) return b;
  let lo=0, hi=f.idx.length;
  while(lo<hi) {const m=(lo+hi)>>>1; if(f.idx[m]<b) lo=m+1; else hi=m;}
  return lo;
}
function run(f,op) {
  let out=0;
  // Keep native ASCII/BMP loops free of the codepoint adapter. Dispatch once,
  // as in the Rust match, rather than comparing operation names per index.
  if(!f.idx) {
    const s=f.s;
    if(op==='at') {for(let i=0;i<N;i++) out += s.at(i)==='a';}
    else if(op==='slice') {for(let i=0;i<N;i++) out += s.slice(i,i+1)==='a';}
    else if(op==='codepoint') {for(let i=0;i<N;i++) out += s.codePointAt(i);}
    else if(op==='random') {for(let i=0;i<N;i++) out += s.codePointAt((i*7919)%N);}
    else {let i,pos=0;while((i=s.indexOf('a',pos))!==-1) pos=i+1;out=pos;}
  } else {
    if(op==='at' || op==='slice') {for(let i=0;i<N;i++) out += sliceAt(f,i)==='a';}
    else if(op==='codepoint') {for(let i=0;i<N;i++) out += codepoint(f,i);}
    else if(op==='random') {for(let i=0;i<N;i++) out += codepoint(f,(i*7919)%N);}
    else {let i,pos=0;while((i=findFrom(f,pos))!==-1) pos=i+1;out=pos;}
  }
  return out;
}

function append(read,kind) {
  let s='',hits=0;
  const part=unit(kind);
  for(let i=0;i<N/10;i++) {
    s += part;
    // The last codepoint is ASCII in every fixture, so this native tail read
    // is equivalent and needs no index construction.
    if(read) hits += s.at(-1)==='j';
  }
  // Consume the constructed string's length. Each astral fixture unit has
  // one extra UTF-16 code unit, so this is its equivalent codepoint count.
  return hits+s.length-(kind==='astral' ? N/10 : 0);
}
function replaceReads(kind) {
  let s=unit(kind).repeat(N/10), hits=0;
  for(let round=0;round<50;round++) {
    s=s.replaceAll(round%2===0 ? 'gh':'GH',round%2===0 ? 'GH':'gh');
    const f=prepare(s,kind);
    for(let r=0;r<100;r++) hits += charAt(f,(r*7919+round)%N)==='a';
  }
  return hits;
}
function bench(name,fn) {
  if(!name.includes(filter)) return;
  const result=fn();
  if(verify) {console.log(JSON.stringify({bench:name,result}));return;}
  let iters=1;
  for(;;) {const t=performance.now();for(let i=0;i<iters;i++) sink^=fn();if(performance.now()-t>=5 || iters>=1<<20) break;iters*=2;}
  const warm=performance.now();
  while(performance.now()-warm<200) {for(let i=0;i<iters;i++) sink^=fn();}
  const times=[];
  for(let sample=0;sample<15;sample++) {const t=performance.now();for(let i=0;i<iters;i++) sink^=fn();times.push((performance.now()-t)*1e6/iters);}
  times.sort((a,b)=>a-b);
  console.log(JSON.stringify({bench:name,n:N,median_ns:times[7],min_ns:times[0],result}));
}
for(const kind of ['ascii','bmp','astral']) {
  for(const repr of ['flat','slice','rope','rope_shared']) {
    const f=prepare(fixture(kind,repr),kind);
    for(const op of ['at','slice','codepoint','random','find_from']) bench(`${op}_${kind}_${repr}`,()=>run(f,op));
  }
  bench(`append_tail_${kind}`,()=>append(true,kind));
  bench(`append_tail_shared_${kind}`,()=>append(true,kind));
  bench(`append_only_${kind}`,()=>append(false,kind));
  bench(`build_then_read_${kind}`,()=>run(prepare(fixture(kind,'rope'),kind),'at'));
  bench(`build_then_read_shared_${kind}`,()=>run(prepare(fixture(kind,'rope_shared'),kind),'at'));
  bench(`replace_and_read_${kind}`,()=>replaceReads(kind));
  bench(`cold_random_${kind}`,()=>{const f=prepare(unit(kind).repeat(N/10),kind);let out=0;for(let i=0;i<100;i++) out+=codepoint(f,(i*7919)%N);return out;});
}
if(sink===0.5) console.log(sink);

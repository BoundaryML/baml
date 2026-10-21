function md(a,b){return b===0?0:Math.trunc(a/b)}
function main() {
  let s=0;
  for(let i=0;i<1000000;i++)s+=md(i,i+1);
  return s;
}

// JavaScript has no lock; this is a minimal re-entrant one for one owner.
class Lock {
  constructor(){this.owner=0;this.depth=0}
  with(fn){
    if(this.owner!==1){if(this.owner!==0)throw new Error("held");this.owner=1}
    this.depth++;
    try{return fn()}finally{if(--this.depth===0)this.owner=0}
  }
}
function main() {
  const lock=new Lock();
  const acc={n:0};
  const add=()=>{acc.n+=2};
  for(let i=0;i<1000000;i++)lock.with(add);
  return acc.n;
}

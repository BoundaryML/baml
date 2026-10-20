function leaf(n){return n}
function main() {
  let s=0;
  for(let i=0;i<1000000;i++)s+=leaf(i);
  return s;
}

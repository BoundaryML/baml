function apply(f,x){return f(x)}
function main() {
  const double=x=>x*2;
  let s=0;
  for(let i=0;i<1000000;i++)s+=apply(double,i);
  return s;
}

function collatz(n){let s=0,x=n;while(x!==1){x=x%2===0?x/2:3*x+1;s++}return s}
function main() {
  let s=0;
  for(let i=1;i<100000;i++)s+=collatz(i);
  return s;
}

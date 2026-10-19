function fib(n){let a=0,b=1;for(let i=0;i<n;i++){let t=b;b=a+b;a=t}return a}
function main() {
  let s=0;
  for(let i=0;i<100000;i++)s+=fib(50);
  return s;
}

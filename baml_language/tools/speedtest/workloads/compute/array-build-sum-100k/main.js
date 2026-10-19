function main() {
  const a=[];
  for(let i=0;i<100000;i++)a.push(i);
  let s=0;
  for(const x of a)s+=x;
  return s;
}

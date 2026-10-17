function main() {
  const grid=[];
  for(let i=0;i<1000;i++){const row=[];for(let j=0;j<100;j++)row.push(i*100+j);grid.push(row)}
  let s=0;
  for(const row of grid)for(const v of row)s+=v;
  return s;
}

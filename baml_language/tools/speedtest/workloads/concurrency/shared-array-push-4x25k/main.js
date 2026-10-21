function pushN(arr, start, n){
  for(let i=0;i<n;i++) arr.push(start + i);
}
function main() {
  const arr = [];
  pushN(arr, 0, 25000);
  pushN(arr, 25000, 25000);
  pushN(arr, 50000, 25000);
  pushN(arr, 75000, 25000);
  return arr.length;
}

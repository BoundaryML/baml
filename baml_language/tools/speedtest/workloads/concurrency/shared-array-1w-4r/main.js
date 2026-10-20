function writeN(arr,n){ for(let i=0;i<n;i++) arr.push(i); }
function readN(arr,n){
  let acc=0;
  for(let i=0;i<n;i++){
    if(arr.length>0) acc += arr[0];
  }
  return acc;
}
function main() {
  const arr = [0];
  writeN(arr, 50000);
  readN(arr, 12500);
  readN(arr, 12500);
  readN(arr, 12500);
  readN(arr, 12500);
  return arr.length;
}

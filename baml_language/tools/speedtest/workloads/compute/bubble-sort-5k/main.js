function main() {
  const arr=[];
  for(let i=0;i<5000;i++)arr.push(5000-i);
  const n=arr.length;
  for(let i=0;i<n;i++)for(let j=0;j<n-i-1;j++)if(arr[j]>arr[j+1]){const t=arr[j];arr[j]=arr[j+1];arr[j+1]=t}
  return arr[0]+arr[4999];
}

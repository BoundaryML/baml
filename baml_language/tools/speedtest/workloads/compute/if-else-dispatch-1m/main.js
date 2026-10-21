function classify(x){const r=x%5;if(r===0)return 1;if(r===1)return 2;if(r===2)return 3;if(r===3)return 4;return 5}
function main() {
  let s=0;
  for(let i=0;i<1000000;i++)s+=classify(i);
  return s;
}

class Point{constructor(x,y){this.x=x;this.y=y}}
function main() {
  let sum=0;
  let s="";
  const arr=[];
  for(let i=0;i<5000;i++){
    sum+=i*3-1;
    s=s+"x";
    arr.push(i);
    const p=new Point(i,i+1);
    sum+=p.x+p.y;
    if(i>2500)sum+=1;
  }
  return sum+s.length+arr.length;
}

class Point{constructor(x,y){this.x=x;this.y=y}}
function main() {
  let s=0;
  for(let i=0;i<100000;i++){const p=new Point(i,i*2);s+=p.x+p.y}return s;
}

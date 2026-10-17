class Vec{constructor(x,y){this.x=x;this.y=y}norm2(){return this.x*this.x+this.y*this.y}}
function main() {
  let acc=0;
  for(let i=0;i<100000;i++){const v=new Vec(i,i+1);acc+=v.norm2()}return acc;
}

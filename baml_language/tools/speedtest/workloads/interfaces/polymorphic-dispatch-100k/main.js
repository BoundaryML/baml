class Square{constructor(side){this.side=side}area(){return this.side*this.side}}
class Rect{constructor(w,h){this.w=w;this.h=h}area(){return this.w*this.h}}
function pick(i){return i%2===0?new Square(i):new Rect(i,3)}
function main() {
  let acc=0;
  for(let i=0;i<100000;i++){acc+=pick(i).area()}return acc;
}

class Inc{constructor(base){this.base=base}step(){return 1}total(){return this.base+this.step()}}
class Dbl{constructor(base){this.base=base}step(){return this.base}total(){return this.base+this.step()}}
function pick(i){return i%2===0?new Inc(i):new Dbl(i)}
function main() {
  let acc=0;
  for(let i=0;i<100000;i++){acc+=pick(i).total()}return acc;
}

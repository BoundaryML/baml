class Dog{noise(){return 3}}
class Cat{noise(){return 5}}
function pick(i){return i%2===0?new Dog():new Cat()}
function code(a){if(a instanceof Dog){return a.noise()}else if(a instanceof Cat){return a.noise()}else{return 0}}
function main() {
  let acc=0;
  for(let i=0;i<100000;i++){acc+=code(pick(i))}return acc;
}

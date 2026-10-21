class Inner{constructor(v){this.value=v}}
class Middle{constructor(i){this.inner=i}}
class Outer{constructor(m){this.middle=m}}
function main() {
  let s=0;
  for(let i=0;i<500000;i++){const o=new Outer(new Middle(new Inner(i)));s+=o.middle.inner.value}return s;
}

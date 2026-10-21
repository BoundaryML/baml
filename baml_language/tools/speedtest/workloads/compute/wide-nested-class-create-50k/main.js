class Leaf{constructor(a,b,c,d){this.a=a;this.b=b;this.c=c;this.d=d}}
class Node{constructor(left,right,n0,n1,n2,n3){this.left=left;this.right=right;this.n0=n0;this.n1=n1;this.n2=n2;this.n3=n3}}
class BigRecord{constructor(id,a0,a1,a2,a3,a4,a5,a6,a7,primary,secondary,tail){this.id=id;this.a0=a0;this.a1=a1;this.a2=a2;this.a3=a3;this.a4=a4;this.a5=a5;this.a6=a6;this.a7=a7;this.primary=primary;this.secondary=secondary;this.tail=tail}}
function main() {
  let s=0;
  for(let i=0;i<50000;i++){
    const r=new BigRecord(i,i+1,i+2,i+3,i+4,i+5,i+6,i+7,i+8,
      new Node(new Leaf(i,i+1,i+2,i+3),new Leaf(i+4,i+5,i+6,i+7),i+8,i+9,i+10,i+11),
      new Node(new Leaf(i+12,i+13,i+14,i+15),new Leaf(i+16,i+17,i+18,i+19),i+20,i+21,i+22,i+23),
      new Leaf(i+24,i+25,i+26,i+27));
    s+=r.id+r.a7+r.primary.left.c+r.primary.right.d+r.secondary.n3+r.secondary.right.b+r.tail.d;
  }
  return s;
}

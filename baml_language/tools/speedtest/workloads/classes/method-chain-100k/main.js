class Account{constructor(balance){this.balance=balance}fee(){return Math.floor(this.balance/100)}net(){return this.balance-this.fee()}}
function main() {
  let acc=0;
  for(let i=0;i<100000;i++){const a=new Account(i);acc+=a.net()}return acc;
}

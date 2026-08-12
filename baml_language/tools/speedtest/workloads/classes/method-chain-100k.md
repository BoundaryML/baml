# classes::method chain 100k

## BAML
```baml
class Account {
  balance: int
  function fee(self) -> int {
    return self.balance / 100
  }
  function net(self) -> int {
    return self.balance - self.fee()
  }
}
function main() -> int {
  let acc = 0;
  for (let i = 0; i < 100000; i += 1) {
    let a = Account { balance: i };
    acc += a.net();
  };
  return acc;
}
```

## Python
```python
class Account:
    __slots__ = ('balance',)
    def __init__(self, balance): self.balance = balance
    def fee(self): return self.balance // 100
    def net(self): return self.balance - self.fee()
acc = 0
for i in range(100000):
    a = Account(i); acc += a.net()
print(acc)
```

## Typescript
```ts
class Account{constructor(balance){this.balance=balance}fee(){return Math.floor(this.balance/100)}net(){return this.balance-this.fee()}}
let acc=0;for(let i=0;i<100000;i++){const a=new Account(i);acc+=a.net()}console.log(acc)
```

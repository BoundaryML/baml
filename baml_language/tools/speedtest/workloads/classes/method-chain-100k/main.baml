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

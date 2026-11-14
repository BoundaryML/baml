package main

type Account struct {
	balance int
}

func (a *Account) fee() int {
	return a.balance / 100
}

func (a *Account) net() int {
	return a.balance - a.fee()
}

func Main() int {
	acc := 0
	for i := 0; i < 100000; i++ {
		a := &Account{balance: i}
		acc += a.net()
	}
	return acc
}

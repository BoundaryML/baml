package main

import (
	"encoding/json"
	"strconv"
	"strings"
)

type Account struct {
	ID      string   `json:"id"`
	Owner   string   `json:"owner"`
	Balance int      `json:"balance"`
	Tags    []string `json:"tags"`
}

// Pointer fields tell a missing field from a zero value, as a typed decode does.
type createAccountBody struct {
	Owner   *string   `json:"owner"`
	Balance *int      `json:"balance"`
	Tags    *[]string `json:"tags"`
}

type transferBody struct {
	Source *string `json:"source"`
	Target *string `json:"target"`
	Amount *int    `json:"amount"`
}

type Created struct {
	ID      string `json:"id"`
	Balance int    `json:"balance"`
}

type Balance struct {
	Balance int `json:"balance"`
}

type Moved struct {
	SourceBalance int `json:"source_balance"`
	TargetBalance int `json:"target_balance"`
}

type ApiError struct {
	Error string `json:"error"`
}

type Response struct {
	status int
	body   string
}

type Service struct {
	accounts map[string]*Account
	nextID   int
}

func stringify(value any) string {
	out, _ := json.Marshal(value)
	return string(out)
}

func fail(status int, message string) Response {
	return Response{status: status, body: stringify(ApiError{Error: message})}
}

func (s *Service) createAccount(input createAccountBody) Response {
	s.nextID++
	id := "acc" + strconv.Itoa(s.nextID)
	s.accounts[id] = &Account{ID: id, Owner: *input.Owner, Balance: *input.Balance, Tags: *input.Tags}
	return Response{status: 201, body: stringify(Created{ID: id, Balance: *input.Balance})}
}

func showAccount(account *Account, balanceOnly bool) Response {
	if balanceOnly {
		return Response{status: 200, body: stringify(Balance{Balance: account.Balance})}
	}
	return Response{status: 200, body: stringify(account)}
}

func moveFunds(source, target *Account, amount int) Response {
	if amount <= 0 {
		return fail(422, "amount must be positive")
	}
	if source.Balance < amount {
		return fail(409, "insufficient funds")
	}
	source.Balance -= amount
	target.Balance += amount
	return Response{status: 200, body: stringify(Moved{SourceBalance: source.Balance, TargetBalance: target.Balance})}
}

func (s *Service) transfer(input transferBody) Response {
	source, okSource := s.accounts[*input.Source]
	target, okTarget := s.accounts[*input.Target]
	if !okSource || !okTarget {
		return fail(404, "no such account")
	}
	return moveFunds(source, target, *input.Amount)
}

func (s *Service) handle(raw string) Response {
	lines := strings.Split(raw, "\n")
	head := strings.Split(lines[0], " ")
	if len(head) != 2 {
		return fail(400, "bad request line")
	}
	method := head[0]
	target := strings.Split(head[1], "?")
	segments := strings.Split(target[0], "/")
	resource := segments[1]
	switch {
	case resource == "accounts" && method == "POST" && len(segments) == 2 && len(lines) == 2:
		var input createAccountBody
		if err := json.Unmarshal([]byte(lines[1]), &input); err != nil || input.Owner == nil || input.Balance == nil || input.Tags == nil {
			return fail(400, "invalid body")
		}
		return s.createAccount(input)
	case resource == "accounts" && method == "GET" && len(segments) == 3:
		account, ok := s.accounts[segments[2]]
		if !ok {
			return fail(404, "no such account")
		}
		return showAccount(account, len(target) == 2 && target[1] == "fields=balance")
	case resource == "accounts":
		return fail(405, "method not allowed")
	case resource == "transfer" && method == "POST" && len(lines) == 2:
		var input transferBody
		if err := json.Unmarshal([]byte(lines[1]), &input); err != nil || input.Source == nil || input.Target == nil || input.Amount == nil {
			return fail(400, "invalid body")
		}
		return s.transfer(input)
	}
	return fail(404, "no such route")
}

type Rng struct {
	seed int
}

func (r *Rng) draw(n int) int {
	r.seed = r.seed * 16807 % 2147483647
	return r.seed % n
}

func Setup() []string {
	rng := &Rng{seed: 29}
	owners := []string{"ada", "grace", "linus", "barbara", "ken", "margaret", "dennis", "frances"}
	requests := []string{}
	made := 0
	for i := 0; i < 2000; i++ {
		kind := rng.draw(10)
		if kind < 4 {
			owner := owners[rng.draw(8)]
			balance := rng.draw(5000)
			first := rng.draw(5)
			second := rng.draw(5)
			requests = append(requests, "POST /accounts\n{\"owner\": \""+owner+"\", \"balance\": "+strconv.Itoa(balance)+", \"tags\": [\"t"+strconv.Itoa(first)+"\", \"t"+strconv.Itoa(second)+"\"]}")
			made++
		} else if kind < 7 {
			id := 1 + rng.draw(made+3)
			line := "GET /accounts/acc" + strconv.Itoa(id)
			if rng.draw(2) == 1 {
				line = line + "?fields=balance"
			}
			requests = append(requests, line)
		} else if kind < 9 {
			source := 1 + rng.draw(made+1)
			target := 1 + rng.draw(made+1)
			amount := rng.draw(3000)
			requests = append(requests, "POST /transfer\n{\"source\": \"acc"+strconv.Itoa(source)+"\", \"target\": \"acc"+strconv.Itoa(target)+"\", \"amount\": "+strconv.Itoa(amount)+"}")
		} else {
			switch rng.draw(4) {
			case 0:
				requests = append(requests, "POST /accounts\n{\"owner\": \"x\"")
			case 1:
				requests = append(requests, "GET /nothing")
			case 2:
				requests = append(requests, "DELETE /accounts")
			default:
				requests = append(requests, "POST /transfer\n{\"source\": \"acc1\"}")
			}
		}
	}
	return requests
}

func Run(requests []string) int {
	service := &Service{accounts: map[string]*Account{}}
	check := 0
	for _, raw := range requests {
		response := service.handle(raw)
		check = (check*31 + response.status + len(response.body)) % 1000000007
	}
	total := 0
	for _, account := range service.accounts {
		total += account.Balance
	}
	return (check + total + service.nextID) % 1000000007
}

func Main() int {
	return Run(Setup())
}

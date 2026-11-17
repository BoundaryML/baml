package main

import (
	"encoding/json"
	"math"
	"slices"
	"strconv"
	"strings"
)

type Row struct {
	Account     string      `json:"account"`
	AmountCents json.Number `json:"amount_cents"`
	Active      bool        `json:"active"`
}

type Total struct {
	Account    string `json:"account"`
	TotalCents int    `json:"total_cents"`
}

type Invalid struct {
	message string
}

func (e *Invalid) Error() string {
	return e.message
}

func rowsJSON(n int) string {
	seed := 11
	parts := []string{}
	for i := 0; i < n; i++ {
		seed = seed * 16807 % 2147483647
		account := strconv.Itoa(seed % 97)
		for len(account) < 3 {
			account = "0" + account
		}
		amount := strconv.Itoa(seed % 100000)
		if i%5 == 0 {
			amount = amount + ".0"
		}
		active := "false"
		if seed%7 != 0 {
			active = "true"
		}
		parts = append(parts, "{\"account\": \"acct-"+account+"\", \"amount_cents\": "+amount+", \"active\": "+active+"}")
	}
	return "[" + strings.Join(parts, ", ") + "]"
}

func isASCII(s string) bool {
	for i := 0; i < len(s); i++ {
		if s[i] >= 0x80 {
			return false
		}
	}
	return true
}

func aggregate(raw string) (string, error) {
	var rows []Row
	if err := json.Unmarshal([]byte(raw), &rows); err != nil {
		return "", err
	}
	totals := map[string]int{}
	for _, row := range rows {
		if !isASCII(row.Account) {
			return "", &Invalid{message: "non-ASCII account"}
		}
		var amount int
		if n, err := row.AmountCents.Int64(); err == nil {
			amount = int(n)
		} else {
			f, err := row.AmountCents.Float64()
			if err != nil {
				return "", err
			}
			if f != math.Floor(f) || f < 0.0 || f > 1000000.0 {
				return "", &Invalid{message: "invalid amount"}
			}
			amount = int(f)
		}
		if amount < 0 || amount > 1000000 {
			return "", &Invalid{message: "invalid amount"}
		}
		if row.Active {
			totals[row.Account] += amount
		}
	}
	accounts := []string{}
	for account := range totals {
		accounts = append(accounts, account)
	}
	slices.Sort(accounts)
	output := []Total{}
	for _, account := range accounts {
		output = append(output, Total{Account: account, TotalCents: totals[account]})
	}
	out, err := json.Marshal(output)
	if err != nil {
		return "", err
	}
	return string(out), nil
}

func Main() int {
	raw := rowsJSON(2048)
	check := 0
	for i := 0; i < 20; i++ {
		out, err := aggregate(raw)
		if err != nil {
			out = "invalid"
		}
		h := 0
		for _, c := range out {
			h = (h*31 + int(c)) % 1000000007
		}
		check = (check + h + len(out)) % 1000000007
	}
	return check
}

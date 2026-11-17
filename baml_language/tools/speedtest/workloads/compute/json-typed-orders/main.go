package main

import (
	"encoding/json"
	"fmt"
	"strconv"
	"strings"
)

type OrderStatus int

const (
	Pending OrderStatus = iota
	Shipped
	Delivered
)

func (s *OrderStatus) UnmarshalText(text []byte) error {
	switch string(text) {
	case "Pending":
		*s = Pending
	case "Shipped":
		*s = Shipped
	case "Delivered":
		*s = Delivered
	default:
		return fmt.Errorf("unknown OrderStatus %q", text)
	}
	return nil
}

type Address struct {
	Street string `json:"street"`
	City   string `json:"city"`
	Zip    string `json:"zip"`
}

type Customer struct {
	Name    string  `json:"name"`
	Email   string  `json:"email"`
	Address Address `json:"address"`
}

type LineItem struct {
	Sku        string `json:"sku"`
	Qty        int    `json:"qty"`
	PriceCents int    `json:"price_cents"`
}

type Order struct {
	ID       int         `json:"id"`
	Customer Customer    `json:"customer"`
	Shipping Address     `json:"shipping"`
	Status   OrderStatus `json:"status"`
	Items    []LineItem  `json:"items"`
}

type Rng struct {
	seed int
}

func draw(rng *Rng, n int) int {
	rng.seed = rng.seed * 16807 % 2147483647
	return rng.seed % n
}

func address(rng *Rng) string {
	cities := []string{"Lisbon", "Oslo", "Lima", "Kyoto", "Austin"}
	return "{\"street\":\"" + strconv.Itoa(1+draw(rng, 999)) + " Main St\",\"city\":\"" + cities[draw(rng, 5)] +
		"\",\"zip\":\"" + strconv.Itoa(10000+draw(rng, 89999)) + "\"}"
}

func Setup() []string {
	rng := &Rng{seed: 41}
	names := []string{"ada", "bob", "cy", "dee", "eve", "fay"}
	statuses := []string{"Pending", "Shipped", "Delivered"}
	orders := []string{}
	for i := 0; i < 1000; i++ {
		name := names[draw(rng, 6)]
		items := []string{}
		count := 1 + draw(rng, 3)
		for k := 0; k < count; k++ {
			items = append(items, "{\"sku\":\"SKU"+strconv.Itoa(draw(rng, 500))+"\",\"qty\":"+strconv.Itoa(1+draw(rng, 4))+
				",\"price_cents\":"+strconv.Itoa(99+draw(rng, 5000))+"}")
		}
		order := "{\"id\":" + strconv.Itoa(i) +
			",\"customer\":{\"name\":\"" + name + "\",\"email\":\"" + name + "@example.com\",\"address\":" + address(rng) + "}" +
			",\"shipping\":" + address(rng) +
			",\"status\":\"" + statuses[draw(rng, 3)] + "\"" +
			",\"items\":[" + strings.Join(items, ",") + "]}"
		orders = append(orders, order)
	}
	return orders
}

func Run(orders []string) int {
	check := 0
	for _, raw := range orders {
		var o Order
		if err := json.Unmarshal([]byte(raw), &o); err != nil {
			panic(err)
		}
		h := o.ID + len(o.Customer.Name) + len(o.Customer.Address.City) + len(o.Shipping.Zip)
		switch o.Status {
		case Pending:
			h += 1
		case Shipped:
			h += 2
		case Delivered:
			h += 3
		}
		for _, item := range o.Items {
			h += item.Qty*item.PriceCents + len(item.Sku)
		}
		check = (check + h) % 1000000007
	}
	return check
}

func Main() int {
	return Run(Setup())
}

package main

import (
	"encoding/json"
	"errors"
	"fmt"
	"math"
	"strconv"
	"strings"
)

type Address struct {
	Street string
	City   string
	Zip    string
}

type Order struct {
	Sku   string
	Qty   int
	Price float64
}

type User struct {
	ID        int
	Name      string
	Email     string
	Age       int
	Active    bool
	Score     float64
	Tags      []string
	Address   Address
	Orders    []Order
	ManagerID *int
}

type Tree struct {
	doc   any
	users []any
}

type Rng struct {
	seed int
}

func draw(rng *Rng, n int) int {
	rng.seed = rng.seed * 16807 % 2147483647
	return rng.seed % n
}

func quoted(s string) string {
	return "\"" + s + "\""
}

func Setup() *Tree {
	rng := &Rng{seed: 37}
	words := []string{"alpha", "bravo", "charlie", "delta", "echo", "foxtrot", "golf", "hotel", "india", "juliet", "kilo", "lima"}
	users := []string{}
	for i := 0; i < 3200; i++ {
		tags := []string{}
		tagCount := 1 + draw(rng, 4)
		for t := 0; t < tagCount; t++ {
			tags = append(tags, quoted(words[draw(rng, 12)]))
		}
		orders := []string{}
		orderCount := 1 + draw(rng, 3)
		for o := 0; o < orderCount; o++ {
			sku := quoted("sku-" + strconv.Itoa(draw(rng, 100000)))
			price := strconv.Itoa(draw(rng, 1000)) + "." + strconv.Itoa(draw(rng, 100))
			orders = append(orders, "{\"sku\":"+sku+",\"qty\":"+strconv.Itoa(1+draw(rng, 9))+",\"price\":"+price+"}")
		}
		name := words[draw(rng, 12)] + "_" + strconv.Itoa(i)
		street := strconv.Itoa(1+draw(rng, 999)) + " " + words[draw(rng, 12)] + " street"
		address := "{\"street\":" + quoted(street) + ",\"city\":" + quoted(words[draw(rng, 12)]+"ville") + ",\"zip\":" + quoted(strconv.Itoa(10000+draw(rng, 89999))) + "}"
		manager := "null"
		if draw(rng, 3) != 0 {
			manager = strconv.Itoa(draw(rng, 3200))
		}
		user := "{\"id\":" + strconv.Itoa(i) + ",\"name\":" + quoted(name) + ",\"email\":" + quoted(name+"@example.com")
		active := "true"
		user = user + ",\"age\":" + strconv.Itoa(18+draw(rng, 60))
		if draw(rng, 4) == 0 {
			active = "false"
		}
		user = user + ",\"active\":" + active
		user = user + ",\"score\":" + strconv.Itoa(draw(rng, 100)) + "." + strconv.Itoa(draw(rng, 1000))
		user = user + ",\"tags\":[" + strings.Join(tags, ",") + "],\"address\":" + address + ",\"orders\":[" + strings.Join(orders, ",") + "],\"manager_id\":" + manager + "}"
		users = append(users, user)
	}
	var doc any
	if err := json.Unmarshal([]byte("{\"meta\":{\"version\":3,\"count\":3200},\"users\":["+strings.Join(users, ",")+"]}"), &doc); err != nil {
		panic(err)
	}
	list, err := path(doc, ".users")
	if err != nil {
		panic(err)
	}
	return &Tree{doc: doc, users: list.([]any)}
}

// path follows the `.field` and `[index]` steps of a jq-style selector.
func path(j any, selector string) (any, error) {
	current := j
	i := 0
	for i < len(selector) {
		if selector[i] == '.' {
			i++
			start := i
			for i < len(selector) && selector[i] != '.' && selector[i] != '[' {
				i++
			}
			if i > start {
				key := selector[start:i]
				object, ok := current.(map[string]any)
				if !ok {
					return nil, fmt.Errorf("missing field %s", key)
				}
				current, ok = object[key]
				if !ok {
					return nil, fmt.Errorf("missing field %s", key)
				}
			}
		} else {
			i++
			start := i
			for selector[i] != ']' {
				i++
			}
			index, err := strconv.Atoi(selector[start:i])
			i++
			if err != nil {
				return nil, err
			}
			list, ok := current.([]any)
			if !ok || index >= len(list) {
				return nil, fmt.Errorf("bad index %d", index)
			}
			current = list[index]
		}
	}
	return current, nil
}

var errDecode = errors.New("does not decode")

// encoding/json parses every number of an untyped document as a float64.
func toInt(v any) (int, error) {
	f, ok := v.(float64)
	if !ok || f != math.Trunc(f) {
		return 0, errDecode
	}
	return int(f), nil
}

func toString(v any) (string, error) {
	s, ok := v.(string)
	if !ok {
		return "", errDecode
	}
	return s, nil
}

func toFloat(v any) (float64, error) {
	f, ok := v.(float64)
	if !ok {
		return 0, errDecode
	}
	return f, nil
}

func toBool(v any) (bool, error) {
	b, ok := v.(bool)
	if !ok {
		return false, errDecode
	}
	return b, nil
}

func toList[T any](v any, item func(any) (T, error)) ([]T, error) {
	list, ok := v.([]any)
	if !ok {
		return nil, errDecode
	}
	out := make([]T, len(list))
	for i, element := range list {
		value, err := item(element)
		if err != nil {
			return nil, err
		}
		out[i] = value
	}
	return out, nil
}

func toAddress(v any) (Address, error) {
	j, ok := v.(map[string]any)
	if !ok {
		return Address{}, errDecode
	}
	var a Address
	var err error
	if a.Street, err = toString(j["street"]); err != nil {
		return a, err
	}
	if a.City, err = toString(j["city"]); err != nil {
		return a, err
	}
	if a.Zip, err = toString(j["zip"]); err != nil {
		return a, err
	}
	return a, nil
}

func toOrder(v any) (Order, error) {
	j, ok := v.(map[string]any)
	if !ok {
		return Order{}, errDecode
	}
	var o Order
	var err error
	if o.Sku, err = toString(j["sku"]); err != nil {
		return o, err
	}
	if o.Qty, err = toInt(j["qty"]); err != nil {
		return o, err
	}
	if o.Price, err = toFloat(j["price"]); err != nil {
		return o, err
	}
	return o, nil
}

func toUser(v any) (*User, error) {
	j, ok := v.(map[string]any)
	if !ok {
		return nil, errDecode
	}
	u := &User{}
	var err error
	if u.ID, err = toInt(j["id"]); err != nil {
		return nil, err
	}
	if u.Name, err = toString(j["name"]); err != nil {
		return nil, err
	}
	if u.Email, err = toString(j["email"]); err != nil {
		return nil, err
	}
	if u.Age, err = toInt(j["age"]); err != nil {
		return nil, err
	}
	if u.Active, err = toBool(j["active"]); err != nil {
		return nil, err
	}
	if u.Score, err = toFloat(j["score"]); err != nil {
		return nil, err
	}
	if u.Tags, err = toList(j["tags"], toString); err != nil {
		return nil, err
	}
	if u.Address, err = toAddress(j["address"]); err != nil {
		return nil, err
	}
	if u.Orders, err = toList(j["orders"], toOrder); err != nil {
		return nil, err
	}
	if manager := j["manager_id"]; manager != nil {
		id, err := toInt(manager)
		if err != nil {
			return nil, err
		}
		u.ManagerID = &id
	}
	return u, nil
}

func pathInt(doc any, selector string) (int, error) {
	v, err := path(doc, selector)
	if err != nil {
		return 0, err
	}
	return toInt(v)
}

func pathString(doc any, selector string) (string, error) {
	v, err := path(doc, selector)
	if err != nil {
		return "", err
	}
	return toString(v)
}

func run(tree *Tree) (int, error) {
	rng := &Rng{seed: 41}
	check := 0
	for k := 0; k < 400; k++ {
		base := ".users[" + strconv.Itoa(draw(rng, 3200)) + "]"
		age, err := pathInt(tree.doc, base+".age")
		if err != nil {
			return 0, err
		}
		check += age
		city, err := pathString(tree.doc, base+".address.city")
		if err != nil {
			return 0, err
		}
		check += len(city)
		qty, err := pathInt(tree.doc, base+".orders[0].qty")
		if err != nil {
			return 0, err
		}
		check += qty
		manager, err := path(tree.doc, base+".manager_id")
		if err != nil {
			return 0, err
		}
		if manager == nil {
			check += 7
		} else {
			id, err := toInt(manager)
			if err != nil {
				return 0, err
			}
			check += id
		}
		user, err := toUser(tree.users[draw(rng, 3200)])
		if err != nil {
			return 0, err
		}
		check += len(user.Tags)*3 + len(user.Orders) + len(user.Name)
		if user.Active {
			check += 1
		}
		check = check % 1000000007
	}
	return check, nil
}

func Run(tree *Tree) int {
	check, err := run(tree)
	if err != nil {
		panic(err)
	}
	return check
}

func Main() int {
	return Run(Setup())
}

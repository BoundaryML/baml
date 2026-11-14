package main

import (
	"encoding/json"
	"errors"
	"fmt"
	"strconv"
	"strings"
)

// Go cannot create a named type with methods at run time, so a minted class is
// a small dynamic-object model: a Class value built per iteration (name, field
// types, interface witnesses) and Objects that point at their Class.

type Labeled interface {
	Label() string
}

// shout is Labeled's default method.
func shout(l Labeled) string {
	return l.Label() + "!"
}

// Witness says how a minted class satisfies an interface: which of its fields
// backs each interface field.
type Witness struct {
	fields map[string]string
}

type Class struct {
	name   string
	fields map[string]string
	impls  map[string]*Witness
}

func newClass(name string, fields map[string]string, impls map[string]*Witness) *Class {
	return &Class{name: name, fields: fields, impls: impls}
}

type Object struct {
	class  *Class
	values map[string]any
}

var errDecode = errors.New("decode error")

// decode builds an Object of this class from JSON text, checking every
// declared field against its type.
func (c *Class) decode(text string) (*Object, error) {
	var raw map[string]any
	if err := json.Unmarshal([]byte(text), &raw); err != nil {
		return nil, err
	}
	values := map[string]any{}
	for field, typ := range c.fields {
		v, ok := raw[field]
		if !ok {
			return nil, errDecode
		}
		switch typ {
		case "string":
			if _, ok := v.(string); !ok {
				return nil, errDecode
			}
		default:
			return nil, errDecode
		}
		values[field] = v
	}
	return &Object{class: c, values: values}, nil
}

// labeledObject is an Object seen through its class's Labeled witness.
type labeledObject struct {
	object  *Object
	witness *Witness
}

func (l *labeledObject) Label() string {
	return l.object.values[l.witness.fields["label"]].(string)
}

// asLabeled is the run-time interface test for a dynamic object.
func asLabeled(value any) (Labeled, bool) {
	switch v := value.(type) {
	case Labeled:
		return v, true
	case *Object:
		if w, ok := v.class.impls["Labeled"]; ok {
			return &labeledObject{object: v, witness: w}, true
		}
	}
	return nil, false
}

type Status string

const (
	Pending   Status = "Pending"
	Shipped   Status = "Shipped"
	Delivered Status = "Delivered"
)

type Money struct {
	Cents int `json:"cents"`
}

func (m Money) String() string {
	return "$" + strconv.Itoa(m.Cents/100) + "." + strconv.Itoa(m.Cents%100)
}

type Address struct {
	Street string `json:"street"`
	City   string `json:"city"`
}

type Customer struct {
	Name    string  `json:"name"`
	Address Address `json:"address"`
}

type Item struct {
	Sku   string `json:"sku"`
	Qty   int    `json:"qty"`
	Price Money  `json:"price"`
}

type Order struct {
	ID       int      `json:"id"`
	Customer Customer `json:"customer"`
	Status   Status   `json:"status"`
	Items    []Item   `json:"items"`
}

func shoutOf(value any) string {
	if labeled, ok := asLabeled(value); ok {
		return shout(labeled)
	}
	return ""
}

func Setup() []string {
	cities := []string{"Lisbon", "Oslo", "Lima", "Kyoto"}
	statuses := []string{"Pending", "Shipped", "Delivered"}
	orders := []string{}
	for i := 0; i < 8; i++ {
		items := []string{}
		for k := 0; k < 1+i%3; k++ {
			items = append(items, "{\"sku\":\"S"+strconv.Itoa(i*10+k)+"\",\"qty\":"+strconv.Itoa(1+k)+
				",\"price\":{\"cents\":"+strconv.Itoa(199+i*37+k)+"}}")
		}
		orders = append(orders, "{\"id\":"+strconv.Itoa(i)+",\"customer\":{\"name\":\"c"+strconv.Itoa(i)+
			"\",\"address\":{\"street\":\""+strconv.Itoa(i+1)+" Main St\",\"city\":\""+cities[i%4]+"\"}}"+
			",\"status\":\""+statuses[i%3]+"\",\"items\":["+strings.Join(items, ",")+"]}")
	}
	return orders
}

func Run(orders []string) int {
	total := 0
	for i := 0; i < 200; i++ {
		minted := newClass(
			"Row"+strconv.Itoa(i),
			map[string]string{"label": "string"},
			map[string]*Witness{"Labeled": {fields: map[string]string{"label": "label"}}},
		)
		row, err := minted.decode("{\"label\": \"r" + strconv.Itoa(i) + "\"}")
		if err != nil {
			panic(err)
		}
		total += len(shoutOf(row))
		var order Order
		if err := json.Unmarshal([]byte(orders[i%8]), &order); err != nil {
			panic(err)
		}
		if order.Status == Shipped {
			total++
		}
		for _, item := range order.Items {
			total += len(fmt.Sprintf("%d x %s @ %s", item.Qty, item.Sku, item.Price))
		}
		total += len(fmt.Sprintf("%s in %s", order.Customer.Name, order.Customer.Address.City)) + order.ID
	}
	return total
}

func Main() int {
	return Run(Setup())
}

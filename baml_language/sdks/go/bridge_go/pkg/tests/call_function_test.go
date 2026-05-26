package tests

import (
	"context"
	"testing"

	"bridge_go/pkg"
)

const callFunctionBamlSource = `
function ReturnOne() -> int {
  1
}

function Identity(s: string) -> string {
  s
}

enum Color {
  Red
  Blue
}

function PickColor(n: int) -> Color {
  if (n == 0) { Color.Red } else { Color.Blue }
}

class Point {
  x int
  y int
}

function MakePoint(x: int, y: int) -> Point {
  Point { x: x, y: y }
}

class Address {
  city string
  zip int
}

class Contact {
  name string
  address Address
}

function MakeContact(name: string, city: string, zip: int) -> Contact {
  Contact { name: name, address: Address { city: city, zip: zip } }
}

function IntOrString(useInt: bool, n: int) -> int | string {
  if (useInt) { n } else { "hello" }
}

function MakeIntList(a: int, b: int, c: int) -> int[] {
  [a, b, c]
}

function MakeStringMap() -> map<string, int> {
  { "a": 1, "b": 2, "c": 3 }
}
`

var testRT *pkg.BamlRuntime

func getTestRuntime(t *testing.T) *pkg.BamlRuntime {
	t.Helper()
	if testRT != nil {
		return testRT
	}
	rt, err := pkg.NewRuntime(".", map[string]string{"main.baml": callFunctionBamlSource})
	if err != nil {
		t.Fatalf("NewRuntime failed: %v", err)
	}
	testRT = rt
	return rt
}

func TestReturnInt(t *testing.T) {
	rt := getTestRuntime(t)
	result, err := rt.CallFunction(context.Background(), "ReturnOne", nil)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if result != int64(1) {
		t.Fatalf("expected 1, got %v (%T)", result, result)
	}
}

func TestIdentityString(t *testing.T) {
	rt := getTestRuntime(t)
	result, err := rt.CallFunction(context.Background(), "Identity", map[string]any{"s": "hello"})
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if result != "hello" {
		t.Fatalf("expected 'hello', got %v", result)
	}
}

func TestEnumResult(t *testing.T) {
	rt := getTestRuntime(t)
	result, err := rt.CallFunction(context.Background(), "PickColor", map[string]any{"n": 0})
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	e, ok := result.(pkg.DynamicEnum)
	if !ok {
		t.Fatalf("expected DynamicEnum, got %T: %v", result, result)
	}
	if e.Name != "Color" {
		t.Fatalf("expected enum name 'Color', got %q", e.Name)
	}
	if e.Value != "Red" {
		t.Fatalf("expected enum value 'Red', got %q", e.Value)
	}
}

func TestClassResult(t *testing.T) {
	rt := getTestRuntime(t)
	result, err := rt.CallFunction(context.Background(), "MakePoint", map[string]any{"x": 10, "y": 20})
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	c, ok := result.(pkg.DynamicClass)
	if !ok {
		t.Fatalf("expected DynamicClass, got %T: %v", result, result)
	}
	if c.Name != "Point" {
		t.Fatalf("expected class name 'Point', got %q", c.Name)
	}
	if c.Fields["x"] != int64(10) || c.Fields["y"] != int64(20) {
		t.Fatalf("expected {x:10, y:20}, got %v", c.Fields)
	}
}

func TestNestedClass(t *testing.T) {
	rt := getTestRuntime(t)
	result, err := rt.CallFunction(context.Background(), "MakeContact", map[string]any{
		"name": "Bob", "city": "Seattle", "zip": 98101,
	})
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	c, ok := result.(pkg.DynamicClass)
	if !ok {
		t.Fatalf("expected DynamicClass, got %T: %v", result, result)
	}
	if c.Name != "Contact" {
		t.Fatalf("expected class name 'Contact', got %q", c.Name)
	}
	if c.Fields["name"] != "Bob" {
		t.Fatalf("expected name=Bob, got %v", c.Fields["name"])
	}
	addr, ok := c.Fields["address"].(pkg.DynamicClass)
	if !ok {
		t.Fatalf("expected address to be DynamicClass, got %T", c.Fields["address"])
	}
	if addr.Fields["city"] != "Seattle" || addr.Fields["zip"] != int64(98101) {
		t.Fatalf("expected address={city:Seattle, zip:98101}, got %v", addr.Fields)
	}
}

func TestUnionIntOrString(t *testing.T) {
	rt := getTestRuntime(t)

	// Int branch
	result, err := rt.CallFunction(context.Background(), "IntOrString", map[string]any{"useInt": true, "n": 42})
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	u, ok := result.(pkg.DynamicUnion)
	if !ok {
		t.Fatalf("expected DynamicUnion, got %T: %v", result, result)
	}
	if u.Value != int64(42) {
		t.Fatalf("expected 42, got %v (%T)", u.Value, u.Value)
	}

	// String branch
	result, err = rt.CallFunction(context.Background(), "IntOrString", map[string]any{"useInt": false, "n": 0})
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	u, ok = result.(pkg.DynamicUnion)
	if !ok {
		t.Fatalf("expected DynamicUnion, got %T: %v", result, result)
	}
	if u.Value != "hello" {
		t.Fatalf("expected 'hello', got %v", u.Value)
	}
}

func TestListReturn(t *testing.T) {
	rt := getTestRuntime(t)
	result, err := rt.CallFunction(context.Background(), "MakeIntList", map[string]any{"a": 1, "b": 2, "c": 3})
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	list, ok := result.([]any)
	if !ok {
		t.Fatalf("expected []any, got %T", result)
	}
	if len(list) != 3 || list[0] != int64(1) || list[1] != int64(2) || list[2] != int64(3) {
		t.Fatalf("expected [1,2,3], got %v", list)
	}
}

func TestMapReturn(t *testing.T) {
	rt := getTestRuntime(t)
	result, err := rt.CallFunction(context.Background(), "MakeStringMap", nil)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	m, ok := result.(map[string]any)
	if !ok {
		t.Fatalf("expected map, got %T", result)
	}
	if m["a"] != int64(1) || m["b"] != int64(2) || m["c"] != int64(3) {
		t.Fatalf("expected {a:1, b:2, c:3}, got %v", m)
	}
}

func TestUnknownFunction(t *testing.T) {
	rt := getTestRuntime(t)
	_, err := rt.CallFunction(context.Background(), "NonExistentFunction", nil)
	if err == nil {
		t.Fatal("expected error for unknown function")
	}
}

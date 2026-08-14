package baml_go

import "testing"

func TestOptionalArgDistinguishesOmittedNullAndValue(t *testing.T) {
	var omitted OptionalArg[*int64]
	if omitted.IsSet() {
		t.Fatal("zero OptionalArg unexpectedly reports a supplied value")
	}
	if value, ok := omitted.Get(); ok || value != nil {
		t.Fatalf("omitted Get() = (%v, %v), want (nil, false)", value, ok)
	}

	explicitNull := NewOptionalArg[*int64](nil)
	if value, ok := explicitNull.Get(); !ok || value != nil {
		t.Fatalf("explicit-null Get() = (%v, %v), want (nil, true)", value, ok)
	}

	want := int64(42)
	supplied := NewOptionalArg(&want)
	if value, ok := supplied.Get(); !ok || value == nil || *value != want {
		t.Fatalf("supplied Get() = (%v, %v), want (&42, true)", value, ok)
	}
}

func TestOptionalArgOrUsesFallbackOnlyWhenOmitted(t *testing.T) {
	var omitted OptionalArg[int64]
	if got := omitted.Or(7); got != 7 {
		t.Fatalf("omitted.Or(7) = %d, want 7", got)
	}
	if got := NewOptionalArg(int64(0)).Or(7); got != 0 {
		t.Fatalf("supplied zero.Or(7) = %d, want 0", got)
	}

	fallback := int64(7)
	if got := NewOptionalArg[*int64](nil).Or(&fallback); got != nil {
		t.Fatalf("explicit null.Or(&7) = %v, want nil", got)
	}
}

package baml_go

import (
	"errors"
	"fmt"
	"runtime"
	"sync"

	"github.com/boundaryml/baml-go/internal/cffi"
)

// HostValue is a `baml.host.Value`: a Go value BAML holds by reference and
// never looks into. Sent to BAML, it is registered with this process; back
// from BAML, it is the same HostValue again.
//
// Copies of a HostValue, and every decode of one BAML sent back, are one host
// value: Equal holds, and BAML sees one value (`==` holds) however often it is
// sent. Each NewHostValue is a distinct host value, even of one Go value.
type HostValue struct {
	cell *hostValueCell
}

// hostValueCell is the identity of one host value, and the object its key
// names in the host-value registry.
type hostValueCell struct {
	value any
	mu    sync.Mutex
	// key is the registration the cell was last sent under, or 0. It may
	// have been released since; a send then registers the cell again.
	key uint64
}

// NewHostValue wraps value to pass to BAML.
func NewHostValue(value any) HostValue {
	return HostValue{cell: &hostValueCell{value: value}}
}

// registerHostCell registers a fresh cell for value, for one send.
func registerHostCell(value any) (*hostValueCell, uint64) {
	cell := &hostValueCell{value: value}
	cell.key = registerHostObject(cell)
	return cell, cell.key
}

// Value returns the Go value.
func (value HostValue) Value() any {
	if value.cell == nil {
		return nil
	}
	return value.cell.value
}

// Equal reports whether value and other are the same host value.
func (value HostValue) Equal(other HostValue) bool {
	return value.cell == other.cell
}

// HostValueInput sends value to BAML: under the registration BAML holds it
// by, taking one more reference to it, or, if BAML holds it no longer, under
// a fresh one. The reference is the call's until the engine has the request.
func HostValueInput(value HostValue) Input {
	if value.cell == nil {
		return InvalidInput("uninitialized BAML host value")
	}
	cell := value.cell
	return Input{deferred: &inputEncoder{encode: func(transaction *inputTransaction) (*cffi.InboundValue, error) {
		if transaction == nil {
			return nil, errors.New("BAML host value requires an input transaction")
		}
		cell.mu.Lock()
		defer cell.mu.Unlock()
		if cell.key == 0 || retainHostValue(cell.key) != nil {
			cell.key = registerHostObject(cell)
		}
		transaction.ownHostValue(cell.key)
		return hostValueInput(cell.key, cffi.BamlHandleType_HOST_VALUE_OPAQUE), nil
	}}}
}

// BAMLInput sends the host value wherever a dynamic value goes.
func (value HostValue) BAMLInput() Input { return HostValueInput(value) }

// BAMLType supplies exact empty-container and union metadata to reflective
// encoders.
func (HostValue) BAMLType() BAMLType { return HostValueBAMLType() }

// HostValueBAMLType describes `baml.host.Value`, the type of a host's own
// value.
func HostValueBAMLType() BAMLType { return ClassBAMLType("baml.host.Value") }

// HostValue decodes a host value: the Go value this process sent.
func (value Value) HostValue() (HostValue, error) {
	unwrapped, err := value.unwrapUnionVariants()
	if err != nil {
		return HostValue{}, err
	}
	value = unwrapped
	if value.value == nil {
		return HostValue{}, fmt.Errorf("BAML value is uninitialized")
	}
	encoded, ok := value.value.Value.(*cffi.BamlOutboundValue_HandleValue)
	if !ok || encoded.HandleValue == nil {
		return HostValue{}, fmt.Errorf("expected a BAML host value handle, got %T", value.value.Value)
	}
	handle := encoded.HandleValue
	if handle.HandleType != cffi.BamlHandleType_HOST_VALUE_OPAQUE {
		return HostValue{}, fmt.Errorf("BAML returned handle type %d for a host value, expected %d", handle.HandleType, cffi.BamlHandleType_HOST_VALUE_OPAQUE)
	}
	// Only this process registers its values, and the pin in value's owner
	// keeps the key registered while it is read: a miss is a lifetime bug.
	object, live := lookupHostObject(handle.Key)
	runtime.KeepAlive(value.owner)
	cell, isCell := object.(*hostValueCell)
	if handle.Key == 0 || !live || !isCell {
		return HostValue{}, fmt.Errorf("BAML returned host value %d, which this process does not hold", handle.Key)
	}
	return HostValue{cell: cell}, nil
}

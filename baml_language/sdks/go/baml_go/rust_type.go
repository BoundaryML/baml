package baml_go

import (
	"fmt"
	"runtime"

	"github.com/boundaryml/baml-go/internal/cffi"
)

// RustType is an opaque, BAML-owned `$rust_type` value. Copies of a RustType
// share one Go-owned native handle safely. Passing the value back into BAML
// transfers a fresh clone, so the Go value remains reusable and concurrent
// calls do not compete for ownership.
//
// RustType has no Go constructor and intentionally exposes no payload. It is
// produced only by decoding a BAML result, usually as the private native state
// of a generated standard-library class.
type RustType struct {
	handle *rustTypeHandle
}

type rustTypeHandle struct {
	key uint64
}

// OpaqueHandle retains an engine-owned value carried through an `unknown`
// field. It is intentionally payload-free and exists for generated stdlib
// facades such as reflect.Package.
type OpaqueHandle struct {
	handle *opaqueHandle
}

type opaqueHandle struct {
	key        uint64
	handleType cffi.BamlHandleType
}

var (
	cloneOutboundRustTypeHandle = nativeHandleClone
	releaseRustTypeHandle       = nativeHandleRelease
)

func rustTypeFromOwnedHandle(handle *cffi.BamlOutboundHandle) (RustType, error) {
	if handle == nil {
		return RustType{}, fmt.Errorf("BAML returned an empty $rust_type handle")
	}
	if handle.Key == 0 {
		return RustType{}, fmt.Errorf("BAML returned a zero $rust_type handle")
	}
	if handle.HandleType != cffi.BamlHandleType_UNTAGGED_RUST_DATA {
		return RustType{}, fmt.Errorf("BAML returned handle type %d for $rust_type, expected %d", handle.HandleType, cffi.BamlHandleType_UNTAGGED_RUST_DATA)
	}
	if handle.Ty != nil {
		return RustType{}, fmt.Errorf("BAML returned unexpected type metadata on an untagged $rust_type handle")
	}
	cloned, err := cloneOutboundRustTypeHandle(handle.Key)
	if err != nil {
		return RustType{}, fmt.Errorf("clone BAML $rust_type handle: %w", err)
	}
	if cloned == 0 {
		return RustType{}, fmt.Errorf("clone BAML $rust_type handle: runtime returned a zero handle")
	}
	owned := &rustTypeHandle{key: cloned}
	runtime.SetFinalizer(owned, finalizeRustTypeHandle)
	return RustType{handle: owned}, nil
}

func finalizeRustTypeHandle(handle *rustTypeHandle) {
	if handle != nil && handle.key != 0 {
		releaseRustTypeHandle(handle.key)
		handle.key = 0
	}
}

func finalizeOpaqueHandle(handle *opaqueHandle) {
	if handle != nil && handle.key != 0 {
		releaseRustTypeHandle(handle.key)
		handle.key = 0
	}
}

func (value RustType) validate() error {
	if value.handle == nil || value.handle.key == 0 {
		return fmt.Errorf("uninitialized BAML $rust_type value")
	}
	return nil
}

// RustTypeInput encodes an opaque value for BAML re-entry. The cloned wire
// handle is transaction-owned until the native runtime synchronously drains
// it, so failures cannot leak native table rows.
func RustTypeInput(value RustType) Input {
	if err := value.validate(); err != nil {
		return InvalidInput(err.Error())
	}
	handle := value.handle
	return Input{deferred: &inputEncoder{encode: func(transaction *inputTransaction) (*cffi.InboundValue, error) {
		cloned, err := cloneInboundHandle(handle.key)
		runtime.KeepAlive(handle)
		if err != nil {
			return nil, fmt.Errorf("clone BAML $rust_type handle for input: %w", err)
		}
		transaction.own(cloned)
		return &cffi.InboundValue{Value: &cffi.InboundValue_Handle{Handle: &cffi.BamlHandle{
			Key: cloned, HandleType: cffi.BamlHandleType_UNTAGGED_RUST_DATA,
		}}}, nil
	}}}
}

// OpaqueHandleInput passes an engine-owned unknown value back to BAML while
// retaining an independent host clone.
func OpaqueHandleInput(value OpaqueHandle) Input {
	if value.handle == nil || value.handle.key == 0 {
		return InvalidInput("uninitialized BAML opaque handle")
	}
	handle := value.handle
	return Input{deferred: &inputEncoder{encode: func(transaction *inputTransaction) (*cffi.InboundValue, error) {
		cloned, err := cloneInboundHandle(handle.key)
		runtime.KeepAlive(handle)
		if err != nil {
			return nil, fmt.Errorf("clone BAML opaque handle for input: %w", err)
		}
		transaction.own(cloned)
		return &cffi.InboundValue{Value: &cffi.InboundValue_Handle{Handle: &cffi.BamlHandle{
			Key: cloned, HandleType: handle.handleType,
		}}}, nil
	}}}
}

// BAMLInput allows `$rust_type` values to participate in dynamic unions and
// containers without exposing their wire representation.
func (value RustType) BAMLInput() Input { return RustTypeInput(value) }

// BAMLType supplies exact empty-container and union metadata to reflective
// encoders.
func (RustType) BAMLType() BAMLType { return RustTypeBAMLType() }

// RustType decodes one opaque native handle. The returned value owns an
// independent clone, so releasing the enclosing call result cannot invalidate
// it.
func (value Value) RustType() (RustType, error) {
	unwrapped, err := value.unwrapUnionVariants()
	if err != nil {
		return RustType{}, err
	}
	value = unwrapped
	if value.value == nil {
		return RustType{}, fmt.Errorf("BAML value is uninitialized")
	}
	encoded, ok := value.value.Value.(*cffi.BamlOutboundValue_HandleValue)
	if !ok {
		return RustType{}, fmt.Errorf("expected BAML $rust_type handle, got %T", value.value.Value)
	}
	decoded, err := rustTypeFromOwnedHandle(encoded.HandleValue)
	runtime.KeepAlive(value.owner)
	return decoded, err
}

// OpaqueHandle decodes one engine-owned untagged handle from an `unknown`
// field. Both legacy Rust-data and BEX heap handles are accepted.
func (value Value) OpaqueHandle() (OpaqueHandle, error) {
	unwrapped, err := value.unwrapUnionVariants()
	if err != nil {
		return OpaqueHandle{}, err
	}
	value = unwrapped
	if value.value == nil {
		return OpaqueHandle{}, fmt.Errorf("BAML value is uninitialized")
	}
	encoded, ok := value.value.Value.(*cffi.BamlOutboundValue_HandleValue)
	if !ok || encoded.HandleValue == nil {
		return OpaqueHandle{}, fmt.Errorf("expected BAML opaque handle, got %T", value.value.Value)
	}
	handleType := encoded.HandleValue.HandleType
	if handleType != cffi.BamlHandleType_UNTAGGED_RUST_DATA && handleType != cffi.BamlHandleType_UNTAGGED_BEX_HEAP {
		return OpaqueHandle{}, fmt.Errorf("BAML returned handle type %d for opaque value", handleType)
	}
	cloned, err := cloneOutboundRustTypeHandle(encoded.HandleValue.Key)
	if err != nil {
		return OpaqueHandle{}, fmt.Errorf("clone BAML opaque handle: %w", err)
	}
	owned := &opaqueHandle{key: cloned, handleType: handleType}
	runtime.SetFinalizer(owned, finalizeOpaqueHandle)
	runtime.KeepAlive(value.owner)
	return OpaqueHandle{handle: owned}, nil
}

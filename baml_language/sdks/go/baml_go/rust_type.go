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

// OpaqueHandle retains a reference to an instance only the engine builds,
// which crosses as an ADT_RUNTIME_VALUE handle: a standard-library capability
// whose state is the runtime's own (reflect.Package, regex.Regex, ...), or an
// instance of a class or enum declared at run time. Go holds the reference and
// passes it back, but never reads into it or builds one. A generated class
// wrapper holds one for each instance of a class it names. Two OpaqueHandles
// reference the same instance exactly when Equal reports so.
type OpaqueHandle struct {
	handle *opaqueHandle
}

type opaqueHandle struct {
	key uint64
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
		cloned, err := cloneForInput(transaction, handle.key, "$rust_type")
		runtime.KeepAlive(handle)
		if err != nil {
			return nil, err
		}
		return &cffi.InboundValue{Value: &cffi.InboundValue_Handle{Handle: &cffi.BamlHandle{
			Key: cloned, HandleType: cffi.BamlHandleType_UNTAGGED_RUST_DATA,
		}}}, nil
	}}}
}

// OpaqueHandleInput passes an opaque instance back to BAML as the handle it
// crossed as, while retaining an independent host clone.
func OpaqueHandleInput(value OpaqueHandle) Input {
	if value.handle == nil || value.handle.key == 0 {
		return InvalidInput("uninitialized BAML opaque handle")
	}
	handle := value.handle
	return Input{deferred: &inputEncoder{encode: func(transaction *inputTransaction) (*cffi.InboundValue, error) {
		cloned, err := cloneForInput(transaction, handle.key, "opaque")
		runtime.KeepAlive(handle)
		if err != nil {
			return nil, err
		}
		return &cffi.InboundValue{Value: &cffi.InboundValue_Handle{Handle: &cffi.BamlHandle{
			Key: cloned, HandleType: cffi.BamlHandleType_ADT_RUNTIME_VALUE,
		}}}, nil
	}}}
}

// Equal reports whether value and other reference the same instance. The
// runtime keys one instance by one handle for as long as any reference to it
// is held, and every decode holds its own, so this is equality by handle key.
func (value OpaqueHandle) Equal(other OpaqueHandle) bool {
	if value.handle == nil || other.handle == nil {
		return value.handle == other.handle
	}
	return value.handle.key == other.handle.key
}

// BAMLInput allows `$rust_type` values to participate in dynamic unions and
// containers without exposing their wire representation.
func (value RustType) BAMLInput() Input { return RustTypeInput(value) }

// BAMLInput passes the instance back as the handle it crossed as, so a
// decoded runtime value can be sent wherever a dynamic value goes.
func (value OpaqueHandle) BAMLInput() Input { return OpaqueHandleInput(value) }

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

// OpaqueHandle decodes the reference an opaque instance of the non-generic
// class named className crosses as: an ADT_RUNTIME_VALUE handle whose type
// names that class. The returned value owns an independent clone, so releasing
// the enclosing call result cannot invalidate it.
func (value Value) OpaqueHandle(className string) (OpaqueHandle, error) {
	return value.OpaqueHandleWithTypeArgs(className, nil)
}

// OpaqueHandleWithTypeArgs decodes the reference an opaque instance of the
// class named className crosses as, validating that the instance is of that
// class at exactly typeArgs. Generated class wrappers decode through it.
func (value Value) OpaqueHandleWithTypeArgs(className string, typeArgs []BAMLType) (OpaqueHandle, error) {
	return value.runtimeValue(className, func(handle *cffi.BamlOutboundHandle) error {
		class := handle.GetTy().GetClassTy()
		if name := class.GetName(); name != className {
			return fmt.Errorf("expected BAML %s handle, got one of class %q", className, name)
		}
		_, err := validateClassTypeArgs(className, class.GetTypeArgs(), typeArgs)
		return err
	})
}

// RuntimeValue decodes the reference any instance only the engine builds
// crosses as, whatever its class or enum: an instance of a declaration made at
// run time names no class a Go type could, so this is how it surfaces. The
// returned value owns an independent clone, so releasing the enclosing call
// result cannot invalidate it.
func (value Value) RuntimeValue() (OpaqueHandle, error) {
	return value.runtimeValue("runtime value", func(*cffi.BamlOutboundHandle) error { return nil })
}

func (value Value) runtimeValue(what string, check func(*cffi.BamlOutboundHandle) error) (OpaqueHandle, error) {
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
		return OpaqueHandle{}, fmt.Errorf("expected BAML %s handle, got %T", what, value.value.Value)
	}
	handle := encoded.HandleValue
	if handle.HandleType != cffi.BamlHandleType_ADT_RUNTIME_VALUE {
		return OpaqueHandle{}, fmt.Errorf("BAML returned handle type %d for %s, expected %d", handle.HandleType, what, cffi.BamlHandleType_ADT_RUNTIME_VALUE)
	}
	if err := check(handle); err != nil {
		return OpaqueHandle{}, err
	}
	if handle.Key == 0 {
		return OpaqueHandle{}, fmt.Errorf("BAML returned a zero %s handle", what)
	}
	cloned, err := cloneOutboundRustTypeHandle(handle.Key)
	if err != nil {
		return OpaqueHandle{}, fmt.Errorf("clone BAML %s handle: %w", what, err)
	}
	if cloned == 0 {
		return OpaqueHandle{}, fmt.Errorf("clone BAML %s handle: runtime returned a zero handle", what)
	}
	owned := &opaqueHandle{key: cloned}
	runtime.SetFinalizer(owned, finalizeOpaqueHandle)
	runtime.KeepAlive(value.owner)
	return OpaqueHandle{handle: owned}, nil
}

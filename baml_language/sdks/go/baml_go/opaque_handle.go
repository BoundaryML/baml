package baml_go

import (
	"fmt"
	"runtime"

	"github.com/boundaryml/baml-go/internal/cffi"
)

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

var cloneOutboundHandle = nativeHandleClone

func finalizeOpaqueHandle(handle *opaqueHandle) {
	if handle != nil && handle.key != 0 {
		releaseOutboundHandle(handle.key)
		handle.key = 0
	}
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

// BAMLInput passes the instance back as the handle it crossed as, so a
// decoded runtime value can be sent wherever a dynamic value goes.
func (value OpaqueHandle) BAMLInput() Input { return OpaqueHandleInput(value) }

// OpaqueHandle decodes the reference an opaque instance of the class named
// className crosses as: an ADT_RUNTIME_VALUE handle whose type names that
// class. The class's type arguments are not checked: the engine judges the
// live object whenever the handle comes back. The returned value owns an
// independent clone, so releasing the enclosing call result cannot invalidate
// it. Generated class wrappers decode through it.
func (value Value) OpaqueHandle(className string) (OpaqueHandle, error) {
	return value.runtimeValue(className, func(handle *cffi.BamlOutboundHandle) error {
		if name := handle.GetTy().GetClassTy().GetName(); name != className {
			return fmt.Errorf("expected BAML %s handle, got one of class %q", className, name)
		}
		return nil
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
	cloned, err := cloneOutboundHandle(handle.Key)
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

package baml_go

import (
	"context"
	"errors"
	"github.com/boundaryml/baml-go/internal/cffi"
	"google.golang.org/protobuf/proto"
	"math"
	"runtime"
	"sort"
	"time"
)

// InvocationControls is lowered only by the generated SDK's typed facade.
// Input encoding remains deferred until a runtime-bound allocation exists.
type InvocationControls struct {
	Trace   Input
	Cancel  Input
	Timeout time.Duration
}

func WithInvocationOptions(value InvocationControls) CallOption {
	return func(options *callOptions) {
		if options.controlsSet {
			options.err = errors.New("more than one WithOptions in one BAML invocation")
			return
		}
		options.controlsSet = true
		options.controls = value
	}
}

type PreparedCallOptions struct{ options callOptions }

func ApplyInvocationOptions(arguments map[string]Input, defaults []TypeArgument, values ...CallOption) PreparedCallOptions {
	options := callOptions{arguments: arguments, typeArgs: append([]TypeArgument(nil), defaults...)}
	for _, option := range values {
		if option != nil {
			option(&options)
		}
	}
	return PreparedCallOptions{options}
}
func CallWithPreparedOptions(ctx context.Context, target string, arguments map[string]Input, prepared PreparedCallOptions) (Value, error) {
	if prepared.options.err != nil {
		return Value{}, prepared.options.err
	}
	return callWithTypeArgs(ctx, target, arguments, prepared.options.typeArgs, prepared.options.controls)
}
func prepareInvocation(ctx context.Context, id uint64, values []InvocationControls) (*cffi.InvocationOptions, *inputTransaction, error) {
	transaction := &inputTransaction{}
	fail := func(err error) (*cffi.InvocationOptions, *inputTransaction, error) {
		transaction.rollback()
		return nil, transaction, err
	}
	options, err := invocationOptions(ctx, id)
	if err != nil {
		return fail(err)
	}
	if len(values) == 0 {
		return options, transaction, nil
	}
	controls := values[0]
	if controls.Timeout < 0 {
		return fail(errors.New("negative BAML invocation timeout"))
	}
	if controls.Timeout > 0 {
		now, err := nativeInvocationClockNs(id)
		if err != nil {
			return fail(err)
		}
		if uint64(controls.Timeout) > math.MaxUint64-now {
			return fail(errors.New("BAML invocation deadline overflow"))
		}
		deadline := now + uint64(controls.Timeout)
		if options.DeadlineNs == nil || deadline < *options.DeadlineNs {
			options.DeadlineNs = &deadline
		}
	}
	if controls.Trace.value != nil || controls.Trace.deferred != nil || controls.Trace.err != nil {
		value, err := controls.Trace.encodeValue(transaction)
		if err != nil {
			return fail(err)
		}
		var key uint64
		for _, field := range value.GetClassValue().GetFields() {
			if field.GetStringKey() == "_handle" {
				key = field.GetValue().GetHandle().GetKey()
			}
		}
		if key == 0 {
			return fail(errors.New("trace selection must be a generated trace value"))
		}
		encoded, owner, err := nativeTraceSelection(id, key)
		if err != nil {
			return fail(err)
		}
		if owner != 0 {
			transaction.own(owner)
		}
		options.Trace = &cffi.TraceSelection{}
		if err := proto.Unmarshal(encoded, options.Trace); err != nil {
			return fail(err)
		}
	}
	if controls.Cancel.value != nil || controls.Cancel.deferred != nil || controls.Cancel.err != nil {
		value, err := controls.Cancel.encodeValue(transaction)
		if err != nil {
			return fail(err)
		}
		options.Cancel = value
	}
	return options, transaction, nil
}

// CurrentInvocation exposes the dispatch-owned context without retaining a callback ID.
type Invocation struct {
	ctx     context.Context
	carrier *invocationStateCarrier
}

func CurrentInvocation(ctx context.Context) *Invocation {
	if ctx == nil {
		return nil
	}
	carrier, _ := ctx.Value(invocationStateContextKey{}).(*invocationStateCarrier)
	if carrier == nil {
		return nil
	}
	return &Invocation{ctx, carrier}
}
func (active *Invocation) CancelValue() Value {
	return Value{value: active.carrier.cancel, owner: active.carrier.cancelOwner}
}
func CurrentTraceContext(ctx context.Context) (Value, error) {
	var state uint64
	if active := CurrentInvocation(ctx); active != nil {
		state = active.carrier.state
	}
	bytes, err := nativeInvocationContext(state)
	runtime.KeepAlive(ctx)
	if err != nil {
		return Value{}, err
	}
	value := &cffi.BamlOutboundValue{}
	if err := proto.Unmarshal(bytes, value); err != nil {
		return Value{}, err
	}
	return Value{value: value, owner: ownOutboundHandles(value)}, nil
}

// Target is a closed choice of a name or an owning returned BAML callable.
type Target struct {
	name     string
	function *Function
}

func NamedTarget(name string) Target { return Target{name: name} }
func FunctionTarget(value interface{ BAMLFunction() Function }) Target {
	function := value.BAMLFunction()
	return Target{function: &function}
}

type Arguments = map[string]Input
type TypeBindings = map[string]BAMLType

func Invoke(ctx context.Context, target Target, arguments Arguments, types TypeBindings, values ...CallOption) (Value, error) {
	options := ApplyInvocationOptions(map[string]Input{}, nil, values...).options
	if options.err != nil {
		return Value{}, options.err
	}
	if len(options.arguments) != 0 || len(options.typeArgs) != 0 {
		return Value{}, errors.New("dynamic invocation options cannot contain application or type-binding setters")
	}
	if target.function != nil {
		if len(types) != 0 {
			return Value{}, errors.New("specialized callable rejects type bindings")
		}
		return callHandle(ctx, target.function.key, arguments, options.controls)
	}
	if target.name == "" {
		return Value{}, errors.New("invalid BAML invocation target")
	}
	names := make([]string, 0, len(types))
	for name := range types {
		names = append(names, name)
	}
	sort.Strings(names)
	bindings := make([]TypeArgument, 0, len(types))
	for _, name := range names {
		bindings = append(bindings, TypeArgument{Name: name, Type: types[name]})
	}
	return callWithTypeArgs(ctx, target.name, arguments, bindings, options.controls)
}

package pkg

import (
	"fmt"
	"reflect"

	pb "bridge_go/cffi/proto/baml_core/cffi/v1"

	"google.golang.org/protobuf/proto"
)

// encodeCallArgs converts Go kwargs to CallFunctionArgs protobuf bytes.
//
// Callables encountered while encoding are registered in the per-process
// host-value registry (see `goToInboundValue`'s reflective fallback). Those
// entries are normally released only when the engine GCs the corresponding
// `HostClosure` and fires `bamlHostRelease` (a GC-timed release — see the
// note on `registerHostValue`). But if a *later* kwarg fails to encode, the
// call never reaches the engine, so the engine never decodes — and never
// releases — the callables we registered for earlier kwargs, leaking them
// for the life of the process. To avoid that, we track every key registered
// during this encode and unregister them all if any kwarg fails.
func encodeCallArgs(kwargs map[string]any) ([]byte, error) {
	var registered []uint64
	entries, err := encodeKwargs(kwargs, &registered)
	if err != nil {
		rollbackRegisteredHostValues(registered)
		return nil, err
	}
	out, err := proto.Marshal(&pb.CallFunctionArgs{Kwargs: entries})
	if err != nil {
		// Marshal failure is unusual (the messages are valid by
		// construction), but treat it the same: the call won't reach the
		// engine, so release everything we registered.
		rollbackRegisteredHostValues(registered)
		return nil, err
	}
	return out, nil
}

// encodeKwargs encodes the kwargs map into inbound entries, recording every
// host-value key it registers into `registered` so the caller can roll them
// back on failure.
func encodeKwargs(kwargs map[string]any, registered *[]uint64) ([]*pb.InboundMapEntry, error) {
	var entries []*pb.InboundMapEntry
	for k, v := range kwargs {
		iv, err := goToInboundValueTracking(v, registered)
		if err != nil {
			return nil, fmt.Errorf("encoding arg %q: %w", k, err)
		}
		entries = append(entries, &pb.InboundMapEntry{
			Key:   &pb.InboundMapEntry_StringKey{StringKey: k},
			Value: iv,
		})
	}
	return entries, nil
}

// rollbackRegisteredHostValues drops every registry entry created during a
// failed encode, dropping the bridge's reference to the user's callables.
func rollbackRegisteredHostValues(keys []uint64) {
	for _, key := range keys {
		unregisterHostValue(key)
	}
}

// goToInboundValue encodes a single Go value with no registration tracking.
// Used by the host-call *result* encode path (`runHostCallable`), where the
// result is handed straight to the engine — there is no later sibling whose
// failure could orphan a callable, so no rollback bookkeeping is needed.
func goToInboundValue(v any) (*pb.InboundValue, error) {
	return goToInboundValueTracking(v, nil)
}

// goToInboundValueTracking encodes a single Go value. When `registered` is
// non-nil, every host-value key minted by the reflective callable fallback is
// appended to it so the encode path can roll the registrations back on a
// later failure (see `encodeCallArgs`).
func goToInboundValueTracking(v any, registered *[]uint64) (*pb.InboundValue, error) {
	if v == nil {
		return &pb.InboundValue{}, nil
	}
	switch val := v.(type) {
	case string:
		return &pb.InboundValue{Value: &pb.InboundValue_StringValue{StringValue: val}}, nil
	case int:
		return &pb.InboundValue{Value: &pb.InboundValue_IntValue{IntValue: int64(val)}}, nil
	case int64:
		return &pb.InboundValue{Value: &pb.InboundValue_IntValue{IntValue: val}}, nil
	case float64:
		return &pb.InboundValue{Value: &pb.InboundValue_FloatValue{FloatValue: val}}, nil
	case bool:
		return &pb.InboundValue{Value: &pb.InboundValue_BoolValue{BoolValue: val}}, nil
	case []byte:
		return &pb.InboundValue{Value: &pb.InboundValue_Uint8ArrayValue{Uint8ArrayValue: val}}, nil
	case []any:
		var items []*pb.InboundValue
		for _, item := range val {
			iv, err := goToInboundValueTracking(item, registered)
			if err != nil {
				return nil, err
			}
			items = append(items, iv)
		}
		return &pb.InboundValue{Value: &pb.InboundValue_ListValue{
			ListValue: &pb.InboundListValue{Values: items},
		}}, nil
	case map[string]any:
		var entries []*pb.InboundMapEntry
		for k, v2 := range val {
			iv, err := goToInboundValueTracking(v2, registered)
			if err != nil {
				return nil, err
			}
			entries = append(entries, &pb.InboundMapEntry{
				Key:   &pb.InboundMapEntry_StringKey{StringKey: k},
				Value: iv,
			})
		}
		return &pb.InboundValue{Value: &pb.InboundValue_MapValue{
			MapValue: &pb.InboundMapValue{Entries: entries},
		}}, nil
	case BamlHandle:
		return &pb.InboundValue{Value: &pb.InboundValue_Handle{
			Handle: &pb.BamlHandle{
				Key:        val.Key,
				HandleType: pb.BamlHandleType(val.HandleType),
			},
		}}, nil
	default:
		// Reflective fallback: a Go `func` (any signature) becomes a host
		// callable. We register it in the per-process registry and emit a
		// `Handle{key, HOST_VALUE_CALLABLE}`. The Rust decoder constructs a
		// `BexExternalValue::HostValue`; BAML invocations dispatch back into
		// Go via `bamlHostDispatch`.
		//
		// This deliberately accepts any `func(...) ...` shape; the dispatch
		// path coerces decoded args to the declared parameter types and
		// reports arity / type mismatches as `HostCallableInvalidArgument`.
		rv := reflect.ValueOf(v)
		if rv.IsValid() && rv.Kind() == reflect.Func {
			key := registerHostValue(rv)
			if registered != nil {
				*registered = append(*registered, key)
			}
			return &pb.InboundValue{Value: &pb.InboundValue_Handle{
				Handle: &pb.BamlHandle{
					Key:        key,
					HandleType: pb.BamlHandleType_HOST_VALUE_CALLABLE,
				},
			}}, nil
		}
		return nil, fmt.Errorf("unsupported type: %T", v)
	}
}

// decodeResult converts BamlOutboundValue protobuf bytes to a Go value.
func decodeResult(data []byte) (any, error) {
	var val pb.BamlOutboundValue
	if err := proto.Unmarshal(data, &val); err != nil {
		return nil, fmt.Errorf("decoding BamlOutboundValue: %w", err)
	}
	return outboundToGo(&val)
}

func outboundToGo(val *pb.BamlOutboundValue) (any, error) {
	if val == nil {
		return nil, nil
	}
	switch v := val.Value.(type) {
	case *pb.BamlOutboundValue_NullValue:
		return nil, nil
	case *pb.BamlOutboundValue_StringValue:
		return v.StringValue, nil
	case *pb.BamlOutboundValue_IntValue:
		return v.IntValue, nil
	case *pb.BamlOutboundValue_FloatValue:
		return v.FloatValue, nil
	case *pb.BamlOutboundValue_BoolValue:
		return v.BoolValue, nil
	case *pb.BamlOutboundValue_Uint8ArrayValue:
		return v.Uint8ArrayValue, nil
	case *pb.BamlOutboundValue_EnumValue:
		name := ""
		if v.EnumValue.Name != nil {
			name = v.EnumValue.Name.Name
		}
		return DynamicEnum{Name: name, Value: v.EnumValue.Value}, nil
	case *pb.BamlOutboundValue_ClassValue:
		name := ""
		if v.ClassValue.Name != nil {
			name = v.ClassValue.Name.Name
		}
		fields := make(map[string]any)
		for _, f := range v.ClassValue.Fields {
			fv, err := outboundToGo(f.Value)
			if err != nil {
				return nil, err
			}
			fields[f.Key] = fv
		}
		return DynamicClass{Name: name, Fields: fields}, nil
	case *pb.BamlOutboundValue_ListValue:
		var items []any
		for _, item := range v.ListValue.Items {
			gv, err := outboundToGo(item)
			if err != nil {
				return nil, err
			}
			items = append(items, gv)
		}
		return items, nil
	case *pb.BamlOutboundValue_MapValue:
		m := make(map[string]any)
		for _, entry := range v.MapValue.Entries {
			ev, err := outboundToGo(entry.Value)
			if err != nil {
				return nil, err
			}
			m[entry.Key] = ev
		}
		return m, nil
	case *pb.BamlOutboundValue_UnionVariantValue:
		inner, err := outboundToGo(v.UnionVariantValue.Value)
		if err != nil {
			return nil, err
		}
		return DynamicUnion{Variant: v.UnionVariantValue.ValueOptionName, Value: inner}, nil
	case *pb.BamlOutboundValue_HandleValue:
		return BamlHandle{
			Key:        v.HandleValue.Key,
			HandleType: int32(v.HandleValue.HandleType),
		}, nil
	case *pb.BamlOutboundValue_LiteralValue:
		lit := v.LiteralValue
		switch l := lit.Literal.(type) {
		case *pb.BamlTyLiteral_StringLiteral:
			return l.StringLiteral.Value, nil
		case *pb.BamlTyLiteral_IntLiteral:
			return l.IntLiteral.Value, nil
		case *pb.BamlTyLiteral_BoolLiteral:
			return l.BoolLiteral.Value, nil
		default:
			return nil, fmt.Errorf("unknown literal type: %T", lit.Literal)
		}
	default:
		return nil, fmt.Errorf("unsupported outbound value type: %T", val.Value)
	}
}

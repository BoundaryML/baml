package pkg

import (
	"fmt"

	pb "bridge_go/cffi/proto/baml_core/cffi/v1"

	"google.golang.org/protobuf/proto"
)

// encodeCallArgs converts Go kwargs to CallFunctionArgs protobuf bytes.
func encodeCallArgs(kwargs map[string]any) ([]byte, error) {
	var entries []*pb.InboundMapEntry
	for k, v := range kwargs {
		iv, err := goToInboundValue(v)
		if err != nil {
			return nil, fmt.Errorf("encoding arg %q: %w", k, err)
		}
		entries = append(entries, &pb.InboundMapEntry{
			Key:   &pb.InboundMapEntry_StringKey{StringKey: k},
			Value: iv,
		})
	}
	return proto.Marshal(&pb.CallFunctionArgs{Kwargs: entries})
}

func goToInboundValue(v any) (*pb.InboundValue, error) {
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
			iv, err := goToInboundValue(item)
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
			iv, err := goToInboundValue(v2)
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

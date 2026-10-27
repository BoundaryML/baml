package baml_go

import (
	"fmt"

	"github.com/boundaryml/baml-go/internal/cffi"
)

// How deep the wire nests, measured as the engine's decoder measures it: the
// decoder refuses a message nested more than wireNestingLimit protobuf
// messages deep, so a value is measured the same way before it is sent. The
// accounting mirrors bridge_ctypes' wire_depth.rs message for message: every
// list level costs two messages, every map or class level three, and a
// node's type statement counts beside its payload.
const (
	// wireNestingLimit is the decoder's recursion limit.
	wireNestingLimit = 100
	// argumentDepth is the messages enclosing a call argument's value, a
	// type argument's type or an invocation control's value:
	// CallFunctionArgs, then the InboundMapEntry, BamlTyArg or
	// InvocationOptions holding it.
	argumentDepth = 2
	// resultDepth is the message enclosing a callback's result or thrown
	// value: the HostCallCompletion whose outcome holds it.
	resultDepth = 1
)

// wireDepthError is a value nested deeper than the wire carries. It is the
// bridge's own refusal: the value may be perfectly valid BAML.
type wireDepthError struct {
	depth int
}

func (err *wireDepthError) Error() string {
	return fmt.Sprintf("value nests %d protobuf messages deep, deeper than the %d the wire carries", err.depth, wireNestingLimit)
}

// checkInboundDepth refuses value when it nests deeper than the wire carries
// below enclosing messages.
func checkInboundDepth(value *cffi.InboundValue, enclosing int) error {
	if depth := enclosing + inboundValueDepth(value); depth > wireNestingLimit {
		return &wireDepthError{depth: depth}
	}
	return nil
}

// checkTypeDepth refuses ty when it nests deeper than the wire carries below
// enclosing messages, counting ty's own message.
func checkTypeDepth(ty *cffi.BamlTy, enclosing int) error {
	if depth := enclosing + bamlTyDepth(ty); depth > wireNestingLimit {
		return &wireDepthError{depth: depth}
	}
	return nil
}

// inboundValueDepth is how many messages value nests below itself, counting
// its own type statement and the payload of every leaf. A scalar is 0; a list
// of scalars is 2 (InboundListValue, InboundValue).
func inboundValueDepth(value *cffi.InboundValue) int {
	if value == nil {
		return 0
	}
	statement := 0
	if value.ValueType != nil {
		statement = 1 + bamlTyDepth(value.ValueType)
	}
	payload := 0
	switch item := value.Value.(type) {
	case nil,
		*cffi.InboundValue_StringValue,
		*cffi.InboundValue_IntValue,
		*cffi.InboundValue_FloatValue,
		*cffi.InboundValue_BoolValue,
		*cffi.InboundValue_Uint8ArrayValue,
		*cffi.InboundValue_BigintValue,
		*cffi.InboundValue_JsNumberValue:
		payload = 0
	case *cffi.InboundValue_EnumValue, *cffi.InboundValue_Handle:
		// InboundEnumValue, BamlHandle: one message, no nesting.
		payload = 1
	case *cffi.InboundValue_ListValue:
		// InboundListValue, then each InboundValue.
		deepest := 0
		for _, element := range item.ListValue.GetValues() {
			deepest = max(deepest, 1+inboundValueDepth(element))
		}
		payload = 1 + deepest
	case *cffi.InboundValue_MapValue:
		// InboundMapValue, then InboundMapEntry, then InboundValue.
		payload = 1 + entriesDepth(item.MapValue.GetEntries())
	case *cffi.InboundValue_ClassValue:
		// InboundClassValue, then InboundMapEntry, then InboundValue.
		payload = 1 + entriesDepth(item.ClassValue.GetFields())
	case *cffi.InboundValue_TyValue:
		payload = 1 + bamlTyDepth(item.TyValue)
	case *cffi.InboundValue_MediaValue:
		payload = 1 + mediaDepth(item.MediaValue)
	case *cffi.InboundValue_PromptAstValue:
		payload = 1 + promptDepth(item.PromptAstValue)
	}
	return max(statement, payload)
}

func entriesDepth(entries []*cffi.InboundMapEntry) int {
	deepest := 0
	for _, entry := range entries {
		deepest = max(deepest, 1+entryDepth(entry))
	}
	return deepest
}

// entryDepth is what an InboundMapEntry nests: its value, or an
// InboundEnumValue key, the one key kind that is a message.
func entryDepth(entry *cffi.InboundMapEntry) int {
	key := 0
	if _, ok := entry.GetKey().(*cffi.InboundMapEntry_EnumKey); ok {
		key = 1
	}
	value := 0
	if entry.GetValue() != nil {
		value = 1 + inboundValueDepth(entry.GetValue())
	}
	return max(key, value)
}

// bamlTyDepth is how many messages a BamlTy nests below itself. A primitive
// is 1 (BamlTyPrimitive); list<int> is 3 (BamlTyList, BamlTy,
// BamlTyPrimitive).
func bamlTyDepth(ty *cffi.BamlTy) int {
	if ty == nil {
		return 0
	}
	nested := func(child *cffi.BamlTy) int {
		if child == nil {
			return 0
		}
		return 1 + bamlTyDepth(child)
	}
	deepest := func(children []*cffi.BamlTy) int {
		depth := 0
		for _, child := range children {
			depth = max(depth, nested(child))
		}
		return depth
	}
	switch item := ty.Ty.(type) {
	case nil:
		return 0
	case *cffi.BamlTy_Primitive,
		*cffi.BamlTy_Enum,
		*cffi.BamlTy_Literal,
		*cffi.BamlTy_Unknown,
		*cffi.BamlTy_Media,
		*cffi.BamlTy_EnumVariant,
		*cffi.BamlTy_RustType,
		*cffi.BamlTy_MetaType,
		*cffi.BamlTy_Resource,
		*cffi.BamlTy_PromptAst,
		*cffi.BamlTy_TypeVar,
		*cffi.BamlTy_Never:
		return 1
	case *cffi.BamlTy_ClassTy:
		return 1 + deepest(item.ClassTy.GetTypeArgs())
	case *cffi.BamlTy_TypeAlias:
		return 1 + deepest(item.TypeAlias.GetTypeArgs())
	case *cffi.BamlTy_List:
		return 1 + nested(item.List.GetItem())
	case *cffi.BamlTy_Map:
		return 1 + max(nested(item.Map.GetKey()), nested(item.Map.GetValue()))
	case *cffi.BamlTy_Optional:
		return 1 + nested(item.Optional.GetInner())
	case *cffi.BamlTy_Union:
		return 1 + deepest(item.Union.GetOptions())
	case *cffi.BamlTy_Interface:
		bindings := 0
		for _, binding := range item.Interface.GetBindings() {
			bindings = max(bindings, 1+nested(binding.GetTy()))
		}
		return 1 + max(deepest(item.Interface.GetTypeArgs()), bindings)
	case *cffi.BamlTy_Function:
		params := 0
		for _, param := range item.Function.GetParams() {
			params = max(params, 1+nested(param.GetTy()))
		}
		return 1 + max(params, nested(item.Function.GetRet()), nested(item.Function.GetThrows()))
	case *cffi.BamlTy_Future:
		return 1 + max(nested(item.Future.GetValue()), nested(item.Future.GetError()))
	case *cffi.BamlTy_AssociatedTypeProjection:
		projection := item.AssociatedTypeProjection
		return 1 + max(nested(projection.GetBase()), nested(projection.GetInterface()))
	default:
		panic(fmt.Sprintf("bamlTyDepth: unhandled BamlTy arm %T", item))
	}
}

func mediaDepth(media *cffi.BamlValueMedia) int {
	switch media.GetValue().(type) {
	case *cffi.BamlValueMedia_FileContent:
		return 1
	default:
		return 0
	}
}

func promptDepth(prompt *cffi.BamlValuePromptAst) int {
	switch item := prompt.GetValue().(type) {
	case *cffi.BamlValuePromptAst_Simple:
		return 1 + promptSimpleDepth(item.Simple)
	case *cffi.BamlValuePromptAst_Message:
		if item.Message.GetContent() == nil {
			return 1
		}
		return 2 + promptSimpleDepth(item.Message.GetContent())
	case *cffi.BamlValuePromptAst_Multiple:
		deepest := 0
		for _, element := range item.Multiple.GetItems() {
			deepest = max(deepest, 1+promptDepth(element))
		}
		return 1 + deepest
	default:
		return 0
	}
}

func promptSimpleDepth(simple *cffi.BamlValuePromptAstSimple) int {
	switch item := simple.GetValue().(type) {
	case *cffi.BamlValuePromptAstSimple_Media:
		return 1 + mediaDepth(item.Media)
	case *cffi.BamlValuePromptAstSimple_Multiple:
		deepest := 0
		for _, element := range item.Multiple.GetItems() {
			deepest = max(deepest, 1+promptSimpleDepth(element))
		}
		return 1 + deepest
	default:
		return 0
	}
}

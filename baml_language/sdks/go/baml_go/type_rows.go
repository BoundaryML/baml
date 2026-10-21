package baml_go

import (
	"errors"
	"fmt"
	"runtime"
	"slices"

	"github.com/boundaryml/baml-go/internal/cffi"
	"google.golang.org/protobuf/proto"
)

// typePosition is where a type crosses the wire, which decides the heads it
// may carry.
type typePosition uint8

const (
	// typeAnnotation is metadata beside an outbound value. It references
	// nothing, so it may label an unnameable declaration but never carries a
	// live row.
	typeAnnotation typePosition = iota + 1
	// typeHeld is a BAMLType Go holds: built from names, or returned by the
	// engine with its live rows and display labels.
	typeHeld
	// typeStated is a type Go sends. The engine resolves every head, so a
	// display label, which names nothing, is refused.
	typeStated
)

// headed is the head oneof every nominal type message carries.
type headed interface {
	GetName() string
	GetLive() uint64
	GetAnonymousLabel() string
}

// nominalNode returns value's head when value is a nominal type.
func nominalNode(value *cffi.BamlTy) (kind string, node headed, ok bool) {
	switch item := value.GetTy().(type) {
	case *cffi.BamlTy_ClassTy:
		return "class", item.ClassTy, item.ClassTy != nil
	case *cffi.BamlTy_Enum:
		return "enum", item.Enum, item.Enum != nil
	case *cffi.BamlTy_TypeAlias:
		return "type alias", item.TypeAlias, item.TypeAlias != nil
	case *cffi.BamlTy_Interface:
		return "interface", item.Interface, item.Interface != nil
	case *cffi.BamlTy_EnumVariant:
		return "enum variant", item.EnumVariant, item.EnumVariant != nil
	default:
		return "", nil, false
	}
}

// validateHead checks that a nominal head names its declaration in a way
// position admits. The oneof sets at most one arm, and an empty name, zero
// key or empty label names nothing, so the getters decide the arm exactly.
func validateHead(kind string, node headed, position typePosition) error {
	switch {
	case node.GetLive() != 0:
		if position == typeAnnotation {
			return fmt.Errorf("%s annotation carries a runtime declaration reference", kind)
		}
	case node.GetName() != "":
	case node.GetAnonymousLabel() != "":
		if position == typeStated {
			return fmt.Errorf("%s %q is an anonymous declaration, which cannot be stated", kind, node.GetAnonymousLabel())
		}
	default:
		return fmt.Errorf("%s descriptor names no declaration", kind)
	}
	return nil
}

// setLiveHead points a nominal node's live head at key.
func setLiveHead(value *cffi.BamlTy, key uint64) {
	switch item := value.GetTy().(type) {
	case *cffi.BamlTy_ClassTy:
		item.ClassTy.Head = &cffi.BamlTyClass_Live{Live: key}
	case *cffi.BamlTy_Enum:
		item.Enum.Head = &cffi.BamlTyEnum_Live{Live: key}
	case *cffi.BamlTy_TypeAlias:
		item.TypeAlias.Head = &cffi.BamlTyTypeAlias_Live{Live: key}
	case *cffi.BamlTy_Interface:
		item.Interface.Head = &cffi.BamlTyInterface_Live{Live: key}
	case *cffi.BamlTy_EnumVariant:
		item.EnumVariant.Head = &cffi.BamlTyEnumVariant_Live{Live: key}
	default:
		panic(fmt.Sprintf("baml_go: %T has no head to point at a live row", value.GetTy()))
	}
}

// bamlTypeChildren lists the types directly beneath value.
func bamlTypeChildren(value *cffi.BamlTy) []*cffi.BamlTy {
	switch item := value.GetTy().(type) {
	case *cffi.BamlTy_ClassTy:
		return item.ClassTy.GetTypeArgs()
	case *cffi.BamlTy_TypeAlias:
		return item.TypeAlias.GetTypeArgs()
	case *cffi.BamlTy_Interface:
		children := slices.Clone(item.Interface.GetTypeArgs())
		for _, binding := range item.Interface.GetBindings() {
			children = append(children, binding.GetTy())
		}
		return children
	case *cffi.BamlTy_List:
		return []*cffi.BamlTy{item.List.GetItem()}
	case *cffi.BamlTy_Map:
		return []*cffi.BamlTy{item.Map.GetKey(), item.Map.GetValue()}
	case *cffi.BamlTy_Optional:
		return []*cffi.BamlTy{item.Optional.GetInner()}
	case *cffi.BamlTy_Union:
		return item.Union.GetOptions()
	case *cffi.BamlTy_Function:
		children := make([]*cffi.BamlTy, 0, len(item.Function.GetParams())+2)
		for _, param := range item.Function.GetParams() {
			children = append(children, param.GetTy())
		}
		return append(children, item.Function.GetRet(), item.Function.GetThrows())
	case *cffi.BamlTy_Future:
		return []*cffi.BamlTy{item.Future.GetValue(), item.Future.GetError()}
	case *cffi.BamlTy_AssociatedTypeProjection:
		return []*cffi.BamlTy{item.AssociatedTypeProjection.GetBase(), item.AssociatedTypeProjection.GetInterface()}
	default:
		return nil
	}
}

// visitLiveHeads calls visit on every node of value whose head is live, once
// per occurrence, stopping at the first error.
func visitLiveHeads(value *cffi.BamlTy, visit func(node *cffi.BamlTy, key uint64) error) error {
	stack := []*cffi.BamlTy{value}
	for len(stack) > 0 {
		last := len(stack) - 1
		node := stack[last]
		stack = stack[:last]
		if node == nil {
			continue
		}
		if _, head, ok := nominalNode(node); ok && head.GetLive() != 0 {
			if err := visit(node, head.GetLive()); err != nil {
				return err
			}
		}
		stack = append(stack, bamlTypeChildren(node)...)
	}
	return nil
}

// liveHeadKeys lists the row key of every live head in value, once per
// occurrence: each occurrence is one engine reference.
func liveHeadKeys(value *cffi.BamlTy) []uint64 {
	var keys []uint64
	_ = visitLiveHeads(value, func(_ *cffi.BamlTy, key uint64) error {
		keys = append(keys, key)
		return nil
	})
	return keys
}

// repointLiveHeads returns a copy of value whose every live head names the
// key replace returns for it, leaving value untouched.
func repointLiveHeads(value *cffi.BamlTy, replace func(uint64) (uint64, error)) (*cffi.BamlTy, error) {
	copied := proto.Clone(value).(*cffi.BamlTy)
	err := visitLiveHeads(copied, func(node *cffi.BamlTy, key uint64) error {
		replacement, err := replace(key)
		if err != nil {
			return err
		}
		setLiveHead(node, replacement)
		return nil
	})
	return copied, err
}

// liveTypeRows owns the engine references behind a BAMLType's live heads,
// one per occurrence, and releases them once no BAMLType shares it. A type
// composed from others retains their rows rather than cloning them, so
// Array, Optional and the composite constructors never release twice.
type liveTypeRows struct {
	// keys are the references this owner releases.
	keys []uint64
	// parts are the owners of composed subtrees, kept reachable.
	parts []*liveTypeRows
}

var (
	cloneOutboundTypeRow = nativeHandleClone
	releaseTypeRow       = nativeHandleRelease
)

func finalizeLiveTypeRows(rows *liveTypeRows) {
	for _, key := range rows.keys {
		releaseTypeRow(key)
	}
	rows.keys = nil
}

// composeBAMLType is the type value built around parts' trees. It shares
// their rows and keeps the first part's error, so a composite of an invalid
// part reports that part's fault.
func composeBAMLType(value *cffi.BamlTy, parts ...BAMLType) BAMLType {
	var owners []*liveTypeRows
	for _, part := range parts {
		if part.err != nil {
			return BAMLType{err: part.err}
		}
		if part.rows != nil && !slices.Contains(owners, part.rows) {
			owners = append(owners, part.rows)
		}
	}
	switch len(owners) {
	case 0:
		return BAMLType{value: value}
	case 1:
		return BAMLType{value: value, rows: owners[0]}
	default:
		return BAMLType{value: value, rows: &liveTypeRows{parts: owners}}
	}
}

// adoptReturnedType copies a `type` value the engine returned into a BAMLType
// holding its own reference per live head, so it outlives the result that
// carried it, as media and opaque handles do.
func adoptReturnedType(returned *cffi.BamlTy) (BAMLType, error) {
	rows := &liveTypeRows{}
	adopted, err := repointLiveHeads(returned, func(key uint64) (uint64, error) {
		cloned, err := cloneOutboundTypeRow(key)
		if err != nil {
			return 0, fmt.Errorf("clone runtime declaration reference: %w", err)
		}
		if cloned == 0 {
			return 0, errors.New("clone runtime declaration reference: runtime returned a zero handle")
		}
		rows.keys = append(rows.keys, cloned)
		return cloned, nil
	})
	if err != nil {
		finalizeLiveTypeRows(rows)
		return BAMLType{}, err
	}
	if len(rows.keys) == 0 {
		return BAMLType{value: adopted}, nil
	}
	runtime.SetFinalizer(rows, finalizeLiveTypeRows)
	return BAMLType{value: adopted, rows: rows}, nil
}

// transfers reports whether stating value hands the engine references, so
// its encoding must wait for a call's input transaction.
func (value BAMLType) transfers() bool {
	return value.err == nil && len(liveHeadKeys(value.value)) != 0
}

// stated is value's wire spelling for one inbound occurrence. The engine
// drains one reference per live head, so each is a fresh clone of the row Go
// keeps, journaled so a request that never reaches the engine releases it.
func (value BAMLType) stated(transaction *inputTransaction) (*cffi.BamlTy, error) {
	if err := validateBAMLTypeValue(value, typeStated); err != nil {
		return nil, err
	}
	if !value.transfers() {
		return value.value, nil
	}
	if value.rows == nil {
		return nil, errors.New("type names a runtime declaration without holding a reference to it")
	}
	if transaction == nil {
		return nil, errors.New("a type naming a runtime declaration is sent only within a call's input transaction")
	}
	stated, err := repointLiveHeads(value.value, func(key uint64) (uint64, error) {
		cloned, err := cloneInboundHandle(key)
		if err != nil {
			return 0, fmt.Errorf("clone runtime declaration reference for input: %w", err)
		}
		transaction.own(cloned)
		return cloned, nil
	})
	runtime.KeepAlive(value.rows)
	if err != nil {
		return nil, err
	}
	return stated, nil
}

// typesTransfer reports whether stating any of types hands the engine
// references.
func typesTransfer(types []BAMLType) bool {
	return slices.ContainsFunc(types, BAMLType.transfers)
}

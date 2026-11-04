// Go-specific threshold/runtime coverage. The default typed-union arity is 3,
// so these four-arm shapes deliberately project to `any` while BAML retains
// and validates the full canonical type.

type SmallAlias = int | string
type SmallAliasReordered = string | int
type LargeAlias = int | string | float | bool

function round_trip_small_alias(value: SmallAlias) -> SmallAliasReordered {
  value
}

function round_trip_composed_alias(value: SmallAlias | bool) -> bool | SmallAliasReordered {
  value
}

function round_trip_large_alias(value: LargeAlias) -> LargeAlias | null {
  value
}

function round_trip_nullable_large_union(value: LargeAlias | null) -> null | LargeAlias {
  value
}

function round_trip_large_union(value: int | string | float | bool) -> int | string | float | bool {
  value
}

function round_trip_large_union_list(values: (int | string | float | bool)[]) -> (int | string | float | bool)[] {
  values
}

function round_trip_union_of_containers(value: int[] | string[] | map<string, int> | map<string, string>) -> int[] | string[] | map<string, int> | map<string, string> {
  value
}

// These structurally overlapping arms require authoritative selected-arm
// metadata. Empty payloads have no values from which to infer nullability, and
// non-empty optional containers may contain only non-null values.
function round_trip_overlapping_lists(value: int[] | int?[]) -> int?[] | int[] {
  value
}

function round_trip_overlapping_maps(value: map<string, int> | map<string, int?>) -> map<string, int?> | map<string, int> {
  value
}

// Literal arms share their Go surface type with one another and with their
// broad primitive. Generated literal constructors are value-free and the wire
// envelope carries the exact chosen type.
function round_trip_string_and_literal(value: string | "draft") -> "draft" | string {
  value
}

function round_trip_string_literals(value: "draft" | "published") -> "published" | "draft" {
  value
}

type ReorderedLiterals = "published" | "draft"

function round_trip_reordered_literals(value: ReorderedLiterals) -> "draft" | "published" {
  value
}

function round_trip_int_literals(value: -1 | 42) -> 42 | -1 {
  value
}

function round_trip_bool_literals(value: true | false) -> false | true {
  value
}

function round_trip_bigint_literals(value: 42n | 123456789012345678901234567890n) -> 123456789012345678901234567890n | 42n {
  value
}

// Float literal union types cannot be spelled in BAML today: the parser
// deliberately rejects them. The Go generator still handles compiler-owned
// Float literal CodegenTy values, covered by generator and runtime bit tests.

// Adversarial nominal identities: primitive-vs-class, same nominal leaf from
// two namespaces, enum-vs-class, and a protected/predeclared-looking name.
function round_trip_string_or_class(value: string | root.go_codegen.union_edges.left.String) -> root.go_codegen.union_edges.left.String | string {
  value
}

function round_trip_same_leaf_classes(value: root.go_codegen.union_edges.left.Shared | root.go_codegen.union_edges.right.Shared) -> root.go_codegen.union_edges.right.Shared | root.go_codegen.union_edges.left.Shared {
  value
}

function round_trip_enum_or_class(value: root.go_codegen.union_edges.left.Clash | root.go_codegen.union_edges.right.Clash) -> root.go_codegen.union_edges.right.Clash | root.go_codegen.union_edges.left.Clash {
  value
}

function round_trip_protected_nominal(value: string | root.go_codegen.union_edges.left.Type) -> root.go_codegen.union_edges.left.Type | string {
  value
}

class DynamicUnionContainer {
  value int | string | float | bool
  values (int | string | float | bool)[]
  lookup map<string, int | string | float | bool>
}

function round_trip_dynamic_union_container(value: DynamicUnionContainer) -> DynamicUnionContainer {
  value
}

// A union of four classes exceeds the typed-union arity, so the field projects
// to `any` in Go. A value the SDK decodes from such a field (directly or
// through a generic `T`) must encode back under the same declared type.
class ClassArmAws {
  role_arn string
  platform "aws"
}

class ClassArmGcp {
  project string
  platform "gcp"
}

class ClassArmAzure {
  tenant string
  platform "azure"
}

class ClassArmOther {
  note string
  platform "other"
}

type ClassArms = ClassArmAws | ClassArmGcp | ClassArmAzure | ClassArmOther

class ClassUnionHolder {
  config ClassArms?
}

function make_class_union_holder() -> ClassUnionHolder {
  ClassUnionHolder { config: ClassArmAws { role_arn: "arn", platform: "aws" } }
}

function round_trip_class_union_holder(value: ClassUnionHolder) -> ClassUnionHolder {
  value
}

function class_union_holder_to_json<T>(value: T) -> string throws baml.json.SerializationError {
  baml.json.stringify(value)
}

function class_union_holder_from_json<T>(source: string) -> T throws baml.json.ParseError | baml.json.DecodeError {
  baml.json.decode<T>(source)
}

// Add alongside the shared interfaces fixture for generated input-role probes.
interface Mapper<T> {
    type Error = never
    function map(self, value: T) -> T throws Self.Error
}
class DualMapper {
    implements Mapper<int> {
        type Error = string
        function map(self, value: int) -> int throws never { value + 1 }
    }
    implements Mapper<string> {
        function map(self, value: string) -> string throws never { value + "!" }
    }
}
function dual_mapper() -> DualMapper throws never { DualMapper {} }
function use_int_mapper(value: Mapper<int, Error=string>) -> int throws string { value.map(9) }
function use_text_mapper(value: Mapper<string, Error=never>) -> string throws never { value.map("Ada") }
function optional_mapper(value: Mapper<string, Error=never>?) -> string throws never { value?.map("Ada") ?? "none" }
// Real declarations win collisions with derived role names.
class MapperInput { name: string }
class _MapperView { name: string }

interface Tagged {}
class TaggedRecord {
    name: string,
    implements Tagged {}
}
function use_tagged(value: Tagged) -> string throws never { "tagged" }

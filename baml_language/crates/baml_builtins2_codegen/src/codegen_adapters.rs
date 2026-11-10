//! Adapters against the shared object format (design section 5).
//!
//! For every [`triage`](crate::triage) row whose form has an adapter, emit:
//!
//! - one function per slot kind (`{fn_name}_raw8`, `_ptr`, `_tagged16`)
//!   that calls the body `bodies::{fn_name}` monomorphized at that kind,
//!   public when the row is [`AdapterForm::Specialized`];
//! - the generic adapter `{fn_name}`, which reads the receiver's shape,
//!   converts generic arguments from their universal [`Tagged`] form to the
//!   slot's representation, and branches to the per-kind function.
//!
//! String receivers have one slot kind (pointer), so they get one per-kind
//! function and a generic adapter that checks the receiver's kind.
//!
//! The emitted code uses only `bex_vm_types::layout` plus the includer's
//! `bodies` and `support` modules.

use std::collections::BTreeMap;

use proc_macro2::TokenStream;
use quote::{format_ident, quote};

use crate::{
    BamlType, NativeBuiltin,
    triage::{AdapterForm, TriageRow},
};

/// Why an adapter could not be generated.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AdapterGenError {
    pub path: String,
    pub problem: AdapterProblem,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AdapterProblem {
    /// The triage row names no VM native declaration.
    MissingDeclaration,
    /// Only `Array`, `Map` and `String` instance methods have adapters.
    UnsupportedReceiver(String),
    /// A parameter or return type has no slot-kind mapping.
    UnsupportedType(String),
}

impl std::fmt::Display for AdapterGenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let path = &self.path;
        match &self.problem {
            AdapterProblem::MissingDeclaration => {
                write!(f, "adapter `{path}`: no VM native declaration")
            }
            AdapterProblem::UnsupportedReceiver(class) => {
                write!(
                    f,
                    "adapter `{path}`: receiver `{class}` has no adapter shape"
                )
            }
            AdapterProblem::UnsupportedType(ty) => {
                write!(f, "adapter `{path}`: type `{ty}` has no slot-kind mapping")
            }
        }
    }
}

/// The slot kinds a per-kind function is monomorphized at.
#[derive(Clone, Copy)]
enum Arm {
    Raw8,
    Ptr,
    Tagged16,
}

const ARMS: [Arm; 3] = [Arm::Raw8, Arm::Ptr, Arm::Tagged16];

impl Arm {
    const fn suffix(self) -> &'static str {
        match self {
            Arm::Raw8 => "raw8",
            Arm::Ptr => "ptr",
            Arm::Tagged16 => "tagged16",
        }
    }

    fn elem(self) -> TokenStream {
        match self {
            Arm::Raw8 => quote!(RawWord),
            Arm::Ptr => quote!(ObjPtr),
            Arm::Tagged16 => quote!(Tagged),
        }
    }

    fn pattern(self) -> TokenStream {
        match self {
            Arm::Raw8 => quote!(SlotKind::Raw(_)),
            Arm::Ptr => quote!(SlotKind::Ptr),
            Arm::Tagged16 => quote!(SlotKind::Tagged16),
        }
    }
}

/// The receiver classes adapters exist for.
#[derive(Clone, Copy, PartialEq, Eq)]
enum ReceiverShape {
    List,
    Map,
    Str,
}

impl ReceiverShape {
    fn of_class(class: &str) -> Option<Self> {
        const CLASSES: [(&str, ReceiverShape); 3] = [
            ("Array", ReceiverShape::List),
            ("Map", ReceiverShape::Map),
            ("String", ReceiverShape::Str),
        ];
        CLASSES
            .iter()
            .find(|(name, _)| *name == class)
            .map(|(_, shape)| *shape)
    }

    fn param(self) -> proc_macro2::Ident {
        match self {
            ReceiverShape::List => format_ident!("array"),
            ReceiverShape::Map => format_ident!("map"),
            ReceiverShape::Str => format_ident!("string"),
        }
    }

    /// Which declared class generics are the element and the key.
    fn roles(self, generics: &[String]) -> Roles<'_> {
        let generic = |index: usize| generics.get(index).map(String::as_str);
        match self {
            ReceiverShape::List => Roles {
                elem: generic(0),
                key: None,
            },
            ReceiverShape::Map => Roles {
                elem: generic(1),
                key: generic(0),
            },
            ReceiverShape::Str => Roles {
                elem: None,
                key: None,
            },
        }
    }

    /// The `layout::Kind` variant a receiver of this shape has.
    fn kind(self) -> proc_macro2::Ident {
        match self {
            ReceiverShape::List => format_ident!("List"),
            ReceiverShape::Map => format_ident!("Map"),
            ReceiverShape::Str => format_ident!("Str"),
        }
    }

    /// The `Shape` check that validates the receiver and yields its slot.
    fn slot_of(self) -> TokenStream {
        match self {
            ReceiverShape::List => quote!(list_elem),
            ReceiverShape::Map => quote!(map_value),
            ReceiverShape::Str => quote!(string),
        }
    }

    /// The arms a receiver of this shape is monomorphized at.
    fn arms(self) -> &'static [Arm] {
        match self {
            ReceiverShape::List | ReceiverShape::Map => &ARMS,
            ReceiverShape::Str => &ARMS[1..2],
        }
    }
}

/// A parameter or return position in ABI terms.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Abi {
    Int,
    Bool,
    Unit,
    /// A string object.
    Str,
    /// A list object.
    List,
    /// A `uint8array` object.
    Bytes,
    /// The receiver's varying slot: a list's `T`, a map's `V`.
    Elem,
    OptElem,
    /// A map's `K`; adapters take string keys.
    Key,
}

/// Which declared generics play which role.
struct Roles<'a> {
    elem: Option<&'a str>,
    key: Option<&'a str>,
}

impl Abi {
    fn of(ty: &BamlType, roles: &Roles<'_>) -> Option<Abi> {
        let is = |role: Option<&str>, name: &str| role == Some(name);
        match ty {
            BamlType::Int => Some(Abi::Int),
            BamlType::Bool => Some(Abi::Bool),
            BamlType::Null => Some(Abi::Unit),
            BamlType::String => Some(Abi::Str),
            BamlType::List(_) => Some(Abi::List),
            BamlType::Uint8Array => Some(Abi::Bytes),
            BamlType::Generic(name) if is(roles.elem, name) => Some(Abi::Elem),
            BamlType::Generic(name) if is(roles.key, name) => Some(Abi::Key),
            BamlType::Optional(inner) if Abi::of(inner, roles) == Some(Abi::Elem) => {
                Some(Abi::OptElem)
            }
            _ => None,
        }
    }

    /// The Rust type in a function whose varying slot is `elem`. Generic
    /// adapters pass `Tagged` for `elem` and take keys universally.
    fn rust_type(self, elem: &TokenStream, generic: bool) -> TokenStream {
        match self {
            Abi::Int => quote!(i64),
            Abi::Bool => quote!(bool),
            Abi::Unit => quote!(()),
            Abi::Str | Abi::List | Abi::Bytes => quote!(ObjPtr),
            Abi::Elem => elem.clone(),
            Abi::OptElem => quote!(Option<#elem>),
            Abi::Key if generic => quote!(Tagged),
            Abi::Key => quote!(ObjPtr),
        }
    }

    /// A statement converting a generic adapter's argument before the
    /// per-kind call. Conversions read or allocate through the runtime, so
    /// they cannot nest in a call that already borrows it.
    fn convert_arg(self, name: &proc_macro2::Ident) -> Option<TokenStream> {
        match self {
            Abi::Elem => Some(quote!(let #name = Slots::untag(rt, #name, slot)?;)),
            Abi::Int | Abi::Bool | Abi::Unit | Abi::Str | Abi::List | Abi::Bytes => None,
            Abi::OptElem | Abi::Key => None,
        }
    }

    /// `call` with its result bound to `result`, or just run when it is `()`.
    fn bind_result(self, call: &TokenStream) -> TokenStream {
        match self {
            Abi::Unit => quote!(#call?;),
            Abi::Int | Abi::Bool | Abi::Str | Abi::List | Abi::Bytes => {
                quote!(let result = #call?;)
            }
            Abi::Elem | Abi::OptElem | Abi::Key => quote!(let result = #call?;),
        }
    }

    /// The generic adapter's result expression for a per-kind `result`.
    fn convert_result(self) -> TokenStream {
        match self {
            Abi::Elem => quote!(Ok(Slots::to_tagged(&*rt, result, slot))),
            Abi::OptElem => quote!(Ok(result.map(|elem| Slots::to_tagged(&*rt, elem, slot)))),
            Abi::Unit => quote!(Ok(())),
            Abi::Int | Abi::Bool | Abi::Str | Abi::List | Abi::Bytes | Abi::Key => {
                quote!(Ok(result))
            }
        }
    }

    /// The interpreter glue's extraction of `args[index]`.
    fn value_arg(self, index: usize) -> TokenStream {
        match self {
            Abi::Int => quote!(m.int_arg(args[#index])?),
            Abi::Str => quote!(m.string_arg(args[#index])?),
            Abi::List => quote!(m.list_arg(args[#index])?),
            Abi::Elem => quote!(args[#index]),
            Abi::Key => quote!(m.key_arg(args[#index])?),
            Abi::Bool | Abi::Unit | Abi::Bytes | Abi::OptElem => {
                unreachable!("no adapter parameter is resolved as {self:?}")
            }
        }
    }

    /// The interpreter glue's `Value` for a body's `result`.
    fn value_result(self) -> TokenStream {
        match self {
            Abi::Int => quote!(crate::package_baml::value_model::int_value(result)),
            Abi::Bool => quote!(Ok(Value::bool(result))),
            Abi::Unit => quote!(Ok(Value::NULL)),
            Abi::Str | Abi::List | Abi::Bytes => {
                quote!(Ok(crate::package_baml::value_model::ptr_value(result)))
            }
            Abi::Elem => quote!(Ok(result)),
            Abi::OptElem => quote!(Ok(result.unwrap_or(Value::NULL))),
            Abi::Key => unreachable!("no adapter returns a map key"),
        }
    }
}

impl std::fmt::Debug for Abi {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = match self {
            Abi::Int => "int",
            Abi::Bool => "bool",
            Abi::Unit => "null",
            Abi::Str => "string",
            Abi::List => "list",
            Abi::Bytes => "uint8array",
            Abi::Elem => "element",
            Abi::OptElem => "optional element",
            Abi::Key => "map key",
        };
        f.write_str(name)
    }
}

/// A declaration resolved into ABI terms.
struct AdapterSig<'a> {
    decl: &'a NativeBuiltin,
    specialized: bool,
    receiver: ReceiverShape,
    params: Vec<(proc_macro2::Ident, Abi)>,
    ret: Abi,
}

impl<'a> AdapterSig<'a> {
    fn resolve(decl: &'a NativeBuiltin, form: AdapterForm) -> Result<Self, AdapterProblem> {
        let receiver = decl
            .receiver
            .as_ref()
            .filter(|r| !r.receiver_type.is_static());
        let class = receiver.map_or("", |r| r.class_name.as_str());
        let shape = ReceiverShape::of_class(class)
            .ok_or_else(|| AdapterProblem::UnsupportedReceiver(class.to_string()))?;
        let generics = receiver.map_or(&[][..], |r| r.class_generics.as_slice());
        let roles = shape.roles(generics);
        let abi = |ty: &BamlType| {
            Abi::of(ty, &roles).ok_or_else(|| AdapterProblem::UnsupportedType(format!("{ty:?}")))
        };
        let params = decl
            .params
            .iter()
            .map(|p| Ok((format_ident!("{}", p.name), abi(&p.ty)?)))
            .collect::<Result<Vec<_>, AdapterProblem>>()?;
        let ret = abi(&decl.return_type)?;
        Ok(Self {
            decl,
            specialized: form == AdapterForm::Specialized,
            receiver: shape,
            params,
            ret,
        })
    }

    fn name(&self) -> proc_macro2::Ident {
        format_ident!("{}", self.decl.fn_name)
    }

    fn per_kind_name(&self, arm: Arm) -> proc_macro2::Ident {
        format_ident!("{}_{}", self.decl.fn_name, arm.suffix())
    }

    /// `name: Type` for every parameter after the receiver.
    fn param_list(&self, elem: &TokenStream, generic: bool) -> Vec<TokenStream> {
        self.params
            .iter()
            .map(|(name, abi)| {
                let ty = abi.rust_type(elem, generic);
                quote!(#name: #ty)
            })
            .collect()
    }

    fn doc(&self, form: &str) -> String {
        let params: Vec<String> = self
            .decl
            .params
            .iter()
            .map(|p| format!("{}: {}", p.name, type_text(&p.ty)))
            .collect();
        format!(
            " {form} adapter for `{}(self, {}) -> {}`.",
            self.decl.path,
            params.join(", "),
            type_text(&self.decl.return_type)
        )
    }

    /// The body's turbofish for slot representation `elem`; string bodies
    /// have no element parameter.
    fn turbofish(&self, runtime: &TokenStream, elem: &TokenStream) -> TokenStream {
        match self.receiver {
            ReceiverShape::List | ReceiverShape::Map => quote!(::<#runtime, #elem>),
            ReceiverShape::Str => quote!(),
        }
    }

    fn per_kind_fn(&self, arm: Arm) -> TokenStream {
        let name = self.per_kind_name(arm);
        let body = self.name();
        let receiver = self.receiver.param();
        let elem = arm.elem();
        let params = self.param_list(&elem, false);
        let args = self.params.iter().map(|(name, _)| name);
        let ret = self.ret.rust_type(&elem, false);
        let turbofish = self.turbofish(&quote!(R), &elem);
        let vis = if self.specialized {
            quote!(pub)
        } else {
            quote!()
        };
        let doc = self.doc(&format!("Slot kind `{}`", arm.suffix()));
        quote! {
            #[doc = #doc]
            #vis fn #name<R: ObjectModel>(rt: &mut R, #receiver: ObjPtr, #(#params),*) -> Result<#ret, Fault> {
                bodies::#body #turbofish(rt, #receiver, #(#args),*)
            }
        }
    }

    fn generic_fn(&self) -> TokenStream {
        let name = self.name();
        let receiver = self.receiver.param();
        let tagged = quote!(Tagged);
        let params = self.param_list(&tagged, true);
        let ret = self.ret.rust_type(&tagged, true);
        let doc = self.doc("Generic");
        let body = match self.receiver {
            ReceiverShape::List | ReceiverShape::Map => self.slot_dispatch(),
            ReceiverShape::Str => self.str_dispatch(),
        };
        quote! {
            #[doc = #doc]
            pub fn #name<R: ObjectModel>(rt: &mut R, #receiver: ObjPtr, #(#params),*) -> Result<#ret, Fault> {
                #body
            }
        }
    }

    /// The call to the per-kind function for `arm`, converted to and from
    /// the generic adapter's universal forms.
    fn per_kind_call(&self, arm: Arm) -> TokenStream {
        let per_kind = self.per_kind_name(arm);
        let receiver = self.receiver.param();
        let conversions = self
            .params
            .iter()
            .filter_map(|(name, abi)| abi.convert_arg(name));
        let args = self.params.iter().map(|(name, _)| name);
        let bind = self
            .ret
            .bind_result(&quote!(#per_kind(rt, #receiver, #(#args),*)));
        let result = self.ret.convert_result();
        quote! {{
            #(#conversions)*
            #bind
            #result
        }}
    }

    /// Read the receiver's slot and branch to the matching per-kind function.
    fn slot_dispatch(&self) -> TokenStream {
        let receiver = self.receiver.param();
        let slot_of = self.receiver.slot_of();
        let arms = ARMS.iter().map(|&arm| {
            let pattern = arm.pattern();
            let call = self.per_kind_call(arm);
            quote!(#pattern => #call)
        });
        let keys = self
            .params
            .iter()
            .filter(|(_, abi)| *abi == Abi::Key)
            .map(|(name, _)| quote!(let #name = string_key(rt, #name)?;));
        quote! {
            let slot = rt.shape(#receiver).#slot_of()?;
            #(#keys)*
            match slot {
                #(#arms)*
            }
        }
    }

    /// Check the receiver is a string and call the pointer function.
    fn str_dispatch(&self) -> TokenStream {
        let receiver = self.receiver.param();
        let call = self.per_kind_call(Arm::Ptr);
        quote! {
            rt.shape(#receiver).string()?;
            #call
        }
    }

    fn emit(&self) -> TokenStream {
        let per_kind = self
            .receiver
            .arms()
            .iter()
            .map(|&arm| self.per_kind_fn(arm));
        let generic = self.generic_fn();
        quote! { #generic #(#per_kind)* }
    }

    /// The interpreter's `__glue_{method}` (and thread-local twin) for this
    /// builtin: the body instantiated at `Value` over the interpreter's
    /// `ValueModel`, with the yielding path as fallback when the builtin may
    /// yield.
    fn value_glue(&self, method: &str) -> TokenStream {
        let shared = self.value_glue_fn(method, Locality::Shared);
        let local = self
            .decl
            .local_twin
            .then(|| self.value_glue_fn(method, Locality::ThreadLocal));
        quote! { #shared #local }
    }

    fn value_glue_fn(&self, method: &str, locality: Locality) -> TokenStream {
        let body = self.name();
        let receiver = self.receiver.param();
        let kind = self.receiver.kind();
        let extractions = self.params.iter().enumerate().map(|(i, (name, abi))| {
            let value = abi.value_arg(i + 1);
            quote!(let #name = #value;)
        });
        let args = self.params.iter().map(|(name, _)| name);
        let turbofish = self.turbofish(
            &quote!(crate::package_baml::value_model::ValueModel<'_>),
            &quote!(Value),
        );
        let bind = self.ret.bind_result(
            &quote!(crate::package_baml::bodies::#body #turbofish(m, #receiver, #(#args),*)),
        );
        let result = self.ret.value_result();
        let fallback = if self.decl.may_yield {
            let yielding = format_ident!("{}_yielding", self.decl.fn_name);
            quote!(Some(crate::package_baml::bodies::#yielding))
        } else {
            quote!(None)
        };
        let glue = quote! {
            |m, args| {
                let #receiver = m.receiver(args[0], bex_vm_types::layout::Kind::#kind)?;
                #(#extractions)*
                #bind
                #result
            }
        };
        match locality {
            Locality::Shared => {
                let name = format_ident!("__glue_{method}");
                quote! {
                    fn #name(vm: &mut BexVm, args: &[Value]) -> NativeCallResult {
                        crate::package_baml::value_model::run(vm, args, #fallback, #glue)
                    }
                }
            }
            Locality::ThreadLocal => {
                let name = format_ident!("__glue_{method}__local");
                quote! {
                    /// The glue for a receiver escape analysis proved thread-local.
                    ///
                    /// SAFETY: the native backend selects this `$local` twin only for
                    /// a receiver escape analysis proved thread-local, which is
                    /// `run_thread_local`'s contract.
                    #[allow(unsafe_code)]
                    fn #name(vm: &mut BexVm, args: &[Value]) -> NativeCallResult {
                        unsafe {
                            crate::package_baml::value_model::run_thread_local(vm, args, #fallback, #glue)
                        }
                    }
                }
            }
        }
    }
}

/// Which container access an interpreter glue function uses.
#[derive(Clone, Copy)]
enum Locality {
    Shared,
    ThreadLocal,
}

/// BAML spelling of a declared type, for doc comments.
fn type_text(ty: &BamlType) -> String {
    match ty {
        BamlType::String => "string".into(),
        BamlType::Int => "int".into(),
        BamlType::Bigint => "bigint".into(),
        BamlType::Float => "float".into(),
        BamlType::Bool => "bool".into(),
        BamlType::Null => "null".into(),
        BamlType::Uint8Array => "uint8array".into(),
        BamlType::List(inner) => format!("{}[]", type_text(inner)),
        BamlType::Map(key, value) => format!("map<{}, {}>", type_text(key), type_text(value)),
        BamlType::Optional(inner) => format!("{}?", type_text(inner)),
        BamlType::Generic(name) | BamlType::Media(name) | BamlType::Named(name) => name.clone(),
        BamlType::RustType => "$rust_type".into(),
    }
}

/// The interpreter glue methods (`__glue_{method}` and its thread-local
/// twin) for a builtin whose triage row has an adapter, as trait-method
/// source; `None` for every other builtin, which keeps its `Value` glue.
///
/// # Panics
///
/// When an adapter row's declaration has no adapter shape; the same check
/// fails [`generate_adapters`] with the reason.
pub(crate) fn value_glue_methods(decl: &NativeBuiltin, method: &str) -> Option<String> {
    let row = crate::triage::row_for(&decl.path).filter(|row| row.form.has_adapter())?;
    let sig = AdapterSig::resolve(decl, row.form).unwrap_or_else(|problem| {
        panic!("baml codegen error: adapter `{}`: {problem:?}", decl.path)
    });
    let glue = sig.value_glue(method);
    let formatted = crate::format_tokens(&quote!(trait Glue { #glue }));
    let methods: Vec<&str> = formatted.lines().skip(1).collect();
    let methods = &methods[..methods.len().saturating_sub(1)];
    Some(methods.join("\n") + "\n")
}

/// Emit adapters for every `rows` entry with an adapter form, resolved
/// against `declarations` (VM natives).
pub fn generate_adapters(
    declarations: &[NativeBuiltin],
    rows: &[TriageRow],
) -> Result<String, AdapterGenError> {
    let by_path: BTreeMap<&str, &NativeBuiltin> =
        declarations.iter().map(|d| (d.path.as_str(), d)).collect();
    let sigs = rows
        .iter()
        .filter(|row| row.form.has_adapter())
        .map(|row| {
            let fail = |problem| AdapterGenError {
                path: row.path.to_string(),
                problem,
            };
            let decl = by_path
                .get(row.path)
                .ok_or_else(|| fail(AdapterProblem::MissingDeclaration))?;
            AdapterSig::resolve(decl, row.form).map_err(fail)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let paths = sigs.iter().map(|sig| sig.decl.path.as_str());
    let adapters = sigs.iter().map(AdapterSig::emit);
    let tokens = quote! {
        use bex_vm_types::layout::{
            Fault, ObjPtr, ObjectModel, RawWord, SlotKind, Slots, Tagged, string_key,
        };

        use crate::package_baml::bodies;

        /// Native keys with a generated adapter, in triage order.
        pub const ADAPTER_PATHS: &[&str] = &[#(#paths),*];

        #(#adapters)*
    };
    Ok(crate::format_tokens(&tokens))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::triage::{self, Group};

    fn generated() -> String {
        let declarations = triage::native_inventory().unwrap();
        generate_adapters(&declarations, triage::rows()).unwrap_or_else(|e| panic!("{e}"))
    }

    #[test]
    fn every_adapter_row_generates() {
        let output = generated();
        let adapters = triage::rows().iter().filter(|r| r.form.has_adapter());
        for row in adapters {
            let name = row.path.replace(['.', '$'], "_").to_lowercase();
            assert!(
                output.contains(&format!("pub fn {name}<R: ObjectModel>")),
                "{name}"
            );
        }
    }

    #[test]
    fn specialized_rows_export_one_function_per_slot_kind() {
        let output = generated();
        for suffix in ["raw8", "ptr", "tagged16"] {
            assert!(output.contains(&format!("pub fn baml_array_push_{suffix}<R: ObjectModel>")));
        }
        assert!(output.contains("pub fn baml_string_char_count_ptr<R: ObjectModel>"));
        assert!(!output.contains("baml_string_char_count_raw8"));
    }

    #[test]
    fn generic_only_rows_keep_their_per_kind_functions_private() {
        let output = generated();
        assert!(output.contains("\nfn baml_array_concat_raw8<R: ObjectModel>"));
        assert!(!output.contains("pub fn baml_array_concat_raw8"));
    }

    #[test]
    fn generic_adapters_branch_on_the_receiver_slot() {
        let output = generated();
        let start = output.find("pub fn baml_array_at<R").unwrap();
        let generic = &output[start..output[start..].find("\n}\n").unwrap() + start];
        assert!(
            generic.contains("rt.shape(array).list_elem()?"),
            "{generic}"
        );
        assert!(generic.contains("SlotKind::Raw(_) =>"), "{generic}");
        assert!(
            generic.contains("map(|elem| Slots::to_tagged(&*rt, elem, slot))"),
            "{generic}"
        );
    }

    #[test]
    fn pending_and_value_glue_rows_generate_nothing() {
        let output = generated();
        assert!(!output.contains("fn baml_array_filter"));
        assert!(!output.contains("fn baml_int_abs"));
    }

    #[test]
    fn an_unsupported_signature_fails_generation() {
        let declarations = triage::native_inventory().unwrap();
        let rows = [TriageRow {
            path: "baml.Array.filled",
            group: Group::Collection,
            form: AdapterForm::Generic,
        }];
        let error = generate_adapters(&declarations, &rows).unwrap_err();
        assert_eq!(error.path, "baml.Array.filled");
        assert!(matches!(
            error.problem,
            AdapterProblem::UnsupportedReceiver(_)
        ));
    }

    #[test]
    fn a_row_without_a_declaration_fails_generation() {
        let rows = [TriageRow {
            path: "baml.Array.nonexistent",
            group: Group::Collection,
            form: AdapterForm::Generic,
        }];
        let error = generate_adapters(&[], &rows).unwrap_err();
        assert_eq!(error.problem, AdapterProblem::MissingDeclaration);
    }
}

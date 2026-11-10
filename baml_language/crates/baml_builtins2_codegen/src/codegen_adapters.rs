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

    /// The support call that validates the receiver and yields its slot.
    fn slot_of(self) -> TokenStream {
        match self {
            ReceiverShape::List => quote!(list_elem_slot),
            ReceiverShape::Map => quote!(map_value_slot),
            ReceiverShape::Str => quote!(str_receiver),
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
    Float,
    Bool,
    Unit,
    /// A string, list or bytes object.
    Object,
    /// The receiver's varying slot: a list's `T`, a map's `V`.
    Elem,
    OptElem,
    /// A map's `K`; adapters take string keys.
    Key,
    /// `(T) -> U`, called per element.
    Callback,
    /// The descriptor of a returned list whose element type is a method
    /// generic (a callback's result), resolved by the caller.
    ListDesc,
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
            BamlType::Float => Some(Abi::Float),
            BamlType::Bool => Some(Abi::Bool),
            BamlType::Null => Some(Abi::Unit),
            BamlType::String | BamlType::List(_) | BamlType::Uint8Array => Some(Abi::Object),
            BamlType::Generic(name) if is(roles.elem, name) => Some(Abi::Elem),
            BamlType::Generic(name) if is(roles.key, name) => Some(Abi::Key),
            BamlType::Optional(inner) if Abi::of(inner, roles) == Some(Abi::Elem) => {
                Some(Abi::OptElem)
            }
            BamlType::Named(name) if name == "function" => Some(Abi::Callback),
            _ => None,
        }
    }

    /// The Rust type in a function whose varying slot is `elem`. Generic
    /// adapters pass `Tagged` for `elem` and take keys universally.
    fn rust_type(self, elem: &TokenStream, generic: bool) -> TokenStream {
        match self {
            Abi::Int => quote!(i64),
            Abi::Float => quote!(f64),
            Abi::Bool => quote!(bool),
            Abi::Unit => quote!(()),
            Abi::Object => quote!(ObjPtr),
            Abi::Elem => elem.clone(),
            Abi::OptElem => quote!(Option<#elem>),
            Abi::Key if generic => quote!(Tagged),
            Abi::Key => quote!(ObjPtr),
            Abi::Callback => quote!(&mut dyn FnMut(&mut R, #elem) -> Result<Tagged, Fault>),
            Abi::ListDesc => quote!(ListOf),
        }
    }

    /// The generic adapter's argument expression for a per-kind call.
    fn convert_arg(self, name: &proc_macro2::Ident) -> TokenStream {
        match self {
            Abi::Elem => quote!(Elem::from_tagged(#name, slot)?),
            Abi::Callback => quote!(&mut |rt: &mut R, elem| #name(rt, Elem::to_tagged(elem, slot))),
            Abi::Int
            | Abi::Float
            | Abi::Bool
            | Abi::Unit
            | Abi::Object
            | Abi::OptElem
            | Abi::Key
            | Abi::ListDesc => quote!(#name),
        }
    }

    /// A statement converting the argument before the per-kind call, for
    /// conversions that read the heap (and so cannot nest in a call that
    /// already borrows the runtime).
    fn hoist(self, name: &proc_macro2::Ident) -> Option<TokenStream> {
        match self {
            Abi::Key => Some(quote!(let #name = string_key(rt, #name)?;)),
            Abi::Int
            | Abi::Float
            | Abi::Bool
            | Abi::Unit
            | Abi::Object
            | Abi::Elem
            | Abi::OptElem
            | Abi::Callback
            | Abi::ListDesc => None,
        }
    }

    /// The generic adapter's result expression around a per-kind `call`.
    fn convert_result(self, call: TokenStream) -> TokenStream {
        match self {
            Abi::Elem => quote!(Ok(Elem::to_tagged(#call?, slot))),
            Abi::OptElem => quote!(Ok(#call?.map(|elem| Elem::to_tagged(elem, slot)))),
            Abi::Int
            | Abi::Float
            | Abi::Bool
            | Abi::Unit
            | Abi::Object
            | Abi::Key
            | Abi::Callback
            | Abi::ListDesc => call,
        }
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
        let roles = match shape {
            ReceiverShape::List => Roles {
                elem: generics.first().map(String::as_str),
                key: None,
            },
            ReceiverShape::Map => Roles {
                elem: generics.get(1).map(String::as_str),
                key: generics.first().map(String::as_str),
            },
            ReceiverShape::Str => Roles {
                elem: None,
                key: None,
            },
        };
        let abi = |ty: &BamlType| {
            Abi::of(ty, &roles).ok_or_else(|| AdapterProblem::UnsupportedType(format!("{ty:?}")))
        };
        let params = decl
            .params
            .iter()
            .map(|p| Ok((format_ident!("{}", p.name), abi(&p.ty)?)))
            .collect::<Result<Vec<_>, AdapterProblem>>()?;
        let ret = abi(&decl.return_type)?;
        let out_desc = returns_method_generic_list(&decl.return_type, generics)
            .then(|| (format_ident!("out"), Abi::ListDesc));
        let params = params.into_iter().chain(out_desc).collect::<Vec<_>>();
        let varies = params
            .iter()
            .map(|(_, a)| *a)
            .chain([ret])
            .any(|a| matches!(a, Abi::Elem | Abi::OptElem | Abi::Callback));
        if varies && shape == ReceiverShape::Str {
            return Err(AdapterProblem::UnsupportedType(
                "generic on a string receiver".into(),
            ));
        }
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

    fn per_kind_fn(&self, arm: Arm) -> TokenStream {
        let name = self.per_kind_name(arm);
        let body = self.name();
        let receiver = self.receiver.param();
        let elem = arm.elem();
        let params = self.param_list(&elem, false);
        let args = self.params.iter().map(|(name, _)| name);
        let ret = self.ret.rust_type(&elem, false);
        let turbofish = match self.receiver {
            ReceiverShape::List | ReceiverShape::Map => quote!(::<R, #elem>),
            ReceiverShape::Str => quote!(),
        };
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
        let args = self.params.iter().map(|(name, abi)| abi.convert_arg(name));
        self.ret
            .convert_result(quote!(#per_kind(rt, #receiver, #(#args),*)))
    }

    /// Read the receiver's slot and branch to the matching per-kind function.
    fn slot_dispatch(&self) -> TokenStream {
        let receiver = self.receiver.param();
        let slot_of = self.receiver.slot_of();
        let arms = ARMS.iter().map(|&arm| {
            let pattern = arm.pattern();
            let call = self.per_kind_call(arm);
            quote!(#pattern => #call,)
        });
        let hoisted = self.params.iter().filter_map(|(name, abi)| abi.hoist(name));
        quote! {
            let slot = #slot_of(rt, #receiver)?;
            #(#hoisted)*
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
            str_receiver(rt, #receiver)?;
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
}

/// Whether `ty` is a list of a generic the receiver class does not declare.
fn returns_method_generic_list(ty: &BamlType, class_generics: &[String]) -> bool {
    let BamlType::List(inner) = ty else {
        return false;
    };
    matches!(&**inner, BamlType::Generic(name) if !class_generics.contains(name))
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
        use bex_vm_types::layout::{Elem, Fault, ListOf, ObjPtr, ObjectModel, RawWord, SlotKind, Tagged};

        use super::{bodies, support::{list_elem_slot, map_value_slot, str_receiver, string_key}};

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
        assert!(generic.contains("list_elem_slot(rt, array)?"), "{generic}");
        assert!(generic.contains("SlotKind::Raw(_) =>"), "{generic}");
        assert!(
            generic.contains("map(|elem| Elem::to_tagged(elem, slot))"),
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

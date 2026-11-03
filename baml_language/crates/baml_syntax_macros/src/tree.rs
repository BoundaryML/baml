//! `define_tree! { root: R, nodes { … }, pools { … } }`.

use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{
    Ident, Token, Type, braced,
    parse::{Parse, ParseStream},
    punctuated::Punctuated,
};

pub(crate) struct TreeInput {
    root: Ident,
    nodes: Vec<Ident>,
    pools: Vec<Type>,
}

impl Parse for TreeInput {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut root = None;
        let mut nodes = None;
        let mut pools = None;
        while !input.is_empty() {
            let key: Ident = input.parse()?;
            match key.to_string().as_str() {
                "root" => {
                    input.parse::<Token![:]>()?;
                    root = Some(input.parse()?);
                }
                "nodes" => {
                    let content;
                    braced!(content in input);
                    let list: Punctuated<Ident, Token![,]> =
                        content.parse_terminated(Ident::parse, Token![,])?;
                    nodes = Some(list.into_iter().collect());
                }
                "pools" => {
                    let content;
                    braced!(content in input);
                    let list: Punctuated<Type, Token![,]> =
                        content.parse_terminated(Type::parse, Token![,])?;
                    pools = Some(list.into_iter().collect());
                }
                other => {
                    return Err(syn::Error::new(
                        key.span(),
                        format!(
                            "unknown `define_tree!` section `{other}`; expected `root`, `nodes` or `pools`"
                        ),
                    ));
                }
            }
            if !input.is_empty() {
                input.parse::<Token![,]>()?;
            }
        }
        let root: Ident =
            root.ok_or_else(|| input.error("`define_tree!` needs `root: <NodeType>`"))?;
        let nodes: Vec<Ident> =
            nodes.ok_or_else(|| input.error("`define_tree!` needs a `nodes { … }` list"))?;
        if !nodes.contains(&root) {
            return Err(syn::Error::new(
                root.span(),
                "the root type must be one of the listed nodes",
            ));
        }
        if !nodes.iter().any(|n| n == "Bogus") {
            return Err(syn::Error::new(
                root.span(),
                "the node list must include `Bogus` (`baml_syntax::tree::Bogus`), the recovery node",
            ));
        }
        Ok(Self {
            root,
            nodes,
            pools: pools.unwrap_or_default(),
        })
    }
}

/// Field names that would collide with a keyword are raw (`Type` → `r#type`).
const KEYWORDS: [&str; 10] = [
    "type", "for", "if", "else", "match", "while", "loop", "return", "break", "continue",
];

/// `FunctionDef` → `function_def`.
fn snake_case(ident: &Ident) -> Ident {
    let mut out = String::new();
    for (i, c) in ident.to_string().chars().enumerate() {
        if c.is_ascii_uppercase() {
            if i > 0 {
                out.push('_');
            }
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    if KEYWORDS.contains(&out.as_str()) {
        Ident::new_raw(&out, ident.span())
    } else {
        format_ident!("{out}")
    }
}

pub(crate) fn emit(input: &TreeInput) -> TokenStream {
    let root = &input.root;
    let nodes = &input.nodes;
    let node_names: Vec<String> = nodes.iter().map(Ident::to_string).collect();
    let arena_fields: Vec<Ident> = nodes.iter().map(snake_case).collect();
    let raws: Vec<u16> = (0..nodes.len())
        .map(|i| {
            u16::try_from(i)
                .unwrap_or_else(|_| unreachable!("a grammar has fewer than 65536 node types"))
        })
        .collect();
    let pool_fields: Vec<Ident> = (0..input.pools.len())
        .map(|i| format_ident!("pool_{i}"))
        .collect();
    let pools = &input.pools;
    let count = nodes.len();

    quote! {
        /// Every node type in the tree, in `define_tree!` order.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
        #[repr(u16)]
        pub enum NodeKind {
            #( #nodes = #raws, )*
        }

        impl NodeKind {
            pub const ALL: [NodeKind; #count] = [ #( NodeKind::#nodes, )* ];

            pub fn from_raw(raw: u16) -> ::core::option::Option<Self> {
                Self::ALL.get(usize::from(raw)).copied()
            }

            pub fn to_raw(self) -> u16 {
                self as u16
            }

            pub fn name(self) -> &'static str {
                match self { #( NodeKind::#nodes => #node_names, )* }
            }
        }

        #(
            impl ::baml_syntax::tree::NodeKindOf for #nodes {
                const RAW: u16 = #raws;
                const NAME: &'static str = #node_names;
            }
        )*

        /// Every node of the tree, one arena per node type, plus the pools
        /// that `List<T>` fields index into. The tree under construction: a
        /// [`Builder`] allocates into it, and once parsing is over it becomes
        /// part of a read-only [`FileTree`].
        #[derive(Debug, Default, Clone, PartialEq, Eq, Hash)]
        pub struct Arenas {
            #( #arena_fields: ::baml_syntax::tree::NodeArena<#nodes>, )*
            #( #pool_fields: ::std::vec::Vec<#pools>, )*
        }

        impl Arenas {
            pub fn alloc<T: ::baml_syntax::tree::NodeMarker>(&mut self, node: T, span: ::baml_syntax::tree::TokenRange) -> ::baml_syntax::tree::Id<T>
            where
                Self: ::baml_syntax::tree::HasArena<T>,
            {
                ::baml_syntax::tree::HasArena::<T>::arena_mut(self).alloc(node, span)
            }

            pub fn alloc_list<T: ::baml_syntax::tree::PoolItem>(&mut self, items: impl ::std::iter::IntoIterator<Item = T>) -> ::baml_syntax::tree::List<T>
            where
                Self: ::baml_syntax::tree::HasPool<T>,
            {
                ::baml_syntax::tree::alloc_list(self, items)
            }

            /// A list that must not be empty: `first`, then `rest`.
            pub fn alloc_list1<T: ::baml_syntax::tree::PoolItem>(&mut self, first: T, rest: impl ::std::iter::IntoIterator<Item = T>) -> ::baml_syntax::tree::List1<T>
            where
                Self: ::baml_syntax::tree::HasPool<T>,
            {
                ::baml_syntax::tree::alloc_list1(self, first, rest)
            }

            /// The node behind a typed id.
            pub fn get<T: ::baml_syntax::tree::NodeMarker>(&self, id: ::baml_syntax::tree::Id<T>) -> &T
            where
                Self: ::baml_syntax::tree::HasArena<T>,
            {
                &::baml_syntax::tree::HasArena::<T>::arena(self)[id]
            }

            /// The token span of a typed id.
            pub fn node_span<T: ::baml_syntax::tree::NodeMarker>(&self, id: ::baml_syntax::tree::Id<T>) -> ::baml_syntax::tree::TokenRange
            where
                Self: ::baml_syntax::tree::HasArena<T>,
            {
                ::baml_syntax::tree::HasArena::<T>::arena(self).span(id)
            }

            /// The items of a `List<T>`.
            pub fn list<T: ::baml_syntax::tree::PoolItem>(&self, list: ::baml_syntax::tree::List<T>) -> &[T]
            where
                Self: ::baml_syntax::tree::HasPool<T>,
            {
                list.get(self)
            }

            /// Every node of type `T`, in allocation order.
            pub fn ids<'s, T: ::baml_syntax::tree::NodeMarker + 's>(&'s self) -> impl Iterator<Item = ::baml_syntax::tree::Id<T>> + 's
            where
                Self: ::baml_syntax::tree::HasArena<T>,
            {
                ::baml_syntax::tree::HasArena::<T>::arena(self).ids()
            }

            /// Every node of every type.
            pub fn all_ids(&self) -> ::std::vec::Vec<::baml_syntax::tree::AnyNodeId> {
                let mut all = ::std::vec::Vec::new();
                #( all.extend(self.#arena_fields.ids().map(::baml_syntax::tree::AnyNodeId::of)); )*
                all
            }
        }

        #(
            impl ::baml_syntax::tree::HasArena<#nodes> for Arenas {
                fn arena(&self) -> &::baml_syntax::tree::NodeArena<#nodes> { &self.#arena_fields }
                fn arena_mut(&mut self) -> &mut ::baml_syntax::tree::NodeArena<#nodes> { &mut self.#arena_fields }
            }
        )*

        #(
            impl ::baml_syntax::tree::sealed::Sealed for #pools {}
            impl ::baml_syntax::tree::PoolItem for #pools {}
        )*

        #(
            impl ::baml_syntax::tree::HasPool<#pools> for Arenas {
                fn pool(&self) -> &[#pools] { &self.#pool_fields }
                fn pool_mut(&mut self) -> &mut ::std::vec::Vec<#pools> { &mut self.#pool_fields }
            }
        )*

        /// A speculative parse: everything allocated through it is kept by
        /// [`Speculation::commit`] and discarded otherwise — by
        /// [`Speculation::rollback`], or by dropping it. It borrows the
        /// arenas, so an inner speculation — built from `&mut *outer` — has
        /// to end before the outer one can be touched: the borrow checker
        /// orders them.
        #[must_use = "a speculation that is not committed is rolled back"]
        pub struct Speculation<'a> {
            arenas: &'a mut Arenas,
            point: SpeculationPoint,
        }

        /// The size of every arena and pool when a speculation began.
        struct SpeculationPoint {
            #( #arena_fields: usize, )*
            #( #pool_fields: usize, )*
        }

        impl<'a> Speculation<'a> {
            pub fn new(arenas: &'a mut Arenas) -> Self {
                let point = SpeculationPoint {
                    #( #arena_fields: arenas.#arena_fields.len(), )*
                    #( #pool_fields: arenas.#pool_fields.len(), )*
                };
                Self { arenas, point }
            }

            /// Keep what was allocated.
            pub fn commit(self) {
                let _kept = ::std::mem::ManuallyDrop::new(self);
            }

            /// Drop every node and list item allocated since the speculation
            /// began; what dropping it does, named.
            pub fn rollback(self) {}
        }

        impl ::std::ops::Deref for Speculation<'_> {
            type Target = Arenas;
            fn deref(&self) -> &Arenas {
                self.arenas
            }
        }

        impl ::std::ops::DerefMut for Speculation<'_> {
            fn deref_mut(&mut self) -> &mut Arenas {
                self.arenas
            }
        }

        /// Rolls back: nothing allocated before the speculation can refer
        /// to anything after it, because a parser builds parents after
        /// children, so truncating is sound at any point.
        impl Drop for Speculation<'_> {
            fn drop(&mut self) {
                #( self.arenas.#arena_fields.truncate(self.point.#arena_fields); )*
                #( self.arenas.#pool_fields.truncate(self.point.#pool_fields); )*
            }
        }

        /// Builds a tree: allocation, plus the two recovery values with the
        /// diagnostic each one owes. `E` is the parser's diagnostic type. A
        /// `Missing` or a `Bogus` exists only through [`Builder::missing`] and
        /// [`Builder::bogus`], which take the report — so neither can be in a
        /// tree without it.
        pub struct Builder<'a, E> {
            arenas: &'a mut Arenas,
            errors: &'a mut ::std::vec::Vec<E>,
        }

        impl<'a, E> Builder<'a, E> {
            pub fn new(arenas: &'a mut Arenas, errors: &'a mut ::std::vec::Vec<E>) -> Self {
                Self { arenas, errors }
            }

            /// The tree built so far.
            pub fn arenas(&self) -> &Arenas {
                self.arenas
            }

            pub fn alloc<T: ::baml_syntax::tree::NodeMarker>(&mut self, node: T, span: ::baml_syntax::tree::TokenRange) -> ::baml_syntax::tree::Id<T>
            where
                Arenas: ::baml_syntax::tree::HasArena<T>,
            {
                self.arenas.alloc(node, span)
            }

            pub fn alloc_list<T: ::baml_syntax::tree::PoolItem>(&mut self, items: impl ::std::iter::IntoIterator<Item = T>) -> ::baml_syntax::tree::List<T>
            where
                Arenas: ::baml_syntax::tree::HasPool<T>,
            {
                self.arenas.alloc_list(items)
            }

            /// A list that must not be empty: `first`, then `rest`.
            pub fn alloc_list1<T: ::baml_syntax::tree::PoolItem>(&mut self, first: T, rest: impl ::std::iter::IntoIterator<Item = T>) -> ::baml_syntax::tree::List1<T>
            where
                Arenas: ::baml_syntax::tree::HasPool<T>,
            {
                self.arenas.alloc_list1(first, rest)
            }

            pub fn report(&mut self, error: E) {
                self.errors.push(error);
            }

            /// A required position the text did not fill, at `at`, with its
            /// report.
            pub fn missing(&mut self, at: ::text_size::TextSize, error: E) -> ::baml_syntax::tree::Missing {
                self.errors.push(error);
                ::baml_syntax::tree::Missing::new(at)
            }

            /// Tokens that fit no node, with their report.
            pub fn bogus(&mut self, tokens: ::baml_syntax::tree::TokenRange, error: E) -> ::baml_syntax::tree::Id<::baml_syntax::tree::Bogus> {
                self.errors.push(error);
                self.arenas.alloc(::baml_syntax::tree::Bogus::new(tokens), tokens)
            }

            /// A `Bogus` whose report lives elsewhere.
            #[deprecated = "a `Bogus` without its own diagnostic: for a parser whose grammar is not complete yet, deleted with its last stub"]
            pub fn placeholder(&mut self, tokens: ::baml_syntax::tree::TokenRange) -> ::baml_syntax::tree::Id<::baml_syntax::tree::Bogus> {
                self.arenas.alloc(::baml_syntax::tree::Bogus::new(tokens), tokens)
            }

            /// Run `f` with a builder for a speculative parse: what it allocates
            /// is kept when `keep` says so of the result and discarded
            /// otherwise. The diagnostics it reports stay either way, so a
            /// discarded parse never loses its report and the arenas never hold
            /// a node nothing refers to.
            pub fn speculate<R>(
                &mut self,
                f: impl FnOnce(Builder<'_, E>) -> R,
                keep: impl FnOnce(&R) -> bool,
            ) -> R {
                let mut speculation = Speculation::new(self.arenas);
                let result = f(Builder { arenas: &mut speculation, errors: self.errors });
                if keep(&result) {
                    speculation.commit();
                } else {
                    speculation.rollback();
                }
                result
            }
        }

        /// A reference to a node of any type.
        #[derive(Debug, Clone, Copy)]
        pub enum AnyNodeRef<'t> {
            #( #nodes(&'t #nodes), )*
        }

        impl Arenas {
            /// The kind of `id`.
            pub fn kind(id: ::baml_syntax::tree::AnyNodeId) -> NodeKind {
                NodeKind::from_raw(id.kind_raw())
                    .unwrap_or_else(|| unreachable!("an AnyNodeId is only made from a listed node type"))
            }

            /// The node behind `id`.
            pub fn node(&self, id: ::baml_syntax::tree::AnyNodeId) -> AnyNodeRef<'_> {
                match Self::kind(id) {
                    #( NodeKind::#nodes => AnyNodeRef::#nodes(
                        &::baml_syntax::tree::HasArena::<#nodes>::arena(self)[id.cast::<#nodes>()
                            .unwrap_or_else(|| unreachable!("kind was just matched"))]
                    ), )*
                }
            }

            /// The token span of `id`.
            pub fn span(&self, id: ::baml_syntax::tree::AnyNodeId) -> ::baml_syntax::tree::TokenRange {
                match Self::kind(id) {
                    #( NodeKind::#nodes => ::baml_syntax::tree::HasArena::<#nodes>::arena(self)
                        .span(id.cast::<#nodes>().unwrap_or_else(|| unreachable!("kind was just matched"))), )*
                }
            }

            /// Visit the children of `id`, in source order.
            pub fn walk_node(&self, id: ::baml_syntax::tree::AnyNodeId, visitor: &mut dyn ::baml_syntax::tree::Visitor) {
                match self.node(id) {
                    #( AnyNodeRef::#nodes(node) => ::baml_syntax::tree::NodeFields::<Arenas>::walk_fields(node, self, visitor), )*
                }
            }
        }

        impl ::baml_syntax::tree::TreeWalk for Arenas {
            fn kind_name(&self, id: ::baml_syntax::tree::AnyNodeId) -> &'static str {
                Self::kind(id).name()
            }
            fn span(&self, id: ::baml_syntax::tree::AnyNodeId) -> ::baml_syntax::tree::TokenRange {
                Arenas::span(self, id)
            }
            fn walk_node(&self, id: ::baml_syntax::tree::AnyNodeId, visitor: &mut dyn ::baml_syntax::tree::Visitor) {
                Arenas::walk_node(self, id, visitor)
            }
        }

        /// One parsed file: its text, tokens, nodes and root. Read-only:
        /// built once from the parser's arenas, never changed afterwards.
        #[derive(Debug, Clone, PartialEq, Eq, Hash)]
        pub struct FileTree {
            text: ::std::boxed::Box<str>,
            tokens: ::baml_syntax::token::Tokens,
            arenas: Arenas,
            root: ::baml_syntax::tree::Id<#root>,
        }

        impl FileTree {
            /// `root` is a node of `arenas`, which were built from `tokens`,
            /// which were lexed from `text`.
            pub fn new(
                text: ::std::boxed::Box<str>,
                tokens: ::baml_syntax::token::Tokens,
                arenas: Arenas,
                root: ::baml_syntax::tree::Id<#root>,
            ) -> Self {
                debug_assert!(
                    ::std::convert::TryFrom::try_from(root.raw())
                        .is_ok_and(|raw: usize| raw < ::baml_syntax::tree::HasArena::<#root>::arena(&arenas).len()),
                    "the root is a node of these arenas"
                );
                Self { text, tokens, arenas, root }
            }

            pub fn text(&self) -> &str {
                &self.text
            }

            pub fn tokens(&self) -> &::baml_syntax::token::Tokens {
                &self.tokens
            }

            pub fn arenas(&self) -> &Arenas {
                &self.arenas
            }

            pub fn root(&self) -> ::baml_syntax::tree::Id<#root> {
                self.root
            }

            /// The root as an untyped id, for generic walks.
            pub fn root_id(&self) -> ::baml_syntax::tree::AnyNodeId {
                ::baml_syntax::tree::AnyNodeId::of(self.root)
            }

            /// The node behind a typed id.
            pub fn get<T: ::baml_syntax::tree::NodeMarker>(&self, id: ::baml_syntax::tree::Id<T>) -> &T
            where
                Arenas: ::baml_syntax::tree::HasArena<T>,
            {
                self.arenas.get(id)
            }

            /// The token span of a typed id.
            pub fn span<T: ::baml_syntax::tree::NodeMarker>(&self, id: ::baml_syntax::tree::Id<T>) -> ::baml_syntax::tree::TokenRange
            where
                Arenas: ::baml_syntax::tree::HasArena<T>,
            {
                self.arenas.node_span(id)
            }

            /// The items of a `List<T>`.
            pub fn list<T: ::baml_syntax::tree::PoolItem>(&self, list: ::baml_syntax::tree::List<T>) -> &[T]
            where
                Arenas: ::baml_syntax::tree::HasPool<T>,
            {
                self.arenas.list(list)
            }

            /// The source text of a token.
            pub fn token_text(&self, token: ::baml_syntax::token::TokenIdx) -> &str {
                self.tokens.text(&self.text, token)
            }

            /// An indented dump of the whole tree, for tests and debugging.
            pub fn debug_tree(&self) -> ::std::string::String {
                ::baml_syntax::debug::debug_tree(&self.arenas, &self.tokens, &self.text, self.root_id())
            }
        }

        /// Invoke `$m!` with the comma-separated list of every node type
        /// (`nodes::*` and `tree::Bogus` must be in scope).
        #[macro_export]
        macro_rules! for_each_node_type {
            ($m:ident) => { $m! { #( #nodes ),* } };
        }
    }
}

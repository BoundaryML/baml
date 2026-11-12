//! Function hot swapping (BEP-087, stage 1): what a host calls to install a new
//! version of a running package (`BexEngine::hotswap_patch`), and the checks a
//! replacement must pass.
//!
//! A redirect lives on the target [`Function`]; every call path selects it once
//! at entry (see `BexVm::hotswap_entry`). This module decides what to redirect:
//! it compares the new source with the program as built, links it against the
//! live package, so unchanged declarations keep their identity, and rejects
//! any edit to the public interface.

use std::{
    collections::BTreeSet,
    sync::atomic::{AtomicBool, Ordering},
};

use baml_base::Name;
use baml_linker_types::EmittedPackage;
use bex_heap::TlabHolder;
use bex_vm_types::{
    BodyKey, DeclFingerprints, DeclPath, FnPath, Function, FunctionKind, FunctionMeta, HeapPtr,
    ImplBodyKey, Object, Value,
    types::{ExportSurface, FunctionIdentity, LocalName, Objects, Package, PackageKind, Slots},
};
use indexmap::IndexMap;

use crate::{
    BexVm,
    compile_artifact::RuntimeCompileArtifact,
    package_reflect::{
        graft::{Commit, LinkError, LoadTarget, PatchPlan, load_package},
        reflect,
    },
};

/// The function behind a package cell's value: a named function is a
/// `GenericFunction` wrapper over it, and an interface implementation a
/// capture-less closure over its body.
fn function_behind(vm: &BexVm, ptr: HeapPtr) -> Option<(HeapPtr, &Function)> {
    let ptr = match vm.get_object(ptr) {
        Object::GenericFunction(generic) => vm.generic_function_authored_ptr(generic).ok()?,
        Object::Closure(closure) => closure.function,
        _ => ptr,
    };
    match vm.get_object(ptr) {
        Object::Function(function) => Some((ptr, function)),
        _ => None,
    }
}

/// Whether the source declares type parameters. Unnamed slots are inferred
/// effects (a callback parameter with no stated `throws`, or an LLM function's
/// injected `on_event`), which are not generics the source wrote.
fn is_generic(function: &Function) -> bool {
    !function.type_param_names.is_empty() || !function.display_type_params.is_empty()
}

/// The function behind the live cell value `target`, or why it cannot be
/// swapped.
fn swappable_target(vm: &BexVm, target: HeapPtr) -> Result<HeapPtr, String> {
    let (ptr, function) = function_behind(vm, target).expect("a function cell holds a function");
    let name = function.name();
    if !matches!(function.kind, FunctionKind::Bytecode) || function.compiled_body.is_some() {
        return Err(format!("`{name}` does not run as bytecode"));
    }
    if matches!(function.body_meta, Some(FunctionMeta::Llm))
        && !matches!(
            function.identity,
            FunctionIdentity::Declared(FnPath::Free(_))
        )
    {
        return Err(format!(
            "`{name}` is an LLM method, whose companions cannot be named"
        ));
    }
    Ok(ptr)
}

/// Why generic `function` does not declare what generic `target` declares: the
/// same type parameters and bounds, and the same signature over them. The
/// comparison is structural, since neither signature is realized here.
fn generic_signature_problem(target: &Function, function: &Function) -> Option<String> {
    let name = target.name();
    if function.type_param_names != target.type_param_names {
        return Some(format!(
            "the replacement's type parameters ({}) differ from those of `{name}` ({})",
            function.type_param_names.join(", "),
            target.type_param_names.join(", ")
        ));
    }
    if function.generic_param_bounds != target.generic_param_bounds {
        return Some(format!(
            "the replacement's type parameter bounds differ from those of `{name}`"
        ));
    }
    if function.param_types != target.param_types || function.return_type != target.return_type {
        return Some(format!(
            "the replacement's signature differs from the generic signature of `{name}`"
        ));
    }
    if function.throws_type != target.throws_type {
        return Some(format!(
            "the replacement's `throws` differs from that of `{name}`"
        ));
    }
    None
}

/// What a call of the target should enter, or why `replacement` cannot stand
/// in for it. The replacement must declare exactly the signature of the
/// function as built: a narrower one would still change the public interface.
fn checked_replacement(
    vm: &BexVm,
    target: HeapPtr,
    replacement: HeapPtr,
) -> Result<HeapPtr, String> {
    let Object::Function(target_fn) = vm.get_object(target) else {
        unreachable!("swappable_target returns only functions")
    };
    let (authored, function) =
        function_behind(vm, replacement).expect("a function cell holds a function");
    // A call enters an implementation's closure, and a named function itself.
    let entered = match vm.get_object(replacement) {
        Object::Closure(_) => replacement,
        _ => authored,
    };
    let name = target_fn.name();
    if matches!(target_fn.body_meta, Some(FunctionMeta::Llm))
        && !matches!(function.body_meta, Some(FunctionMeta::Llm))
    {
        return Err(format!(
            "`{name}` is an LLM function, and the replacement is not"
        ));
    }
    if function.generic_param_bounds.len() != target_fn.generic_param_bounds.len() {
        return Err(format!(
            "the replacement's type argument slots differ from those of `{name}`"
        ));
    }
    if function.arity != target_fn.arity {
        return Err(format!(
            "`{name}` takes {} arguments, but the replacement takes {}",
            target_fn.arity, function.arity
        ));
    }
    if function.argument_layout() != target_fn.argument_layout() {
        return Err(format!(
            "the replacement's optional parameters differ from those of `{name}`"
        ));
    }
    // `self` included: an instance method cannot become a static one.
    if function.param_names != target_fn.param_names {
        return Err(format!(
            "the replacement's parameters ({}) are not named like those of `{name}` ({})",
            function.param_names.join(", "),
            target_fn.param_names.join(", ")
        ));
    }
    let problem = if is_generic(target_fn) {
        generic_signature_problem(target_fn, function)
    } else {
        crate::package_reflect::reflect::replacement_type_problem(vm, target_fn, function)
    };
    match problem {
        Some(problem) => Err(problem),
        None => Ok(entered),
    }
}

/// The companion suffixes an LLM function can carry (`baml_compiler2_ast`'s
/// `companions.rs`). A function and its companions swap as one unit.
const COMPANIONS: [&str; 5] = ["spec", "render_prompt", "build_request", "parse", "stream"];

/// `function`'s companion `suffix`, found by name in the package that owns it.
fn companion_of(vm: &BexVm, function: &Function, suffix: &str) -> Option<HeapPtr> {
    let FunctionIdentity::Declared(FnPath::Free(path)) = &function.identity else {
        return None;
    };
    let Object::Package(package) = vm.get_object(function.owner) else {
        return None;
    };
    let name = LocalName::new(
        path.namespace.clone(),
        Name::new(format!("{}@{suffix}", path.name)),
    );
    let cell = *package
        .globals
        .get(&DeclPath::Function(FnPath::Free(name)))?;
    let value = crate::package_reflect::graft::cell_value(vm, package, cell)?;
    function_behind(vm, value.as_object_ptr()?).map(|(ptr, _)| ptr)
}

/// Every (companion, replacement companion) pair the swap must also install,
/// or why it cannot: the replacement must have each companion the target has.
fn companion_swaps(
    vm: &BexVm,
    target: HeapPtr,
    entered: HeapPtr,
) -> Result<Vec<(HeapPtr, HeapPtr)>, String> {
    let function = |ptr| function_behind(vm, ptr).expect("checked above").1;
    let (target_fn, entered_fn) = (function(target), function(entered));
    let mut swaps = Vec::new();
    for suffix in COMPANIONS {
        let Some(companion) = companion_of(vm, target_fn, suffix) else {
            continue;
        };
        let Some(replacement) = companion_of(vm, entered_fn, suffix) else {
            return Err(format!(
                "`{}` has a `@{suffix}` companion, but the replacement does not",
                target_fn.name()
            ));
        };
        let entered = checked_replacement(vm, companion, replacement)?;
        swaps.push((companion, entered));
    }
    Ok(swaps)
}

fn namespace_prefix(name: &LocalName) -> String {
    let mut prefix = String::new();
    for segment in &name.namespace {
        prefix.push_str(segment.as_str());
        prefix.push('.');
    }
    prefix
}

/// The fingerprint key of an implementation's body: the interface, the target
/// type and the method, each by its last name, as source spells them.
fn impl_body_key(body: &ImplBodyKey) -> String {
    struct LastName;
    impl baml_type::TyRenderStrategy<baml_type::PathName> for LastName {
        fn qtn(&self, head: &baml_type::PathName) -> String {
            head.name().to_string()
        }
        fn type_var(&self, name: &Name) -> String {
            name.to_string()
        }
    }
    let target = body
        .coherence
        .for_ty_pattern
        .to_display_ty()
        .render_with(&LastName);
    let target = target.split('<').next().unwrap_or(&target);
    format!(
        "implmethod {} for {target}.{}",
        body.interface.path.name, body.method
    )
}

/// The fingerprint keys a declaration could sit under, if it has source of its
/// own. A companion answers for the LLM function it derives from.
fn decl_keys(path: &DeclPath) -> Vec<String> {
    match path {
        DeclPath::Function(FnPath::Free(name)) => {
            let base = name.name.as_str().split('@').next().unwrap_or_default();
            vec![format!("fn {}{base}", namespace_prefix(name))]
        }
        DeclPath::Function(FnPath::Method { class, name }) => {
            vec![format!(
                "method {}{}.{name}",
                namespace_prefix(class),
                class.name
            )]
        }
        DeclPath::InterfaceBody(BodyKey::ImplMethod(body)) => vec![impl_body_key(body)],
        DeclPath::InterfaceBody(BodyKey::Default { .. }) => Vec::new(),
        DeclPath::Let(name) => {
            let prefix = namespace_prefix(name);
            vec![
                format!("const {prefix}{}", name.name),
                format!("client {prefix}{}", name.name),
            ]
        }
        DeclPath::Class(_)
        | DeclPath::Enum(_)
        | DeclPath::Interface(_)
        | DeclPath::TypeAlias(_) => Vec::new(),
    }
}

/// What a new version of a package changes.
#[derive(Default)]
struct PatchClassification {
    /// Declarations that get a new body.
    changed: BTreeSet<String>,
    /// Changed clients, which are built anew, once.
    fresh: BTreeSet<String>,
    /// Edits that need a restart.
    rejected: Vec<String>,
}

fn label(key: &str) -> &str {
    key.split_once(' ').map_or(key, |(_, name)| name)
}

fn is_function_key(key: &str) -> bool {
    key.starts_with("fn ") || key.starts_with("method ") || key.starts_with("implmethod ")
}

/// What `new` changes relative to `build`, the program as built.
fn classify(build: &DeclFingerprints, new: &DeclFingerprints) -> PatchClassification {
    let mut result = PatchClassification::default();
    let mut rejected = Vec::new();
    let mut reject = |what: String| rejected.push(format!("{what}, which needs a restart"));
    let mut changed_clients = Vec::new();
    for (key, before) in &build.decls {
        let name = label(key);
        match new.decls.get(key) {
            Some(after) if after == before => {}
            _ if key.starts_with("throws ") => {
                reject(format!("`{}` changed its declared `throws`", label(name)));
            }
            None => reject(format!("`{name}` was deleted")),
            Some(_) if is_function_key(key) => {
                result.changed.insert(key.clone());
            }
            Some(_) if key.starts_with("client ") => {
                result.fresh.insert(key.clone());
                changed_clients.push(key.clone());
            }
            Some(_) if key.starts_with("const ") => {
                reject(format!("the value of `{name}` changed"));
            }
            Some(_) => reject(format!(
                "`{name}` changed, which changes the public interface"
            )),
        }
    }
    for key in new
        .decls
        .keys()
        .filter(|key| !build.decls.contains_key(*key))
    {
        if let Some(function) = key.strip_prefix("throws ") {
            if build.decls.contains_key(function) {
                reject(format!(
                    "`{}` changed its declared `throws`",
                    label(function)
                ));
            }
            continue;
        }
        // Every declaration is public until private ones exist (BEP-082).
        reject(format!(
            "`{}` is new, and a swap cannot add declarations",
            label(key)
        ));
    }
    propagate_clients(build, new, &changed_clients, &mut result, &mut reject);
    result.rejected = rejected;
    result
}

/// Follow a client change to everything built from or using the client.
///
/// A client is a value: a function that names it reaches the new one only
/// through a new body, and a client or constant built from it, directly or
/// through a function that returns it, holds the old one until it is built
/// again. Code that cannot be swapped and names it keeps the old one.
fn propagate_clients(
    build: &DeclFingerprints,
    new: &DeclFingerprints,
    changed_clients: &[String],
    result: &mut PatchClassification,
    reject: &mut impl FnMut(String),
) {
    let simple = |key: &str| {
        label(key)
            .rsplit('.')
            .next()
            .unwrap_or(label(key))
            .to_string()
    };
    let refers = |key: &str, names: &BTreeSet<String>| {
        new.refs
            .get(key)
            .and_then(|refs| refs.iter().find(|name| names.contains(*name)))
            .cloned()
    };
    // Clients with a new value, and functions whose result may hold one.
    let mut values: BTreeSet<String> = changed_clients.iter().map(|key| simple(key)).collect();
    let mut reach = BTreeSet::new();
    loop {
        let reached: BTreeSet<String> = values.union(&reach).cloned().collect();
        let mut grew = false;
        for key in new.decls.keys() {
            if is_function_key(key)
                && !reach.contains(&simple(key))
                && refers(key, &reached).is_some()
            {
                grew |= reach.insert(simple(key));
            } else if key.starts_with("client ")
                && build.decls.contains_key(key)
                && !result.fresh.contains(key)
                && refers(key, &reached).is_some()
            {
                result.fresh.insert(key.clone());
                grew |= values.insert(simple(key));
            }
        }
        if !grew {
            break;
        }
    }
    let reached: BTreeSet<String> = values.union(&reach).cloned().collect();
    for key in new.decls.keys() {
        if is_function_key(key) {
            if refers(key, &values).is_some() {
                result.changed.insert(key.clone());
            }
        } else if key.starts_with("const ") {
            if let Some(name) = refers(key, &reached) {
                reject(format!(
                    "`{}` was built from `{name}`, which reaches a changed client",
                    label(key)
                ));
            }
        } else if !key.starts_with("client ")
            && let Some(name) = refers(key, &values)
        {
            // Code that cannot be swapped, such as an interface's default body.
            reject(format!(
                "client `{name}` changed, and `{}`, which cannot be swapped, uses it",
                label(key)
            ));
        }
    }
}

/// The swaps a source change from `before` to `after` causes, named as
/// `hotswap_prepare_patch` names them: functions whose body changed, or that
/// use a client that did. A host reports a save with this, since every patch
/// lists all it swaps relative to the build.
pub fn hotswap_changes(before: &DeclFingerprints, after: &DeclFingerprints) -> BTreeSet<String> {
    classify(before, after)
        .changed
        .iter()
        .map(|key| label(key).to_string())
        .collect()
}

impl PatchClassification {
    fn replaces(&self, path: &DeclPath) -> bool {
        decl_keys(path)
            .iter()
            .any(|key| self.changed.contains(key) || self.fresh.contains(key))
    }
}

/// `$init` helpers of the clients a patch must initialize, in `$init`'s order.
fn fresh_commits<'a>(
    emitted: &'a EmittedPackage,
    builds: &dyn Fn(&DeclPath) -> bool,
) -> Vec<Commit<'a>> {
    let Some(tail) = &emitted.tail else {
        return Vec::new();
    };
    let mut commits = Vec::new();
    for (slot, &object) in tail.slot_objects.iter().enumerate() {
        let Some(Object::Function(function)) = tail.objects.get(object as usize) else {
            continue;
        };
        let FunctionIdentity::InitLet(path) = &function.identity else {
            continue;
        };
        if builds(&DeclPath::Let(path.clone())) {
            commits.push(Commit {
                helper: u32::try_from(slot).expect("a tail's slots fit u32"),
                target: path,
            });
        }
    }
    commits
}

/// The live function and the new one, for every declaration a patch replaces.
/// Companions follow their function, so they are not listed.
fn patch_swaps(
    vm: &BexVm,
    live: HeapPtr,
    patched: HeapPtr,
    emitted: &EmittedPackage,
    classification: &PatchClassification,
) -> Result<Vec<(String, HeapPtr, HeapPtr)>, Vec<String>> {
    let (Object::Package(live_package), Object::Package(new_package)) =
        (vm.get_object(live), vm.get_object(patched))
    else {
        unreachable!("a patch links two packages")
    };
    let function_at = |package: &Package, path: &DeclPath| {
        let cell = *package.globals.get(path)?;
        crate::package_reflect::graft::cell_value(vm, package, cell)?.as_object_ptr()
    };
    let mut swaps = Vec::new();
    let mut problems = Vec::new();
    let mut found = BTreeSet::new();
    for (path, _) in &emitted.unit.exports.objects {
        if !matches!(path, DeclPath::Function(_) | DeclPath::InterfaceBody(_))
            || !classification.replaces(path)
            || matches!(path, DeclPath::Function(FnPath::Free(name)) if name.name.as_str().contains('@'))
        {
            continue;
        }
        let keys = decl_keys(path);
        found.extend(keys.iter().cloned());
        let name = label(&keys[0]).to_string();
        let (Some(target), Some(replacement)) = (
            function_at(live_package, path),
            function_at(new_package, path),
        ) else {
            problems.push(format!("`{name}` has no running implementation to replace"));
            continue;
        };
        swaps.push((name, target, replacement));
    }
    // Source names a declaration the way it was written, and the program the
    // way it resolved, such as an implementation for a type alias. An edit
    // neither can place would otherwise be lost without a word.
    for key in classification.changed.difference(&found) {
        problems.push(format!(
            "`{}` changed, but no running function could be matched to it, which needs a restart",
            label(key)
        ));
    }
    if problems.is_empty() {
        Ok(swaps)
    } else {
        Err(problems)
    }
}

/// Copy the value of each client `reuses` names from `previous`, the patch
/// holding the current clients, into `patched`.
fn reuse_clients(
    vm: &BexVm,
    previous: HeapPtr,
    patched: HeapPtr,
    reuses: &dyn Fn(&DeclPath) -> bool,
) {
    let (Object::Package(previous), Object::Package(patched)) =
        (vm.get_object(previous), vm.get_object(patched))
    else {
        unreachable!("a patch is a package")
    };
    let Slots::Own { cells, .. } = &patched.slots else {
        unreachable!("a patch's package owns its cells")
    };
    for (path, &cell) in &patched.globals {
        if reuses(path)
            && let Some(value) = previous
                .globals
                .get(path)
                .and_then(|&cell| crate::package_reflect::graft::cell_value(vm, previous, cell))
        {
            cells[cell as usize].store(value);
        }
    }
}

/// Point the new package's function cells at each function's original identity,
/// so a call through one still selects whatever is installed and `f == g`
/// holds across a swap; a replacement has no redirect of its own.
fn restore_identities(vm: &BexVm, live: HeapPtr, patched: HeapPtr) {
    let (Object::Package(live), Object::Package(patched)) =
        (vm.get_object(live), vm.get_object(patched))
    else {
        unreachable!("a patch links two packages")
    };
    let Slots::Own { cells, .. } = &patched.slots else {
        unreachable!("a patch's package owns its cells")
    };
    for (path, &cell) in &patched.globals {
        if !matches!(path, DeclPath::Function(_) | DeclPath::InterfaceBody(_)) {
            continue;
        }
        if let Some(original) = live
            .globals
            .get(path)
            .and_then(|&cell| crate::package_reflect::graft::cell_value(vm, live, cell))
        {
            cells[cell as usize].store(original);
        }
    }
}

/// A patch linked against the live package and checked: what is left is to run
/// its new initializers and install it. It lives on the VM that prepared it,
/// whose roots keep it current across a collection.
pub(crate) struct PreparedPatch {
    patched: HeapPtr,
    /// `$init` helpers for changed clients, in `$init`'s order, each with the
    /// cell it fills. They run before the swaps install, under a preview of
    /// them, so an initializer sees the patched code it was written against.
    initializers: Vec<(HeapPtr, usize)>,
    swaps: Vec<(HeapPtr, Option<HeapPtr>)>,
    /// What the patch swaps, as source names them.
    names: Vec<String>,
    /// Whether the patch holds changed clients, built or reused.
    has_clients: bool,
}

impl PreparedPatch {
    /// The replacement a call of `function` enters under this patch, given
    /// the one `installed` now: the patch's own when it swaps the function.
    pub(crate) fn preview(&self, function: HeapPtr, installed: Option<HeapPtr>) -> Option<HeapPtr> {
        self.swaps
            .iter()
            .find(|(target, _)| *target == function)
            .map_or(installed, |(_, replacement)| *replacement)
    }

    pub(crate) fn visit_roots(&mut self, visitor: &mut dyn bex_vm_types::RootVisitor) {
        visitor.visit(&mut self.patched);
        for (helper, _) in &mut self.initializers {
            visitor.visit(helper);
        }
        for (target, replacement) in &mut self.swaps {
            visitor.visit(target);
            if let Some(replacement) = replacement {
                visitor.visit(replacement);
            }
        }
    }
}

/// Set once any swap is queued in this process. Telemetry call paths then
/// hold functions weakly, so a replaced implementation can be collected.
static ANY_QUEUED: AtomicBool = AtomicBool::new(false);

pub(crate) fn any_queued() -> bool {
    ANY_QUEUED.load(Ordering::Relaxed)
}

impl BexVm {
    /// The package the program was compiled for.
    pub fn hotswap_root(&self) -> HeapPtr {
        self.packages.root()
    }

    /// The name of the package the program was compiled for.
    pub fn hotswap_package_name(&self) -> String {
        let Object::Package(package) = self.get_object(self.hotswap_root()) else {
            unreachable!("the root is a package")
        };
        package.name.to_string()
    }

    /// `package`'s own dependencies by the names it reaches them under; the
    /// language packages are left out, since every compile reaches them.
    pub fn hotswap_dependencies(&self, package: HeapPtr) -> IndexMap<String, HeapPtr> {
        let Object::Package(package) = self.get_object(package) else {
            unreachable!("the root is a package")
        };
        package
            .edges
            .iter()
            .filter(|(_, edge)| {
                matches!(
                    self.get_object(edge.target),
                    Object::Package(target) if target.kind == PackageKind::Ordinary
                )
            })
            .map(|(name, edge)| (name.to_string(), edge.target))
            .collect()
    }

    /// Link `artifact`, the complete new source of `live` whose declarations
    /// are `fingerprints`, against the running package and check every swap
    /// it implies relative to `baseline`, the source as built. A function the new source leaves as built goes back to its
    /// own body. Installs nothing: the patch waits on this VM, previewed, for
    /// its initializers and [`Self::hotswap_queue`]. Returns how many
    /// initializers it has.
    pub fn hotswap_prepare_patch(
        &mut self,
        live: HeapPtr,
        artifact: &RuntimeCompileArtifact,
        baseline: &DeclFingerprints,
        applied: &DeclFingerprints,
        fingerprints: &DeclFingerprints,
        dependencies: &IndexMap<String, HeapPtr>,
    ) -> Result<usize, Vec<String>> {
        let vm = self;
        let classification = classify(baseline, fingerprints);
        if !classification.rejected.is_empty() {
            return Err(classification.rejected);
        }
        let Object::Package(live_package) = vm.get_object(live) else {
            unreachable!("a swappable package is a package")
        };
        let patched = Package {
            name: live_package.name.clone(),
            kind: PackageKind::Ordinary,
            edges: reflect::runtime_edges(vm, dependencies),
            classes: IndexMap::new(),
            enums: IndexMap::new(),
            interfaces: IndexMap::new(),
            impl_rules: IndexMap::new(),
            type_aliases: IndexMap::new(),
            globals: IndexMap::new(),
            slots: Slots::Own {
                cells: Box::new([]),
                initialized: false,
            },
            objects: Objects::Own(Box::new([])),
            surface: ExportSurface::Compiled(artifact.interface_blob.clone()),
            init: None,
            test_init: None,
            diagnostics: artifact.diagnostics.clone(),
            session: None,
        };
        let patched = vm.alloc(Object::Package(Box::new(patched)));
        // A client that differs from the build is built only when it changed
        // since the last patch; otherwise that patch's value is reused.
        let previous = vm.heap.hotswap_clients();
        let changed_since = classify(applied, fingerprints).fresh;
        let builds = |path: &DeclPath| {
            classification.replaces(path)
                && (previous.is_none()
                    || decl_keys(path)
                        .iter()
                        .any(|key| changed_since.contains(key)))
        };
        let reuses = |path: &DeclPath| {
            matches!(path, DeclPath::Let(_)) && classification.replaces(path) && !builds(path)
        };
        let commits = fresh_commits(&artifact.emitted, &builds);
        let keep = |path: &DeclPath| !classification.replaces(path);
        let plan = PatchPlan { live, keep: &keep };
        let loaded = load_package(
            vm,
            patched,
            LoadTarget::Patch(&plan),
            &artifact.emitted,
            &commits,
        )
        .map_err(|error| {
            vec![match error {
                LinkError::Link(message) => message,
                LinkError::Internal(error) => error.to_string(),
            }]
        })?;
        if let Some(previous) = previous {
            reuse_clients(vm, previous, patched, &reuses);
        }
        let listed = patch_swaps(vm, live, patched, &artifact.emitted, &classification)?;
        let mut swaps = Vec::new();
        let mut names = Vec::new();
        let mut problems = Vec::new();
        for (name, target, replacement) in listed {
            let checked = swappable_target(vm, target).and_then(|function| {
                let entered = checked_replacement(vm, function, replacement)?;
                let companions = companion_swaps(vm, function, entered)?;
                Ok((function, entered, companions))
            });
            match checked {
                Ok((function, entered, companions)) => {
                    swaps.extend(
                        companions
                            .into_iter()
                            .map(|(companion, new)| (companion, (new != companion).then_some(new))),
                    );
                    swaps.push((function, (entered != function).then_some(entered)));
                    names.push(name);
                }
                Err(problem) => {
                    problems.push(format!(
                        "`{name}` cannot be swapped, which needs a restart: {problem}"
                    ));
                }
            }
        }
        if !problems.is_empty() {
            return Err(problems);
        }
        // Every swap compares with the build, so a function swapped earlier
        // that this source leaves as built returns to its own body.
        for target in vm.heap.hotswap_swapped() {
            if swaps.iter().any(|(swapped, _)| *swapped == target) {
                continue;
            }
            swaps.push((target, None));
            // Named as source names it; a companion follows its function.
            let path = match vm.get_object(target) {
                Object::Function(function) => match &function.identity {
                    FunctionIdentity::Declared(FnPath::Free(name))
                        if name.name.as_str().contains('@') =>
                    {
                        None
                    }
                    FunctionIdentity::Declared(path) => Some(DeclPath::Function(path.clone())),
                    FunctionIdentity::InterfaceBody(body) => {
                        Some(DeclPath::InterfaceBody(body.clone()))
                    }
                    _ => None,
                },
                _ => None,
            };
            if let Some(key) = path.and_then(|path| decl_keys(&path).into_iter().next()) {
                names.push(label(&key).to_string());
            }
        }
        names.dedup();
        // Before any initializer runs: a value it builds from a function, such
        // as a bound method, must hold the function's original identity, or a
        // later swap could never reach it. The preview routes its calls.
        restore_identities(vm, live, patched);
        let initializers = loaded.commits.len();
        vm.hotswap_prepared = Some(PreparedPatch {
            patched,
            initializers: loaded.commits,
            swaps,
            names,
            has_clients: !classification.fresh.is_empty(),
        });
        Ok(initializers)
    }

    /// The `$init` helper of the prepared patch's initializer `index`, which
    /// builds a changed client.
    pub fn hotswap_initializer(&self, index: usize) -> HeapPtr {
        self.hotswap_prepared
            .as_ref()
            .expect("a prepared patch")
            .initializers[index]
            .0
    }

    /// Store what the prepared patch's initializer `index` returned.
    pub fn hotswap_store_initialized(&self, index: usize, value: Value) {
        let prepared = self.hotswap_prepared.as_ref().expect("a prepared patch");
        let cell = prepared.initializers[index].1;
        let Object::Package(package) = self.get_object(prepared.patched) else {
            unreachable!("a patch is a package")
        };
        let Slots::Own { cells, .. } = &package.slots else {
            unreachable!("a patch's package owns its cells")
        };
        cells[cell].store(value);
    }

    /// Queue every swap of the prepared patch on the heap, where the queue
    /// roots it, for the engine to install under a park. Returns the swapped
    /// names.
    pub fn hotswap_queue(&mut self) -> Vec<String> {
        let prepared = self.hotswap_prepared.take().expect("a prepared patch");
        for (target, replacement) in &prepared.swaps {
            self.heap.hotswap_request(*target, *replacement);
        }
        self.heap
            .hotswap_set_clients(prepared.has_clients.then_some(prepared.patched));
        if !prepared.swaps.is_empty() {
            ANY_QUEUED.store(true, Ordering::Relaxed);
        }
        prepared.names
    }
}

#[cfg(test)]
mod tests {
    use bex_vm_types::DeclFingerprints;

    use super::classify;

    fn fingerprints(decls: &[(&str, &str)]) -> DeclFingerprints {
        // Every word of a test fingerprint stands for a reference.
        DeclFingerprints {
            decls: decls
                .iter()
                .map(|(key, text)| ((*key).to_string(), (*text).to_string()))
                .collect(),
            refs: decls
                .iter()
                .map(|(key, text)| {
                    let words = text.split_whitespace().map(str::to_string).collect();
                    ((*key).to_string(), words)
                })
                .collect(),
        }
    }

    /// Code that cannot be swapped and uses a changed client would keep the
    /// old one, so the change needs a restart.
    #[test]
    fn a_client_used_by_code_that_cannot_swap_is_rejected() {
        let client = ("client Base", "client Base = make ( 1 )");
        let changed = ("client Base", "client Base = make ( 2 )");
        let ask = ("fn Ask", "function Ask ( ) { client : Base }");
        let describe = (
            "interface Describe",
            "interface Describe { function d ( ) { Base } }",
        );

        // Only functions use it, so the change swaps them.
        let result = classify(
            &fingerprints(&[client, ask]),
            &fingerprints(&[changed, ask]),
        );
        assert!(result.rejected.is_empty(), "{:?}", result.rejected);
        assert!(result.changed.contains("fn Ask"));

        let result = classify(
            &fingerprints(&[client, ask, describe]),
            &fingerprints(&[changed, ask, describe]),
        );
        assert_eq!(
            result.rejected,
            [
                "client `Base` changed, and `Describe`, which cannot be swapped, uses it, which needs a restart"
            ]
        );
    }
}

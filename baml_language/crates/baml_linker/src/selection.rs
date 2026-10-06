//! Mark original declarations, then project the original operand spaces onto
//! compact output placements. Input units and their authenticated records are
//! never edited. Generic-value interning supplies canonical identities before
//! marking; no linked Program is constructed to discover reachability.
use std::collections::{HashMap, VecDeque};

use bex_vm_types::{
    DeclPath, FnPath, FunctionKind, Object, TyTemplate,
    head_walk::visit_object_heads,
    relink::{IndexOperandRef, visit_object_operands_ref},
};

use super::{
    LinkError, LinkPackageId, LinkSet, PerPackage,
    layout::{Bucket, Placed, TailPart},
    order::{LayoutOrder, Objects, Slots},
    resolve::PathKey,
    space::{Imports, OperandSpace, TailSpace, UnitSpace},
};

/// The host-facing roots of an executable. SDKs must retain the complete host
/// surface, including functions invoked directly by name through the bridge.
#[derive(Clone, Debug)]
pub enum LinkRoots {
    /// Exact qualified callable names, already resolved by the caller.
    EntryPoints(Vec<String>),
    /// Every callable and exposed type in the root package and its direct
    /// dependency/prelude viewpoints. Generated wrappers are not the contract.
    HostSurface,
}

#[derive(Clone, Copy)]
struct Source {
    package: LinkPackageId,
    tail: bool,
    local: usize,
}

struct Reachability<'s, 'a, 'l> {
    set: &'s LinkSet<'a>,
    slots: &'l Slots,
    objects: &'l Objects,
    units: &'l PerPackage<Imports>,
    tails: &'l PerPackage<Imports>,
    sources: Vec<Source>,
    cells: Vec<Option<usize>>,
    object_cells: Vec<Vec<usize>>,
    impl_edges: HashMap<usize, Vec<usize>>,
    named: HashMap<String, usize>,
    kept_objects: Vec<bool>,
    kept_globals: Vec<bool>,
    queue: VecDeque<(bool, usize)>,
    full_image: bool,
}

impl<'a> Reachability<'_, 'a, '_> {
    fn unit_space(&self, id: LinkPackageId) -> UnitSpace<'_> {
        let p = self.set.package(id);
        UnitSpace {
            name: &p.name,
            ordinal: id.0 as usize,
            unit: p.unit,
            objects: &self.objects.units[id],
            slots: &self.slots.units[id],
            imports: &self.units[id],
        }
    }
    fn tail_space(&self, id: LinkPackageId) -> TailSpace<'_> {
        TailSpace {
            name: self.set.name(id),
            ordinal: id.0 as usize,
            tail: self
                .set
                .package(id)
                .tail
                .expect("validated source has a tail"),
            objects: &self.objects.tails[id],
            slots: &self.slots.tails[id],
            imports: &self.tails[id],
        }
    }
    fn source_object(&self, source: Source) -> &'a Object {
        let p = self.set.package(source.package);
        if source.tail {
            &p.tail.expect("tail source").objects[source.local]
        } else {
            let mut offset = source.local;
            for bucket in Bucket::ALL {
                let objects = bucket.objects(p.unit);
                if offset < objects.len() {
                    return &objects[offset];
                }
                offset -= objects.len();
            }
            unreachable!("validated original object source")
        }
    }
    fn object(&mut self, i: usize) {
        if !std::mem::replace(&mut self.kept_objects[i], true) {
            self.queue.push_back((false, i));
        }
    }
    fn global(&mut self, i: usize) {
        if !std::mem::replace(&mut self.kept_globals[i], true) {
            self.queue.push_back((true, i));
        }
    }
    fn named(&mut self, name: &str) -> Result<(), LinkError> {
        let i = self.named.get(name).copied().ok_or_else(|| {
            LinkError::invalid(format!(
                "required native/host declaration `{name}` is missing"
            ))
        })?;
        self.object(i);
        Ok(())
    }
    fn templates(
        &self,
        id: LinkPackageId,
        templates: impl IntoIterator<Item = &'a TyTemplate>,
    ) -> Result<Vec<usize>, LinkError> {
        let space = self.unit_space(id);
        let mut out = Vec::new();
        let mut error = None;
        for t in templates {
            t.visit_heads(&mut |head| match space.head(head) {
                Ok(h) => out.push(
                    h.tag()
                        .static_index()
                        .expect("original declaration identity"),
                ),
                Err(e) => {
                    error.get_or_insert(e);
                }
            });
        }
        error.map_or(Ok(out), Err)
    }
    fn object_edges(&self, source: Source) -> Result<(Vec<usize>, Vec<usize>), LinkError> {
        let unit = self.unit_space(source.package);
        let tail = source.tail.then(|| self.tail_space(source.package));
        let space: &dyn OperandSpace = tail.as_ref().map_or(&unit as &dyn OperandSpace, |s| s);
        let object = self.source_object(source);
        if let Object::Function(f) = object {
            for instruction in &f.bytecode.instructions {
                if matches!(instruction, bex_vm_types::Instruction::VirtualCall { nargs, self_arg, .. } if self_arg >= nargs)
                {
                    return Err(LinkError::invalid(
                        "virtual call receiver is outside its arguments",
                    ));
                }
            }
            for table in &f.bytecode.switch_tables {
                let bex_vm_types::bytecode::SwitchDispatch::Keys(keys) = &table.dispatch else {
                    return Err(LinkError::invalid("unit switch table must state keys"));
                };
                for key in keys {
                    if let bex_vm_types::bytecode::SwitchKey::Declaration(i) = key {
                        space.declaration(i.raw())?;
                    }
                }
            }
        }
        // A dead malformed method/default must not disappear before the
        // runtime's structural validation could reject it.
        let function_ref = |operand: usize, body: bool| -> Result<(), LinkError> {
            let i = space.object(operand)?;
            if matches!(self.source_object(self.sources[i]), Object::Function(f) if f.is_interface_body == body)
            {
                Ok(())
            } else {
                Err(LinkError::invalid(
                    "method/default reference has the wrong function kind",
                ))
            }
        };
        match object {
            Object::Class(class) => {
                for method in class.methods.values() {
                    function_ref(method.function.raw(), false)?;
                }
            }
            Object::Interface(interface) => {
                if let Some(default) = &interface.structural_default {
                    function_ref(default.function.raw(), false)?;
                }
                for method in &interface.methods {
                    if let Some(default) = method.default {
                        function_ref(default.raw(), true)?;
                    }
                }
            }
            _ => {}
        }
        let mut objects = Vec::new();
        let mut globals = Vec::new();
        let mut error = None;
        visit_object_operands_ref(object, |operand| {
            let r = match operand {
                IndexOperandRef::Object(i) => space.object(i.raw()).map(|i| objects.push(i)),
                IndexOperandRef::Global(i) => space.global(i.raw()).map(|i| globals.push(i)),
            };
            if let Err(e) = r {
                error.get_or_insert(e);
            }
        });
        visit_object_heads(object, &mut |h| match space.head(h) {
            Ok(h) => objects.push(
                h.tag()
                    .static_index()
                    .expect("original declaration identity"),
            ),
            Err(e) => {
                error.get_or_insert(e);
            }
        });
        error.map_or(Ok((objects, globals)), Err)
    }
    fn rule_edges(
        &self,
        id: LinkPackageId,
        rule: &'a baml_linker_types::ProgramImplRuleFrag,
    ) -> Result<(usize, Vec<usize>), LinkError> {
        let space = self.unit_space(id);
        let interface = space.declaration(rule.interface_head.raw())?;
        if !matches!(
            self.source_object(self.sources[interface]),
            Object::Interface(_)
        ) {
            return Err(LinkError::invalid(
                "implementation head is not an interface",
            ));
        }
        let mut templates = vec![&rule.for_ty_pattern];
        let mut edges = Vec::new();
        for b in rule.generic_param_bounds.iter().flatten() {
            let i = space.declaration(
                b.interface
                    .try_operand()
                    .ok_or_else(|| LinkError::invalid("bound head outside unit convention"))?
                    .raw(),
            )?;
            if !matches!(self.source_object(self.sources[i]), Object::Interface(_)) {
                return Err(LinkError::invalid(
                    "implementation bound is not an interface",
                ));
            }
            edges.push(i);
            templates.extend(b.args.iter());
            templates.extend(b.assoc.iter().map(|(_, t)| t));
        }
        templates.extend(rule.interface_args.iter());
        templates.extend(rule.interface_assoc.iter().map(|(_, t)| t));
        for (_, body) in &rule.methods {
            let i = self.objects.units[id]
                .code
                .get(body.code_offset as usize)
                .and_then(|p| p.index())
                .ok_or_else(|| LinkError::invalid("impl method outside original code bucket"))?;
            if !matches!(self.source_object(self.sources[i]), Object::Function(f) if f.is_interface_body)
            {
                return Err(LinkError::invalid(
                    "implementation method is not an interface body",
                ));
            }
            edges.push(i);
            templates.extend(body.frame.iter());
        }
        edges.extend(self.templates(id, templates)?);
        Ok((interface, edges))
    }
    fn run(&mut self) -> Result<(), LinkError> {
        while let Some((global, i)) = self.queue.pop_front() {
            if global {
                if let Some(object) = self.cells[i] {
                    self.object(object);
                }
                continue;
            }
            let source = self.sources[i];
            let object = self.source_object(source);
            let (objects, globals) = self.object_edges(source)?;
            for i in objects {
                self.object(i);
            }
            for i in globals {
                self.global(i);
            }
            // Named declarations keep their value cells as well. Body cells
            // matter to generic function values and direct default-body calls.
            for k in self.object_cells[i].clone() {
                self.global(k);
            }
            if let Object::Function(function) = object {
                let key = match function.kind {
                    FunctionKind::Bytecode => None,
                    FunctionKind::SysOp(op) => Some(op.path()),
                    FunctionKind::NativeUnresolved => function.native_key.as_deref(),
                    FunctionKind::Native(_) => {
                        self.full_image = true;
                        return Ok(());
                    }
                };
                if !matches!(function.kind, FunctionKind::Bytecode) {
                    let Some(deps) = key.and_then(super::native::dependencies) else {
                        self.full_image = true;
                        return Ok(());
                    };
                    for dep in deps {
                        self.named(dep)?;
                    }
                }
            }
            if let Some(edges) = self.impl_edges.get(&i).cloned() {
                for k in edges {
                    self.object(k);
                }
            }
        }
        Ok(())
    }
}

/// Original placement and binding tables, projected together onto the output.
pub(super) struct SelectionLayout<'l> {
    pub slots: &'l mut Slots,
    pub objects: &'l mut Objects,
    pub units: &'l mut PerPackage<Imports>,
    pub tails: &'l mut PerPackage<Imports>,
    pub exports: &'l mut HashMap<PathKey, usize>,
}

/// Select original identities and replace only output placement tables. The
/// unselected layout is an identity space, not a linked/serialized Program.
/// Every import is bound and every original operand checked before selection.
pub(super) fn select(
    set: &LinkSet<'_>,
    order: &LayoutOrder,
    roots: &LinkRoots,
    layout: SelectionLayout<'_>,
) -> Result<Option<PerPackage<Vec<bool>>>, LinkError> {
    let SelectionLayout {
        slots,
        objects,
        units,
        tails,
        exports,
    } = layout;
    let dummy = Source {
        package: set.root,
        tail: false,
        local: 0,
    };
    let mut sources = vec![dummy; objects.total];
    let mut cells = vec![None; slots.total];
    for id in set.ids() {
        let p = set.package(id);
        let mut flat = 0;
        for bucket in Bucket::ALL {
            for k in 0..bucket.objects(p.unit).len() {
                let i = objects.units[id]
                    .local(p.unit, flat)
                    .expect("original placement");
                if bucket != Bucket::Code || !objects.units[id].code[k].is_shadow() {
                    sources[i] = Source {
                        package: id,
                        tail: false,
                        local: flat,
                    };
                }
                flat += 1;
            }
        }
        for (path, local) in &p.unit.exports.globals {
            if !matches!(path, DeclPath::Let(_)) {
                let object = exports.get(&(id, path.clone())).copied().ok_or_else(|| {
                    LinkError::invalid("function cell has no original object export")
                })?;
                cells[slots.units[id]
                    .local(*local as usize)
                    .expect("validated original slot")] = Some(object);
            }
        }
        if let Some(tail) = p.tail {
            for (k, placed) in objects.tails[id].iter().enumerate() {
                if !placed.is_shadow() {
                    sources[placed.abs()] = Source {
                        package: id,
                        tail: true,
                        local: k,
                    };
                }
            }
            for (k, &object) in tail.slot_objects.iter().enumerate() {
                cells[slots.tails[id].local(tail, k).expect("original tail slot")] =
                    Some(objects.tails[id][object as usize].abs());
            }
        }
    }
    // Names enter only at host/native boundaries, from the root's viewpoint.
    // A user's same-spelled transitive package cannot replace a prelude edge.
    let root = set.package(set.root);
    let viewpoints = std::iter::once((&root.name, set.root))
        .chain(root.edges.iter().map(|e| (&e.name, e.target)));
    let mut named = HashMap::new();
    let mut surface = Vec::new();
    for (name, id) in viewpoints {
        for (path, local) in &set.package(id).unit.exports.objects {
            let suffix = match path {
                DeclPath::Class(n)
                | DeclPath::Enum(n)
                | DeclPath::Interface(n)
                | DeclPath::TypeAlias(n) => n.to_string(),
                DeclPath::Function(FnPath::Free(n)) => n.to_string(),
                DeclPath::Function(FnPath::Method { class, name }) => format!("{class}.{name}"),
                DeclPath::InterfaceBody(_) | DeclPath::Let(_) => continue,
            };
            let i = objects.units[id]
                .export(set.package(id).unit, *local)
                .expect("validated export");
            named.insert(format!("{name}.{suffix}"), i);
            surface.push(i);
        }
    }
    let mut object_cells = vec![Vec::new(); objects.total];
    for (cell, object) in cells.iter().enumerate() {
        if let Some(i) = object {
            object_cells[*i].push(cell);
        }
    }
    let mut mark = Reachability {
        set,
        slots,
        objects,
        units,
        tails,
        sources,
        cells,
        object_cells,
        impl_edges: HashMap::new(),
        named,
        kept_objects: vec![false; objects.total],
        kept_globals: vec![false; slots.total],
        queue: VecDeque::new(),
        full_image: false,
    };
    // A malformed dead function is still a malformed cache entry. Selection
    // must not conceal broken indices or type-head/import shape violations.
    for source in mark.sources.iter().copied() {
        mark.object_edges(source)?;
    }
    for id in set.ids() {
        for rule in &set.package(id).unit.impl_rules {
            let (interface, edges) = mark.rule_edges(id, rule)?;
            mark.impl_edges.entry(interface).or_default().extend(edges);
        }
    }
    match roots {
        LinkRoots::EntryPoints(names) => {
            if names.is_empty() {
                return Err(LinkError::invalid("entry-point selection requires a root"));
            }
            for name in names {
                let i = mark.named.get(name).copied().ok_or_else(|| {
                    LinkError::invalid(format!("entry point `{name}` is missing"))
                })?;
                if !matches!(mark.source_object(mark.sources[i]), Object::Function(_)) {
                    return Err(LinkError::invalid(format!(
                        "entry point `{name}` is not a callable"
                    )));
                }
                mark.object(i);
            }
        }
        LinkRoots::HostSurface => {
            for i in surface {
                mark.object(i);
            }
        }
    }
    // Engine startup, panic/error conversion and host JSON input/output are
    // implicit callers. The latter must honor user overrides at every depth.
    let implicit: Vec<_> = mark
        .named
        .iter()
        .filter_map(|(n, &i)| {
            (n.starts_with("baml.errors.")
                || n.starts_with("baml.panics.")
                || n == "baml.time.Duration")
                .then_some(i)
        })
        .collect();
    for i in implicit {
        mark.object(i);
    }
    for name in [
        "baml.json.serialize",
        "baml.json.deserialize",
        "baml.json.to",
        "baml.ToJson",
        "baml.FromJson",
        "baml.ToString",
        "baml.ops.Equals",
    ] {
        if mark.named.contains_key(name) {
            mark.named(name)?;
        }
    }
    // Initializer execution and test registration remain observable effects.
    // Conservatively preserve every tail and let cell, in their original order.
    for id in set.ids() {
        for (path, local) in &set.package(id).unit.exports.globals {
            if matches!(path, DeclPath::Let(_)) {
                mark.global(
                    slots.units[id]
                        .local(*local as usize)
                        .expect("validated let cell"),
                );
            }
        }
        if let Some(tail) = set.package(id).tail {
            for p in &objects.tails[id] {
                mark.object(p.abs());
            }
            for k in 0..tail.slot_objects.len() {
                mark.global(slots.tails[id].local(tail, k).expect("original tail slot"));
            }
        }
    }
    mark.run()?;
    if mark.full_image {
        return Ok(None);
    }
    let rules = PerPackage::try_from_set(set, |id, p| {
        p.unit
            .impl_rules
            .iter()
            .map(|r| {
                mark.unit_space(id)
                    .object(r.interface_head.raw())
                    .map(|i| mark.kept_objects[i])
            })
            .collect()
    })?;
    let kept_objects = mark.kept_objects;
    let kept_globals = mark.kept_globals;
    compact(
        set,
        order,
        SelectionLayout {
            slots,
            objects,
            units,
            tails,
            exports,
        },
        &kept_objects,
        &kept_globals,
    )?;
    Ok(Some(rules))
}

fn compact(
    set: &LinkSet<'_>,
    order: &LayoutOrder,
    layout: SelectionLayout<'_>,
    kept_objects: &[bool],
    kept_globals: &[bool],
) -> Result<(), LinkError> {
    fn placements(keep: &[bool]) -> (Vec<Option<usize>>, usize) {
        let mut cursor = 0;
        let map = keep
            .iter()
            .map(|&k| {
                k.then(|| {
                    let n = cursor;
                    cursor += 1;
                    n
                })
            })
            .collect();
        (map, cursor)
    }
    let SelectionLayout {
        slots,
        objects,
        units,
        tails,
        exports,
    } = layout;
    let (omap, onext) = placements(kept_objects);
    let (gmap, gnext) = placements(kept_globals);
    let mut gcursor = 0;
    for &id in order.members() {
        let unit = set.package(id).unit;
        let count = Bucket::ALL.iter().map(|b| b.objects(unit).len()).sum();
        let flat = (0..count)
            .map(|k| objects.units[id].local(unit, k).expect("original object"))
            .collect::<Vec<_>>();
        objects.units[id].selected = Some(flat.into_iter().map(|k| omap[k]).collect());
        for p in &mut objects.units[id].code {
            *p = match omap[p.abs()] {
                Some(i) if p.is_shadow() => Placed::Shadow(i),
                Some(i) => Placed::Own(i),
                None => Placed::Dropped,
            };
        }
        for p in &mut objects.tails[id] {
            let i = omap[p.abs()].expect("all tail objects retained");
            *p = if p.is_shadow() {
                Placed::Shadow(i)
            } else {
                Placed::Own(i)
            };
        }
        let old = &slots.units[id];
        let functions = old.func_count;
        let lets = old.let_count;
        let flat = (0..functions + lets)
            .map(|k| gmap[old.local(k).expect("original slot")])
            .collect::<Vec<_>>();
        let f = flat[..functions].iter().filter(|x| x.is_some()).count();
        let l = flat[functions..].iter().filter(|x| x.is_some()).count();
        slots.units[id].func_base = gcursor;
        slots.units[id].func_count = f;
        gcursor += f;
        slots.units[id].let_base = gcursor;
        slots.units[id].let_count = l;
        gcursor += l;
        slots.units[id].selected = Some(flat);
        for part in LayoutOrder::parts(set, id) {
            match part {
                TailPart::Init => slots.tails[id].init_base = gcursor,
                TailPart::Test => slots.tails[id].test_base = gcursor,
            }
            gcursor += part
                .slots(set.package(id).tail.expect("ordered tail"))
                .len();
        }
        for import in [&mut units[id], &mut tails[id]] {
            for i in &mut import.objects {
                *i = i.and_then(|k| omap[k]);
            }
            for i in &mut import.globals {
                *i = i.and_then(|k| gmap[k]);
            }
        }
    }
    if gcursor != gnext {
        return Err(LinkError::invalid(
            "selected global layout disagrees with reachability",
        ));
    }
    slots.total = gnext;
    objects.total = onext;
    exports.retain(|_, i| {
        if let Some(n) = omap[*i] {
            *i = n;
            true
        } else {
            false
        }
    });
    Ok(())
}

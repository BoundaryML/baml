//! What a runtime compile produces and what a `reflect.CompileArtifact`
//! holds between `_compile` and `_finish`.

use baml_linker_types::{EmittedPackage, ItemPath, SessionInitializer};
use bex_heap::Handle;
use bex_vm_types::{RuntimeCompileDiagnostic, SessionEvalLease, SessionVisibleSymbol};
use indexmap::IndexMap;

/// Successful compiler output retained by the runtime: one package's
/// output in the unit convention, loaded by the graft against the packages
/// it was compiled against.
#[derive(Debug)]
pub struct RuntimeCompileArtifact {
    /// The package (or submission) as emitted: builtin and dependency
    /// declarations are imports the graft binds.
    pub emitted: EmittedPackage,
    /// Versioned artifact containing the enriched check surface for mounting
    /// this package in a later compile.
    pub interface_blob: Vec<u8>,
    /// Non-error diagnostics produced by the successful compilation.
    pub diagnostics: Vec<RuntimeCompileDiagnostic>,
    pub kind: ArtifactKind,
}

/// Which runtime compilation door produced an artifact.
#[derive(Debug)]
pub enum ArtifactKind {
    Package,
    Session {
        meta: RuntimeSessionCompileArtifact,
        /// S-9 permit transferred from the request to the successful artifact.
        lease: SessionEvalLease,
    },
}

/// Compiler-owned Session metadata retained after the fresh database drops.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeSessionCompileArtifact {
    pub submission_name: String,
    /// Hoisted declarations are committed before the first initializer runs.
    pub declaration_source: String,
    pub declarations: IndexMap<String, SessionVisibleSymbol>,
    pub steps: Vec<RuntimeSessionStep>,
    /// The step whose value is the submission's observable result.
    pub result_step: Option<usize>,
    /// The submission's `let` helpers as the emitter recorded them, in
    /// execution order.
    pub initializers: Vec<SessionInitializer>,
}

/// What one emitted initializer commits when it returns successfully.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeSessionStep {
    /// The generated `let` receiving the initializer's result.
    pub global: ItemPath,
    /// An existing session cell to update after this initializer succeeds.
    /// `None` means the generated `let` itself receives the value.
    pub commit_global: Option<ItemPath>,
    pub kind: RuntimeSessionStepKind,
}

/// The two legal commit shapes of a Session initializer step.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RuntimeSessionStepKind {
    Expression,
    Binding {
        /// Source-visible binding committed by this step.
        name: String,
        symbol: SessionVisibleSymbol,
        /// Replayed source fragment appended only after this step succeeds.
        replay_source: String,
    },
}

/// A compile's output together with the dependency packages it was compiled
/// against, each pinned by a rooted [`Handle`] under the alias the request
/// spelled it by. `_finish` binds the package's dependencies from these pins
/// — never from its own argument, which it only checks against them — so the
/// declarations the output references are the ones the compiler read, and
/// they stay alive until the artifact is consumed or dropped. A session's
/// artifact pins nothing: its dependency table is the session package's.
pub struct PinnedArtifact {
    pub artifact: RuntimeCompileArtifact,
    pub pins: IndexMap<String, Handle>,
}

/// One-shot storage used by the BAML `CompileArtifact` wrapper: holds the
/// artifact until `_finish` takes it.
pub struct RuntimeCompileArtifactSlot(std::sync::Mutex<Option<PinnedArtifact>>);

impl RuntimeCompileArtifactSlot {
    pub fn new(artifact: PinnedArtifact) -> Self {
        Self(std::sync::Mutex::new(Some(artifact)))
    }

    /// The artifact, if it has not been taken.
    pub fn take(&self) -> Result<Option<PinnedArtifact>, std::sync::PoisonError<()>> {
        self.0
            .lock()
            .map(|mut slot| slot.take())
            .map_err(|_| std::sync::PoisonError::new(()))
    }
}

impl bex_vm_types::BexRustData for RuntimeCompileArtifactSlot {
    fn measure(&self, meter: &mut bex_vm_types::Meter) {
        // Only `_finish` locks the slot, for one call on the VM thread.
        if let Ok(mut slot) = self.0.try_lock()
            && let Some(pinned) = slot.as_mut()
        {
            let artifact = &mut pinned.artifact;
            meter.bytes(artifact.interface_blob.capacity());
            measure_vec(meter, &artifact.diagnostics);
            for diagnostic in &artifact.diagnostics {
                measure_diagnostic(meter, diagnostic);
            }
            meter.bytes(pinned.pins.capacity() * size_of::<(String, Handle, usize)>());
            for key in pinned.pins.keys() {
                meter.bytes(key.capacity());
            }
            let emitted = &mut artifact.emitted;
            meter.bytes(emitted.record.interface_blob.capacity());
            let unit = &mut emitted.unit;
            for objects in [
                &mut unit.classes,
                &mut unit.enums,
                &mut unit.interfaces,
                &mut unit.type_alias_objects,
                &mut unit.code,
            ] {
                measure_vec(meter, objects);
                for object in objects {
                    object.measure(meter);
                }
            }
            // Declaration metadata is shallow, as in Object::measure; code
            // and literal payloads above are measured through the same meter.
            measure_vec(meter, &unit.dependencies);
            measure_vec(meter, &unit.object_imports);
            measure_vec(meter, &unit.global_imports);
            measure_vec(meter, &unit.exports.objects);
            measure_vec(meter, &unit.exports.globals);
            measure_vec(meter, &unit.impl_rules);
            for rule in &unit.impl_rules {
                measure_vec(meter, &rule.generic_param_bounds);
                for bounds in &rule.generic_param_bounds {
                    measure_vec(meter, bounds);
                }
                measure_vec(meter, &rule.interface_args);
                measure_vec(meter, &rule.interface_assoc);
                measure_vec(meter, &rule.methods);
                for (_, method) in &rule.methods {
                    measure_vec(meter, &method.frame);
                }
                meter.bytes(size_of_val(&*rule.field_links));
            }
            if let Some(tail) = &mut emitted.tail {
                measure_vec(meter, &tail.objects);
                for object in &mut tail.objects {
                    object.measure(meter);
                }
                measure_vec(meter, &tail.dependencies);
                measure_vec(meter, &tail.object_imports);
                measure_vec(meter, &tail.global_imports);
                measure_vec(meter, &tail.slot_objects);
            }
            if let ArtifactKind::Session { meta, .. } = &artifact.kind {
                meter.bytes(meta.submission_name.capacity());
                meter.bytes(meta.declaration_source.capacity());
                meter.bytes(
                    meta.declarations.capacity()
                        * size_of::<(String, SessionVisibleSymbol, usize)>(),
                );
                for (name, symbol) in &meta.declarations {
                    meter.bytes(name.capacity());
                    measure_session_symbol(meter, symbol);
                }
                measure_vec(meter, &meta.steps);
                for step in &meta.steps {
                    if let RuntimeSessionStepKind::Binding {
                        name,
                        replay_source,
                        symbol,
                    } = &step.kind
                    {
                        meter.bytes(name.capacity());
                        meter.bytes(replay_source.capacity());
                        measure_session_symbol(meter, symbol);
                    }
                }
                measure_vec(meter, &meta.initializers);
            }
        }
    }
}

fn measure_session_symbol(meter: &mut bex_vm_types::Meter, symbol: &SessionVisibleSymbol) {
    meter.bytes(symbol.internal.capacity());
    if let bex_vm_types::SessionVisibleKind::TypeBinding { type_value } = &symbol.kind {
        meter.bytes(type_value.capacity());
    }
}

fn measure_vec<T>(meter: &mut bex_vm_types::Meter, values: &Vec<T>) {
    meter.bytes(values.capacity() * size_of::<T>());
}

fn measure_diagnostic(meter: &mut bex_vm_types::Meter, diagnostic: &RuntimeCompileDiagnostic) {
    meter.bytes(diagnostic.code.capacity());
    meter.bytes(diagnostic.message.capacity());
    if let Some(span) = &diagnostic.span {
        meter.bytes(span.file.capacity());
    }
    if let Some(details) = &diagnostic.details {
        meter.bytes(size_of_val(&**details));
        meter.bytes(details.headline.capacity());
        meter.bytes(details.primary_label.as_ref().map_or(0, String::capacity));
        measure_vec(meter, &details.message_highlights);
        measure_vec(meter, &details.annotations);
        for annotation in &details.annotations {
            meter.bytes(annotation.span.file.capacity());
            meter.bytes(annotation.message.as_ref().map_or(0, String::capacity));
            measure_vec(meter, &annotation.message_highlights);
        }
        measure_vec(meter, &details.related_info);
        for related in &details.related_info {
            meter.bytes(related.span.file.capacity());
            meter.bytes(related.message.capacity());
            meter.bytes(related.file_path.as_ref().map_or(0, String::capacity));
            measure_vec(meter, &related.message_highlights);
        }
    }
}

#[cfg(test)]
mod memory_tests {
    use bex_vm_types::{BexRustData, Meter, Object, RuntimeDiagnosticSeverity};

    use super::*;

    #[test]
    fn artifact_counts_emitted_literals_and_diagnostic_text_until_taken() {
        let mut unit = baml_linker_types::CompilationUnit::default();
        unit.code
            .push(Object::String(bex_str::BexStr::from("x".repeat(100_000))));
        let slot = RuntimeCompileArtifactSlot::new(PinnedArtifact {
            artifact: RuntimeCompileArtifact {
                emitted: EmittedPackage {
                    unit,
                    record: baml_linker_types::PackageRecord::default(),
                    tail: None,
                },
                interface_blob: Vec::new(),
                diagnostics: vec![RuntimeCompileDiagnostic {
                    code: "warning".into(),
                    message: "y".repeat(50_000),
                    severity: RuntimeDiagnosticSeverity::Warning,
                    span: None,
                    details: None,
                }],
                kind: ArtifactKind::Package,
            },
            pins: IndexMap::new(),
        });
        let mut meter = Meter::census();
        slot.measure(&mut meter);
        assert!(meter.total() >= 150_000);
        let _artifact = slot.take().unwrap();
        let mut meter = Meter::census();
        slot.measure(&mut meter);
        assert_eq!(meter.total(), 0);
    }
}

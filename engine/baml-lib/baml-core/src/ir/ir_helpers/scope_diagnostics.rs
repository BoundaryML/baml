#[allow(dead_code)]
type ScopeName = Vec<String>;

trait ScopeTrait {
    fn push(&mut self, name: String, scope_type: ScopeType);

    fn pop(
        &mut self,
        errors_as_warnings: bool,
    ) -> (Vec<(ScopeName, String)>, Vec<(ScopeName, String)>);
}

#[allow(dead_code)]
enum ScopeType {
    Generic,
    Class,
    Array,
    Union,
}

#[allow(dead_code)]
enum Scope {
    Generic(GenericScope),
    Class(ClassScope),
    Array(ArrayScope),
    Union(UnionScope),
}

#[derive(Debug)]
struct GenericScope {
    name: Option<String>,
    errors: Vec<String>,
    warnings: Vec<String>,
}

#[allow(dead_code)]
struct ClassScope {
    name: String,
    fields: Vec<Scope>,
}

#[allow(dead_code)]
struct ArrayScope {
    items: Vec<Scope>,
}

#[allow(dead_code)]
struct UnionScope {
    options: Vec<Scope>,
}

impl GenericScope {
    fn new(name: String) -> Self {
        Self {
            name: Some(name),
            errors: Vec::new(),
            warnings: Vec::new(),
        }
    }

    #[allow(dead_code)]
    fn push_type_error(&mut self, expected: &str, got: &str) {
        self.errors
            .push(format!("Expected type {expected}, got `{got}`"));
    }
}

#[derive(Debug)]
pub struct ScopeStack {
    // Always contains at least one scope
    scopes: Vec<GenericScope>,
}

impl std::error::Error for ScopeStack {}

impl std::fmt::Display for ScopeStack {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (depth, scope) in self.scopes.iter().enumerate() {
            if scope.errors.is_empty() && scope.warnings.is_empty() {
                continue;
            }
            let indent = "  ".repeat(depth);
            if let Some(name) = &scope.name {
                writeln!(f, "{indent}{name}:")?;
            }
            for error in &scope.errors {
                writeln!(f, "{indent}  Error: {error}")?;
            }
            for warning in &scope.warnings {
                writeln!(f, "{indent}  Warning: {warning}")?;
            }
        }
        Ok(())
    }
}

impl ScopeStack {
    pub fn new() -> Self {
        Self {
            scopes: vec![GenericScope {
                name: None,
                errors: Vec::new(),
                warnings: Vec::new(),
            }],
        }
    }

    pub fn has_errors(&self) -> bool {
        self.scopes.iter().any(|s| !s.errors.is_empty())
    }

    pub fn push(&mut self, name: String) {
        self.scopes.push(GenericScope::new(name));
    }

    pub fn pop(&mut self, errors_as_warnings: bool) {
        if self.scopes.len() == 1 {
            // never pop the root scope
            return;
        }

        let scope = self.scopes.pop().unwrap();
        let parent_scope = self.scopes.last_mut().unwrap();

        if let Some(name) = scope.name {
            if errors_as_warnings {
                parent_scope
                    .warnings
                    .extend(scope.errors.iter().map(|e| format!("{name}: {e}")));
            } else {
                parent_scope
                    .errors
                    .extend(scope.errors.iter().map(|e| format!("{name}: {e}")));
            }
            parent_scope
                .warnings
                .extend(scope.warnings.iter().map(|e| format!("{name}: {e}")));
        } else {
            if errors_as_warnings {
                parent_scope.warnings.extend(scope.errors);
            } else {
                parent_scope.errors.extend(scope.errors);
            }
            parent_scope.warnings.extend(scope.warnings);
        }
    }

    pub fn push_error(&mut self, error: String) {
        self.scopes.last_mut().unwrap().errors.push(error);
    }
}

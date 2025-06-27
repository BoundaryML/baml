use baml_types::baml_value::TypeLookups;
use dir_writer::IntermediateRepr;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Package {
    package_path: Vec<String>,
    /// used in scenarios like class properties, or type aliases RHS
    type_definition_scope: bool,
}

impl Package {
    fn new(package: &str) -> Self {
        let parts: Vec<_> = package.split('.').map(str::to_string).collect();
        if parts.is_empty() {
            panic!("Package cannot be empty");
        }
        // ensure the first part is baml_client
        if parts[0] != "baml_client" && parts[0] != "baml_py" {
            panic!("Package must start with baml_client: {package}");
        }
        Package {
            package_path: parts,
            type_definition_scope: false,
        }
    }

    pub fn clone_as_type_definition(&self) -> Self {
        Self {
            package_path: self.package_path.clone(),
            type_definition_scope: true,
        }
    }

    pub fn in_type_definition(&self) -> bool {
        self.type_definition_scope
    }

    pub fn relative_from(&self, other: &CurrentRenderPackage) -> String {
        // Py does wierd imports, so we return only the last part of the package
        // unless the other package is the same as self, in which case we return empty
        let other = other.get();
        if self.package_path == other.package_path {
            return "".to_string();
        }
        format!("{}.", self.package_path.last().unwrap())
    }

    pub fn types() -> Package {
        Package::new("baml_client.types")
    }

    pub fn stream_types() -> Package {
        Package::new("baml_client.stream_types")
    }

    pub fn checked() -> Package {
        Package::types()
    }

    pub fn stream_state() -> Package {
        Package::stream_types()
    }

    pub fn imported_base() -> Package {
        Package::new("baml_py")
    }
}

impl std::fmt::Display for Package {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.package_path.join("."))
    }
}

#[derive(Clone)]
pub(crate) struct CurrentRenderPackage {
    package: std::sync::Arc<std::sync::Mutex<std::sync::Arc<Package>>>,
    lookup: std::sync::Arc<IntermediateRepr>,
    pub is_pydantic_2: bool,
}

impl CurrentRenderPackage {
    pub fn new(
        package: &str,
        lookup: std::sync::Arc<IntermediateRepr>,
        is_pydantic_2: bool,
    ) -> Self {
        Self {
            package: std::sync::Arc::new(std::sync::Mutex::new(std::sync::Arc::new(Package::new(
                package,
            )))),
            lookup,
            is_pydantic_2,
        }
    }

    pub fn in_type_definition(&self) -> Self {
        Self {
            package: std::sync::Arc::new(std::sync::Mutex::new(std::sync::Arc::new(
                self.get().clone_as_type_definition(),
            ))),
            lookup: self.lookup.clone(),
            is_pydantic_2: self.is_pydantic_2,
        }
    }

    pub fn lookup(&self) -> &impl TypeLookups {
        self.lookup.as_ref()
    }

    pub fn get(&self) -> std::sync::Arc<Package> {
        self.package.lock().unwrap().clone()
    }

    pub fn set(&self, package: &str) {
        match self.package.lock() {
            Ok(mut orig) => {
                *orig = std::sync::Arc::new(Package::new(package));
            }
            Err(e) => {
                panic!("Failed to get package: {e}");
            }
        }
    }

    pub fn name(&self) -> String {
        self.get().package_path.last().unwrap().clone()
    }
}

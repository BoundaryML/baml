use baml_types::baml_value::TypeLookups;
use dir_writer::IntermediateRepr;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Package {
    package_path: Vec<String>,
}

impl Package {
    fn new(package: &str) -> Self {
        let parts: Vec<_> = package.split('.').map(|s| s.to_string()).collect();
        if parts.is_empty() {
            panic!("Package cannot be empty");
        }
        // ensure the first part is baml_client
        if parts[0] != "baml_client" && parts[0] != "baml" {
            panic!("Package must start with baml_client");
        }
        Package {
            package_path: parts,
        }
    }

    pub fn relative_from(&self, other: &CurrentRenderPackage) -> String {
        // Go does wierd imports, so we return only the last part of the package
        // unless the other package is the same as self, in which case we return empty
        let other = other.get();
        if self.package_path == other.package_path {
            return "".to_string();
        }
        format!("{}.", self.package_path.last().unwrap())
    }

    pub fn current(&self) -> String {
        self.package_path.last().unwrap().clone()
    }

    pub fn types() -> Package {
        Package::new("baml_client.types")
    }

    pub fn stream_types() -> Package {
        Package::new("baml_client.stream_types")
    }

    pub fn types_builder() -> Package {
        Package::new("baml_client.types_builder")
    }

    pub fn checked() -> Package {
        Package::types()
    }

    pub fn stream_state() -> Package {
        Package::imported_base()
    }

    pub fn imported_base() -> Package {
        Package::new("baml")
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
}

impl CurrentRenderPackage {
    pub fn new(package: &str, lookup: std::sync::Arc<IntermediateRepr>) -> Self {
        Self {
            package: std::sync::Arc::new(std::sync::Mutex::new(std::sync::Arc::new(Package::new(
                package,
            )))),
            lookup,
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

    pub fn namespace(&self) -> String {
        // This is always one of two:
        match self.name().as_str() {
            "types" => "cffi.CFFITypeNamespace_TYPES".to_string(),
            "stream_types" => "cffi.CFFITypeNamespace_STREAM_TYPES".to_string(),
            other => panic!("Invalid package for a namespace call: {other}"),
        }
    }
}

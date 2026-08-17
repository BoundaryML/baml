//! Registered host-bridge identity shared by native and WebAssembly builds.

use std::sync::OnceLock;

/// Stable identity of an official host-language bridge.
///
/// Discriminants are part of the C ABI and may never be reused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum BridgeLanguage {
    NodeJs = 1,
    Python = 2,
    Go = 3,
    Rust = 4,
    CSharp = 5,
    Cpp = 6,
    Java = 7,
    Swift = 8,
    Web = 9,
    Ruby = 10,
}

impl BridgeLanguage {
    pub(crate) const fn inbound_union_ambiguity_policy(
        self,
    ) -> bex_project::InboundUnionAmbiguityPolicy {
        match self {
            Self::NodeJs | Self::Python | Self::Web | Self::Ruby => {
                bex_project::InboundUnionAmbiguityPolicy::SelectDefault
            }
            Self::Go | Self::Rust | Self::CSharp | Self::Cpp | Self::Java | Self::Swift => {
                bex_project::InboundUnionAmbiguityPolicy::Reject
            }
        }
    }

    pub const fn telemetry_name(self) -> &'static str {
        match self {
            Self::NodeJs => "nodejs",
            Self::Python => "python",
            Self::Go => "go",
            Self::Rust => "rust",
            Self::CSharp => "csharp",
            Self::Cpp => "cpp",
            Self::Java => "java",
            Self::Swift => "swift",
            Self::Web => "web",
            Self::Ruby => "ruby",
        }
    }

    pub(crate) const fn display_name(self) -> &'static str {
        match self {
            Self::NodeJs => "Node.js",
            Self::Python => "Python",
            Self::Go => "Go",
            Self::Rust => "Rust",
            Self::CSharp => "C#",
            Self::Cpp => "C++",
            Self::Java => "Java",
            Self::Swift => "Swift",
            Self::Web => "Web",
            Self::Ruby => "Ruby",
        }
    }

    pub(crate) const fn package_kind(self) -> &'static str {
        match self {
            Self::NodeJs | Self::Web => "the npm package",
            Self::Python => "the Python package",
            Self::Go => "the Go module",
            Self::Rust => "the Rust crate",
            Self::CSharp => "the NuGet package",
            Self::Cpp => "the C++ bridge",
            Self::Java => "the Maven package",
            Self::Swift => "the Swift package",
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) const fn legacy_runtime_name(self) -> &'static str {
        match self {
            Self::NodeJs => "@boundaryml/baml-bridge",
            Self::Python => "baml-bridge",
            Self::Go => "github.com/boundaryml/baml-go",
            Self::Rust => "baml_bridge",
            Self::CSharp => "baml-bridge",
            Self::Cpp => "BAML C++ bridge",
            Self::Java => "com.boundaryml:baml-bridge",
            Self::Swift => "baml-swift",
            Self::Web => "@boundaryml/baml-bridge-web",
            Self::Ruby => "Baml::Bridge",
        }
    }
}

impl TryFrom<u32> for BridgeLanguage {
    type Error = String;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::NodeJs),
            2 => Ok(Self::Python),
            3 => Ok(Self::Go),
            4 => Ok(Self::Rust),
            5 => Ok(Self::CSharp),
            6 => Ok(Self::Cpp),
            7 => Ok(Self::Java),
            8 => Ok(Self::Swift),
            9 => Ok(Self::Web),
            10 => Ok(Self::Ruby),
            _ => Err(format!("unknown BAML bridge language ID {value}")),
        }
    }
}

/// Host bridge metadata retained by the runtime for diagnostics and telemetry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BridgeInfo {
    pub language: BridgeLanguage,
    pub bridge_runtime_name: String,
    pub bridge_runtime_version: String,
    pub toolchain_version: String,
}

struct BridgeRegistry {
    info: OnceLock<BridgeInfo>,
}

impl BridgeRegistry {
    const fn new() -> Self {
        Self {
            info: OnceLock::new(),
        }
    }

    fn register(&self, requested: BridgeInfo) -> Result<&BridgeInfo, String> {
        if let Some(existing) = self.info.get() {
            return compatible_registration(existing, &requested).map(|()| existing);
        }
        if let Err(requested) = self.info.set(requested) {
            let existing = self
                .info
                .get()
                .expect("bridge registration was initialized concurrently");
            return compatible_registration(existing, &requested).map(|()| existing);
        }
        Ok(self
            .info
            .get()
            .expect("bridge registration was just initialized"))
    }
}

fn compatible_registration(existing: &BridgeInfo, requested: &BridgeInfo) -> Result<(), String> {
    if existing == requested {
        return Ok(());
    }
    Err(format!(
        "BAML native runtime is already registered by {} {} ({} SDK); cannot also register {} {} ({} SDK)",
        existing.bridge_runtime_name,
        existing.bridge_runtime_version,
        existing.language.display_name(),
        requested.bridge_runtime_name,
        requested.bridge_runtime_version,
        requested.language.display_name(),
    ))
}

static REGISTERED_BRIDGE: BridgeRegistry = BridgeRegistry::new();

pub fn registered_bridge() -> Option<&'static BridgeInfo> {
    REGISTERED_BRIDGE.info.get()
}

pub fn ensure_version_compatible(expected_version: &str) -> Result<(), String> {
    let actual_version = baml_version::CANONICAL_VERSION;
    if expected_version == actual_version {
        Ok(())
    } else {
        Err(format!(
            "BAML runtime version mismatch: bridge requires toolchain {expected_version}, but library reports {actual_version}"
        ))
    }
}

pub fn register_bridge(info: BridgeInfo) -> Result<&'static BridgeInfo, String> {
    if info.bridge_runtime_name.is_empty() {
        return Err("BAML bridge runtime name must not be empty".to_string());
    }
    if info.bridge_runtime_version.is_empty() {
        return Err("BAML bridge runtime version must not be empty".to_string());
    }
    if ensure_version_compatible(&info.toolchain_version).is_err() {
        return Err(format!(
            "BAML {} {} cannot use native runtime {} because it requires BAML toolchain {}",
            info.bridge_runtime_name,
            info.bridge_runtime_version,
            baml_version::CANONICAL_VERSION,
            info.toolchain_version,
        ));
    }
    let registered = REGISTERED_BRIDGE.register(info)?;
    bex_project::register_inbound_union_ambiguity_policy(
        registered.language.inbound_union_ambiguity_policy(),
    )?;
    Ok(registered)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(language: BridgeLanguage) -> BridgeInfo {
        BridgeInfo {
            language,
            bridge_runtime_name: language.legacy_runtime_name().to_string(),
            bridge_runtime_version: baml_version::CANONICAL_VERSION.to_string(),
            toolchain_version: baml_version::CANONICAL_VERSION.to_string(),
        }
    }

    #[test]
    fn local_registry_is_idempotent_for_identical_registration() {
        let registry = BridgeRegistry::new();
        let info = info(BridgeLanguage::NodeJs);
        assert_eq!(registry.register(info.clone()).unwrap(), &info);
        assert_eq!(registry.register(info.clone()).unwrap(), &info);
    }

    #[test]
    fn local_registry_rejects_conflicting_registration() {
        let registry = BridgeRegistry::new();
        registry.register(info(BridgeLanguage::NodeJs)).unwrap();
        let error = registry.register(info(BridgeLanguage::Python)).unwrap_err();
        assert!(error.contains("already registered by @boundaryml/baml-bridge"));
        assert!(error.contains("cannot also register baml-bridge"));
    }

    #[test]
    fn ruby_identity_is_stable_and_uses_dynamic_union_selection() {
        assert_eq!(BridgeLanguage::try_from(10), Ok(BridgeLanguage::Ruby));
        assert_eq!(BridgeLanguage::Ruby.telemetry_name(), "ruby");
        assert_eq!(BridgeLanguage::Ruby.legacy_runtime_name(), "Baml::Bridge");
        assert_eq!(
            BridgeLanguage::Ruby.inbound_union_ambiguity_policy(),
            bex_project::InboundUnionAmbiguityPolicy::SelectDefault,
        );
    }
}

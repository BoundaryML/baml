use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
};

use baml_codegen_types::Name;

use crate::names::{allocate_scope, namespace_segment};

pub(crate) const ROOT_NAMESPACE: &str = "BamlSdk";

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct Leaf {
    pub(crate) namespace: String,
    pub(crate) path: PathBuf,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum NamespaceNode {
    Root,
    VendorRoot,
    User(Vec<String>),
    VendorPackage(String),
    VendorNamespace {
        package: String,
        segments: Vec<String>,
    },
}

impl NamespaceNode {
    fn preferred(&self) -> String {
        match self {
            Self::VendorRoot => "Vendor".to_string(),
            Self::User(segments) | Self::VendorNamespace { segments, .. } => {
                namespace_segment(segments.last().expect("non-root namespace segment"))
            }
            Self::VendorPackage(package) => namespace_segment(package),
            Self::Root => unreachable!("the namespace root has no projected segment"),
        }
    }

    fn identity(&self) -> String {
        match self {
            Self::Root => "generated:root".to_string(),
            Self::VendorRoot => "generated:vendor-root".to_string(),
            Self::User(segments) => format!("user:namespace:{}", encode_identity(segments)),
            Self::VendorPackage(package) => {
                format!("vendor:package:{}:{package}", package.len())
            }
            Self::VendorNamespace { package, segments } => format!(
                "vendor:package:{}:{package}:namespace:{}",
                package.len(),
                encode_identity(segments)
            ),
        }
    }
}

pub(crate) struct RouteMap {
    leaves: BTreeMap<Name, Leaf>,
}

impl RouteMap {
    pub(crate) fn new<'a>(names: impl IntoIterator<Item = &'a Name>) -> Self {
        let names = names.into_iter().cloned().collect::<BTreeSet<_>>();
        let mut children = BTreeMap::<NamespaceNode, BTreeSet<NamespaceNode>>::new();
        children
            .entry(NamespaceNode::Root)
            .or_default()
            .insert(NamespaceNode::VendorRoot);

        for name in &names {
            if name.is_local() {
                let mut parent = NamespaceNode::Root;
                let mut segments = Vec::new();
                for source in name.namespace() {
                    segments.push(source.as_str().to_string());
                    let child = NamespaceNode::User(segments.clone());
                    children
                        .entry(parent.clone())
                        .or_default()
                        .insert(child.clone());
                    parent = child;
                }
            } else {
                let package = name.package().as_str().to_string();
                let package_node = NamespaceNode::VendorPackage(package.clone());
                children
                    .entry(NamespaceNode::VendorRoot)
                    .or_default()
                    .insert(package_node.clone());
                let mut parent = package_node;
                let mut segments = Vec::new();
                for source in name.namespace() {
                    segments.push(source.as_str().to_string());
                    let child = NamespaceNode::VendorNamespace {
                        package: package.clone(),
                        segments: segments.clone(),
                    };
                    children
                        .entry(parent.clone())
                        .or_default()
                        .insert(child.clone());
                    parent = child;
                }
            }
        }

        let mut allocated = BTreeMap::<(NamespaceNode, NamespaceNode), String>::new();
        allocated.insert(
            (NamespaceNode::Root, NamespaceNode::VendorRoot),
            "Vendor".to_string(),
        );
        for (parent, child_nodes) in &children {
            let mut occupied = BTreeSet::new();
            let requests = child_nodes
                .iter()
                .filter(|child| !matches!(child, NamespaceNode::VendorRoot))
                .map(|child| (child.preferred(), child.identity()))
                .collect::<Vec<_>>();
            if matches!(parent, NamespaceNode::Root) {
                occupied.insert("Vendor".to_string());
            }
            let scope = allocate_scope(requests, &mut occupied);
            for child in child_nodes {
                if matches!(child, NamespaceNode::VendorRoot) {
                    continue;
                }
                allocated.insert(
                    (parent.clone(), child.clone()),
                    scope[&child.identity()].clone(),
                );
            }
        }

        let leaves = names
            .into_iter()
            .map(|name| {
                let leaf = allocated_leaf(&name, &allocated);
                (name, leaf)
            })
            .collect();
        Self { leaves }
    }

    pub(crate) fn leaf(&self, name: &Name) -> Leaf {
        self.leaves
            .get(name)
            .cloned()
            .unwrap_or_else(|| route(name))
    }
}

fn allocated_leaf(
    name: &Name,
    allocated: &BTreeMap<(NamespaceNode, NamespaceNode), String>,
) -> Leaf {
    let mut namespace_segments = vec![ROOT_NAMESPACE.to_string()];
    let mut path = PathBuf::new();
    let mut parent = NamespaceNode::Root;

    if !name.is_local() {
        let vendor = NamespaceNode::VendorRoot;
        namespace_segments.push(allocated[&(parent.clone(), vendor.clone())].clone());
        parent = vendor;
        let package = NamespaceNode::VendorPackage(name.package().as_str().to_string());
        let projected = allocated[&(parent.clone(), package.clone())].clone();
        namespace_segments.push(projected.clone());
        path.push(file_segment(&projected));
        parent = package;
    }

    let mut source_segments = Vec::new();
    for source in name.namespace() {
        source_segments.push(source.as_str().to_string());
        let child = if name.is_local() {
            NamespaceNode::User(source_segments.clone())
        } else {
            NamespaceNode::VendorNamespace {
                package: name.package().as_str().to_string(),
                segments: source_segments.clone(),
            }
        };
        let projected = allocated[&(parent.clone(), child.clone())].clone();
        namespace_segments.push(projected.clone());
        path.push(file_segment(&projected));
        parent = child;
    }

    Leaf {
        namespace: namespace_segments.join("."),
        path,
    }
}

fn encode_identity(segments: &[String]) -> String {
    segments
        .iter()
        .map(|segment| format!("{}:{segment}", segment.len()))
        .collect::<Vec<_>>()
        .join(":")
}

pub(crate) fn route(name: &Name) -> Leaf {
    let mut namespace_segments = vec![ROOT_NAMESPACE.to_string()];
    let mut path = PathBuf::new();

    if !name.is_local() {
        namespace_segments.push("Vendor".to_string());
        let segment = namespace_segment(name.package().as_str());
        namespace_segments.push(segment.clone());
        path.push(file_segment(&segment));
    }

    for source_segment in name.namespace() {
        let segment = namespace_segment(source_segment.as_str());
        namespace_segments.push(segment.clone());
        path.push(file_segment(&segment));
    }

    Leaf {
        namespace: namespace_segments.join("."),
        path,
    }
}

pub(crate) fn functions_path(leaf: &Leaf) -> PathBuf {
    leaf.path.join("Functions.g.cs")
}

pub(crate) fn types_path(leaf: &Leaf) -> PathBuf {
    leaf.path.join("Types.g.cs")
}

fn file_segment(projected: &str) -> String {
    let unescaped = projected.strip_prefix('@').unwrap_or(projected);
    let upper = unescaped.to_ascii_uppercase();
    let reserved = matches!(upper.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || upper.strip_prefix("COM").is_some_and(|suffix| {
            matches!(suffix, "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9")
        })
        || upper.strip_prefix("LPT").is_some_and(|suffix| {
            matches!(suffix, "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9")
        });
    if reserved {
        format!("_{unescaped}")
    } else {
        unescaped.to_string()
    }
}

#[cfg(test)]
mod tests {
    use baml_base::Name as BaseName;

    use super::*;

    #[test]
    fn keeps_windows_device_names_out_of_generated_paths() {
        let name = Name::new(
            BaseName::new("user"),
            vec![BaseName::new("con"), BaseName::new("lpt1")],
            BaseName::new("probe"),
        );

        let leaf = route(&name);
        assert_eq!(leaf.namespace, "BamlSdk.Con.Lpt1");
        assert_eq!(leaf.path, PathBuf::from("_Con/_Lpt1"));
    }

    #[test]
    fn allocates_colliding_namespaces_and_reserves_vendor() {
        let snake = Name::new(
            BaseName::new("user"),
            vec![BaseName::new("foo_bar")],
            BaseName::new("snake"),
        );
        let camel = Name::new(
            BaseName::new("user"),
            vec![BaseName::new("fooBar")],
            BaseName::new("camel"),
        );
        let user_vendor = Name::new(
            BaseName::new("user"),
            vec![BaseName::new("vendor")],
            BaseName::new("user_vendor"),
        );
        let package = Name::new(
            BaseName::new("external"),
            Vec::new(),
            BaseName::new("package_type"),
        );

        let forward = RouteMap::new([&snake, &camel, &user_vendor, &package]);
        let reverse = RouteMap::new([&package, &user_vendor, &camel, &snake]);
        for name in [&snake, &camel, &user_vendor, &package] {
            assert_eq!(forward.leaf(name), reverse.leaf(name));
        }

        let snake_leaf = forward.leaf(&snake);
        let camel_leaf = forward.leaf(&camel);
        assert_ne!(snake_leaf, camel_leaf);
        assert!(snake_leaf.namespace.starts_with("BamlSdk.FooBar_"));
        assert!(camel_leaf.namespace.starts_with("BamlSdk.FooBar_"));
        assert_ne!(
            snake_leaf.path.to_string_lossy().to_ascii_lowercase(),
            camel_leaf.path.to_string_lossy().to_ascii_lowercase()
        );
        assert!(
            forward
                .leaf(&user_vendor)
                .namespace
                .starts_with("BamlSdk.Vendor_")
        );
        assert_eq!(forward.leaf(&package).namespace, "BamlSdk.Vendor.External");
    }
}

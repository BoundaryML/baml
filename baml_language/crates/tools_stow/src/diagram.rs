//! Turning the reduced graph into what gets drawn: clusters, proxy nodes and
//! styled links (spec 9.3, 9.4).

use std::collections::{BTreeMap, BTreeSet};

use crate::{
    config::{Config, Namespace},
    graph::{Graph, Target},
    workspace::Workspace,
};

/// Declared in the order nodes are listed inside a cluster.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Node {
    Crate {
        name: String,
        tag: Option<String>,
    },
    /// A stand-in for a crate that lives in the namespace `origin`.
    Proxy {
        krate: String,
        origin: String,
    },
    External {
        package: String,
        features: Vec<String>,
    },
}

impl Node {
    pub(crate) fn label(&self) -> String {
        match self {
            Node::Crate { name, .. } => name.clone(),
            Node::Proxy { krate, .. } => krate.clone(),
            Node::External { package, features } if features.is_empty() => package.clone(),
            Node::External { package, features } => format!("{package} [{}]", features.join(", ")),
        }
    }
}

/// A node together with the namespace cluster it is drawn in.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Place {
    pub(crate) namespace: String,
    pub(crate) node: Node,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Link {
    pub(crate) from: Place,
    pub(crate) to: Place,
    pub(crate) dashed: bool,
}

impl Link {
    fn solid(from: Place, to: Place) -> Self {
        Link {
            from,
            to,
            dashed: false,
        }
    }

    fn dashed(from: Place, to: Place) -> Self {
        Link {
            from,
            to,
            dashed: true,
        }
    }
}

#[derive(Debug)]
pub(crate) struct Cluster {
    pub(crate) namespace: String,
    pub(crate) nodes: Vec<Node>,
}

#[derive(Debug)]
pub(crate) struct Diagram {
    /// Sorted by namespace.
    pub(crate) clusters: Vec<Cluster>,
    pub(crate) links: BTreeSet<Link>,
}

pub(crate) fn plan(graph: &Graph, workspace: &Workspace, config: &Config) -> Diagram {
    let namespaces: BTreeMap<&str, &Namespace> = config
        .namespaces
        .iter()
        .rev()
        .map(|namespace| (namespace.name.as_str(), namespace))
        .collect();
    let namespace_of = |krate: &str| -> &str {
        workspace
            .krate(krate)
            .and_then(|krate| krate.namespace)
            .map_or("", |index| config.namespaces[index].name.as_str())
    };
    let real = |krate: &str| Place {
        namespace: namespace_of(krate).to_owned(),
        node: Node::Crate {
            name: krate.to_owned(),
            tag: config.tag_of(krate),
        },
    };
    let proxy = |host: &str, krate: &str| Place {
        namespace: host.to_owned(),
        node: Node::Proxy {
            krate: krate.to_owned(),
            origin: namespace_of(krate).to_owned(),
        },
    };
    let links_to = |namespace: &str, krate: &str| {
        namespace != namespace_of(krate)
            && namespaces
                .get(namespace)
                .is_some_and(|n| n.link_crates.iter().any(|link| link == krate))
    };
    // The namespaces, in name order, that embed a crate from elsewhere.
    let hosts = |krate: &str| -> Vec<&str> {
        namespaces
            .iter()
            .filter(|(name, namespace)| {
                **name != namespace_of(krate)
                    && namespace
                        .embed_crates
                        .iter()
                        .any(|embedded| embedded == krate)
            })
            .map(|(name, _)| *name)
            .collect()
    };

    let mut links = BTreeSet::new();
    for (from, to) in &graph.edges {
        let home = namespace_of(from);
        let link = match to {
            Target::External(package) => {
                let defaults = workspace.deps.get(package).map(|dep| &dep.features);
                let features = workspace
                    .krate(from)
                    .map(|krate| krate.features_for(package, workspace))
                    .unwrap_or_default()
                    .into_iter()
                    .filter(|feature| !defaults.is_some_and(|d| d.contains(feature)))
                    .collect();
                let node = Node::External {
                    package: package.clone(),
                    features,
                };
                Link::solid(
                    real(from),
                    Place {
                        namespace: home.to_owned(),
                        node,
                    },
                )
            }
            Target::Crate(to) => {
                let there = namespace_of(to);
                if let Some(host) = hosts(from).into_iter().find(|host| *host == there) {
                    links.insert(Link::dashed(real(from), proxy(host, from)));
                    Link::dashed(proxy(host, from), real(to))
                } else if let Some(host) = hosts(from).into_iter().find(|host| links_to(host, to)) {
                    links.insert(Link::dashed(real(from), proxy(host, from)));
                    links.insert(Link::dashed(proxy(host, to), real(to)));
                    Link::dashed(proxy(host, from), proxy(host, to))
                } else if links_to(home, to) {
                    links.insert(Link::dashed(proxy(home, to), real(to)));
                    Link::dashed(real(from), proxy(home, to))
                } else if home != there {
                    Link::dashed(real(from), real(to))
                } else {
                    Link::solid(real(from), real(to))
                }
            }
        };
        links.insert(link);
    }

    let mut clusters: BTreeMap<String, BTreeSet<Node>> = BTreeMap::new();
    let places = graph.crates.iter().map(|krate| real(krate)).chain(
        links
            .iter()
            .flat_map(|link| [link.from.clone(), link.to.clone()]),
    );
    for place in places {
        clusters
            .entry(place.namespace)
            .or_default()
            .insert(place.node);
    }

    Diagram {
        clusters: clusters
            .into_iter()
            .map(|(namespace, nodes)| Cluster {
                namespace,
                nodes: nodes.into_iter().collect(),
            })
            .collect(),
        links,
    }
}

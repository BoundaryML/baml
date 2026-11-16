//! Rendering a [`Diagram`] as Mermaid flowchart text (spec 9.5, 9.6).
//!
//! The output is fully determined by the diagram: fixed layout, sorted lines,
//! ids derived from names and colours taken from fixed tables.

use std::{collections::BTreeMap, fmt::Write};

use crate::{
    diagram::{Cluster, Diagram, Node, Place},
    error::{Error, Result},
};

struct Colours {
    stroke: &'static str,
    fill: &'static str,
}

const NAMESPACE_PALETTE: [Colours; 12] = [
    Colours {
        stroke: "#1f77b4",
        fill: "#dbe9f6",
    },
    Colours {
        stroke: "#d95f02",
        fill: "#fde3cf",
    },
    Colours {
        stroke: "#2ca02c",
        fill: "#dcf0dc",
    },
    Colours {
        stroke: "#9467bd",
        fill: "#e9e0f3",
    },
    Colours {
        stroke: "#8c564b",
        fill: "#eadfdc",
    },
    Colours {
        stroke: "#c2185b",
        fill: "#f8d7e3",
    },
    Colours {
        stroke: "#00838f",
        fill: "#d3eef0",
    },
    Colours {
        stroke: "#7f7f00",
        fill: "#eeeecc",
    },
    Colours {
        stroke: "#37474f",
        fill: "#dde3e6",
    },
    Colours {
        stroke: "#b8860b",
        fill: "#f6ecc9",
    },
    Colours {
        stroke: "#283593",
        fill: "#d9dcf2",
    },
    Colours {
        stroke: "#e53935",
        fill: "#fbd9d8",
    },
];

const TAG_FILLS: [&str; 8] = [
    "#8dd3c7", "#ffffb3", "#bebada", "#fb8072", "#80b1d3", "#fdb462", "#b3de69", "#fccde5",
];

const UNTAGGED_FILL: &str = "#ffffff";

const EXTERNAL: Colours = Colours {
    stroke: "#606060",
    fill: "#d0d0d0",
};

pub(crate) fn render(diagram: &Diagram) -> Result<String> {
    let palette: BTreeMap<&str, &Colours> = diagram
        .clusters
        .iter()
        .enumerate()
        .map(|(index, cluster)| {
            let colours = &NAMESPACE_PALETTE[index % NAMESPACE_PALETTE.len()];
            (cluster.namespace.as_str(), colours)
        })
        .collect();
    let colours_of = |namespace: &str| palette.get(namespace).copied().unwrap_or(&EXTERNAL);

    // Id to the style that goes with it; also where duplicate ids surface.
    let mut cluster_styles = BTreeMap::new();
    let mut node_styles = BTreeMap::new();
    let mut owners: BTreeMap<String, String> = BTreeMap::new();
    let mut claim = |id: &str, owner: String| match owners.insert(id.to_owned(), owner.clone()) {
        Some(first) => Err(Error::DuplicateNodeId {
            id: id.to_owned(),
            first,
            second: owner,
        }),
        None => Ok(()),
    };

    let mut out = String::from("flowchart TB\n");
    for cluster in &diagram.clusters {
        let Cluster { namespace, nodes } = cluster;
        let id = cluster.id();
        claim(&id, format!("namespace {namespace}"))?;
        let home = colours_of(namespace);
        cluster_styles.insert(
            id.clone(),
            format!("fill:{},stroke:{}", home.fill, home.stroke),
        );

        let _ = writeln!(out, "  subgraph {id} [\"{}\"]", quoted(namespace));
        for node in nodes {
            let id = node.id(namespace);
            let label = node.label();
            claim(&id, format!("{label} in {namespace}"))?;
            let style = match node {
                Node::Crate { tag: None, .. } => {
                    format!("fill:{UNTAGGED_FILL},stroke:{}", home.stroke)
                }
                Node::Crate { tag: Some(tag), .. } => {
                    format!("fill:{},stroke:{}", tag_fill(tag), home.stroke)
                }
                Node::Proxy { origin, .. } => {
                    let origin = colours_of(origin);
                    format!(
                        "fill:{},stroke:{},stroke-dasharray:5 5",
                        origin.fill, origin.stroke
                    )
                }
                Node::External { .. } => {
                    format!("fill:{},stroke:{}", EXTERNAL.fill, EXTERNAL.stroke)
                }
            };
            node_styles.insert(id.clone(), style);
            let _ = writeln!(out, "    {id}[\"{}\"]", quoted(&label));
        }
        out.push_str("  end\n");
    }

    let mut links: Vec<(String, String, bool)> = diagram
        .links
        .iter()
        .map(|link| (link.from.id(), link.to.id(), link.dashed))
        .collect();
    links.sort();
    for (from, to, dashed) in links {
        let arrow = if dashed { "-.->" } else { "-->" };
        let _ = writeln!(out, "  {from} {arrow} {to}");
    }

    for (id, style) in cluster_styles.iter().chain(&node_styles) {
        let _ = writeln!(out, "  style {id} {style}");
    }
    Ok(out)
}

impl Cluster {
    fn id(&self) -> String {
        format!("ns_{}", sanitized(&self.namespace))
    }
}

impl Place {
    fn id(&self) -> String {
        self.node.id(&self.namespace)
    }
}

impl Node {
    /// The id of this node when drawn in the cluster of `namespace`.
    fn id(&self, namespace: &str) -> String {
        let namespace = sanitized(namespace);
        match self {
            Node::Crate { name, .. } => format!("c_{}", sanitized(name)),
            Node::Proxy { krate, .. } => format!("p_{namespace}__{}", sanitized(krate)),
            Node::External { package, features } if features.is_empty() => {
                format!("x_{namespace}__{}", sanitized(package))
            }
            Node::External { package, features } => {
                format!(
                    "x_{namespace}__{}__{}",
                    sanitized(package),
                    sanitized(&features.join("_"))
                )
            }
        }
    }
}

fn sanitized(text: &str) -> String {
    text.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

fn quoted(text: &str) -> String {
    text.replace('"', "#quot;")
}

fn tag_fill(tag: &str) -> &'static str {
    let hash = tag.bytes().fold(0u32, |hash, byte| {
        hash.wrapping_mul(31).wrapping_add(u32::from(byte))
    });
    TAG_FILLS[hash as usize % TAG_FILLS.len()]
}

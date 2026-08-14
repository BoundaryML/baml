//! Case-insensitive, Windows-safe routing for generated `.g.cs` files.

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    fmt::Write as _,
    path::PathBuf,
};

use crate::names::{
    BamlFqn, CSharpName, CSharpNameKind, stable_primary_hash, stable_secondary_hash,
};

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct FileRouteRequest {
    pub identity: BamlFqn,
    directory: Vec<CSharpName>,
    source_stem: CSharpName,
}

impl FileRouteRequest {
    pub fn new(
        identity: BamlFqn,
        directory: impl IntoIterator<Item = CSharpName>,
        source_stem: CSharpName,
    ) -> Result<Self, FileRouteError> {
        let directory = directory.into_iter().collect::<Vec<_>>();
        for segment in &directory {
            if segment.kind() != CSharpNameKind::NamespaceSegment {
                return Err(FileRouteError::WrongDirectoryNameKind(segment.kind()));
            }
        }
        if source_stem.kind() != CSharpNameKind::FileStem {
            return Err(FileRouteError::WrongStemNameKind(source_stem.kind()));
        }
        Ok(Self {
            identity,
            directory,
            source_stem,
        })
    }

    fn stable_identity(&self) -> String {
        format!(
            "{}|{}|{}",
            self.identity.stable_identity(),
            self.directory
                .iter()
                .map(CSharpName::logical)
                .collect::<Vec<_>>()
                .join("/"),
            self.source_stem.logical()
        )
    }

    pub(crate) fn allocated_names(&self) -> impl Iterator<Item = &CSharpName> {
        self.directory
            .iter()
            .chain(std::iter::once(&self.source_stem))
    }
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CSharpFileRoute(PathBuf);

impl CSharpFileRoute {
    #[must_use]
    pub fn relative_path(&self) -> &std::path::Path {
        &self.0
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FileRouteError {
    UnsafeDirectorySegment(String),
    WrongDirectoryNameKind(CSharpNameKind),
    WrongStemNameKind(CSharpNameKind),
    DuplicateRequest(Box<FileRouteRequest>),
}

impl fmt::Display for FileRouteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsafeDirectorySegment(segment) => {
                write!(f, "unsafe generated directory segment `{segment}`")
            }
            Self::WrongDirectoryNameKind(kind) => write!(
                f,
                "generated directory name must be a namespace segment, got {kind}"
            ),
            Self::WrongStemNameKind(kind) => {
                write!(
                    f,
                    "generated file stem must be a file-stem name, got {kind}"
                )
            }
            Self::DuplicateRequest(request) => {
                write!(f, "duplicate generated file route request: {request:?}")
            }
        }
    }
}

impl std::error::Error for FileRouteError {}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CSharpFileRoutes {
    routes: BTreeMap<FileRouteRequest, CSharpFileRoute>,
}

impl CSharpFileRoutes {
    pub fn allocate(
        requests: impl IntoIterator<Item = FileRouteRequest>,
    ) -> Result<Self, FileRouteError> {
        Self::allocate_with_primary_hasher(requests, stable_primary_hash)
    }

    #[doc(hidden)]
    pub fn allocate_with_primary_hasher(
        requests: impl IntoIterator<Item = FileRouteRequest>,
        primary_hasher: impl Fn(&[u8]) -> u64,
    ) -> Result<Self, FileRouteError> {
        Self::allocate_with_hashers(requests, primary_hasher, stable_secondary_hash)
    }

    #[doc(hidden)]
    pub fn allocate_with_hashers(
        requests: impl IntoIterator<Item = FileRouteRequest>,
        primary_hasher: impl Fn(&[u8]) -> u64,
        secondary_hasher: impl Fn(&[u8]) -> u64,
    ) -> Result<Self, FileRouteError> {
        let mut unique = BTreeSet::new();
        for request in requests {
            validate_directory(&request.directory)?;
            if !unique.insert(request.clone()) {
                return Err(FileRouteError::DuplicateRequest(Box::new(request)));
            }
        }

        let mut segment_spellings = BTreeMap::<(Vec<String>, String), BTreeSet<String>>::new();
        for request in &unique {
            let mut parent = Vec::new();
            for segment in &request.directory {
                let segment = safe_path_segment(segment.logical());
                let lowered = segment.to_lowercase();
                segment_spellings
                    .entry((parent.clone(), lowered.clone()))
                    .or_default()
                    .insert(segment);
                parent.push(lowered);
            }
        }

        let mut canonical_directories = BTreeMap::new();
        let mut by_directory = BTreeMap::<Vec<String>, Vec<FileRouteRequest>>::new();
        for request in unique {
            let key = request
                .directory
                .iter()
                .map(|segment| safe_path_segment(segment.logical()).to_lowercase())
                .collect::<Vec<_>>();
            let canonical = canonical_directory(&key, &segment_spellings);
            canonical_directories.insert(request.clone(), canonical);
            by_directory.entry(key).or_default().push(request);
        }

        let mut routes = BTreeMap::new();
        for requests in by_directory.values_mut() {
            requests.sort();
            let canonical_directory = canonical_directories
                .get(requests.first().expect("directory bucket is non-empty"))
                .expect("every request has a canonical directory")
                .clone();
            let mut by_stem = BTreeMap::<String, Vec<FileRouteRequest>>::new();
            for request in requests.drain(..) {
                let stem = safe_path_segment(request.source_stem.logical());
                by_stem
                    .entry(stem.to_lowercase())
                    .or_default()
                    .push(request);
            }

            let mut occupied = BTreeSet::new();
            let mut deferred = Vec::new();
            for requests in by_stem.values_mut() {
                requests.sort();
                let winner = requests.remove(0);
                let stem = safe_path_segment(winner.source_stem.logical());
                occupied.insert(stem.to_lowercase());
                insert_route(&mut routes, winner, &canonical_directory, &stem);
                for request in requests.drain(..) {
                    let base = safe_path_segment(request.source_stem.logical());
                    deferred.push((base, request));
                }
            }

            deferred.sort_by(|left, right| left.1.cmp(&right.1));
            for (base, request) in deferred {
                let identity = request.stable_identity();
                let primary = format!("{:016x}", primary_hasher(identity.as_bytes()));
                let secondary = format!("{:016x}", secondary_hasher(identity.as_bytes()));
                let stem = routed_collision_name(&base, &primary, &secondary, &identity, &occupied);
                occupied.insert(stem.to_lowercase());
                insert_route(&mut routes, request, &canonical_directory, &stem);
            }
        }
        Ok(Self { routes })
    }

    #[must_use]
    pub fn get(&self, request: &FileRouteRequest) -> Option<&CSharpFileRoute> {
        self.routes.get(request)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&FileRouteRequest, &CSharpFileRoute)> {
        self.routes.iter()
    }
}

fn canonical_directory(
    lowered: &[String],
    spellings: &BTreeMap<(Vec<String>, String), BTreeSet<String>>,
) -> Vec<String> {
    let mut parent = Vec::new();
    let mut result = Vec::with_capacity(lowered.len());
    for segment in lowered {
        let spelling = spellings
            .get(&(parent.clone(), segment.clone()))
            .and_then(|values| values.first())
            .expect("every lowered segment has an observed spelling");
        result.push(spelling.clone());
        parent.push(segment.clone());
    }
    result
}

fn insert_route(
    routes: &mut BTreeMap<FileRouteRequest, CSharpFileRoute>,
    request: FileRouteRequest,
    directory: &[String],
    stem: &str,
) {
    let mut path = directory.iter().collect::<PathBuf>();
    path.push(format!("{stem}.g.cs"));
    routes.insert(request, CSharpFileRoute(path));
}

fn routed_collision_name(
    base: &str,
    primary: &str,
    secondary: &str,
    identity: &str,
    occupied: &BTreeSet<String>,
) -> String {
    for width in [8, 12, 16] {
        let candidate = format!("{base}_{}", &primary[..width]);
        if !occupied.contains(&candidate.to_lowercase()) {
            return candidate;
        }
    }
    for width in [8, 12, 16] {
        let candidate = format!("{base}_{primary}_{}", &secondary[..width]);
        if !occupied.contains(&candidate.to_lowercase()) {
            return candidate;
        }
    }
    let encoded_identity = identity.bytes().fold(
        String::with_capacity(identity.len() * 2),
        |mut encoded, byte| {
            write!(&mut encoded, "{byte:02x}").expect("writing to String cannot fail");
            encoded
        },
    );
    let candidate = format!("{base}_{primary}_{secondary}_{encoded_identity}");
    assert!(!occupied.contains(&candidate.to_lowercase()));
    candidate
}

fn validate_directory(directory: &[CSharpName]) -> Result<(), FileRouteError> {
    for segment in directory {
        let segment = safe_path_segment(segment.logical());
        if !is_safe_portable_segment(&segment) {
            return Err(FileRouteError::UnsafeDirectorySegment(segment));
        }
    }
    Ok(())
}

fn safe_path_segment(segment: &str) -> String {
    if is_windows_device(segment) {
        format!("_{segment}")
    } else {
        segment.to_string()
    }
}

pub(crate) fn is_windows_device(value: &str) -> bool {
    let stem = value
        .split('.')
        .next()
        .unwrap_or(value)
        .trim_end_matches([' ', '.']);
    let upper = stem.to_ascii_uppercase();
    matches!(upper.as_str(), "CON" | "PRN" | "AUX" | "NUL" | "CLOCK$")
        || upper
            .strip_prefix("COM")
            .or_else(|| upper.strip_prefix("LPT"))
            .is_some_and(|suffix| {
                matches!(suffix, "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9")
            })
}

pub(crate) fn is_safe_portable_segment(segment: &str) -> bool {
    !segment.is_empty()
        && !matches!(segment, "." | "..")
        && !segment.ends_with([' ', '.'])
        && !segment.chars().any(|character| {
            character.is_control()
                || matches!(
                    character,
                    '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*'
                )
        })
        && !is_windows_device(segment)
}

#[cfg(test)]
mod tests {
    use baml_base::Name as BaseName;
    use baml_codegen_types::Name;

    use super::*;
    use crate::names::{
        BamlWireName, CSharpNameOrigin, CSharpNameRequest, CSharpNames, CSharpScope,
        CSharpVisibility,
    };

    fn allocated_name(source: &str, kind: CSharpNameKind) -> CSharpName {
        let symbol = Name::new(BaseName::new("user"), vec![], BaseName::new(source));
        let request = CSharpNameRequest::new(
            BamlFqn::symbol(&symbol),
            BamlWireName::Symbol(symbol),
            source,
            kind,
            CSharpVisibility::Internal,
            CSharpNameOrigin::Source,
            CSharpScope::Namespace {
                package: BaseName::new("user"),
                path: vec![],
            },
        );
        let names = CSharpNames::allocate([request.clone()]);
        names.get(&request).unwrap().clone()
    }

    fn request(source: &str, directory: &[&str]) -> FileRouteRequest {
        let symbol = Name::new(BaseName::new("user"), vec![], BaseName::new(source));
        FileRouteRequest::new(
            BamlFqn::symbol(&symbol),
            directory
                .iter()
                .map(|segment| allocated_name(segment, CSharpNameKind::NamespaceSegment)),
            allocated_name(source, CSharpNameKind::FileStem),
        )
        .unwrap()
    }

    #[test]
    fn routes_are_case_insensitive_and_windows_device_safe() {
        let foo = request("foo", &["Acme"]);
        let upper = request("FOO", &["Acme"]);
        let con = request("con", &["Acme"]);
        let aux = request("AUX", &["Acme"]);
        let routes = CSharpFileRoutes::allocate([upper, con.clone(), foo, aux.clone()]).unwrap();

        let paths = routes
            .iter()
            .map(|(_, route)| route.relative_path().to_string_lossy().to_lowercase())
            .collect::<BTreeSet<_>>();
        assert_eq!(paths.len(), 4);
        assert_eq!(
            routes.get(&con).unwrap().relative_path(),
            std::path::Path::new("Acme/_Con.g.cs")
        );
        assert_eq!(
            routes.get(&aux).unwrap().relative_path(),
            std::path::Path::new("Acme/_Aux.g.cs")
        );
    }

    #[test]
    fn routing_is_order_independent_and_extends_full_hash_collisions() {
        let requests = vec![
            request("foo_bar", &["Acme"]),
            request("fooBar", &["Acme"]),
            request("FooBar", &["Acme"]),
            request("foo-bar", &["Acme"]),
            request("foo bar", &["Acme"]),
            request("foo.bar", &["Acme"]),
            request("foo$bar", &["Acme"]),
            request("foo__bar", &["Acme"]),
        ];
        let expected =
            CSharpFileRoutes::allocate_with_hashers(requests.clone(), |_| 7, |_| 11).unwrap();
        for rank in 0..100 {
            let mut ordered = requests.clone();
            ordered.rotate_left(rank % requests.len());
            if rank % 2 == 0 {
                ordered.reverse();
            }
            assert_eq!(
                CSharpFileRoutes::allocate_with_hashers(ordered, |_| 7, |_| 11).unwrap(),
                expected
            );
        }
        assert_eq!(
            expected
                .iter()
                .map(|(_, route)| route.relative_path().to_path_buf())
                .collect::<BTreeSet<_>>()
                .len(),
            8
        );
        assert!(expected.iter().any(|(_, route)| {
            route
                .relative_path()
                .file_name()
                .unwrap()
                .to_string_lossy()
                .matches('_')
                .count()
                >= 3
        }));
    }

    #[test]
    fn case_insensitive_directory_buckets_use_one_canonical_spelling() {
        let lower = request("alpha", &["acme", "billing"]);
        let upper = request("beta", &["ACME", "BILLING"]);
        let sibling = request("gamma", &["AcMe", "Sales"]);
        let routes =
            CSharpFileRoutes::allocate([lower.clone(), upper.clone(), sibling.clone()]).unwrap();
        assert_eq!(
            routes.get(&lower).unwrap().relative_path().parent(),
            routes.get(&upper).unwrap().relative_path().parent()
        );
        let roots = [&lower, &upper, &sibling]
            .map(|request| {
                routes
                    .get(request)
                    .unwrap()
                    .relative_path()
                    .components()
                    .next()
                    .unwrap()
                    .as_os_str()
                    .to_owned()
            })
            .into_iter()
            .collect::<BTreeSet<_>>();
        assert_eq!(roots.len(), 1);
    }

    #[test]
    fn routes_require_typed_names_and_remap_windows_devices() {
        let symbol = Name::new(BaseName::new("user"), vec![], BaseName::new("value"));
        let identity = BamlFqn::symbol(&symbol);
        let stem = allocated_name("value", CSharpNameKind::FileStem);
        let wrong_directory = allocated_name("Acme", CSharpNameKind::FileStem);
        assert!(matches!(
            FileRouteRequest::new(identity.clone(), [wrong_directory], stem.clone()),
            Err(FileRouteError::WrongDirectoryNameKind(
                CSharpNameKind::FileStem
            ))
        ));

        let directory = allocated_name("Acme", CSharpNameKind::NamespaceSegment);
        let wrong_stem = allocated_name("value", CSharpNameKind::Function);
        assert!(matches!(
            FileRouteRequest::new(identity.clone(), [directory], wrong_stem),
            Err(FileRouteError::WrongStemNameKind(CSharpNameKind::Function))
        ));

        let device = FileRouteRequest::new(
            identity,
            [allocated_name("NUL", CSharpNameKind::NamespaceSegment)],
            stem,
        )
        .unwrap();
        let routes = CSharpFileRoutes::allocate([device.clone()]).unwrap();
        assert_eq!(
            routes.get(&device).unwrap().relative_path(),
            std::path::Path::new("_Nul/Value.g.cs")
        );
    }
}

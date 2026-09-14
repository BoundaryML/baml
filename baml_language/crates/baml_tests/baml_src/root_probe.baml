// True project-root definition, referenced from
// ns_fixtures/ns_namespaces_root_fallback via an explicit `root.` path. The
// corpus otherwise defines everything inside `ns_*` namespaces, so without
// this file "resolve a root-package definition from inside a namespace"
// would have no coverage at all.
class RootFallbackProbe {
    tag string
}

pub struct FunctionGo {
    package: Package,
    documentation: String,
    name: String,
    args: Vec<(String, TypeGo)>,
    return_type: TypeGo,
    stream_return_type: TypeGo,
}

impl FunctionGo {
    pub fn to_go_type(&self, pkg: &Package) -> String {
        format!("func({})", self.args.iter().map(|(name, ty)| format!("{} {}", name, ty.serialize_type(pkg))).collect::<Vec<_>>().join(", "))
    }
}

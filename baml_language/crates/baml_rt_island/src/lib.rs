//! Runtime-compilation island. This crate is deliberately separate from rt:
//! ordinary packs never depend on it and therefore never link an interpreter.
//!
//! Its public value boundary is owned JSON text. No interpreter handles or rt
//! addresses are accepted or returned. C2: every crossing copies its values.
use bex_project::{
    BexEngine, BexExternalValue, CallId, FsPath, FunctionCallContextBuilder, SysOps,
};
use std::{collections::HashMap, sync::Arc};

pub struct Package {
    engine: Arc<BexEngine>,
}
#[derive(Debug)]
pub enum Error {
    Compile(String),
    Evaluate(String),
    Boundary(String),
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Compile(e) => f.write_str(e),
            Self::Evaluate(e) => f.write_str(e),
            Self::Boundary(e) => f.write_str(e),
        }
    }
}
impl std::error::Error for Error {}

impl Package {
    /// Compile into a fresh interpreter heap. Initializers run only on this
    /// heap; the caller's managed objects cannot be referenced by construction.
    pub fn compile(files: HashMap<String, String>, sysops: SysOps) -> Result<Self, Error> {
        let files = files
            .into_iter()
            .map(|(name, text)| (FsPath::from_str(name), text))
            .collect();
        let prepared = bex_project::prepare(
            vfs::VfsPath::new(vfs::MemoryFS::new())
                .join("rt-island")
                .expect("virtual island root"),
            sysops,
            files,
        )
        .map_err(|e| Error::Compile(e.to_string()))?;
        Ok(Self {
            engine: prepared
                .build()
                .map_err(|e| Error::Compile(e.to_string()))?,
        })
    }
    /// Invoke an island adapter taking and returning JSON strings. The adapter
    /// uses the interpreter's normal typed decoder and encoder, including user
    /// from_json/ToJson implementations. It is compiled with the package.
    pub async fn call_json(
        &self,
        function: &str,
        arguments: serde_json::Value,
    ) -> Result<serde_json::Value, Error> {
        let context = FunctionCallContextBuilder::new(CallId::next()).build();
        let input = BexExternalValue::String(arguments.to_string().into());
        let result = self
            .engine
            .call_function(function, vec![input], context, true)
            .await
            .map_err(|e| Error::Evaluate(e.to_string()))?;
        let BexExternalValue::String(json) = result else {
            return Err(Error::Boundary(
                "island adapter must return JSON text".into(),
            ));
        };
        serde_json::from_str(json.as_str()).map_err(|e| Error::Boundary(e.to_string()))
    }
}

/// Persistent incremental Session.eval, entirely inside the island heap.
pub struct Session {
    package: Package,
}
impl Session {
    pub fn new(sysops: SysOps) -> Result<Self, Error> {
        let source = r#"
const SESSION=reflect.Session.new();
function Eval(source:string)->string {
    let value=SESSION.eval<unknown>(source);
    baml.json.stringify(value)
}
"#;
        Ok(Self {
            package: Package::compile(
                HashMap::from([("main.baml".into(), source.into())]),
                sysops,
            )?,
        })
    }
    pub async fn eval(&self, source: String) -> Result<serde_json::Value, Error> {
        let context = FunctionCallContextBuilder::new(CallId::next()).build();
        let result = self
            .package
            .engine
            .call_function(
                "Eval",
                vec![BexExternalValue::String(source.into())],
                context,
                true,
            )
            .await
            .map_err(|e| Error::Evaluate(e.to_string()))?;
        let BexExternalValue::String(json) = result else {
            return Err(Error::Boundary(
                "Session.eval adapter must return JSON text".into(),
            ));
        };
        serde_json::from_str(json.as_str()).map_err(|e| Error::Boundary(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sys_native::SysOpsExt;
    #[tokio::test]
    async fn mutation_and_return_values_do_not_share_caller_storage() {
        let source = r#"
function Change(json:string)->string {
    let values=baml.json.decode<int[]>(json);
    values[0]=99;
    values.push(3);
    baml.json.stringify(values)
}
"#;
        let island = Package::compile(
            HashMap::from([("main.baml".into(), source.into())]),
            SysOps::native(),
        )
        .unwrap();
        let original = serde_json::json!([1, 2]);
        let result = island.call_json("Change", original.clone()).await.unwrap();
        assert_eq!(original, serde_json::json!([1, 2]));
        assert_eq!(result, serde_json::json!([99, 2, 3]));
        drop(island);
        assert_eq!(result[0], 99);
    }
    #[tokio::test]
    async fn eval_keeps_state_only_in_its_own_session() {
        let session = Session::new(SysOps::native()).unwrap();
        assert_eq!(
            session
                .eval("let xs=[1]; xs.push(2);".into())
                .await
                .unwrap(),
            serde_json::Value::Null
        );
        assert_eq!(session.eval("xs.length()".into()).await.unwrap(), 2);
    }
}

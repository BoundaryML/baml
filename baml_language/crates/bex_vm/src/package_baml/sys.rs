use super::{BamlNamespaceSys, PackageBamlImpl};

impl BamlNamespaceSys for PackageBamlImpl {
    #[allow(clippy::cast_possible_truncation)]
    fn now_ms() -> i64 {
        web_time::SystemTime::now()
            .duration_since(web_time::UNIX_EPOCH)
            .expect("system time before UNIX epoch")
            .as_millis() as i64
    }
}

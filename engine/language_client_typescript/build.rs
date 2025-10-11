extern crate napi_build;

use std::{
    env,
    path::{Path, PathBuf},
};

fn main() {
    // These env vars are occasionally set on systems with particular
    // requirements for napi inputs and outputs. Most builds can ignore
    // this and will simply run the napi_build::setup() in the `false`
    // branch.
    if let (Ok(tmp_path), Ok(pkg_name), Ok(out_dir)) = (
        env::var("TYPE_DEF_TMP_PATH"),
        env::var("CARGO_PKG_NAME"),
        env::var("OUT_DIR"),
    ) {
        if Path::new(&tmp_path).parent().is_some() {
            let bridge_dir = PathBuf::from(out_dir).join("napi-type-def");
            let _ = std::fs::create_dir_all(&bridge_dir);
            let symlink_path = bridge_dir.join(pkg_name);

            // Remove any stale symlink/file from previous builds.
            let _ = std::fs::remove_file(&symlink_path);

            #[cfg(unix)]
            {
                let _ = std::os::unix::fs::symlink(&tmp_path, &symlink_path);
            }

            #[cfg(windows)]
            {
                let target = if Path::new(&tmp_path).is_dir() {
                    std::os::windows::fs::symlink_dir(&tmp_path, &symlink_path)
                } else {
                    std::os::windows::fs::symlink_file(&tmp_path, &symlink_path)
                };
                let _ = target;
            }

            // Point napi-derive at the directory that now contains the symlink so it writes
            // through to the legacy TYPE_DEF_TMP_PATH provided by the older CLI.
            println!(
                "cargo:rustc-env=NAPI_TYPE_DEF_TMP_FOLDER={}",
                bridge_dir.display()
            );
        }
    }

    napi_build::setup();
}

//! Where the game's assets actually live.
//!
//! Bevy does **not** resolve `assets/` against the working directory. It resolves it against the
//! crate's manifest directory when run through cargo, and against the executable's own directory
//! otherwise -- so an app launched from the repo root looked for
//! `native/coaching/target/debug/assets/` and found nothing. Worse, a missing glTF fails
//! *silently*: no character ever loaded and nothing said so.
//!
//! The assets are shared by two crates and several binaries, so they sit at the repo root and
//! this resolves that one place for all of them.

use std::path::PathBuf;

/// The repo's `assets/` directory, as an absolute path.
///
/// Resolved from this crate's manifest at compile time, since `native/cube-soccer` is always two
/// levels below the root. `TACTIC_LAB_ASSETS` overrides it, which is the hook a packaged build
/// would use -- there is no packaged build yet, and when there is, this is the one line it needs.
pub fn root() -> PathBuf {
    resolve(std::env::var("TACTIC_LAB_ASSETS").ok())
}

/// [`root`] with the override handed in rather than read from the environment.
///
/// Split out so the override can be tested without a test setting a process-wide variable that
/// every other test in the binary can see. It used to, and the comment claiming the two tests
/// were serialised was wrong -- the harness runs them in parallel, so whether the other test
/// caught the variable mid-flight came down to how long the rest of the suite took.
fn resolve(override_path: Option<String>) -> PathBuf {
    match override_path {
        Some(path) => PathBuf::from(path),
        None => PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets")),
    }
}

/// An [`AssetPlugin`](bevy::asset::AssetPlugin) pointed at that directory.
///
/// Every binary that renders needs this; `DefaultPlugins.set(asset_plugin())`.
pub fn asset_plugin() -> bevy::asset::AssetPlugin {
    bevy::asset::AssetPlugin {
        file_path: root().to_string_lossy().into_owned(),
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_asset_root_is_the_one_the_assets_are_actually_in() {
        let root = root();
        assert!(root.is_dir(), "{} is not a directory", root.display());
        // The two things that are loaded at runtime. Committed precisely so this holds on a
        // fresh clone with no GPU and nothing forged.
        for expected in ["characters/base.glb", "icons/powers/beam_blast.png"] {
            let path = root.join(expected);
            assert!(path.exists(), "{} is missing", path.display());
        }
    }

    #[test]
    fn an_override_wins_so_a_packaged_build_has_somewhere_to_point() {
        assert_eq!(
            resolve(Some("/somewhere/else".to_owned())),
            PathBuf::from("/somewhere/else")
        );
    }

    #[test]
    fn without_an_override_the_repos_own_assets_are_used() {
        assert_eq!(resolve(None), root(), "no override should resolve to the repo's assets");
    }
}

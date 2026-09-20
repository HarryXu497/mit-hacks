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
    root_from(std::env::var("TACTIC_LAB_ASSETS").ok())
}

/// Where the assets are, given an override or none.
///
/// Split from [`root`] so the override can be tested without writing to the process environment.
/// `cargo test` runs a crate's tests as threads in one process, so a test that sets
/// `TACTIC_LAB_ASSETS` and one that reads it race -- and did, failing or passing on interleaving.
fn root_from(override_path: Option<String>) -> PathBuf {
    if let Some(override_path) = override_path {
        return PathBuf::from(override_path);
    }
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets"))
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
            root_from(Some("/somewhere/else".to_owned())),
            PathBuf::from("/somewhere/else")
        );
    }

    #[test]
    fn with_no_override_it_falls_back_to_the_repository_assets() {
        assert!(root_from(None).is_dir(), "the committed assets must be findable");
    }
}

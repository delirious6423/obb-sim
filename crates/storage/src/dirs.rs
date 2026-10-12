//! Where the game keeps its saves by default.

use std::path::PathBuf;

/// The environment variable that names the save folder.
pub const SAVES_ENV: &str = "OBB_SIM_SAVES";
/// The variable's name before the game was renamed; still honoured.
pub const LEGACY_SAVES_ENV: &str = "BASEBALL_SIM_SAVES";

/// The default save folder: `$OBB_SIM_SAVES`, else `$BASEBALL_SIM_SAVES`,
/// else `~/.obb-sim/saves`. Saves made before the rename live in
/// `~/.baseball-sim/saves`; that folder is used while the new one does not
/// exist, so nobody loses a save. `None` when there is no home folder.
#[must_use]
pub fn default_saves_dir() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os(SAVES_ENV).or_else(|| std::env::var_os(LEGACY_SAVES_ENV)) {
        return Some(PathBuf::from(dir));
    }
    let home = PathBuf::from(std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"))?);
    Some(pick(
        home.join(".obb-sim").join("saves"),
        home.join(".baseball-sim").join("saves"),
    ))
}

fn pick(new: PathBuf, old: PathBuf) -> PathBuf {
    if !new.exists() && old.is_dir() {
        old
    } else {
        new
    }
}

#[cfg(test)]
mod tests {
    use super::pick;

    #[test]
    fn the_old_folder_is_used_until_the_new_one_exists() {
        let tmp = tempfile::tempdir().unwrap();
        let new = tmp.path().join("new");
        let old = tmp.path().join("old");
        assert_eq!(pick(new.clone(), old.clone()), new, "neither exists");
        std::fs::create_dir(&old).unwrap();
        assert_eq!(pick(new.clone(), old.clone()), old, "only the old one");
        std::fs::create_dir(&new).unwrap();
        assert_eq!(pick(new.clone(), old), new, "both");
    }
}

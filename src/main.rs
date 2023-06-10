use anyhow::{anyhow, Error};
use std::{
    env, fs,
    path::{Path, PathBuf},
};

fn get_buck2_dir() -> Result<PathBuf, Error> {
    let mut dir = match env::var("BUCKLE_HOME") {
        Ok(home) => Ok(PathBuf::from(home)),
        Err(_) => match env::consts::OS {
            "linux" => {
                if let Ok(base_dir) = env::var("XDG_CACHE_HOME") {
                    return Ok(PathBuf::from(base_dir));
                }

                if let Ok(base_dir) = env::var("HOME") {
                    let mut path = PathBuf::from(base_dir);
                    path.push(".cache");
                    return Ok(path);
                }

                return Err(anyhow!("neither $XDG_CACHE_HOME nor $HOME are defined. Either define them or specify a $BUCKLE_HOME"));
            }
            "macos" => {
                let mut base_dir = env::var("HOME")
                    .map(PathBuf::from)
                    .map_err(|_| anyhow!("%LocalAppData% is not defined"))?;
                base_dir.push("Library");
                base_dir.push("Caches");
                Ok(base_dir)
            }
            "windows" => Ok(env::var("LocalAppData")
                .map(PathBuf::from)
                .map_err(|_| anyhow!("%LocalAppData% is not defined"))?),
            os => Err(anyhow!(
                "'{os}' is currently an unsupported OS. Feel free to contribute a patch."
            )),
        },
    }?;
    dir.push("buckle");
    Ok(dir)
}

/// Use the most recent .buckconfig except if a .buckroot is found.
fn find_project_root() -> Result<Option<PathBuf>, Error> {
    Ok(None)
}

fn choose_buck2_version() -> Result<String, Error> {
    if let Ok(version) = env::var("USE_BUCK2_VERSION") {
        return Ok(version);
    }

    if let Some(mut root) = find_project_root()? {
        root.push(".buckversion");
        if root.exists() {
            return Ok(fs::read_to_string(root)?);
        }
    }

    Ok(String::from("latest"))
}

fn get_buck2_path() -> Result<PathBuf, Error> {
    let buck2_dir = get_buck2_dir()?;
    fs::create_dir_all(buck2_dir)?;

    let buck2_version = choose_buck2_version()?;

    todo!()
}

fn main() -> Result<(), Error> {
    let buck2_path = get_buck2_path()?;

    Ok(())
}

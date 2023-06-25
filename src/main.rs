use anyhow::{anyhow, Error};
use serde::{Deserialize, Serialize};
use std::{
    env,
    fs::{self, File},
    path::{Path, PathBuf},
    process::{Command, Stdio},
};
use std::{io::Write, time::SystemTime};
use url::Url;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

fn get_buckle_dir() -> Result<PathBuf, Error> {
    let mut dir = match env::var("BUCKLE_HOME") {
        Ok(home) => Ok(PathBuf::from(home)),
        Err(_) => match env::consts::OS {
            "linux" => {
                if let Ok(base_dir) = env::var("XDG_CACHE_HOME") {
                    Ok(PathBuf::from(base_dir))
                } else if let Ok(base_dir) = env::var("HOME") {
                    let mut path = PathBuf::from(base_dir);
                    path.push(".cache");
                    Ok(path)
                } else {
                    Err(anyhow!("neither $XDG_CACHE_HOME nor $HOME are defined. Either define them or specify a $BUCKLE_HOME"))
                }
            }
            "macos" => {
                let mut base_dir = env::var("HOME")
                    .map(PathBuf::from)
                    .map_err(|_| anyhow!("$HOME is not defined"))?;
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Asset {
    pub name: String,
    pub browser_download_url: Url,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Release {
    pub url: Url,
    pub html_url: Url,
    pub assets_url: Url,
    pub upload_url: String,
    pub tarball_url: Option<Url>,
    pub zipball_url: Option<Url>,
    pub id: usize,
    pub node_id: String,
    pub tag_name: String,
    pub target_commitish: String,
    pub name: Option<String>,
    pub body: Option<String>,
    pub draft: bool,
    pub prerelease: bool,
    pub created_at: Option<String>,
    pub published_at: Option<String>,
    pub author: serde_json::Value,
    pub assets: Vec<Asset>,
}

fn get_releases(path: &Path) -> Result<Vec<Release>, Error> {
    let mut releases_json_path = path.to_path_buf();
    releases_json_path.push("releases.json");

    if releases_json_path.exists() {
        use std::os::unix::fs::MetadataExt;
        let meta = fs::metadata(&releases_json_path)?;
        let last_modification_time = meta.mtime();
        let curr_time = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)?
            .as_secs() as i64;
        if (curr_time - last_modification_time).abs() < 60 * 60 {
            let buf = fs::read_to_string(releases_json_path)?;
            return Ok(serde_json::from_str(&buf)?);
        }
    }

    let client = reqwest::blocking::Client::builder()
        .user_agent("buckle")
        .build()?;
    let releases = client
        .get("http://api.github.com/repos/facebook/buck2/releases")
        .send()?;
    let text = releases.text_with_charset("utf-8")?;
    let mut file = File::create(releases_json_path)?;
    file.write_all(text.as_bytes())?;
    file.flush()?;
    Ok(serde_json::from_str(&text)?)
}

fn get_arch() -> Result<&'static str, Error> {
    Ok(match env::consts::ARCH {
        "x86_64" => match env::consts::OS {
            "linux" => "x86_64-unknown-linux-gnu",
            "darwin" | "macos" => "x86_64-apple-darwin",
            "windows" => "x86_64-pc-windows-msvc",
            unknown => return Err(anyhow!("Unsupported Arch/OS {}", unknown)),
        },
        "aarch64" => match env::consts::OS {
            "linux" => "aarch64-unknown-linux-gnu",
            "darwin" | "macos" => "aarch64-apple-darwin",
            unknown => return Err(anyhow!("Unsupported Arch/OS {}", unknown)),
        },
        _ => return Err(anyhow!("Unsupported Architecture")),
    })
}

fn download_http(version: String, output_dir: &Path) -> Result<PathBuf, Error> {
    let releases = get_releases(output_dir)?;
    let mut path = output_dir.to_path_buf();

    let mut artefact = None;
    let arch = get_arch()?;

    for release in releases {
        if release.name.as_ref() == Some(&version) {
            if release.tag_name == "latest" {
                path.push(release.target_commitish);
            }
            for asset in release.assets {
                let name = asset.name;
                let url = asset.browser_download_url;
                if name == format!("buck2-{}.zst", arch) {
                    artefact = Some((name, url));
                    break;
                }
            }
        }
    }

    let (_name, url) = if let Some((name, url)) = artefact {
        (name, url)
    } else {
        return Err(anyhow!(
            "Meta has changed their releasing method. Please update buckle."
        ));
    };

    path.push("buck2");
    if path.exists() {
        return Ok(path);
    }
    if let Some(prefix) = path.parent() {
        fs::create_dir_all(prefix)?;
    }

    let buck2_bin = File::create(&path)?;
    let resp = reqwest::blocking::get(url)?;
    zstd::stream::copy_decode(resp, buck2_bin).unwrap();
    let permissions = fs::Permissions::from_mode(0o755);
    fs::set_permissions(&path, permissions)?;

    Ok(path)
}

fn read_buck2_version() -> Result<String, Error> {
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
    let buck2_dir = get_buckle_dir()?.join("buck2");

    if !buck2_dir.exists() {
        fs::create_dir_all(&buck2_dir)?;
    }

    let buck2_version = read_buck2_version()?;

    // or any version, but that isn't supported yet
    // TODO probably a regex here.
    if buck2_version == "latest" {
        Ok(download_http(buck2_version, &buck2_dir)?)
    } else {
        // TODO fetch from git.
        todo!()
    }
}

fn main() -> Result<(), Error> {
    let buck2_path = dbg!(get_buck2_path()?);

    // Collect information indented for buck2 binary.
    let mut args = env::args_os();
    args.next(); // Skip buckle
    let envs = env::vars_os();

    // Pass all file descriptors through as well.
    Command::new(buck2_path)
        .args(args)
        .envs(envs)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .output()
        .expect("Failed to execute buck2.");

    Ok(())
}

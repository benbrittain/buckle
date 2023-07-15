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

const BASE_URL: &str = "https://github.com/facebook/buck2/releases/download/";
const BUCK_RELEASE_URL: &str = "https://github.com/facebook/buck2/tags";

fn get_buck2_dir() -> Result<PathBuf, Error> {
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
    pub assets: Vec<serde_json::Value>,
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
            "darwin" => "x86_64-apple-darwin",
            "windows" => "x86_64-pc-windows-msvc",
            _ => return Err(anyhow!("Unsupported Arch/OS")),
        },
        "aarch64" => match env::consts::OS {
            "linux" => "aarch64-unknown-linux-gnu",
            "darwin" => "aarch64-apple-darwin",
            _ => return Err(anyhow!("Unsupported Arch/OS")),
        },
        _ => return Err(anyhow!("Unsupported Architecture")),
    })
}

fn download_http(version: String, output_dir: &Path) -> Result<PathBuf, Error> {
    let releases = get_releases(output_dir)?;
    let mut buck2_path = output_dir.to_path_buf();

    let mut release_found = false;
    for release in releases {
        if release.tag_name == version {
            buck2_path.push(release.target_commitish);
            release_found = true;
        }
    }
    if !release_found {
        return Err(anyhow!("{version} was not available. Please check '{BUCK_RELEASE_URL}' for available releases."));
    }

    // Path to directory that caches buck
    let dir_path = buck2_path.clone();
    if dir_path.exists() {
        // Already downloaded
        return Ok(dir_path);
    }

    buck2_path.push("buck2");
    if let Some(prefix) = buck2_path.parent() {
        fs::create_dir_all(prefix)?;
    }

    // Fetch the buck2 archive, decode it, make it executable
    let buck2_bin = File::create(&buck2_path)?;
    let arch = get_arch()?;
    let resp = reqwest::blocking::get(format!("{BASE_URL}/{version}/buck2-{arch}.zst"))?;
    zstd::stream::copy_decode(resp, buck2_bin).unwrap();
    let permissions = fs::Permissions::from_mode(0o755);
    fs::set_permissions(&buck2_path, permissions)?;

    // Also fetch the prelude hash and store it
    let mut prelude_path = dir_path.clone();
    prelude_path.push("prelude_hash");
    let resp = reqwest::blocking::get(format!("{BASE_URL}/{version}/prelude_hash"))?;
    let mut prelude_hash = File::create(prelude_path)?;
    prelude_hash.write_all(&resp.bytes()?)?;

    Ok(dir_path)
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
    let buck2_dir = get_buck2_dir()?;
    if !buck2_dir.exists() {
        fs::create_dir_all(&buck2_dir)?;
    }

    let buck2_version = read_buck2_version()?;
    download_http(buck2_version, &buck2_dir)
}

fn main() -> Result<(), Error> {
    let mut buck2_path = get_buck2_path()?;

    #[cfg(debug_assertions)]
    dbg!(&buck2_path);

    buck2_path.push("buck2");
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

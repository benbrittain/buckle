use anyhow::{anyhow, Error};
use ini::Ini;
use once_cell::sync::OnceCell;
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use std::{
    env,
    fs::{self, File},
    path::{Path, PathBuf},
    process::{Command, Stdio},
};
use tempfile::NamedTempFile;
use url::Url;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
#[cfg(unix)]
use std::time::SystemTime;

const UPSTREAM_BASE_URL: &str = "https://github.com/facebook/buck2/releases/download";
const BUCK_RELEASE_URL: &str = "https://github.com/facebook/buck2/tags";

/// Find the furthest .buckconfig except if a .buckroot is found.
fn get_buck2_project_root() -> Option<&'static Path> {
    static INSTANCE: OnceCell<Option<PathBuf>> = OnceCell::new();
    let path = INSTANCE.get_or_init(|| {
        let path = env::current_dir().unwrap();
        let mut current_root = None;
        for ancestor in path.ancestors() {
            let mut br = ancestor.to_path_buf();
            br.push(".buckroot");
            if br.exists() {
                // A buckroot means you should not check any higher in the file tree.
                return Some(ancestor.to_path_buf());
            }

            let mut bc = ancestor.to_path_buf();
            bc.push(".buckconfig");
            if bc.exists() {
                // This is the highest buckconfig we know about
                current_root = Some(ancestor.to_path_buf());
            }
        }
        current_root
    });
    path.as_deref()
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

    // TODO support last last_modification_time for windows users
    #[cfg(unix)]
    if releases_json_path.exists() {
        use std::os::unix::fs::MetadataExt;
        let meta = fs::metadata(&releases_json_path)?;
        let last_modification_time = meta.mtime();
        let curr_time = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)?
            .as_secs() as i64;
        if (curr_time - last_modification_time).abs() < 4 * 60 * 60 {
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

    if releases.status().is_success() {
        let text = releases.text_with_charset("utf-8")?;
        let mut file = File::create(releases_json_path)?;
        file.write_all(text.as_bytes())?;
        file.flush()?;
        Ok(serde_json::from_str(&text)?)
    } else if releases_json_path.exists() {
        // maybe out of date, but not that bad
        let buf = fs::read_to_string(releases_json_path)?;
        Ok(serde_json::from_str(&buf)?)
    } else {
        Err(anyhow!("No releases.json"))
    }
}

fn get_arch() -> Result<&'static str, Error> {
    Ok(match env::consts::ARCH {
        "x86_64" => match env::consts::OS {
            "linux" => "x86_64-unknown-linux-musl",
            "darwin" | "macos" => "x86_64-apple-darwin",
            "windows" => "x86_64-pc-windows-msvc",
            unknown => return Err(anyhow!("Unsupported Arch/OS: x86_64/{unknown}")),
        },
        "aarch64" => match env::consts::OS {
            "linux" => "aarch64-unknown-linux-gnu",
            "darwin" | "macos" => "aarch64-apple-darwin",
            unknown => return Err(anyhow!("Unsupported Arch/OS: aarch64/{unknown}")),
        },
        arch => return Err(anyhow!("Unsupported Architecture: {arch}")),
    })
}

fn download_http(config: &BuckleConfig, output_dir: &Path) -> Result<PathBuf, Error> {
    let releases = get_releases(output_dir)?;
    let mut buck2_path = output_dir.to_path_buf();

    let version = &config.buck2_version;
    let mut release_found = false;
    for release in releases {
        if release.tag_name == *version {
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

    let base_url = &config.base_download_url;

    // Fetch the buck2 archive, decode it, make it executable
    let mut tmp_buck2_bin = NamedTempFile::new_in(dir_path.clone())?;
    let arch = get_arch()?;
    eprintln!("buckle: fetching buck2 {version}");
    let resp = reqwest::blocking::get(format!("{base_url}/{version}/buck2-{arch}.zst"))?;
    zstd::stream::copy_decode(resp, &tmp_buck2_bin)?;
    tmp_buck2_bin.flush()?;
    #[cfg(unix)]
    {
        let permissions = fs::Permissions::from_mode(0o755);
        fs::set_permissions(&tmp_buck2_bin, permissions)?;
    }
    fs::rename(tmp_buck2_bin.path(), &buck2_path)?;

    // Also fetch the prelude hash and store it
    let mut prelude_path = dir_path.clone();
    prelude_path.push("prelude_hash");
    let resp = reqwest::blocking::get(format!("{base_url}/{version}/prelude_hash"))?;
    let mut prelude_hash = File::create(prelude_path)?;
    prelude_hash.write_all(&resp.bytes()?)?;
    prelude_hash.flush()?;

    Ok(dir_path)
}

fn get_expected_prelude_hash(config: &BuckleConfig) -> &'static str {
    static INSTANCE: OnceCell<String> = OnceCell::new();
    let expected_hash = INSTANCE.get_or_init(|| {
        let mut prelude_hash_path = get_buck2_dir(config).unwrap();
        prelude_hash_path.push("prelude_hash");

        let mut prelude_hash = File::open(prelude_hash_path).unwrap();
        let mut buf = vec![];
        prelude_hash.read_to_end(&mut buf).unwrap();
        std::str::from_utf8(&buf)
            .unwrap()
            .to_string()
            .trim()
            .to_string()
    });
    expected_hash
}

fn get_buck2_dir(config: &BuckleConfig) -> Result<PathBuf, Error> {
    let buckle_dir = &config.buckle_dir;
    if !buckle_dir.exists() {
        fs::create_dir_all(buckle_dir)?;
    }

    download_http(config, buckle_dir)
}

// Warn if the prelude does not match expected
fn verify_prelude(config: &BuckleConfig, prelude_path: &str) -> Result<(), Error> {
    if let Some(project_root) = get_buck2_project_root() {
        let mut absolute_prelude_path = project_root.to_path_buf();
        absolute_prelude_path.push(prelude_path);
        // It's ok if it's not a git repo, but we don't have support
        // for checking other methods yet. Do not throw an error.
        if let Ok(repo) = git2::Repository::open_from_env() {
            // It makes no sense for buck2 to be invoked on a bare git repo.
            let git_workdir = repo
                .workdir()
                .ok_or(anyhow!("buck2 is not for bare git repos"))?;
            let git_relative_prelude_path = absolute_prelude_path
                .strip_prefix(git_workdir)
                .map_err(|_err| {
                    anyhow!(
                        "{}/.buckconfig indicates the prelude should be \
                        located at {} which is not within this git repo.",
                        project_root.display(),
                        absolute_prelude_path.display(),
                    )
                })?
                .to_str()
                .ok_or(anyhow!("Could not convert the prelude path to a string"))?;
            // If there is a prelude known
            if let Ok(prelude) = repo.find_submodule(git_relative_prelude_path) {
                // Don't check if there is no ID.
                if let Some(prelude_hash) = prelude.workdir_id() {
                    let prelude_hash = prelude_hash.to_string();
                    let expected_hash = get_expected_prelude_hash(config);
                    if prelude_hash != expected_hash {
                        mismatched_prelude_msg(&absolute_prelude_path, &prelude_hash, expected_hash)
                    }
                }
            }
        }
    }
    Ok(())
}

/// Notify user of prelude mismatch and suggest solution.
// TODO make this much better
fn mismatched_prelude_msg(absolute_prelude_path: &Path, prelude_hash: &str, expected_hash: &str) {
    eprintln!(
        "buckle: Git submodule for prelude ({prelude_hash}) is not the expected {expected_hash}."
    );
    let abs_path = absolute_prelude_path.display();
    eprintln!("buckle: cd {abs_path} && git fetch && git checkout {expected_hash}");
}

#[derive(Debug)]
struct BuckleConfig {
    buck2_version: String,
    base_download_url: String,
    check_prelude: bool,
    buckle_dir: PathBuf,
}

fn read_config() -> Result<BuckleConfig, Error> {
    #[derive(Default, Deserialize)]
    struct BuckleFileConfig {
        buck2_version: Option<String>,
        base_download_url: Option<String>,
        check_prelude: Option<bool>,
        cache_dir: Option<PathBuf>,
    }

    let file_config = (|| -> Result<BuckleFileConfig, Error> {
        for dir in std::env::current_dir()?.ancestors() {
            let config_file = dir.join(".buckleconfig.toml");
            if config_file.exists() {
                return Ok(config::Config::builder()
                    .add_source(config::File::from(config_file))
                    .build()?
                    .try_deserialize::<BuckleFileConfig>()?);
            }
        }
        Ok(BuckleFileConfig::default())
    })()?;

    let buck2_version = if let Ok(version) = env::var("USE_BUCK2_VERSION") {
        version
    } else if let Some(version) = file_config.buck2_version {
        version.clone()
    } else if let Some(root) = get_buck2_project_root() {
        let root: PathBuf = [root, Path::new(".buckversion")].iter().collect();
        if root.exists() {
            eprintln!("buckle: reading Buck2 version from deprecated {root:?}, please use a .buckleconfig.toml file instead");
            fs::read_to_string(root)?.trim().to_string()
        } else {
            String::from("latest")
        }
    } else {
        String::from("latest")
    };

    let base_download_url = if let Ok(url) = env::var("BUCKLE_DOWNLOAD_URL") {
        url
    } else if let Some(url) = file_config.base_download_url {
        url.clone()
    } else {
        UPSTREAM_BASE_URL.to_owned()
    };

    let check_prelude =
        if let Ok(check) = env::var("BUCKLE_PRELUDE_CHECK").map(|var| var.to_uppercase() != "NO") {
            check
        } else if let Some(check) = file_config.check_prelude {
            check
        } else {
            true
        };

    fn get_os_cache_dir() -> Result<PathBuf, Error> {
        match env::consts::OS {
            "linux" => {
                if let Ok(base_dir) = env::var("XDG_CACHE_HOME") {
                    Ok(PathBuf::from(base_dir))
                } else if let Ok(base_dir) = env::var("HOME") {
                    let mut path = PathBuf::from(base_dir);
                    path.push(".cache");
                    Ok(path)
                } else {
                    Err(anyhow!("neither $XDG_CACHE_HOME nor $HOME are defined. Either define them or specify a $BUCKLE_CACHE"))
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
        }
    }

    let cache_dir = if let Ok(cache_dir) = env::var("BUCKLE_CACHE") {
        PathBuf::from(cache_dir)
    } else if let Some(cache_dir) = file_config.cache_dir {
        cache_dir
    } else {
        get_os_cache_dir()?
    };
    let buckle_dir = cache_dir.join("buckle");

    Ok(BuckleConfig {
        buck2_version,
        base_download_url,
        check_prelude,
        buckle_dir,
    })
}

fn main() -> Result<(), Error> {
    let config = match read_config() {
        Ok(config) => config,
        Err(e) => return Err(anyhow!("Failed to read configuration: {e}")),
    };

    let buck2_path: PathBuf = [get_buck2_dir(&config)?, PathBuf::from("buck2")]
        .iter()
        .collect();
    if !buck2_path.exists() {
        return Err(anyhow!(
            "The buckle cache is corrupted. Suggested fix is to remove {}",
            config.buckle_dir.display()
        ));
    }

    // mode() is only available on unix systems
    #[cfg(unix)]
    if buck2_path.exists() {
        let metadata = buck2_path.metadata()?;
        let permissions = metadata.permissions();
        let is_exec = metadata.is_file() && permissions.mode() & 0o111 != 0;
        if !is_exec {
            return Err(anyhow!(
                "The buckle cache is corrupted. Suggested fix is to remove {}",
                config.buckle_dir.display()
            ));
        }
    }

    if config.check_prelude {
        // If we can't find the project root, just skip checking the prelude and call the buck2 binary
        if let Some(root) = get_buck2_project_root() {
            // If we fail to parse the ini file, don't throw an error. We can't parse it for
            // some reason, so we should fall back on buck2 to throw a better error.
            let buck2config: PathBuf = [root, Path::new(".buckconfig")].iter().collect();
            if let Ok(ini) = Ini::load_from_file(buck2config) {
                if let Some(repos) = ini.section(Some("repositories")) {
                    if let Some(prelude_path) = repos.get("prelude") {
                        verify_prelude(&config, prelude_path)?;
                    }
                }
            }
        }
    }

    // Collect information indented for buck2 binary.
    let mut args = env::args_os();
    args.next(); // Skip buckle
    let envs = env::vars_os();

    // Pass all file descriptors through as well.
    let status = Command::new(&buck2_path)
        .args(args)
        .envs(envs)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .output()
        .unwrap_or_else(|_| panic!("Failed to execute {}", &buck2_path.display()))
        .status;

    if !status.success() {
        std::process::exit(status.code().unwrap_or(1));
    }

    Ok(())
}

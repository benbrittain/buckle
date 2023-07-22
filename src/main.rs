use anyhow::{anyhow, bail, Context, Error};
use ini::Ini;
use once_cell::sync::OnceCell;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::{
    env,
    fs::{self, File},
    ffi::{OsStr, OsString},
    io::{self, Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
};
use tempfile::NamedTempFile;
use url::Url;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
#[cfg(unix)]
use std::time::SystemTime;

mod config;

use config::{ArchiveConfig, BuckleConfig, BuckleSource, GithubRelease, PackageType};

const BUCKLE_BINARY: &str = "BUCKLE_BINARY";
const BUCKLE_CONFIG: &str = "BUCKLE_CONFIG";
const BUCKLE_CONFIG_FILE: &str = "BUCKLE_CONFIG_FILE";
const BUCKLE_SCRIPT: &str = "BUCKLE_SCRIPT";
const BUCKLE_HOME: &str = "BUCKLE_HOME";
const BUCKLE_REPO_CONFIG: &str = ".buckleconfig.toml";

fn get_buckle_dir() -> Result<PathBuf, Error> {
    let mut dir = match env::var("BUCKLE_CACHE") {
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
        },
    }?;
    dir.push("buckle");
    Ok(dir)
}

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

fn get_releases(gh_release: &GithubRelease, path: &Path) -> Result<Vec<Release>, Error> {
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
        if (curr_time - last_modification_time).abs() < 60 * 60 {
            let buf = fs::read_to_string(releases_json_path)?;
            return Ok(serde_json::from_str(&buf)?);
        }
    }

    let client = reqwest::blocking::Client::builder()
        .user_agent("buckle")
        .build()?;
    let releases = client
        .get(format!(
            "http://api.github.com/repos/{}/{}/releases",
            gh_release.owner, gh_release.repo
        ))
        .send()?;
    let text = releases.text_with_charset("utf-8")?;
    let mut file = File::create(releases_json_path)?;
    file.write_all(text.as_bytes())?;
    file.flush()?;
    Ok(serde_json::from_str(&text)?)
}

// Approximate the target triple for the current platform
// as per https://rust-lang.github.io/rfcs/0131-target-specification.html
fn get_target() -> Result<&'static str, Error> {
    Ok(match env::consts::ARCH {
        "x86_64" => match env::consts::OS {
            "linux" => "x86_64-unknown-linux-gnu",
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

fn get_triple_os() -> &'static str {
    match env::consts::OS {
        "linux" => "linux",
        "macos" => "darwin",
        "windows" => "windows",
        os => panic!("Unsupported OS {}", os),
    }
}

fn download_from_github(
    artifact_pattern: &str,
    gh_release: &GithubRelease,
    output_dir: &Path,
) -> Result<(PathBuf, reqwest::blocking::Response), Error> {
    let releases = get_releases(gh_release, output_dir)?;

    let mut dir_path = output_dir.to_path_buf();
    let mut release_name = None;

    let mut unpackable = None;
    let mut verbatim = vec![];
    // simple templating, we need this even with regex support
    let mut artifact_name = artifact_pattern.to_string();
    artifact_name = artifact_name.replace("%arch%", env::consts::ARCH);
    artifact_name = artifact_name.replace("%os%", get_triple_os());
    artifact_name = artifact_name.replace("%target%", get_target()?);

    let version_re = Regex::new(&gh_release.version)?;

    for release in releases {
        if release
            .name
            .as_ref()
            .map_or(false, |s| version_re.is_match(s))
        {
            if release.tag_name == version {
                dir_path.push(release.target_commitish);
            } else {
                // TODO use sha256 checksum if present
                path.push(
                    release
                        .name
                        .as_ref()
                        .ok_or_else(|| anyhow!("Release has no name"))?,
                );
            }
            let artifact_name = artifact_name.replace(
                "%version%",
                release.name.as_ref().unwrap_or(&gh_release.version),
            );
            let artifact_re = Regex::new(&artifact_name)?;
            for asset in release.assets {
                let name = asset.name;
                let url = asset.browser_download_url;
                if artifact_re.is_match(&name) {
                    unpackable = Some((name, url));
                    release_name = release.name;
                } else if name == "prelude" {
                    verbatim.push((name, url));
                }
            }
        };
    }

    let (name, url) = if let Some(unpackable) = unpackable {
        unpackable
    } else {
        bail!(
            "Could not find {} in {}. Its possible {} has changed their releasing method for {}. Please update buckle.",
            artifact_name,
            release_name.as_ref().unwrap_or(&gh_release.version),
            gh_release.owner,
            gh_release.repo
        )
    };

    if !path.exists() {
        fs::create_dir_all(&dir_path).with_context(|| anyhow!("error creating {:?}", path))?;
    }

    Ok((dir_path, reqwest::blocking::get(url)?))
}

fn extract<R>(
    unpacked_name: &str,
    package_type: &PackageType,
    mut archive_stream: R,
    dir_path: PathBuf,
) -> Result<PathBuf, Error>
where
    R: Read,
{
    for (name, url) in verbatim {
        // Fetch the verbatim items hash and store, do this before the binary so we don't see a partial hash
        // We do this as the complete executable is atomic via tmp_file rename
        let verbatim_path: PathBuf = [&dir_path, Path::new(&name)].iter().collect();
        let resp = reqwest::blocking::get(url)?;
        let mut verbatim_file = File::create(&verbatim_path)
            .with_context(|| anyhow!("problem creating {:?}", verbatim_path))?;
        verbatim_file.write_all(&resp.bytes()?)?;
        verbatim_file.flush()?;
    }

    let mut tmp_file = NamedTempFile::new_in(&dir_path)?;
    match package_type {
        PackageType::SingleFile => {
            io::copy(&mut archive_stream, &mut tmp_file)
                .with_context(|| anyhow!("problem copying to tmp_file"))?;
        }
        PackageType::ZstdSingleFile => zstd::stream::copy_decode(archive_stream, &tmp_file)?,
    }

    tmp_file.flush()?;
    #[cfg(unix)]
    {
        let permissions = fs::Permissions::from_mode(0o755);
        fs::set_permissions(&tmp_file, permissions)
            .with_context(|| anyhow!("problem setting permissions on {:?}", tmp_file))?;
    }
    // only move to final binary_path once fully written and stable
    fs::rename(tmp_file.path(), &binary_path)
        .with_context(|| anyhow!("problem renaming {:?} to {:?}", tmp_file, binary_path))?;

    Ok(dir_path)
}

fn get_expected_prelude_hash() -> &'static str {
    static INSTANCE: OnceCell<String> = OnceCell::new();
    let expected_hash = INSTANCE.get_or_init(|| {
        let mut prelude_hash_path = get_buck2_dir().unwrap();
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

fn download(
    binary_name: &str,
    archive_config: &ArchiveConfig,
    output_dir: &Path,
) -> Result<PathBuf, Error> {
    let (dir_path, stream) = match &archive_config.source {
        BuckleSource::Github(ref gh_release) => {
            download_from_github(&archive_config.artifact_pattern, gh_release, output_dir)?
        }
    };
    extract(binary_name, &archive_config.package_type, stream, dir_path)
}

fn get_binary_path(config: BuckleConfig, binary_name: Option<&String>) -> Result<PathBuf, Error> {
    let binary_name = if let Some(binary_name) = binary_name {
        binary_name
    } else if config.binaries.len() == 1 {
        // only one so default to it
        config.binaries.keys().next().unwrap()
    } else {
        bail!("No binary name provided");
    };

    let bin_config = config
        .binaries
        .get(binary_name)
        .ok_or_else(|| anyhow!("No binary named {} in buckle config", binary_name))?;
    let archive_name = &bin_config.provided_by;
    let archive_config = config
        .archives
        .get(archive_name)
        .ok_or_else(|| anyhow!("No archive named {} in buckle config", archive_name))?;

    let archive_dir = get_buckle_dir()?.join(archive_name);

    if !archive_dir.exists() {
        fs::create_dir_all(&archive_dir)
            .with_context(|| anyhow!("error creating {:?}", archive_dir))?;
    }

    download(binary_name, archive_config, &archive_dir).map_err(Error::into)
}

fn read_buck2_version() -> Result<String, Error> {
    if let Ok(version) = env::var("USE_BUCK2_VERSION") {
        return Ok(version);
    }

    if let Some(root) = get_buck2_project_root() {
        let root: PathBuf = [root, Path::new(".buckversion")].iter().collect();
        if root.exists() {
            return Ok(fs::read_to_string(root)?.trim().to_string());
        }
    }

    Ok(String::from("latest"))
}

fn get_buck2_dir() -> Result<PathBuf, Error> {
    let buckle_dir = get_buckle_dir()?.join("buck2");
    if !buckle_dir.exists() {
        fs::create_dir_all(&buckle_dir)?;
    }

    let buck2_version = read_buck2_version()?;
    download_http(buck2_version, &buckle_dir)
}

fn load_config(args: &mut env::ArgsOs) -> Result<BuckleConfig, Error> {
    if let Ok(config) = env::var(BUCKLE_CONFIG) {
        // Short circuit if the user has given us a config in the environment
        return Ok(toml::from_str(&config)?);
    }

    let config_file = if env::var(BUCKLE_SCRIPT).is_ok() {
        Some(PathBuf::from(args.next().unwrap().to_str().unwrap()))
    } else {
        match env::var(BUCKLE_CONFIG_FILE) {
            Ok(file) => Some(PathBuf::from(file)),
            // Then try repo level config
            Err(_) => {
                if let Some(mut root) = find_project_root()? {
                    root.push(BUCKLE_REPO_CONFIG);
                    if root.exists() {
                        Some(root)
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
        }
    };

    if let Some(config_file) = config_file {
        let config = &fs::read_to_string(&config_file).with_context(|| {
            format!(
                "Could not read config file {:?}",
                config_file.to_string_lossy()
            )
        })?;
        let config = toml::from_str(config)?;
        Ok(config)
    } else {
        //dbg!("No config file found, using builtin buck2 defaults");
        Ok(BuckleConfig::buck2_latest())
    }
}

// Warn if the prelude does not match expected
fn verify_prelude(prelude_path: &str) {
    if let Some(absolute_prelude_path) = get_buck2_project_root() {
        let mut absolute_prelude_path = absolute_prelude_path.to_path_buf();
        absolute_prelude_path.push(prelude_path);
        // It's ok if it's not a git repo, but we don't have support
        // for checking other methods yet. Do not throw an error.
        if let Ok(repo) = git2::Repository::open_from_env() {
            // It makes no sense for buck2 to be invoked on a bare git repo.
            let git_workdir = repo.workdir().expect("buck2 is not for bare git repos");
            let git_relative_prelude_path = absolute_prelude_path
                .strip_prefix(git_workdir)
                .expect("buck2 prelude is not in the same git repo")
                .to_str()
                .unwrap();
            // If there is a prelude known
            if let Ok(prelude) = repo.find_submodule(git_relative_prelude_path) {
                // Don't check if there is no ID.
                if let Some(prelude_hash) = prelude.workdir_id() {
                    let prelude_hash = prelude_hash.to_string();
                    let expected_hash = get_expected_prelude_hash();
                    if prelude_hash != expected_hash {
                        mismatched_prelude_msg(&absolute_prelude_path, &prelude_hash, expected_hash)
                    }
                }
            }
        }
    }
}

// What binary are we trying to run from the buckle config?
fn get_binary_name(invoked_as: Option<&OsString>) -> Option<String> {
    if let Ok(binary_name) = env::var(BUCKLE_BINARY) {
        Some(binary_name)
    } else if let Some(invoked_as) = invoked_as {
        let base_name = Path::new(&invoked_as).file_name();
        if base_name == Some(OsStr::new("buckle")) {
            None
        } else {
            base_name.and_then(|v| v.to_str()).map(|v| v.to_string())
        }
    } else {
        None
    }
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

fn main() -> Result<(), Error> {
    let binary_path: PathBuf = [get_buckle_dir()?, PathBuf::from("buck2")].iter().collect();
    if !binary_path.exists() {
        return Err(anyhow!(
            "The buckle cache is corrupted. Suggested fix is to remove {}",
            get_buckle_dir()?.display()
        ));
    }

    // mode() is only available on unix systems
    #[cfg(unix)]
    if binary_path.exists() {
        let metadata = binary_path.metadata()?;
        let permissions = metadata.permissions();
        let is_exec = metadata.is_file() && permissions.mode() & 0o111 != 0;
        if !is_exec {
            return Err(anyhow!(
                "The buckle cache is corrupted. Suggested fix is to remove {}",
                get_buckle_dir()?.display()
            ));
        }
    }

    if env::var("BUCKLE_PRELUDE_CHECK")
        .map(|var| var != "NO")
        .unwrap_or(true)
    {
        // If we can't find the project root, just skip checking the prelude and call the buck2 binary
        if let Some(root) = get_buck2_project_root() {
            // If we fail to parse the ini file, don't throw an error. We can't parse it for
            // some reason, so we should fall back on buck2 to throw a better error.
            let buck2config: PathBuf = [root, Path::new(".buckconfig")].iter().collect();
            if let Ok(ini) = Ini::load_from_file(buck2config) {
                if let Some(repos) = ini.section(Some("repositories")) {
                    if let Some(prelude_path) = repos.get("prelude") {
                        verify_prelude(prelude_path);
                    }
                }
            }
        }
    }
    // Collect information intended for invoked binary.

    // Collect information indented for buck2 binary.
    let mut args = env::args_os();
    // Figure out what binary we are trying to run.
    let invoked_as = args.next();
    let binary_name = get_binary_name(invoked_as.as_ref());
    let binary_name = binary_name;
    let buckle_config = load_config(&mut args)?;
    let binary_path = get_binary_path(buckle_config, binary_name.as_ref())?;

    if env::var(BUCKLE_SCRIPT).is_err() {
        eprintln!("buckle is running {:?}", binary_path)
    }

    // Remove so any recursive buckle calls need their own #! to be in script mode
    env::remove_var(BUCKLE_SCRIPT);
    let envs = env::vars_os();

    // Pass all file descriptors through as well.
    let status = Command::new(&binary_path)
        .args(args)
        .envs(envs)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .output()
        .unwrap_or_else(|_| panic!("Failed to execute {}", &binary_path.display()))
        .status;

    if !status.success() {
        std::process::exit(status.code().unwrap_or(1));
    }

    Ok(())
}

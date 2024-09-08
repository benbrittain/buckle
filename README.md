# Buckle

Buckle is a launcher for [Buck2](https://buck2.build/). It manages Buck2 on a per-project basis. This enables a project or team to do seamless upgrades of their build system tooling.

It is designed to be minimally intrusive. Buckle only manages fetching Buck2 and enforcing the prelude is upgraded in sync.

## Installation

There are multiple ways to install the `buckle` binary.

### Prebuilts

There are prebuilts available for Linux, Windows, and MacOS hosted on [GitHub](https://github.com/benbrittain/buckle/releases).

```
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/benbrittain/buckle/releases/download/v1.0.0/buckle-installer.sh | sh
```

### Building from source
```
cargo install buckle
```

## How To Use

### Invoke buck2
The most basic thing Buckle does is invoke the `buck2` binary. Use it as follows:

```bash
buckle build //...
```

By default, all of the above installation methods install the binary as `buckle`. You may also wish to add an alias to your shell:


```bash
alias buck2='buckle'
```

This will prevent you from accidently using the incorrect Buck2 version.

### Configuration

Buckle reads a `.buckleconfig.toml` at the root of your project. You can set the following options, which are all optional:

```toml
# `latest` or the release date in format YYYY-MM-DDD. See https://github.com/facebook/buck2/releases.
# Can be overridden by setting the `USE_BUCK2_VERSION` environment variable.
buck2_version = 2024-09-02

# Alternate download url. Given a `base_download_url`, `{base_download_url}/{version}/buck2-{arch}.zst` and `{base_download_url}/{version}/prelude_hash` should exist and serve the same contents as the upstream GitHub releases.
# Note that Buckle will still query GitHub to get a list of releases.
base_download_url = https://my.buck2.mirror/

# Whether or not Buckle should validate that the prelude hash matches the version of Buck2 that is specified.
# There are reasonable scenarios where someone actively working on the build system might be carrying a patch on the standard prelude.
# Can be overridden by setting the `BUCKLE_PRELUDE_CHECK` environment variable to `NO`.
check_prelude = false

# By default, Buckle stores the `buck2` binary in a different place dependent on the OS.
# Linux: `$XDG_CACHE_HOME/buckle` or `$HOME/.cache/buckle`
# MacOS: `$HOME/Library/Caches/buckle`
# Windows `%LocalAppData%/buckle`
# Can be overridden by setting the `BUCKLE_CACHE` environment variable.
buckle_dir = /my/cache/dir/
```

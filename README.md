# Buckle

Buckle is a launcher for buck2 and other binaries. It manages what version of a binary is used on a per-project basis. It picks a good version downloads it from the official releases, and then passes command line arguments through to the managed buck2 binary.

It is designed to be minimally intrusive. Buckle only manages fetching Buck2 and enforcing the prelude is upgraded in sync.

## Installation

There are multiple ways to install the `buckle` binary.

### Prebuilts

There are prebuilts available for Linux, Windows, and MacOS hosted on [GitHub](https://github.com/benbrittain/buckle/releases).

```
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/benbrittain/buckle/releases/download/v0.2.1/buckle-installer.sh | sh
```

### Building from source
```
cargo install buckle
```

## How To Use

In general you use buckle like you would the tool it is running for you.  Behind the scenes it downloads archives, from which it runs binaries.

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


### Specifying a Buck2 version
A `.buckversion` file is what allows you to pin your buck2 installation for all downstream users. Put it in the root of the Buck2 project.


`latest` or the release date in format YYYY-MM-DDD. [buck2 releases](https://github.com/facebook/buck2/releases)

Example `.buckversion`:
```
2023-07-15
```

`buckle` supports an environment variable that can override the `.buckversion` file.
```bash
USE_BUCK2_VERSION=latest buckle //...
```

### Prelude check
When upgraded, `buck2` will likely not be syncronized with the standard prelude anymore. Buckle will notify in this scenario what prelude is expected and how to upgrade.

There are reasonable scenarios where someone actively working on the build system might be carrying a patch on the standard `buck2` prelude. To disable the Buckle warnings of the mismatch:

```bash
export BUCKLE_PRELUDE_CHECK=NO
```
### Changing the installation directory
Buckle stores the `buck2` binary in a different place dependent on the OS.

Linux: `$XDG_CACHE_HOME/buckle` or `$HOME/.cache/buckle`

MacOS: `$HOME/Library/Caches/buckle`

Windows `%LocalAppData%/buckle`

you may also specify an override with the `BUCKLE_CACHE` environment variable.
```bash
export BUCKLE_CACHE=/tmp
``

### Calling tools from config

Here are some example invocations.

Exercise the default buck2 config: `buckle`

Test some scripts that download and run tools using the installed buckle. NB, you can remove the .toml extension if you install the scripts, its just there to show they are valid toml files:
```shell
    ../examples/buck2.toml
    ../examples/bazel7.toml
```

Test a config from build:
```shell
    BUCKLE_CONFIG_FILE=examples/bazel7.toml cargo run -- version
```

Test a config using the installed buckle:
```shell
    BUCKLE_CONFIG_FILE=examples/bazel7.toml buckle version
```

## Environment variables

`BUCKLE_CONFIG_FILE` points to a file to load config from

`BUCKLE_CONFIG` is an environment variable that can hold config. Mostly useful for testing.

`BUCKLE_SCRIPT` is used to tell buckle its being invoked as a script.  We use an env var for this as all command line arguments need to be passed to the underling tool.

`BUCKLE_BINARY` tells buckle which binary to run if there are multiple in the config

## Config syntax

`buckle` config is in tsoml, and allows you to specify which archives to download and cache and which binaries to run from those cached archives

The config file helps buckle find the archives to download and unpack, and from them which binaries to run

If there is no config, buckle with run buck2 with default config pointing to buck2 latest from github

For example config and how to use buckle as a #! interpreter see [examples](./examples/)

### Patterns for archives

When naming the archive you are looking for you can specify using a simple templating syntax.

`%version%`:  the artifact release name;

`%target%`: buckles view of the host's [rust target triple](https://rust-lang.github.io/rfcs/0131-target-specification.html);

`%arch%`: the architecture part of the triple

`%os%`: the os part of the triple


# buckle

Buckle is a launcher for buck2. It manages what version of buck2 is used on a per-project basis. It picks a good version of buck2, downloads it from the official releases, and then passes command line arguments through to the managed buck2 binary.

TODO:
1. Allow bootstrap from source (pinned)
2. Warn on prelude mismatch

## Installation

At this time, only installing through crates.io is supported.

Packaging for various distros and/or releases on GitHub are highly likely.

```
cargo install buckle
```

## .buckversion syntax

latest or date

TODO git sha

BUCKLE_PRELUDE_CHECK=NO

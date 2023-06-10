# buckle

Buckle is a launcher for buck2. It manages what version of buck2 is used on a per-project basis. It picks a good version of buck2, downloads it from the official releases or builds it from source, and then passes command line arguments through to the managed buck2 binary.

It also provides an option to manage updating the standard prelude to ensure buck2 rule compatibility.

## Installation

At this time, only installing through crates.io is supported.

Packaging for various distros and/or releases on GitHub are highly likely.

```
cargo install buckle
```



## What buck2 version does buckle use?

TODO



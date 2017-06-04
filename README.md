[![crates.io](https://img.shields.io/crates/v/stcat.svg)](https://crates.io/crates/stcat)
[![crates.io](https://img.shields.io/crates/d/stcat.svg)](https://crates.io/crates/stcat)

# `stcat`

> Tool to decode strings logged via the [`stlog`] framework

[`stlog`]: https://crates.io/crates/stlog

## Usage

``` console
$ cargo install stcat

$ cat /dev/ttyUSB0 | stcat -e /path/to/device/binary
INFO Hello, world!
WARN The quick brown fox jumps over the lazy dog.
```

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or
  http://www.apache.org/licenses/LICENSE-2.0)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.

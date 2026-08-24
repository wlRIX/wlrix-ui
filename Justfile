#!/usr/bin/env just --justfile
name := 'wlrix-ui'

# List available recipes.
default:
  @just --list

# `--all-features` throughout: `image` is off by default, so a plain build never compiles
# it and it would rot unnoticed.
release:
  cargo build --release --all-features

lint:
  cargo clippy --all-targets --all-features -- -D warnings

test:
  cargo test --all-features

# No `install` recipe, unlike every other wlRIX repo. This is a library: it ships inside the
# binaries that depend on it and puts no file of its own on disk.

clean:
  cargo clean

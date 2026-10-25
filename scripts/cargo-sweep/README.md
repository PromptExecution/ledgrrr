# cargo-sweep (containerised)

Prunes stale Cargo build artifacts so `target/` directories stop growing without bound. On one
developer machine a single `target/` reached 258 GB; `--days 14` would have reclaimed 216 GiB of it.

```sh
just sweep                 # dry run, 14 days  (== scripts/cargo-sweep/sweep.sh)
just sweep 30              # dry run, 30 days
just sweep 14 --apply      # delete
scripts/cargo-sweep/sweep.sh --maxsize 20GB    # trim oldest until each target/ is <= 20GB
```

* Runs [cargo-sweep](https://github.com/holmgr/cargo-sweep) 0.8.0 in a container (`podman` or `docker`),
  so nothing is installed on the host. The image builds once from `Containerfile` in this directory.
* **Dry run by default**, and the repo is mounted read-only unless `--apply` is given.
* Age comes from file access/modification times, so it also works on `target/` directories built by a
  different OS or toolchain (e.g. Windows-built output swept from WSL). `--installed` is intentionally
  not exposed: in the container it would compare against the container's Rust toolchain.
* Deleted artifacts are simply rebuilt by the next `cargo build`.

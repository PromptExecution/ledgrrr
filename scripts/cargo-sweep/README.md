# cargo-sweep (containerised)

Prunes stale Cargo build artifacts so `target/` directories stop growing without bound (Cargo never
garbage-collects old dependency artifacts on its own).

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
* cargo-sweep needs `cargo metadata` to succeed, so a workspace with an unresolved path dependency or an
  uninitialised git submodule is silently skipped by the tool itself. `sweep.sh` prints a note when nothing
  is reported; initialise submodules (`git submodule update --init`) and re-run.

# clean-my-mess

This is a small terminal UI tool that finds dev waste (`node_modules`, `.next`, `dist`, `build`, `target`, etc) and helps you to delete it. It allows navigating through the file system starting at a specified directory.

## Install

You'll need [rustup](https://rustup.rs). It will download the right compiler the first time you build the project.

To install:

```sh
git clone https://github.com/ferrata/clean-my-mess.git
cd clean-my-mess
cargo install --path .
```

To run it without installing:

```sh
cargo run --release
```

## Usage

```sh
clean-my-mess [DIR]
```

`DIR` defaults to the current directory. The tool scans everything below it in the background, so you can browse while it works. Each row is a directory that shows how much waste was found.

## Keys

| Key | Action |
|---|---|
| `↑` `↓` / `j` `k` | move cursor up and down |
| `→` / `l` / `Enter` | open directory |
| `←` / `h` / `Backspace` | go up, to the parent directory |
| `PgUp` `PgDn`, `g` `G` | jump |
| `space` | select or unselect, then move down |
| `a` / `n` | select all / clear selection |
| `d` (or `x`, `Delete`) | show the confirmation dialog for the selection, or the directory under the cursor if nothing is selected |
| `q` / `Esc` | quit |

Selected directories are remembered while you navigate, but only selection in the current screen is considered for the deletion.

## Delete confirmation dialog

`d` always opens a dialog first. Cancel is preselected, so pressing `Enter` right away does nothing.

### Trash

`t` moves selection into `~/.Trash`. It is instant, and space is only freed when you empty the Trash. macOS only, and it fails for folders on a different volume.

### Delete forever

`y` deletes the selection permanently. It cannot be undone, and it is slow on huge trees because it removes file by file.

### Cancel

`n`, `q`, or `Esc` closes the confirmation dialog.

### What happens if you click `Esc` or `s` during deletion

If while deleting, `Esc` or `s` is pressed, the deletion stops after a file being currently processed.

## Deleting and symlinks

Symlinks inside the scanned tree are never followed when scanning or deleting. A symlinked directory is skipped when scanning, and a symlink inside a waste folder is deleted as a link, so whatever it points to is left alone.

There are two exceptions:

1. The directory you start from is resolved once, so if you pass a symlink, the tool scans the real directory it points to.
2. A symlinked marker file (like `package.json`) still counts as present when deciding if a folder is waste.

## Drive panel

If a terminal is at least 130 columns wide, a panel on the right side shows:

- drive usage
- what your selection would free
- free space gained by deleting
  - the free space the OS reports (`statvfs`) after a permanent delete minus before it.
  - it can differ from the amount deleted, which comes from the sizes measured during the scan
- what you have deleted or moved to Trash this session

## What counts as waste

A directory matches by name, and some also need a marker file next to them so ordinary folders with the same name are left alone.

| Type | Directories | Needs next to it |
|---|---|---|
| Node | `node_modules` | |
| Next.js, Nuxt, SvelteKit | `.next`, `.nuxt`, `.svelte-kit` | `package.json` |
| Angular | `.angular` | `angular.json` |
| Turborepo, Parcel | `.turbo`, `.parcel-cache` | |
| JS build | `dist`, `build` | `package.json` |
| Coverage | `coverage` | `package.json` |
| Rust | `target` | `Cargo.toml` |
| Maven | `target` | `pom.xml` |
| Gradle | `build`, `.gradle` | a Gradle build or settings file |
| Python | `__pycache__`, `.pytest_cache`, `.mypy_cache`, `.ruff_cache`, `.tox` | `.tox` needs `tox.ini`, `pyproject.toml` or `setup.py` |
| Python venv | `.venv`, `venv` | `venv` needs `pyvenv.cfg` inside |
| CocoaPods | `Pods` | `Podfile` |
| Xcode | `DerivedData` | |
| Dart | `.dart_tool` | `pubspec.yaml` |
| Terraform | `.terraform` | |

The rules live in `src/scan.rs`. The `.git` directory is never considered.

## Platforms

It was developed and tested on macOS. Everything except Trash is portable Rust, but the thing has not run on Linux or Windows. Also, it uses `statvfs` for drive stats, so they are unix only.

## Development

```sh
cargo test
```

The tests render the UI into an in-memory terminal and run delete flows on throwaway directories under the system temp dir.

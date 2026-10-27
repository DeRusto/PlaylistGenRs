# PlaylistGenRs — Implementation Plan

Port of [DeRusto/PlaylistGen](https://github.com/DeRusto/PlaylistGen) (branch `claude/tv-playlist-interleaver-FcsHX`, file `playlist_gen.py`) from Python to Rust.

**Goal:** a single binary I can copy to an offline Linux machine and run. No runtime network access, no Python, no installer.

> Superseded: the Python app is no longer the spec. See [ADR 0001](docs/adr/0001-drop-python-parity.md).

**The Python source is the spec.** Clone it next to this repo as `reference/` (gitignored), and read `playlist_gen.py` and `test_playlist_gen.py` before writing code. Where this plan and the Python behavior disagree, the Python wins unless this plan explicitly lists a deliberate deviation.

---

## 0. Ground rules

- Crate/package name `playlistgenrs`, binary name `playlistgenrs`, project display name **PlaylistGenRs**.
- License: AGPL-3.0, copyright © 2026 Brandon Douglas Winterton. Copy the LICENSE file over.
- No crate that does network I/O at runtime. No telemetry, no update checks.
- Keep **all logic in the library** (`src/lib.rs` + modules). The frontends (CLI, text menu, GUI) are thin layers that call the library.
- Library functions return data or `Result` rather than printing. Warnings are collected into a `Vec<String>` (or sent through a callback) so each frontend decides where they go. The CLI prints them to stderr exactly as Python does.
- Every milestone ends with `cargo test`, `cargo clippy -- -D warnings`, and `cargo fmt --check` passing.

## 1. Project layout

```
playlistgenrs/
  Cargo.toml
  LICENSE
  README.md
  src/
    main.rs          # arg dispatch: no args → GUI, --text → menu, dirs → one-shot CLI
    lib.rs           # re-exports
    natsort.rs       # natural sort keys / comparators
    scan.rs          # scan_dirs, refresh_shows, collect_show_files, collect_show_seasons
    interleave.rs    # interleave, interleave_in_blocks, interleave_by_season
    build.rs         # build_playlist (slots, groups, ordering) + M3U writer
    layout.rs        # LayoutStore + State (JSON persistence)
    cli.rs           # clap definitions + one-shot run
    tui.rs           # interactive text menu
    gui/             # behind the `gui` feature
      mod.rs
      app.rs
      dialogs.rs
  tests/
    fixtures.rs      # helper to build temp media trees
    interleave.rs
    scan.rs
    build.rs
    layout.rs
    parity.rs        # optional golden tests against the Python impl (see §9)
```

## 2. Dependencies

| Crate | Purpose |
|---|---|
| `clap` (derive) | CLI parsing |
| `walkdir` | recursive scan (equivalent of `rglob`; don't follow symlinks, matching Python ≤3.12 `rglob`) |
| `serde`, `serde_json` with the `preserve_order` feature | layouts JSON; keeps layout insertion order like a Python dict |
| `dirs` | home dir fallback (check `XDG_CONFIG_HOME` manually first, like Python) |
| `rand` | shuffle for random show order |
| `pathdiff` | `os.path.relpath` equivalent |
| `tempfile` (dev) | test fixtures |
| `eframe` / `egui` / `egui_extras` (optional, `gui` feature) | GUI, table widget |
| `egui_file_dialog` (optional, `gui` feature) | pure-egui folder/save picker |

**Do not use `rfd`.** It needs GTK or an xdg-desktop-portal over D-Bus at runtime, which breaks the copy-and-run goal.

Write natural sort by hand rather than pulling in a crate, so it matches Python exactly (§3.1).

`Cargo.toml` features:
```toml
[features]
default = ["gui"]
gui = ["dep:eframe", "dep:egui_extras", "dep:egui_file_dialog"]

[profile.release]
lto = true
strip = true
codegen-units = 1
panic = "abort"
opt-level = 3
```

With `--no-default-features`, running with no args prints a message saying the GUI isn't compiled in and points to `--text`. Exit code 1.

## 3. Core behavior (exact port)

### 3.1 Natural sort (`natsort.rs`)

Python: `_natural_key(s)` lowercases, then splits on `(\d+)`, and digit runs become ints.

- Implement as `Vec<Token>`, where `enum Token { Str(String), Num(BigUint or u128 / digit-string) }`. Digit runs can be long, so compare numerically by stripping leading zeros, then comparing length, then comparing lexically. That avoids overflow.
- Comparisons between keys are lexicographic over the token list.
- **Deliberate deviation:** Python's `_path_sort_key` flattens all path components into one list. That can misalign str/int positions and would raise a `TypeError` in Python on mixed comparisons. In Rust, compare **component by component**: the relative path's components, each compared by natural key. For all normal layouts the ordering is identical. Document this in a comment.
- Show-name sort in `scan_dirs` is a plain case-insensitive sort (`s.lower()`), **not** a natural sort. Preserve that.
- Season-subdir name sort uses the natural key.

### 3.2 Video extensions

`.mkv .mp4 .avi .mov .wmv .flv .m4v .mpg .mpeg .ts .m2ts .webm .ogv`, matched case-insensitively on the extension.

### 3.3 Scanning (`scan.rs`)

`scan_dirs(dirs) -> Vec<(String /*label*/, Vec<PathBuf>)>`
- For each media dir, every immediate subdirectory is a show.
- Merge shows across media dirs **case-insensitively**, keeping the first-seen casing as the label.
- Sort labels by lowercase.
- An unreadable media dir produces the warning `Warning: cannot read '<dir>': <err>` and is skipped.
- Python uses `Path | list[Path]`. In Rust always use `Vec<PathBuf>`.

`collect_show_files(paths) -> Vec<PathBuf>`
- Recursively collect video files under every path.
- Sort by relative path to *its own* base dir, component-wise natural sort.
- Files from different base dirs are merged into one sorted list.

`collect_show_seasons(paths) -> Vec<Vec<PathBuf>>`
- Direct video files in the show dir(s) form the **first** season, if there are any. These are sorted by filename with a natural sort.
- Immediate subdirs are merged across paths case-insensitively (first-seen casing) and sorted by natural key of the name.
- Each subdir becomes one season, containing **all** video files recursively beneath it, sorted by relative path.
- Empty seasons are dropped.

`refresh_shows(dirs) -> Vec<ShowInfo { label, paths, n_seasons, n_episodes }>`
- Only include shows with at least one episode. Counts come from `collect_show_seasons`.

### 3.4 Interleaving (`interleave.rs`)

Make these generic over `T: Clone`, then port the Python unit tests directly.

- `interleave(lists, repeat)`
  - Drop empty lists first.
  - Without repeat: round-robin, skipping exhausted lists.
  - With repeat: cycle each list to `max_len`, then zip round-robin.
- `interleave_in_blocks(lists, block_size, repeat)`
  - Take a block of `block_size` from each list per round.
  - Without repeat: skip empty blocks.
  - With repeat: cycle each list to `max_len` first, then take blocks.
- `interleave_by_season(show_seasons, repeat)`
  - Drop shows with no seasons.
  - For `idx in 0..max_seasons`, for each show:
    - With repeat: `seasons[idx % len]`.
    - Without repeat: `seasons[idx]` if it exists.

### 3.5 Building the playlist (`build.rs`)

```rust
pub struct BuildOptions {
    pub relative: bool,
    pub repeat: bool,
    pub mode: Mode,            // None | Episodes | Seasons  (serialized "none"/"episodes"/"seasons")
    pub block_size: usize,     // default 10
    pub include: Option<HashSet<String>>,
    pub groups: Vec<Vec<String>>,
    pub show_order: ShowOrder, // Alpha | Random (serialized "alpha"/"random")
}
pub struct BuildReport { pub slots: Vec<SlotSummary>, pub entries: usize, pub warnings: Vec<String> }
pub fn build_playlist(dirs, output, opts, rng: &mut impl Rng) -> Result<BuildReport>
```

Follow Python's `build_playlist` step by step:
1. If there are no dirs: `No directories specified.` and return 0 entries.
2. Run `scan_dirs`, then filter by `include`. If nothing remains: `No show subdirectories found.`
3. If the order is random, shuffle the show entries.
4. Filter groups down to labels that are present. Drop groups that end up empty.
5. Map each label to the index of the **first** group containing it.
6. Walk the show entries in order. When you reach a group member, insert the whole group at that position (once).
   - If the order is random, shuffle the group's labels.
   - The group's slot name is `Group {idx+1} ({labels joined ", "})`.
   - The group's episodes are each member's `collect_show_files` concatenated in order. In seasons mode, concatenate each member's `collect_show_seasons` instead.
7. Skip empty slots with the warning `  Skipping '<name>' — no video files found` (two leading spaces, em dash).
8. If there are no slots: `No video files found.`
9. Report `Found N slot(s):` plus per-slot lines:
   - Flat modes: `  <name>: <n> episode(s)`
   - Seasons mode: `  <name>: <total> episode(s) in <k> season group(s)`
   - These go to **stdout** in the CLI.
10. Dispatch to the interleave function for the mode.
11. Write the M3U as UTF-8 with `\n` line endings:
    ```
    #EXTM3U
    #EXTINF:-1,<file stem>
    <path>
    ```
    - The path is absolute (normalized), or relative to `output.parent()` when `relative` is set.
    - Normalize lexically, like `os.path.normpath`; **do not** use `canonicalize`, which resolves symlinks.
    - A write failure gives `Error writing playlist: <err>` and 0 entries.

Take the RNG as a parameter so tests can use a seeded `StdRng`.

### 3.6 Layout persistence (`layout.rs`) — must stay compatible with the Python file

Path:
- `$XDG_CONFIG_HOME/playlist_gen/playlist_layouts.json` if that variable is set and non-empty,
- otherwise `~/.config/playlist_gen/playlist_layouts.json`.

Use the **same path as the Python app**, so existing layouts carry over.

File shape:
```json
{
  "layouts": {
    "<name>": {
      "dirs": ["..."],
      "selected": ["..."],
      "groups": [["...", "..."]],
      "output_path": "",
      "relative": false,
      "repeat": false,
      "show_order": "alpha",
      "interleave_mode": "none",
      "block_size": "10"
    }
  },
  "active_layout_name": "<name>" | null
}
```

- `block_size` is a **string** in the file. Read it leniently: accept a string or a number. Anything empty, non-digit, or ≤0 becomes 10. Always write it as a string.
- Every field is optional on read (`#[serde(default)]`), with the defaults above.
- Write with 2-space indent. Write to `playlist_layouts.tmp` first, then `rename` over the real file (atomic). Create parent dirs.
- A corrupt or non-object file produces `Warning: Failed to load layouts from <path>: <err>` and starts empty. **Never overwrite a corrupt file until the user saves.** (Python does overwrite on the next autosave; matching that is acceptable, but at minimum don't crash.)
- The reserved layout name is `"Last Session State"`, used by autosave when no layout is active.
- API, mirroring Python:
  - `load_all`, `save_all`, `get_layout`, `save_layout(name, state)` (sets active)
  - `autosave_session(state)`
  - `delete_layout(name)`: if it was active, the active layout becomes the first remaining one, or `None`
  - `get_active_state`
- Restoring state:
  - Filter `selected` and `groups` to labels that currently exist.
  - The text menu falls back from the active layout → `"Last Session State"` → the first layout.
  - The GUI falls back from the active layout → `"Last Session State"` only.
  - With no state at all, the text menu selects all shows.

Shared helper used by both frontends:
- `rescan(old_labels, new_show_info, selected, groups)`
- New `selected` = (selected ∩ new) ∪ (new − old), so newly discovered shows are auto-selected.
- Strip missing labels from groups and drop empty groups.

Group-editing helpers belong in the library too, so the text menu and GUI share them:
- `create_group(labels)`: removes those labels from other groups first, drops empty groups, and appends.
- `add_to_group(g_idx, labels)`
- `remove_from_groups(labels)`
- `dissolve_group(g_idx)`

Port the exact index-shift behavior from Python's text-menu `a` command and the GUI's `add_selected_to_group`. The two differ slightly; keep each frontend's behavior.

## 4. CLI (`cli.rs`)

Dispatch:
- `argv.len() == 1` → GUI.
- Otherwise parse with clap:
  - `--text` → text menu, ignoring any dirs given.
  - `media_dir...` (zero or more; error `the following arguments are required: media_dir` if missing and `--text` isn't set)
  - `-o/--output`
  - `--relative`
  - `--repeat`
  - `--interleave {none,episodes,seasons}`, default `none`
  - `--block-size N`, positive int, rejects ≤0

Behavior:
- Resolve dirs to absolute. A non-dir gives `Error: '<d>' is not a directory.` and exit 1.
- `--block-size` without `--interleave episodes` gives `Warning: --block-size has no effect when --interleave is '<mode>'.` on stderr.
- The output defaults to `<first dir>/interleaved.m3u`.
- On success: `\nPlaylist written to: <output>  (<n> entries)`.
- With 0 entries: exit 1.
- Copy the help text and epilog examples, with `playlist_gen.py` replaced by `playlistgenrs`.

Optional additions, only after parity is done and clearly marked as extras:
- `--seed N` for reproducible random order
- `--order alpha|random`
- `--layout NAME`, to run a saved layout headlessly from the CLI. This is actually useful on a server.

## 5. Text menu (`tui.rs`)

Plain stdin/stdout. **No ratatui.** Reproduce Python's menus line for line, including:
- the 54-char `=` and `─` rules
- the conditional commands (`[x]` only with dirs; `[w]`/`[G]` only with shows; `[g]` only with dirs and a selection)
- case-sensitive `G` vs `g`
- the mode submenu
- the selection screen (`[✓]` marks, number toggles, `a`/`n`/`b`)
- the groups screen (`g`, `a`, `r`, `d`, `b`)

Clear the screen with the ANSI `\x1b[2J\x1b[H` instead of shelling out to `clear`. Save state after every change, like Python does. On EOF (Ctrl-D), exit cleanly instead of panicking.

## 6. GUI (`gui/`, `gui` feature)

Use `eframe` with the default `glow` backend and a 950×700 window, minimum 850×600. Recreate the Tk layout:

- **Window title:** `TV Playlist Interleaver — [<layout name or "Unsaved Layout">]`
- **Menu bar → File:**
  - New Layout
  - Save Layout (falls back to Save As when no layout is active)
  - Save Layout As… (prefilled name; confirm before overwriting)
  - Load Layout… (combo box, preselecting the active layout)
  - Delete Layout… (with confirm)
  - Exit
- **Left column:**
  - "Media Source Directories": a list with single select, plus **Add Directory…** (folder picker; info dialog on duplicates) and **Remove** (warns if nothing is selected).
  - "Playlist Settings":
    - Interleave Mode combo (none/episodes/seasons)
    - Block Size field (disabled unless the mode is episodes)
    - Show Order combo (alpha/random)
    - "Write Relative Paths" and "Repeat Shorter Shows" checkboxes
    - Output Playlist path field with a `…` save-picker button (default extension `.m3u`)
- **Right column:**
  - "TV Shows Discovered": an `egui_extras` table with columns Use? (`[✓]`/`[ ]`), Show Name, Group (`Group N`, only shown when the show is used and grouped), Seasons, Episodes.
    - Multi-row selection: click, ctrl-click, shift-click.
    - Space or double-click toggles Use? on the selected rows.
  - "Group Management" buttons:
    - Create Group from Selected (needs ≥2 rows, all of them used)
    - Add Selected to Group… (modal with a group combo labeled `Group N (a, b, c...)`, showing at most 3 names, then `...`)
    - Remove Selected from Group
    - Dissolve Group…
- **Footer:** an italic status label on the left and a **Generate Playlist** button on the right.

Behavior:
- Generation runs on `std::thread::spawn`, and the result comes back over an `mpsc` channel that the UI polls each frame. Call `ctx.request_repaint()` while it runs.
- When it finishes, show a success or warning modal and update the status text.
- Validation errors are shown in modals with the same wording as Python ("Please add at least one media directory first.", "Block Size must be a positive integer.", and so on).
- Autosave is debounced by 500 ms after settings change. Save immediately on directory, selection, and group changes, and flush on close via `on_exit`. Suppress autosave while state is being applied.
- Use `egui::Modal` for dialogs, or a small in-house modal if the egui version lacks it.

Look: the default egui dark/light theme following the system is fine. Don't spend time on styling until parity is done.

## 7. Milestones (do them in order; each one is a commit)

1. **Scaffold.**
   - Done when: `cargo new`, the features, the release profile, LICENSE, and an empty module tree are in place, and `cargo build` passes with and without `--no-default-features`.
2. **Core: natsort + interleave.**
   - Done when: every interleave test from `test_playlist_gen.py` is ported, plus the README examples (a1 b1 a2…, block N=2, seasons, repeat variants).
3. **Core: scan.**
   - Done when: there are temp-dir fixtures covering flat shows, season subdirs, mixed layouts, ep9/ep10 ordering, Season 2/10 ordering, case-insensitive merges across two media dirs, uppercase extensions, non-video files ignored, and empty shows skipped.
4. **Core: build + M3U.**
   - Done when: tests cover groups (placement at the first member's position, naming, concatenation order), `include` filtering, the relative/absolute path output, the exact M3U bytes, a seeded random order being deterministic, and the warnings.
5. **Layouts.**
   - Done when: round-trip tests pass, a real Python-written JSON file loads (the fixture is committed in `tests/data/`), `block_size` as a string or a number both load, a corrupt file doesn't crash, and the atomic write works.
6. **CLI.**
   - Done when: `assert_cmd`-style integration tests (or `std::process::Command` against the built binary) cover each flag, the error messages, and the exit codes.
7. **Text menu.**
   - Done when: `run_menu` is a function generic over `BufRead`/`Write`, and a test drives it with scripted input (add dir → generate → quit).
8. **GUI.**
   - Done when: every control in §6 works, verified manually.
   - Unit tests cover any non-trivial GUI state logic by keeping it in plain structs separate from rendering.
9. **Release build + docs.**
   - Done when: §8 works and the README covers usage and the build.

## 8. Building the single binary

**GUI build (recommended).**

egui loads libGL and X11/Wayland libraries at runtime via dlopen, and that **cannot** work from a fully static musl binary. Build against an old glibc instead, so the binary runs on essentially any desktop Linux:

```bash
cargo install cargo-zigbuild   # needs zig installed
cargo zigbuild --release --target x86_64-unknown-linux-gnu.2.17
# → target/x86_64-unknown-linux-gnu/release/playlistgenrs
```

**Headless/server build (fully static, CLI + text menu only):**

```bash
rustup target add x86_64-unknown-linux-musl
cargo build --release --no-default-features --target x86_64-unknown-linux-musl
```

Add a `justfile` or `Makefile` with `build-gui`, `build-static`, and `test` targets. Verify each build with:
- `file` (static vs dynamic)
- `ldd` (the GUI build should only list libc/libm/libgcc_s/ld-linux-style system libs, nothing exotic)
- `objdump -T … | grep GLIBC_ | sort -u` (nothing newer than 2.17)

**Offline:** build on a connected machine and copy the binary over. To build *on* the offline machine, run `cargo vendor` while connected, commit `vendor/` along with the `.cargo/config.toml` source replacement it prints, and pin the toolchain with `rust-toolchain.toml`.

## 9. Parity testing against Python (strongly recommended)

Add `tests/parity.rs`, gated by an `#[ignore]` attribute or an env var so it only runs when `python3` and `reference/playlist_gen.py` are present.

- Build several fixture media trees in a tempdir (flat, seasons, mixed, two media dirs with overlapping show names in different case).
- For each combination of `{none, episodes(2), episodes(10), seasons} × {repeat on/off} × {relative on/off}`, run both implementations through the CLI and assert that the `.m3u` files are byte-identical. Random order is excluded.
- This is the main proof the port is correct. Don't skip it.

## 10. Out of scope

Don't change the playlist format, the sort semantics (apart from the §3.1 deviation), or the layout file format. Don't add network features, config besides the layout file, or new dependencies beyond §2 without a reason written in the commit message.

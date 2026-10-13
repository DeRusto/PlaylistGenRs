# PlaylistGenRs

A Rust rewrite of [PlaylistGen](https://github.com/DeRusto/PlaylistGen): scan directories of TV
shows and write one M3U playlist with the episodes interleaved across shows. It ships as a
single binary with no Python, no installer and no network access.

```
ShowA/  a1 a2 a3 a4 a5 a6 a7 a8 a9
ShowB/  b1 b2 b3 b4 b5 b6 b7

→  a1 b1 a2 b2 a3 b3 a4 b4 a5 b5 a6 b6 a7 b7 a8 a9
```

Playlists, saved layouts and terminal output match the Python app. A parity test checks that
both programs write byte-identical playlists (see [Testing](#testing)).

## Launch modes

| Command | What runs |
|---|---|
| `playlistgenrs` | Desktop GUI |
| `playlistgenrs --text` | Interactive text menu |
| `playlistgenrs <media_dir>... [options]` | One-shot command line run |

## Command line

```
playlistgenrs [OPTIONS] [media_dir]...
```

| Option | Description |
|---|---|
| `--text` | Launch the interactive text menu (any directories given are ignored) |
| `media_dir` | One or more directories containing TV show subdirectories |
| `-o`, `--output FILE` | Output playlist (default: `<first media_dir>/interleaved.m3u`) |
| `--relative` | Write paths relative to the playlist's folder instead of absolute paths |
| `--repeat` | Cycle shorter shows back to episode 1 instead of leaving gaps |
| `--interleave none\|episodes\|seasons` | How much of each show plays per turn (default `none`) |
| `--block-size N` | Episodes per show per turn with `--interleave episodes` (default 10) |

```bash
playlistgenrs /media/tv
playlistgenrs /media/tv1 /media/tv2 -o ~/combined.m3u
playlistgenrs /media/tv --interleave episodes --block-size 5
playlistgenrs /media/tv --interleave seasons
playlistgenrs /media/tv --relative --repeat
```

Exit code is 0 when a playlist was written, 1 when nothing was written (no shows, no video
files, a bad directory or a write error), and 2 for invalid arguments.

### Interleave modes

- **none** (default): one episode from each show per round.
  `a1 a2 a3 a4 a5` + `b1 b2 b3` → `a1 b1 a2 b2 a3 b3 a4 a5`
- **episodes**: N consecutive episodes per show per round. With N = 2:
  `a1 a2 b1 b2 a3 a4 b3 a5`
- **seasons**: one full season per show per round.
  `S1=[a01 a02] S2=[a03]` + `S1=[b01] S2=[b02 b03]` → `a01 a02 b01 a03 b02 b03`

With `--repeat`, shorter shows (or shows with fewer seasons) start over so every round is
full: `a1 a2 a3 a4` + `b1 b2` → `a1 b1 a2 b2 a3 b1 a4 b2`.

## Media layout

Every subdirectory of a media directory is a show. Shows may be flat or use season folders,
and the two can be mixed:

```
/media/tv/
  Seinfeld/
    Season 1/
      s01e01.mkv
    Season 2/
      s02e01.mkv
  Frasier/
    s01e01.mkv
    s01e02.mkv
```

- Episodes and seasons sort naturally (`ep9` before `ep10`, `Season 2` before `Season 10`).
- Shows with the same name (ignoring case) in different media directories are merged.
- In seasons mode, files directly in the show folder form the first season.
- Recognized extensions (any case): `.mkv .mp4 .avi .mov .wmv .flv .m4v .mpg .mpeg .ts .m2ts .webm .ogv`.
- Shows without video files are skipped with a warning.

## Text menu and GUI

Both let you add and remove media directories, choose which shows to use, set every option
above, and pick the show order (alphabetical or random). Both also support **groups**: shows
in a group share one slot in the rotation and play back to back. For example, grouping two
spin-offs makes them alternate with the other shows as if they were one show.

The GUI adds named layouts in the **File** menu (New, Save, Save As, Load, Delete).
Directories, selections, groups and settings save automatically.

Layouts live in `$XDG_CONFIG_HOME/playlist_gen/playlist_layouts.json`, or
`~/.config/playlist_gen/playlist_layouts.json` when that variable is unset. This is the same
file the Python app uses, so existing layouts carry over in both directions. A corrupt layout
file produces a warning and is left untouched until you explicitly save a layout.

## Building

Prebuilt binaries (the portable Linux GUI build, the static Linux CLI build below, and a
Windows `.exe`) are attached to each [GitHub release](https://github.com/DeRusto/PlaylistGenRs/releases).

To build yourself, you need Rust 1.88 or newer. Build on a connected machine and copy the binary to the offline one.

```bash
cargo build --release                       # GUI build for this machine
cargo build --release --no-default-features # CLI and text menu only
```

### Portable GUI binary

The GUI loads libGL and the X11/Wayland libraries at runtime, so it can't be fully static.
Building against glibc 2.17 instead produces a binary that runs on essentially any desktop
Linux:

```bash
pip install ziglang          # or install zig another way
cargo install cargo-zigbuild
make build-gui               # → target/x86_64-unknown-linux-gnu/release/playlistgenrs
make check-gui               # file / ldd / newest GLIBC symbol (expect GLIBC_2.17)
```

At runtime the target machine needs a desktop's usual graphics libraries: libGL, plus libX11
and libxkbcommon-x11 for X11 or libwayland and libxkbcommon for Wayland. On a machine
without them, use `--text` or the command line.

### Fully static CLI binary (servers)

```bash
rustup target add x86_64-unknown-linux-musl
make build-static            # → target/x86_64-unknown-linux-musl/release/playlistgenrs
make check-static
```

Running this build with no arguments prints a note that the GUI isn't included and exits
with code 1.

### Building on the offline machine

While connected, run `cargo vendor` and commit `vendor/` together with the
`.cargo/config.toml` source replacement it prints. Then pin the toolchain with a
`rust-toolchain.toml`.

## Testing

```bash
make test
```

`tests/parity.rs` compares the output with the Python implementation across 80 combinations
of media layout, mode, `--repeat` and `--relative`. It runs when `python3` is installed and
the Python source is at `reference/playlist_gen.py` (or at the path in `$PLAYLISTGEN_PY`);
otherwise it is skipped:

```bash
git clone -b claude/tv-playlist-interleaver-FcsHX https://github.com/DeRusto/PlaylistGen reference
cargo test --test parity
```

## Differences from the Python version

- When sorting episodes, paths are compared one folder at a time. Python flattens the whole
  path into one key, which can raise a `TypeError` on unusual names. For normal layouts the
  order is the same.
- Natural sort uses the `natord` crate: only ASCII digits count as numbers, and `ep01` and `ep1` are not treated as equal.
- The layout file is written as UTF-8 instead of `\u` escapes. Both programs read either form.
- Autosave doesn't overwrite a layout file that failed to load.
- The text menu shows build warnings in its own output instead of on stderr.

## License

PlaylistGenRs is free software under the [GNU Affero General Public License v3](LICENSE).
Copyright © 2026 Brandon Douglas Winterton.

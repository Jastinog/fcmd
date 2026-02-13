# fcmd

A fast dual-panel terminal file manager with Vim keybindings, built in Rust.

## Features

- **Dual-panel layout** with Tab to switch panels
- **Vim-style navigation** — `hjkl`, `gg`, `G`, `Ctrl-d/u`, `/` search
- **Visual mode** (`v`) — select ranges, yank, delete, paste
- **Tabs** — `:tabnew`, `:tabclose`, `gt`/`gT` to switch
- **File operations** — copy (`yy`), cut (`dd`), paste (`p`/`P`), rename (`:rename`), mkdir, touch, undo (`u`)
- **Telescope-style fuzzy find** — `Space ,` local, `Space .` global (macOS mdfind)
- **Tree view** — `Space t` to toggle sidebar tree
- **File preview** — `Space p` to toggle, `J`/`K` to scroll
- **28 built-in color themes** — `T` to cycle, `:theme <name>` to set
- **Git status indicators** — auto-detected, per-file status in panels
- **Directory size calculation** — `Space d` or `:du`
- **Marks** — `'a`-`'z` bookmarks, `m` for persistent visual marks
- **Glob select** — `:select *.rs`, `:unselect *.log`
- **Sort modes** — name, size, modified, created, extension (`sn`, `ss`, `sd`, `sc`, `se`)
- **Space leader key** with which-key style hints
- **Session persistence** — tabs, paths, theme saved in SQLite

## Install

```
cargo install --path .
```

Or build from source:

```
git clone https://github.com/Jastinog/fcmd.git
cd fc
cargo build --release
./target/release/fcmd
```

## Keybindings

### Navigation

| Key | Action |
|-----|--------|
| `j` / `k` | Move down / up |
| `h` / `l` | Go parent / enter directory |
| `gg` / `G` | Go to top / bottom |
| `Ctrl-d` / `Ctrl-u` | Half-page down / up |
| `Tab` | Switch panel |
| `~` | Go home |
| `/` | Incremental search |
| `n` / `N` | Next / previous search match |

### File Operations

| Key | Action |
|-----|--------|
| `yy` | Yank (copy) selected |
| `dd` | Delete selected (trash) |
| `p` / `P` | Paste / paste (overwrite) |
| `u` | Undo last operation |
| `yp` | Copy path to clipboard |
| `Shift+Up/Down` | Toggle mark and move |

### Modes

| Key | Action |
|-----|--------|
| `v` | Enter visual mode |
| `:` | Command mode |
| `Space ?` | Help |

### Space Leader

| Key | Action |
|-----|--------|
| `Space t` | Toggle tree view |
| `Space h` | Toggle hidden files |
| `Space p` | Toggle preview |
| `Space d` | Calculate directory sizes |
| `Space ,` | Find (local) |
| `Space .` | Find (global) |
| `Space s` | Sort popup |
| `Space a` | Select all |
| `Space n` | Unselect all |

### Commands

| Command | Action |
|---------|--------|
| `:mkdir <name>` | Create directory |
| `:touch <name>` | Create file |
| `:rename <name>` | Rename selected |
| `:cd <path>` | Change directory |
| `:theme <name>` | Set color theme |
| `:sort <mode>` | Set sort mode |
| `:select <glob>` | Select by pattern |
| `:q` | Quit |

## Themes

28 built-in themes including: ayu-dark, gruvbox-dark, catppuccin-mocha, tokyo-night, rose-pine, dracula, nord, kanagawa, everforest-dark, solarized-dark, one-dark, monokai-pro, palenight, and more.

Themes are TOML files stored in `~/.config/fc/themes/`. Custom themes are automatically discovered.

## License

[MIT](LICENSE)

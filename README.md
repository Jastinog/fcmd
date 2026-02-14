<p align="center">
  <h1 align="center">fcmd</h1>
  <p align="center">
    A fast dual-panel terminal file manager with Vim keybindings, built in Rust.
  </p>
</p>

<p align="center">
  <a href="#install">Install</a>&nbsp;&bull;
  <a href="#features">Features</a>&nbsp;&bull;
  <a href="#keybindings">Keybindings</a>&nbsp;&bull;
  <a href="#themes">Themes</a>&nbsp;&bull;
  <a href="LICENSE">License</a>
</p>

---

![Dual panel with tree view](assets/dual-panel.png)
![Find with preview](assets/find-preview.png)

## Features

- **Dual-panel layout** — `Tab` to switch, independent navigation per panel
- **Vim-style navigation** — `hjkl`, `gg`/`G`, `Ctrl-d`/`Ctrl-u`, `/` incremental search
- **Three selection modes**
  - **Visual** (`v`) — select contiguous ranges
  - **Select** (`Shift+Up/Down`) — toggle individual files
  - **Glob** — `:select *.rs`, `:unselect *.log`
- **File operations** — yank (`yy`), delete (`dd`), paste (`p`/`P`), rename (`r`), create (`a`), undo (`u`)
- **Telescope-style fuzzy find** — `f` local, `F` global (macOS `mdfind`)
- **Tree sidebar** — `Space t` to toggle, navigate with `j`/`k`
- **File preview** — `Space p` to toggle, `J`/`K` to scroll
- **Tabs** — `:tabnew`, `:tabclose`, `gt`/`gT` to switch
- **28 built-in color themes** — `T` to cycle, `:theme <name>` to set
- **Git status indicators** — auto-detected, per-file status in panels
- **Directory size calculation** — `Space d` or `:du`
- **Bookmarks** — `'a`–`'z` quick marks, `m` for persistent visual marks (3 levels)
- **Sort modes** — name, size, modified, created, extension (`sn`, `ss`, `sd`, `sc`, `se`)
- **Space leader key** — which-key popup with icons and accent colors
- **Session persistence** — tabs, paths, theme, visual marks saved in SQLite
- **Nerd Font icons** — distinct icons for every mode, overlay, and file type

## Install

```bash
cargo install --path .
```

Or build from source:

```bash
git clone https://github.com/Jastinog/fcmd.git
cd fcmd
cargo build --release
./target/release/fcmd
```

> **Requires** a [Nerd Font](https://www.nerdfonts.com/) for icons to render correctly.

## Keybindings

### Navigation

| Key | Action |
|-----|--------|
| `j` / `k` | Move down / up |
| `h` / `l` | Go parent / enter directory |
| `gg` / `G` | Top / bottom |
| `Ctrl-d` / `Ctrl-u` | Half-page down / up |
| `Tab` | Switch panel |
| `~` | Go home |
| `/` | Incremental search |
| `n` / `N` | Next / prev search match |

### File Operations

| Key | Action |
|-----|--------|
| `yy` | Yank (copy) |
| `dd` | Delete (with confirmation popup) |
| `p` / `P` | Paste / paste (overwrite) |
| `r` | Rename in-place |
| `a` | Create file or directory |
| `u` | Undo last operation |
| `yp` | Copy path to clipboard |

### Selection

| Key | Action |
|-----|--------|
| `v` | Enter Visual mode (range) |
| `Shift+Up/Down` | Enter Select mode (individual toggle) |
| `Space a` | Select all |
| `Space n` | Unselect all |
| `:select <glob>` | Select by pattern |
| `:unselect <glob>` | Unselect by pattern |

### Find

| Key | Action |
|-----|--------|
| `f` | Find in current directory |
| `F` | Find globally (macOS `mdfind`) |
| `Space ,` | Find local (alternative) |
| `Space .` | Find global (alternative) |

### Space Leader

| Key | Action |
|-----|--------|
| `Space t` | Toggle tree sidebar |
| `Space h` | Toggle hidden files |
| `Space p` | Toggle preview |
| `Space d` | Calculate directory sizes |
| `Space s` | Sort popup |
| `Space ?` | Help |

### Commands

| Command | Action |
|---------|--------|
| `:cd <path>` | Change directory |
| `:mkdir <name>` | Create directory |
| `:touch <name>` | Create file |
| `:rename <name>` | Rename selected |
| `:theme <name>` | Set color theme |
| `:sort <mode>` | Set sort mode |
| `:tabnew` | New tab |
| `:tabclose` | Close tab |
| `:q` | Quit |

## Themes

28 built-in themes including: **ayu-dark**, **gruvbox-dark**, **catppuccin-mocha**, **tokyo-night**, **rose-pine**, **dracula**, **nord**, **kanagawa**, **everforest-dark**, **solarized-dark**, **one-dark**, **monokai-pro**, **palenight**, and more.

Themes are TOML files in `~/.config/fcmd/themes/`. Custom themes are auto-discovered.

## License

[MIT](LICENSE)

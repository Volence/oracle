# oracle-frontend

The windowed player: `cargo run --release -p oracle-frontend -- <rom.bin> [--scale N] [--aspect tv|square|integer] [--aether] [--socket PATH] [--x11]`. The module doc at the top of `src/main.rs` is the full reference (controls, lenses, the hosted Aether bus, save states).

## Window icon and desktop entry

The window is titled `Oracle — frame N` and carries the ruled suite mark (`assets/oracle-icon.argb`, generated from `assets/oracle.svg` by `assets/gen-icon.sh`). How far that reaches depends on the windowing backend, because minifb 0.28 exposes very little:

| backend | in-process icon (`_NET_WM_ICON`) | class / app id | icon in taskbar + titlebar |
|---|---|---|---|
| X11 (`--x11`, or a plain X session) | set by `src/icon.rs` | `WM_CLASS=oracle-frontend` set by `src/icon.rs` | from the window itself, or the `.desktop` entry |
| Wayland (the default when `WAYLAND_DISPLAY` is set) | not possible — minifb's `set_icon` is `unimplemented!()` on Wayland | not possible — minifb never sends `set_app_id`; KWin falls back to the executable name `oracle-frontend` | only via the installed `.desktop` entry (`StartupWMClass=oracle-frontend`) |

So on a Wayland desktop, install the entry once:

```sh
crates/oracle-frontend/assets/install-desktop.sh            # uses target/release/oracle-frontend
crates/oracle-frontend/assets/install-desktop.sh /path/to/oracle-frontend
```

It copies the PNGs to `~/.local/share/icons/hicolor/<size>x<size>/apps/oracle.png`, the entry to `~/.local/share/applications/oracle.desktop` (with `Exec=` rewritten to the binary you named), and refreshes the desktop/icon caches where the tools exist. Relaunch the player afterwards. `--x11` is the alternative that needs no install: the window then runs under XWayland and stamps itself.

To regenerate the icon after empyrean re-rules `design/icons/oracle.svg`: copy the new file over `assets/oracle.svg` and run `assets/gen-icon.sh` (needs `rsvg-convert` and Python 3 with Pillow). The tint is the Oracle accent `#38BDF8` (`color.accent.oracle` in `empyrean/design/tokens.json`).

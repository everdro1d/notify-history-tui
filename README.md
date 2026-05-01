# notify-history-tui

A notification history viewer for Linux desktops — a terminal UI (`notify-history`) paired with a background daemon (`notify-history-ctl`) that captures every D-Bus notification and stores it in a history file.

## Overview

| Binary | Role |
|---|---|
| `notify-history-ctl` | Daemon — monitors `org.freedesktop.Notifications` on D-Bus and prepends each notification to a history file |
| `notify-history` | TUI — reads the history file and presents a paginated, filterable, interactive list |

History is stored in `/tmp/notification-history` by default (cleared on shutdown) or in `~/.local/state/notify-history/notification-history` when persistence is enabled.

---

## Installation

### Without Nix (manual)

Requires: Rust toolchain (stable), `dbus-monitor` (part of the `dbus` package).

```sh
git clone https://github.com/everdro1d/notify-history-tui
cd notify-history-tui
cargo build --release

# Copy binaries somewhere on your PATH
install -Dm755 target/release/notify-history     ~/.local/bin/
install -Dm755 target/release/notify-history-ctl ~/.local/bin/

# Start the daemon (add to your autostart / session startup)
notify-history-ctl &

# Open the TUI
notify-history
```

### Nix flake

```nix
# flake.nix
{
  inputs.notify-history-tui.url = "github:everdro1d/notify-history-tui";
}
```

Then use the exposed packages, NixOS module, or Home-Manager module (see below).

---

## Configuration

Create `~/.config/notify-history/config.toml` (all fields optional — defaults shown):

```toml
[display]
show_app   = true  # show app name next to title
body_lines = 4     # body lines per notification (0–4)

[persistence]
enabled = false    # true → store history in ~/.local/state/notify-history/

[colors]
foreground = "#cdd6f4"  # text
background = "#1e1e2e"  # window background
accent     = "#89b4fa"  # titles, markers, key hints
highlight  = "#313244"  # selected item background
matching   = "#a6e3a1"  # fuzzy-match highlight
```

Configuration takes effect immediately when `notify-history` is launched. The daemon (`notify-history-ctl`) re-reads the persistence setting on each start.

---

## Usage

### Daemon

```sh
notify-history-ctl              # start monitoring (blocks; run as a service)
notify-history-ctl --stop        # stop the running daemon
notify-history-ctl --clear-history  # wipe the history file
```

### TUI

```sh
notify-history
```

#### Keybindings

| Key | Action |
|---|---|
| `r` | Refresh from file |
| `x` | Delete current notification |
| `c` | Clear **all** notifications (confirm dialog) |
| `s` | Toggle multi-select on current notification |
| `S` | Delete all selected notifications (confirm dialog) |
| `q` | Quit |
| `g` | Go to start |
| `G` | Go to end |
| `/` | Open filter (fuzzy search) |
| `?` | Show keybinding help |
| `↑` / `k` | Move up |
| `↓` / `j` | Move down |
| `←` / `h` / `PgUp` | Previous page |
| `→` / `l` / `PgDn` | Next page |

**Filter mode** (`/`): type to fuzzy-search across title, app name, and body. Matching characters are highlighted. Press `Enter` to keep the filter active or `Esc` to clear it.

---

## NixOS Module

```nix
# configuration.nix
{
  imports = [ notify-history-tui.nixosModules.default ];

  services.notify-history-ctl = {
    enable  = true;
    package = notify-history-tui.packages.${system}.notify-history-ctl;
  };
}
```

This registers `notify-history-ctl` as a user-level systemd service that starts automatically with the graphical session.

---

## Home-Manager Module

```nix
# home.nix
{
  imports = [ notify-history-tui.homeManagerModules.default ];

  programs.notify-history = {
    enable      = true;
    tui.package = notify-history-tui.packages.${system}.notify-history;
    ctl.package = notify-history-tui.packages.${system}.notify-history-ctl;

    daemon.enable = true;   # manage the daemon as a user service
    persistence   = false;  # true → survive reboots
    bodyLines     = 4;
    showApp       = true;

    colors = {
      foreground = "#cdd6f4";
      background = "#1e1e2e";
      accent     = "#89b4fa";
      highlight  = "#313244";
      matching   = "#a6e3a1";
    };
  };
}
```

The Home-Manager module:
- Installs `notify-history` to `home.packages`
- Writes `~/.config/notify-history/config.toml`
- Optionally manages the daemon service
- Installs bash/zsh shell completions for both binaries

---

## History file format

One notification per line, fields separated by `[*]`:

```
<epoch_seconds>[*]<app_name>[*]<summary>[*]<body>
```

Special characters (`\n`, `\t`, `\\`, etc.) in field values are escaped. The file is prepended on each new notification (newest first). Use `notify-history-ctl --clear-history` to wipe it.

---

## Shell completions (manual install)

```sh
# Bash
notify-history-ctl --generate bash > /etc/bash_completion.d/notify-history-ctl
notify-history     --generate bash > /etc/bash_completion.d/notify-history

# Zsh
notify-history-ctl --generate zsh > "${fpath[1]}/_notify-history-ctl"
notify-history     --generate zsh > "${fpath[1]}/_notify-history"
```

When installing via the Nix flake, completions are installed automatically into the package's share directory and loaded by the Home-Manager module.

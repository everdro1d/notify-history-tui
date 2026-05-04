# Home-Manager module — installs both binaries, writes a config file, and
# optionally manages the daemon as a user systemd service.
#
# Usage in home.nix:
#
#   imports = [ notify-history-tui.homeManagerModules.default ];
#
#   programs.notify-history = {
#     enable = true;
#     tui.package  = notify-history-tui.packages.${system}.notify-history;
#     ctl.package  = notify-history-tui.packages.${system}.notify-history-ctl;
#     daemon.enable = true;
#   };
#
{ config, lib, pkgs, ... }:

with lib;

let
  cfg = config.programs.notify-history;

  # Generate config.toml from HM options
  configFile = (pkgs.formats.toml { }).generate "config.toml" {
    display = {
      show_app = cfg.showApp;
      body_lines = cfg.bodyLines;
      show_hints = cfg.showHints;
      refresh_time = cfg.refreshTime;
      escape_body = cfg.escapeBody;
    };
    persistence = {
      enabled = cfg.persistence;
      max_history = cfg.maxHistory;
    };
    colors = {
      foreground = cfg.colors.foreground;
      background = cfg.colors.background;
      accent = cfg.colors.accent;
      highlight = cfg.colors.highlight;
      matching = cfg.colors.matching;
    };
  };
in
{
  options.programs.notify-history = {
    enable = mkEnableOption "notify-history TUI and daemon";

    tui.package = mkOption {
      type = types.package;
      description = "The notify-history TUI package.";
    };

    ctl.package = mkOption {
      type = types.package;
      description = "The notify-history-ctl daemon package.";
    };

    daemon.enable = mkEnableOption "notify-history-ctl user systemd service";

    # ── Display options ───────────────────────────────────────────────────────

    showApp = mkOption {
      type = types.bool;
      default = true;
      description = "Show the application name next to the notification title.";
    };

    bodyLines = mkOption {
      type = types.ints.between 0 4;
      default = 3;
      description = "Number of body lines displayed per notification (0–4).";
    };

    refreshTime = mkOption {
      type = types.ints.unsigned;
      default = 5;
      description = "Auto-refresh interval in seconds (0 = disabled).";
    };

    showHints = mkOption {
      type = types.bool;
      default = true;
      description = "Show the keybind hint bar at the bottom (can be toggled per-session with F1).";
    };

    escapeBody = mkOption {
      type = types.bool;
      default = false;
      description = ''
        When false (default), literal escape sequences such as <code>\n</code>
        and <code>\t</code> in notification bodies are expanded to real
        characters for display.  When true, those sequences are shown verbatim.
      '';
    };

    persistence = mkOption {
      type = types.bool;
      default = false;
      description = ''
        When true, history is stored in
        <code>~/.local/state/notify-history/notification-history</code>
        so it survives reboots.
        When false, history is stored in <code>/tmp/notification-history</code>
        and cleared on shutdown.
      '';
    };

    maxHistory = mkOption {
      type = types.ints.unsigned;
      default = 0;
      description = "Maximum number of notifications to keep in history (0 = unlimited).";
    };

    # ── Colour scheme ─────────────────────────────────────────────────────────

    colors = {
      foreground = mkOption {
        type = types.str;
        default = "#cdd6f4";
        description = "Text (foreground) colour — hex RGB.";
      };
      background = mkOption {
        type = types.str;
        default = "#1e1e2e";
        description = "Background colour — hex RGB.";
      };
      accent = mkOption {
        type = types.str;
        default = "#89b4fa";
        description = "Accent colour used for titles, markers, and key hints — hex RGB.";
      };
      highlight = mkOption {
        type = types.str;
        default = "#313244";
        description = "Background colour for the selected item — hex RGB.";
      };
      matching = mkOption {
        type = types.str;
        default = "#a6e3a1";
        description = "Foreground colour for fuzzy-matched characters — hex RGB.";
      };
    };
  };

  config = mkIf cfg.enable {
    # Install the TUI binary
    home.packages = [ cfg.tui.package cfg.ctl.package ];

    # Write config.toml
    xdg.configFile."notify-history/config.toml".source = configFile;

    # Optional daemon service
    systemd.user.services.notify-history-ctl = mkIf cfg.daemon.enable {
      Unit = {
        Description = "Notification history daemon";
        Documentation = "https://github.com/everdro1d/notify-history-tui";
        After = [ "graphical-session.target" ];
        PartOf = [ "graphical-session.target" ];
      };
      Service = {
        ExecStart = "${cfg.ctl.package}/bin/notify-history-ctl";
        Restart = "on-failure";
        RestartSec = "5";
        StandardOutput = "journal";
        StandardError = "journal";
      };
      Install.WantedBy = [ "graphical-session.target" ];
    };

    # Shell completions loaded automatically when the daemon package is available

    programs.bash.initExtra = mkIf cfg.daemon.enable ''
      if command -v notify-history-ctl &>/dev/null; then
        source <(notify-history-ctl --generate bash 2>/dev/null)
      fi
      if command -v notify-history &>/dev/null; then
        source <(notify-history --generate bash 2>/dev/null)
      fi
    '';

    programs.zsh.initExtra = mkIf cfg.daemon.enable ''
      if (( $+commands[notify-history-ctl] )); then
        source <(notify-history-ctl --generate zsh 2>/dev/null)
      fi
      if (( $+commands[notify-history] )); then
        source <(notify-history --generate zsh 2>/dev/null)
      fi
    '';
  };
}

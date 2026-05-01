# NixOS module — installs notify-history-ctl as a user systemd service.
#
# Usage in NixOS configuration:
#
#   imports = [ notify-history-tui.nixosModules.default ];
#   services.notify-history-ctl.enable = true;
#   services.notify-history-ctl.package = notify-history-tui.packages.${system}.notify-history-ctl;
#
{ config, lib, pkgs, ... }:

with lib;

let
  cfg = config.services.notify-history-ctl;
in
{
  options.services.notify-history-ctl = {
    enable = mkEnableOption "notification history daemon (notify-history-ctl)";

    package = mkOption {
      type = types.package;
      description = "The notify-history-ctl package to use.";
    };
  };

  config = mkIf cfg.enable {
    # Defined as a user-level systemd service so it has access to the
    # D-Bus session bus and the user's home directory.
    systemd.user.services.notify-history-ctl = {
      description = "Notification history daemon";
      documentation = [ "https://github.com/everdro1d/notify-history-tui" ];

      after = [ "graphical-session.target" ];
      partOf = [ "graphical-session.target" ];
      wantedBy = [ "graphical-session.target" ];

      serviceConfig = {
        ExecStart = "${cfg.package}/bin/notify-history-ctl";
        Restart = "on-failure";
        RestartSec = "5s";
        # Forward output to the journal
        StandardOutput = "journal";
        StandardError = "journal";
      };
    };
  };
}

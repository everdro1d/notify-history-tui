{
  description = "Notification history viewer TUI with daemon";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = inputs @ { self, nixpkgs, flake-utils, rust-overlay, ... }:
    (flake-utils.lib.eachDefaultSystem (system:
      let
        overlays = [ rust-overlay.overlays.default ];
        pkgs = import nixpkgs { inherit system overlays; };

        rustToolchain = pkgs.rust-bin.stable.latest.default.override {
          extensions = [ "rust-src" "rust-analyzer" ];
        };

        rustPlatform = pkgs.makeRustPlatform {
          cargo = rustToolchain;
          rustc = rustToolchain;
        };

        version = (pkgs.lib.importTOML ./Cargo.toml).workspace.package.version;

        commonArgs = {
          src = ./.;
          cargoLock.lockFile = ./Cargo.lock;
          inherit version;
        };

        notify-history = rustPlatform.buildRustPackage (commonArgs // {
          pname = "notify-history";
          cargoBuildFlags = [ "--bin" "notify-history" ];
          cargoTestFlags = [ "--bin" "notify-history" ];

          nativeBuildInputs = [ pkgs.makeWrapper ];

          postInstall = ''
            # Bash completion
            install -Dm644 \
              <($out/bin/notify-history --generate bash) \
              $out/share/bash-completion/completions/notify-history

            # Zsh completion
            install -Dm644 \
              <($out/bin/notify-history --generate zsh) \
              $out/share/zsh/site-functions/_notify-history
          '';

          meta = {
            description = "Notification history viewer TUI";
            mainProgram = "notify-history";
          };
        });

        notify-history-ctl = rustPlatform.buildRustPackage (commonArgs // {
          pname = "notify-history-ctl";
          cargoBuildFlags = [ "--bin" "notify-history-ctl" ];
          cargoTestFlags = [ "--bin" "notify-history-ctl" ];

          nativeBuildInputs = [ pkgs.makeWrapper ];

          postInstall = ''
            # Wrap so dbus-monitor is always on PATH at runtime
            wrapProgram $out/bin/notify-history-ctl \
              --prefix PATH : ${pkgs.lib.makeBinPath [ pkgs.dbus ]}

            # Bash completion
            install -Dm644 \
              <($out/bin/notify-history-ctl --generate bash) \
              $out/share/bash-completion/completions/notify-history-ctl

            # Zsh completion
            install -Dm644 \
              <($out/bin/notify-history-ctl --generate zsh) \
              $out/share/zsh/site-functions/_notify-history-ctl
          '';

          meta = {
            description = "Notification history daemon";
            mainProgram = "notify-history-ctl";
          };
        });

      in
      {
        formatter = pkgs.nixpkgs-fmt;

        packages = {
          inherit notify-history notify-history-ctl;
          default = notify-history;
        };

        devShells.default = pkgs.mkShell {
          name = "notify-history-tui-dev";

          buildInputs = [
            rustToolchain
            pkgs.dbus
          ];

          nativeBuildInputs = [
            pkgs.pkg-config
          ];

          RUST_BACKTRACE = "1";
          RUST_SRC_PATH = "${rustToolchain}/lib/rustlib/src/rust/library";

          shellHook = ''
            echo "notify-history-tui development shell"
            echo "Rust: $(rustc --version)"
            echo ""
            echo "  cargo build --release   build both binaries"
            echo "  cargo clippy            run linter"
            echo "  cargo test              run tests"
            echo ""
          '';
        };
      }
    )) // {
      nixosModules.default = import ./nix/module.nix;
      homeManagerModules.default = import ./nix/hm-module.nix;
    };
}

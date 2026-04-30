{
  description = "Plank monorepo";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs { inherit system; };
        rustPlatform = pkgs.rustPlatform;
        plankDocs = pkgs.stdenvNoCC.mkDerivation {
          pname = "plank-docs";
          version = "0.1.0";
          src = ./plank-doc;
          nativeBuildInputs = [ pkgs.mdbook ];
          buildPhase = ''
            mdbook build
          '';
          installPhase = ''
            mkdir -p $out/share/doc
            cp -r book/. $out/share/doc/
            cp -r src $out/share/doc/src
          '';
        };
      in
      {
        packages.plank = rustPlatform.buildRustPackage {
          pname = "plank";
          version = "0.1.0";
          src = ./plankc;
          cargoLock = {
            lockFile = ./plankc/Cargo.lock;
          };
          cargoBuildFlags = [ "-p" "plank" ];
          postPatch = ''
            cp -r ${./std} ../std
          '';
          nativeBuildInputs = [ pkgs.makeWrapper ];
          postInstall = ''
            mkdir -p $out/share/doc $out/stdlib
            cp -r ${plankDocs}/share/doc/. $out/share/doc/
            cp -r ${./std}/. $out/stdlib/
            wrapProgram $out/bin/plank \
              --set PLANK_DIR $out
          '';
          meta = with pkgs.lib; {
            description = "Plank compiler CLI";
            homepage = "https://github.com/plankevm/plank-monorepo";
            license = licenses.mit;
            mainProgram = "plank";
          };
        };

        packages.tree-sitter-plank = pkgs.tree-sitter.buildGrammar {
          language = "plank";
          version = "0.1.0";
          src = ./plank-tree-sitter;
          meta = with pkgs.lib; {
            description = "Plank grammar for tree-sitter";
            homepage = "https://github.com/plankevm/plank-monorepo";
            license = licenses.mit;
          };
        };
        packages.default = self.packages.${system}.plank;

        checks.examples-build = pkgs.runCommand "plank-examples-build" {
          nativeBuildInputs = [ self.packages.${system}.plank ];
        } ''
          set -euo pipefail

          plank build ${./plankc/examples/erc20_v1.plk}
          plank build ${./plankc/plank-diff-tests/src/examples/erc20.plk}
          plank build ${./plankc/plank-diff-tests/src/examples/merkle_airdrop.plk}
          plank build ${./plankc/plank-diff-tests/src/examples/minimal_proxy.plk}
          plank build ${./plankc/examples/multifile/main.plk} \
            --module-name mymod \
            --module-root ${./plankc/examples/multifile}
          plank build ${./plankc/examples/multifile/main_glob.plk} \
            --module-name mymod \
            --module-root ${./plankc/examples/multifile}

          touch $out
        '';

        checks.default = self.checks.${system}.examples-build;

        apps.plank = {
          type = "app";
          program = "${self.packages.${system}.plank}/bin/plank";
          meta = {
            description = "Plank compiler CLI";
          };
        };

        apps.default = self.apps.${system}.plank;

        devShells.default = pkgs.mkShell {
          packages = with pkgs; [ rustc cargo rustfmt clippy mdbook ];
        };
      });
}

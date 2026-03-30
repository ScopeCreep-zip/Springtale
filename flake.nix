{
  description = "Springtale — local-first, privacy-preserving automation platform";

  inputs = {
    konductor.url = "github:braincraftio/konductor";
    nixpkgs.follows = "konductor/nixpkgs";
  };

  outputs = { self, nixpkgs, konductor, ... }:
    let
      supportedSystems = [ "x86_64-linux" "aarch64-linux" "x86_64-darwin" "aarch64-darwin" ];
      forAllSystems = nixpkgs.lib.genAttrs supportedSystems;
    in
    {
      devShells = forAllSystems (system:
        let
          pkgs = nixpkgs.legacyPackages.${system};
          rustShell = konductor.devShells.${system}.rust;
        in
        {
          default = rustShell.overrideAttrs (old: {
            buildInputs = (old.buildInputs or []) ++ [
              # ── Build tools ──────────────────────────────────────
              pkgs.pkg-config
              pkgs.openssl  # needed for some build scripts even though we use rustls at runtime

              # ── Database ─────────────────────────────────────────
              pkgs.sqlite

              # ── Node.js (TypeScript connector SDK) ──────────────
              pkgs.nodejs_22
              pkgs.pnpm

              # ── WASM tools ───────────────────────────────────────
              pkgs.wabt  # wasm-validate, wasm2wat

              # ── Security tooling ─────────────────────────────────
              pkgs.cargo-deny
              pkgs.cargo-audit
              pkgs.cargo-nextest
              pkgs.gitleaks
              pkgs.trivy
              pkgs.hadolint
              pkgs.cosign
              pkgs.syft
            ];

            shellHook = (old.shellHook or "") + ''
              echo "🌱 Springtale dev shell loaded"
              echo "   cargo build --workspace    # build"
              echo "   cargo nextest run          # test"
              echo "   cargo deny check           # audit"
            '';
          });
        }
      );
    };
}

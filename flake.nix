{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs = {
        nixpkgs.follows = "nixpkgs";
       };
    };
    zombienet = {
      url = "github:paritytech/zombienet";
      inputs = {
        nixpkgs.follows = "nixpkgs";
      };
    };
  };
  outputs = { self, nixpkgs, flake-utils, rust-overlay, zombienet }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        overlays = [ (import rust-overlay) zombienet.overlays.default ];
        pkgs = import nixpkgs {
          inherit system overlays;
        };
        rustToolchain = pkgs.pkgsBuildHost.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;
        buildInputs = with pkgs; [
          # Building Docker images and publishing to Azure Container Registry
          azure-cli
          cargo-expand
          clang
          pkg-config
          rustToolchain
          subxt
          just
          taplo
          polkadot
          mdbook
          mdbook-linkcheck
          cargo-tarpaulin
          # Due to zombienet's flake.nix, needs to be prefixed with pkg.zombienet
          pkgs.zombienet.default
        ]
        ++ (lib.optionals stdenv.isDarwin [
          darwin.apple_sdk.frameworks.Security
          darwin.apple_sdk.frameworks.CoreServices
          darwin.apple_sdk.frameworks.SystemConfiguration
        ])
        ++ (lib.optionals stdenv.isLinux [
          # TODO(@th7nder,#264, 24/08/2024): migrate to tarpaulin, because it's multiplatform:
          cargo-llvm-cov
        ]);
      in
      with pkgs;
      {
        devShells.default = mkShell {
          inherit buildInputs;

          LIBCLANG_PATH = "${llvmPackages.libclang.lib}/lib";
          PROTOC = "${protobuf}/bin/protoc";
          RUST_SRC_PATH = "${rustToolchain}/lib/rustlib/src/rust/library/";
        };
      }
    );
}

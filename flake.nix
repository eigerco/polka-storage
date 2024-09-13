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
          azure-cli # Building Docker images and publishing to Azure Container Registry
          cargo-expand
          cargo-tarpaulin
          clang
          just
          mdbook
          mdbook-linkcheck
          openssl
          pkg-config
          polkadot
          rustToolchain
          subxt
          taplo
          # Due to zombienet's flake.nix, needs to be prefixed with pkg.zombienet
          pkgs.zombienet.default
          ocl-icd
          hwloc
        ]
        ++ (lib.optionals stdenv.isDarwin [
          darwin.apple_sdk.frameworks.Security
          darwin.apple_sdk.frameworks.CoreServices
          darwin.apple_sdk.frameworks.SystemConfiguration
          darwin.apple_sdk.frameworks.OpenCL
        ]);
      in
      with pkgs;
      {
        devShells.default = mkShell {
          inherit buildInputs;

          OPENSSL_NO_VENDOR = 1;
          LIBCLANG_PATH = "${llvmPackages.libclang.lib}/lib";
          PROTOC = "${protobuf}/bin/protoc";
          RUST_SRC_PATH = "${rustToolchain}/lib/rustlib/src/rust/library/";
        };
      }
    );
}

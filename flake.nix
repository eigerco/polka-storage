{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs = {
        nixpkgs.follows = "nixpkgs";
      };
    };
    polkadot.url = "github:andresilva/polkadot.nix";
    polkadot.inputs.nixpkgs.follows = "nixpkgs";
  };
  outputs = { self, nixpkgs, flake-utils, rust-overlay, polkadot }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        overlays = [ (import rust-overlay) polkadot.overlays.default ];
        pkgs = import nixpkgs {
          inherit system overlays;
        };

        rustToolchain = pkgs.pkgsBuildHost.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;

        # It's because on MacOS, we're using Clang and underlying dependency (wasm-opt-rs) is failing to compile.
        # It fails to compile because Nix's Clang wrapper gives a warning about cross-compilation.
        # CRATE_CC_NO_DEFAULTS disables `--target` flag passed to the `clang++`.
        # It may have some unintended consequences, but didn't see any yet.
        # - https://github.com/andresilva/polkadot.nix/issues/36
        # - https://github.com/NixOS/nixpkgs/pull/323869#issuecomment-2635436334
        customPolkadot = if pkgs.stdenv.isDarwin then
          pkgs.polkadot.overrideAttrs (old: {
            CRATE_CC_NO_DEFAULTS = "1";
          })
        else
          pkgs.polkadot;

        buildInputs = with pkgs; [
          git
          cargo-tarpaulin
          clang
          just
          mdbook
          mdbook-linkcheck
          openssl
          pkg-config
          rustToolchain
          subxt
          taplo
          jq
          customPolkadot
          # Due to polkadot's flake.nix, needs to be prefixed with pkgs.zombienet
          pkgs.zombienet
          # rust-fil-proofs OpenCL dependencies (https://github.com/filecoin-project/rust-fil-proofs/blob/5a0523ae1ddb73b415ce2fa819367c7989aaf73f/README.md?plain=1#L74)
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
          CRATE_CC_NO_DEFAULTS = lib.optionalString pkgs.stdenv.isDarwin "1";
          LIBCLANG_PATH = "${llvmPackages.libclang.lib}/lib";
          PROTOC = "${protobuf}/bin/protoc";
          RUST_SRC_PATH = "${rustToolchain}/lib/rustlib/src/rust/library/";
          # Workaround https://github.com/NixOS/nixpkgs/issues/370494#issuecomment-2625163369.
          CFLAGS = "-DJEMALLOC_STRERROR_R_RETURNS_CHAR_WITH_GNU_SOURCE";
        };
      }
    );
}

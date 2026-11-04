# Builds the Rust workspace members with nixpkgs' rustPlatform.
#
# Dependencies are resolved from Cargo.lock via `rustPlatform.importCargoLock`.
# Each member is built with `cargo build -p <crate>`, so dependencies compile
# inside the same derivation as the member that needs them.
#
# Exposes:
#
#   workspace.workspaceMembers."${crateName}".build
#
{
  pkgs,
  lib,
  rustPlatform,
  llvmPackages,
  cmake,
  git,
  vulkan-headers,
  vulkan-loader,
  vulkan-tools,
  shaderc,
  mesa,
  rustfmt,

  # flutter stuff
  flutter335,
}:

let
  onnxruntime = pkgs.callPackage ./onnxruntime.nix { };

  # Registry crates are fetched from crates.io; git deps are fixed-output
  # fetches and need an `outputHashes` entry keyed "<crate-name>-<version>".
  cargoDeps = rustPlatform.importCargoLock {
    lockFile = ./Cargo.lock;
    # Replace temporarily with lib.fakeHash when updating this.
    outputHashes = {
      "llama-cpp-2-0.1.161" = "sha256-7HLuhEYdg2kUzgyrYYFDgiA8mTeYZ9HCjypoRB1a70k=";
      "llama-cpp-sys-2-0.1.161" = "sha256-7HLuhEYdg2kUzgyrYYFDgiA8mTeYZ9HCjypoRB1a70k=";
    };
  };

  # Shared by every member build. Dependencies compile in the same cargo
  # invocation as the member, so these apply to the whole build.
  commonEnv = {
    LIBCLANG_PATH = "${llvmPackages.libclang.lib}/lib/libclang.so";

    # Architecture-specific CPU feature flags
    # For ARM64: use defaults for compatibility with weaker devices (Raspberry Pi, etc.)
    CARGO_CFG_TARGET_FEATURE =
      if pkgs.stdenv.hostPlatform.isx86_64 then "sse4.2,fma,avx,avx512" else "";
  };

  commonNativeBuildInputs = [
    llvmPackages.bintools
    cmake
    rustPlatform.bindgenHook
    rustPlatform.cargoBuildHook
    vulkan-headers
    vulkan-loader
    vulkan-tools
    shaderc
    mesa
    git
    rustfmt
  ];

  commonBuildInputs = [
    vulkan-loader
    vulkan-headers
    shaderc
    mesa
  ];

  buildMember =
    name:
    {
      dir,
      cdylib ? false,
      env ? { },
      nativeBuildInputs ? [ ],
      buildInputs ? [ ],
    }:
    let
      manifest = builtins.fromTOML (builtins.readFile (./. + "/${dir}/Cargo.toml"));
      crateName = manifest.package.name;
    in
    rustPlatform.buildRustPackage (
      {
        pname = crateName;
        inherit (manifest.package) version;
        src = lib.cleanSourceWith {
          src = ./.;
          # keep local build artifacts (target/) out of the store source
          filter =
            path: type: !(type == "directory" && baseNameOf path == "target");
        };
        inherit cargoDeps;

        # Build (and, when enabled via doCheck, test) only this member.
        cargoBuildFlags = [ "-p" crateName ];
        checkFlags = [ "-p" crateName ];
        doCheck = false;

        env = commonEnv // env;
        nativeBuildInputs = commonNativeBuildInputs ++ nativeBuildInputs;
        buildInputs = commonBuildInputs ++ buildInputs;

        preConfigure = ''
          # ort-sys's `copy-dylibs` symlinks onnxruntime libs into
          # OUT_DIR.ancestors(3)/{examples,deps}. OUT_DIR is
          # $(pwd)/target/build/ort-sys.out, so ancestors(3) is the source
          # root — but `examples` and `deps` don't exist there.
          mkdir -p examples deps
        '';
      }
      // lib.optionalAttrs cdylib {
        # shared library in a `lib` output (${member.lib}/lib/lib<name>.so)
        outputs = [ "out" "lib" ];
        postInstall = ''
          if [ -d "$out/lib" ]; then
            mkdir -p "$lib/lib"
            mv "$out/lib/"* "$lib/lib/"
          fi
        '';
      }
    );

  # `buildInputs` entries here are link-time deps (vulkan, onnxruntime).
  members = {
    "nobodywho" = {
      dir = "core";
      buildInputs = [
        vulkan-loader
        onnxruntime
      ];
    };

    "nobodywho-godot" = {
      dir = "godot";
      cdylib = true;
      buildInputs = [
        vulkan-loader
        onnxruntime
      ];
    };

    "nobodywho-flutter" = {
      dir = "flutter/rust";
      cdylib = true;
      env = {
        NOBODYWHO_SKIP_CODEGEN = "1";
      };
      nativeBuildInputs = [
        # flutter on PATH for the flutter_rust_bridge build environment
        flutter335
      ];
      buildInputs = [
        vulkan-loader
        onnxruntime
      ];
    };

    "nobodywho-python" = {
      dir = "python";
      cdylib = true;
      # Stands in for the removed `pyo3/extension-module` cargo feature:
      # without it pyo3 links libpython, which isn't in the nix sandbox.
      env = {
        PYO3_BUILD_EXTENSION_MODULE = "1";
      };
      nativeBuildInputs = [
        pkgs.python3
      ];
      buildInputs = [
        vulkan-loader
        onnxruntime
      ];
    };

    "nobodywho-server".dir = "server";
    "make_stubs".dir = "python/make_stubs";
    "gbnf".dir = "grammar/gbnf";
    "gbnf-macro".dir = "grammar/gbnf-macro";

    "nobodywho-uniffi" = {
      dir = "uniffi";
      cdylib = true;
      buildInputs = [
        vulkan-loader
        onnxruntime
      ];
    };
  };

  # wrapped as { build = <derivation>; } for the
  # workspaceMembers."<crate>".build interface
  workspaceMembers = lib.mapAttrs (name: cfg: { build = buildMember name cfg; }) members;
in
{
  inherit workspaceMembers cargoDeps;
}

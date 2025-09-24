{
  pkgs ? import <nixpkgs> { },
  android-nixpkgs,
  ...
}:

let
  unity_version = "6000.0.47f1";
  unity-editor = pkgs.writeShellScriptBin "unity-editor" ''
    ${pkgs.lib.getExe pkgs.unityhub.fhsEnv} ~/Unity/Hub/Editor/${unity_version}/Editor/Unity "$@"
  '';

  # this NDK is the latest LTS at the time of writing
  # https://developer.android.com/ndk/downloads
  # ndk_version = "27.3.13750724";

  # this NDK is the one mentioned in the godot docs:
  # https://docs.godotengine.org/en/stable/contributing/development/compiling/compiling_for_android.html
  ndk_version = "23.2.8568313";

  ndk_version_dashed = builtins.replaceStrings [ "." ] [ "-" ] ndk_version;
  ndk_version_for_android-nixpkgs = "ndk-${ndk_version_dashed}";
  android-sdk = android-nixpkgs.sdk.x86_64-linux (
    sdkPkgs: with sdkPkgs; [
      cmdline-tools-latest
      build-tools-34-0-0
      platform-tools
      platforms-android-34
      emulator
      # weird hack to get NDK in here
      # see: https://github.com/tadfisher/android-nixpkgs/issues/113#issuecomment-3112489517
      (sdkPkgs.${ndk_version_for_android-nixpkgs}.overrideAttrs (old: {
        autoPatchelfIgnoreMissingDeps = true;
      }))
    ]
  );
in
pkgs.mkShell {
  env.LIBCLANG_PATH = "${pkgs.libclang.lib}/lib/libclang.so";
  env.TEST_MODEL = pkgs.fetchurl {
      name = "Qwen_Qwen3-0.6B-Q4_K_M.gguf";
      url = "https://huggingface.co/bartowski/Qwen_Qwen3-0.6B-GGUF/resolve/main/Qwen_Qwen3-0.6B-Q4_K_M.gguf";
      sha256 = "sha256-ms/B4AExHzS0JSABtiby5GbVkqQgZfZlcb/zeQ1OGxQ=";
    };
    env.TEST_EMBEDDINGS_MODEL = pkgs.fetchurl {
      name = "bge-small-en-v1.5-q8_0.gguf";
      url = "https://huggingface.co/CompendiumLabs/bge-small-en-v1.5-gguf/resolve/main/bge-small-en-v1.5-q8_0.gguf";
      sha256 = "sha256-7Djo2hQllrqpExJK5QVQ3ihLaRa/WVd+8vDLlmDC9RQ=";
    };
    env.TEST_CROSSENCODER_MODEL = pkgs.fetchurl {
      name = "bge-reranker-v2-m3-Q8_0.gguf";
      url = "https://huggingface.co/gpustack/bge-reranker-v2-m3-GGUF/resolve/main/bge-reranker-v2-m3-Q8_0.gguf";
      sha256 = "sha256-pDx8mxGkwVF+W/lRUZYOFiHRty96STNksB44bPGqodM=";
    };

  packages = [
    pkgs.cmake
    pkgs.clang
    pkgs.rustup

    # Unity
    unity-editor
    pkgs.unityhub
    pkgs.libxml2
    pkgs.just
    # lsp dependencies
    pkgs.dotnet-sdk_9
    pkgs.csharpier
    # used to make good stack traces
    pkgs.gdb
    pkgs.lldb

    # these are the dependencies required by llama.cpp to build for vulkan
    # (these packages were discovered by looking at the nix source code in ggerganov/llama.cpp)
    pkgs.vulkan-headers
    pkgs.vulkan-loader
    pkgs.shaderc

    # for android
    # android-sdk
    # pkgs.openjdk11-bootstrap

    # for mkdocs
    pkgs.mkdocs
    pkgs.python312Packages.regex
    pkgs.python312Packages.mkdocs-material

    # flutter
    pkgs.flutter
    pkgs.flutter_rust_bridge_codegen

    # GTK and portal dependencies for file_picker
    pkgs.gtk3
    pkgs.glib
    pkgs.gsettings-desktop-schemas
    pkgs.shared-mime-info
    pkgs.xdg-desktop-portal
    pkgs.xdg-desktop-portal-gtk
  ];
  shellHook = ''
    ulimit -n 2048

    # https://godot-rust.github.io/book/toolchain/export-android.html
    # export ANDROID_NDK="$ANDROID_SDK_ROOT/ndk/${ndk_version}"
    # export CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER="$ANDROID_SDK_ROOT/ndk/${ndk_version}/toolchains/llvm/prebuilt/linux-x86_64/bin/aarch64-linux-android31-clang"

    echo "Environment is set up for android compilation with NDK version ${ndk_version}"
    echo "These paths should be set in Godot Editor settings, for android export:"
    echo "Android SDK Path: " "$ANDROID_SDK_ROOT"
    echo "Java SDK Path: " "TBD"
  '';
}

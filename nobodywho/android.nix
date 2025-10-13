{
  pkgs,
  android-nixpkgs,
  stdenv,
}:
let
  androidEnv = android-nixpkgs.sdk.${stdenv.system} (
    sdkPkgs: with sdkPkgs; [
      cmdline-tools-latest
      build-tools-35-0-0 # needed by our version of flutter
      platform-tools
      platforms-android-33 # needed by our version of flutter
      platforms-android-34 # needed by our version of flutter
      ndk-27-0-12077973 # needed by our version of flutter
    ]
  );

  ndkVersion = "27.0.12077973"; # must be kept in sync with the ndk version mentioned above
  coreShell = pkgs.callPackage (import ./core/shell.nix) { };
in
pkgs.mkShell {
  name = "nobodywho android shell";
  inputsFrom = [ coreShell ];
  env = rec {
    LIBCLANG_PATH = "${coreShell.LIBCLANG_PATH}";
    ANDROID_NDK = "${androidEnv}/share/android-sdk/ndk/${ndkVersion}";
    CC_aarch64_linux_android = "${ANDROID_NDK}/toolchains/llvm/prebuilt/linux-x86_64/bin/aarch64-linux-android31-clang";
    AR_aarch64_linux_android = "${ANDROID_NDK}/toolchains/llvm/prebuilt/linux-x86_64/bin/llvm-ar";
    CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER = "${ANDROID_NDK}/toolchains/llvm/prebuilt/linux-x86_64/bin/aarch64-linux-android31-clang";
    CARGO_TARGET_AARCH64_LINUX_ANDROID_AR = "${ANDROID_NDK}/toolchains/llvm/prebuilt/linux-x86_64/bin/llvm-ar";
    JAVA_HOME = "${pkgs.jdk17}";
  };
  buildInputs = [
    androidEnv
  ];
}

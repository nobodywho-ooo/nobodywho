{ pkgs, fetchurl, stdenv, lib, ... }:

let
  version = "6000.0.34f1";
  unity-src = fetchTarball {
      url = "https://download.unity3d.com/download_unity/faad68ae9e63/LinuxEditorInstaller/Unity-${version}.tar.xz";
      sha256 = "sha256:1df66d1sc6dfzxhl0ij7b7lr6ahnjv06i4lrdp13z9yccrrirxq5";
  };

  unity-editor = pkgs.buildFHSEnv {
    name = "unity-editor";
    inherit version;
    
    runScript = "~/Unity/Hub/Editor/${version}/Editor/Unity";

    targetPkgs = pkgs: with pkgs;
      [
        # Unity Hub binary dependencies
        xorg.libXrandr
        xdg-utils 
        # GTK filepicker
        gsettings-desktop-schemas
        hicolor-icon-theme 
        # Bug Reporter dependencies
        fontconfig
        freetype
        lsb-release
      ];

    multiPkgs = pkgs: with pkgs;
      [
        # Unity Hub ldd dependencies
        cups
        gtk3
        expat
        libxkbcommon
        lttng-ust_2_12
        krb5
        alsa-lib
        nss
        libdrm
        libgbm
        nspr
        atk
        dbus
        at-spi2-core
        pango
        xorg.libXcomposite
        xorg.libXext
        xorg.libXdamage
        xorg.libXfixes
        xorg.libxcb
        xorg.libxshmfence
        xorg.libXScrnSaver
        xorg.libXtst
        # Unity Hub additional dependencies
        libva
        openssl
        cairo
        libnotify
        libuuid
        libsecret
        udev
        libappindicator
        wayland
        cpio
        icu
        libpulseaudio
        # Unity Editor dependencies
        libglvnd # provides ligbl
        xorg.libX11
        xorg.libXcursor
        glib
        gdk-pixbuf
        libxml2
        zlib
        clang
        git # for git-based packages in unity package manager
        # Unity Editor 2019 specific dependencies
        xorg.libXi
        xorg.libXrender
        gnome2.GConf
        libcap
        # Unity Editor 6000 specific dependencies
        harfbuzz
      ];
  };

in {
  # Export only Unity-specific derivations
  unity-editor = unity-editor;
  
  devShell = pkgs.mkShell {
    buildInputs = [
      unity-editor
    ];
  };
}
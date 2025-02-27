{ pkgs, fetchurl, stdenv, lib, ... }:

let
  version = "6000.1.0b7";
  unity-editor = stdenv.mkDerivation {
    pname = "unity-editor";
    inherit version;
    dontStrip = true;

    src = fetchTarball {
      url = "https://download.unity3d.com/download_unity/faad68ae9e63/LinuxEditorInstaller/Unity-${version}.tar.xz";
      sha256 = "sha256:1df66d1sc6dfzxhl0ij7b7lr6ahnjv06i4lrdp13z9yccrrirxq5";
    };

    # Add required runtime dependencies
    buildInputs = [
      pkgs.libxml2
      pkgs.gtk3
      pkgs.freeimage
      pkgs.embree2
      pkgs.libGL
      # Common X11 dependencies
      pkgs.xorg.libX11
      pkgs.xorg.libXcursor
      pkgs.xorg.libXrandr
      # System libraries
      pkgs.systemd  # For libudev
      # Specific libraries from your trace
      pkgs.openexr
      pkgs.gdk-pixbuf
      pkgs.glib
      # Additional libraries from ldd output
      pkgs.zlib         # libz
      pkgs.cairo        # libcairo
      pkgs.pango        # libpango
      pkgs.fontconfig   # libfontconfig
      pkgs.harfbuzz     # libharfbuzz
      pkgs.atk          # libatk
      pkgs.file         # For identifying ELF files
      pkgs.patchelf     # For patching ELF files
      pkgs.icu          # For ICU (International Components for Unicode)
      pkgs.at-spi2-core # For libatk-1.0.so
      pkgs.openssl_1_1  # For the License Client
    ];
    # Use an improved approach to patch all ELF files in the package
    installPhase = ''
      mkdir -p $out/bin $out/share/unity
      cp -r ./* $out/share/unity/
      
      # Define the RPATH with all necessary dependencies
      RPATH=${pkgs.lib.makeLibraryPath [
        pkgs.libxml2
        pkgs.gtk3
        pkgs.freeimage
        pkgs.embree2
        pkgs.libGL
        pkgs.xorg.libX11
        pkgs.xorg.libXcursor
        pkgs.xorg.libXrandr
        pkgs.systemd
        pkgs.openexr
        pkgs.gdk-pixbuf
        pkgs.glib
        pkgs.zlib
        pkgs.cairo
        pkgs.pango
        pkgs.fontconfig
        pkgs.harfbuzz
        pkgs.atk
        pkgs.xorg.libXext
        pkgs.xorg.libXrender
        pkgs.xorg.libXfixes
        pkgs.xorg.libXi
        pkgs.xorg.libXcomposite
        pkgs.xorg.libXdamage
        pkgs.stdenv.cc.cc.lib
        pkgs.libcap
        pkgs.fribidi
        pkgs.tbb
        pkgs.icu
        pkgs.at-spi2-core
        pkgs.openssl_1_1
      ]}:$out/share/unity



      # Find the Unity executable and patch it specifically first
      UNITY_EXEC=$out/share/unity/Unity
      ${pkgs.patchelf}/bin/patchelf --set-interpreter ${pkgs.stdenv.cc.bintools.dynamicLinker} "$UNITY_EXEC"
      ${pkgs.patchelf}/bin/patchelf --set-rpath "$RPATH" "$UNITY_EXEC"
      
      
      # Find and patch all ELF files in the Unity package
      find $out/share/unity -type f -executable -o -name "*.so*" | while read -r file; do
        # Check if it's an ELF file and dynamically linked
        if ${pkgs.file}/bin/file "$file" | grep -q "ELF" && ! ${pkgs.file}/bin/file "$file" | grep -q "statically linked"; then
          echo "Patching $file"
          ${pkgs.patchelf}/bin/patchelf --set-interpreter ${pkgs.stdenv.cc.bintools.dynamicLinker} "$file" 2>/dev/null || true
          ${pkgs.patchelf}/bin/patchelf --set-rpath "$RPATH" "$file" 2>/dev/null || true
        fi
      done
      
      # Create a wrapper script that sets up the environment for the License Client
      cat > $out/bin/unity-editor << EOF
      #!/bin/sh
      
      # Run Unity with environment variables set only for this command and its children
      # These variables will not affect the global environment
      DOTNET_SYSTEM_GLOBALIZATION_INVARIANT=1 \
      LD_LIBRARY_PATH=\$LD_LIBRARY_PATH:${pkgs.openssl_1_1}/lib \
      UNITY_DATADIR=$out/share/unity \
      exec $out/share/unity/Unity "\$@"
      EOF
      
      chmod +x $out/bin/unity-editor
    '';
    dontFixup = true;
  };

  ensure-license = pkgs.writeScriptBin "ensure-license" ''
    #!/usr/bin/env bash
    ${unity-editor}/bin/unity_editor \
      -batchmode \
      -nographics \
      -logFile /dev/null \
      -quit
  '';

  unity-test = pkgs.writeScriptBin "unity-test" ''
    #!/usr/bin/env bash
    ${ensure-license}
    
    ${unity-editor}/bin/unity-editor \
      -batchmode \
      -projectPath . \
      -runTests \
      -testPlatform PlayMode \
      -testResults test-results.xml \
      -quit

    #{unity_parse_test_results}
  '';

in {
  # Export only Unity-specific derivations
  unity_editor = unity-editor;
  
  checks = {
    unity-test = unity-test;
  };
}
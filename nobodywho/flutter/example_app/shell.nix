{
  pkgs,
  android-nixpkgs,
  system ? "x86_64-linux",
}:
let
  androidEnv = android-nixpkgs.sdk.${system} (
    sdkPkgs: with sdkPkgs; [
      cmdline-tools-latest
      build-tools-35-0-0 # needed by our version of flutter
      build-tools-36-0-0
      platform-tools
      platforms-android-33 # needed by our version of flutter
      platforms-android-34 # needed by our version of flutter
      platforms-android-36
      emulator
      ndk-26-1-10909125
      ndk-27-0-12077973 # needed by our version of flutter
      system-images-android-36-google-apis-playstore-x86-64
    ]
  );

  # Create Android licenses as a derivation
  androidLicenses = pkgs.runCommand "android-licenses" { } ''
    mkdir -p $out/licenses

    # Android SDK License
    cat > $out/licenses/android-sdk-license << EOF
    8933bad161af4178b1185d1a37fbf41ea5269c55
    d56f5187479451eabf01fb78af6dfcb131a6481e
    24333f8a63b6825ea9c5514f83c2829b004d1fee
    EOF

    # Android SDK Preview License
    cat > $out/licenses/android-sdk-preview-license << EOF
    84831b9409646a918e30573bab4c9c91346d8abd
    EOF

    # Google TV License
    cat > $out/licenses/googletv-license << EOF
    601085b94cd77f0b54ff86406957099ebe79c4d6
    EOF
  '';

  # Pre-built AVD configuration
  prebuiltAvd =
    pkgs.runCommand "android-avd"
      {
        nativeBuildInputs = [
          androidEnv
          pkgs.coreutils
        ];
      }
      ''
        # Create a minimal AVD configuration
        mkdir -p $out/avd/android_emulator.avd

        # Create config.ini
        cat > $out/avd/android_emulator.avd/config.ini << EOF
        avd.ini.encoding=UTF-8
        AvdId=android_emulator
        PlayStore.enabled=true
        abi.type=x86_64
        avd.ini.displayname=android_emulator
        disk.dataPartition.size=2G
        hw.accelerometer=yes
        hw.arc=false
        hw.audioInput=yes
        hw.battery=yes
        hw.camera.back=virtualscene
        hw.camera.front=emulated
        hw.cpu.arch=x86_64
        hw.cpu.ncore=4
        hw.dPad=no
        hw.device.hash2=MD5:6b5943207fe196d842659d2e43022e20
        hw.device.manufacturer=Google
        hw.device.name=pixel
        hw.gps=yes
        hw.gpu.enabled=yes
        hw.gpu.mode=host
        hw.initialOrientation=Portrait
        hw.keyboard=yes
        hw.lcd.density=420
        hw.lcd.height=1920
        hw.lcd.width=1080
        hw.mainKeys=no
        hw.ramSize=2048
        hw.sdCard=yes
        hw.sensors.orientation=yes
        hw.sensors.proximity=yes
        hw.trackBall=no
        image.sysdir.1=system-images/android-36/google_apis_playstore/x86_64/
        runtime.network.latency=none
        runtime.network.speed=full
        sdcard.size=512M
        showDeviceFrame=yes
        skin.dynamic=yes
        skin.name=pixel_silver
        skin.path=_no_skin
        tag.display=Google Play
        tag.id=google_apis_playstore
        vm.heapSize=256
        EOF

        # Create avd.ini
        cat > $out/avd/android_emulator.ini << EOF
        avd.ini.encoding=UTF-8
        path=$out/avd/android_emulator.avd
        path.rel=avd/android_emulator.avd
        target=android-36
        EOF
      '';

  # Create a wrapper for the Android SDK that includes everything
  wrappedAndroidEnv = pkgs.runCommand "wrapped-android-sdk" { } ''
    mkdir -p $out/share/android-sdk
    cp -LR ${androidEnv}/share/android-sdk/* $out/share/android-sdk/

    # Create the cmake directory structure that Gradle expects
    mkdir -p $out/share/android-sdk/cmake/3.22.1/bin
    ln -sf ${pkgs.cmake}/bin/cmake $out/share/android-sdk/cmake/3.22.1/bin/cmake
    ln -sf ${pkgs.ninja}/bin/ninja $out/share/android-sdk/cmake/3.22.1/bin/ninja

    # Link licenses
    # XXX: removed
    # ln -sf ${androidLicenses}/licenses $out/share/android-sdk/licenses

    # Copy binaries
    mkdir -p $out/bin
    for bin in ${androidEnv}/bin/*; do
      ln -s $bin $out/bin/
    done
  '';

  # All the libraries we need available
  runtimeLibs = with pkgs; [
    # Core runtime libraries
    glibc
    zlib
    ncurses5
    stdenv.cc.cc.lib

    # Qt 5/6 and X11 GUI dependencies
    # XXX: naively removed
    # libsForQt5.qt5.qtbase
    # libsForQt5.qt5.qtsvg
    # libsForQt5.qt5.qtwayland
    # libsForQt5.qt5.qttools
    # libsForQt5.qt5.qtdeclarative
    # qt6.qt5compat
    # libsForQt5.qt5ct
    # qt6.qtbase
    # qt6.qtsvg
    # qt6.qtwayland
    # qt6.qt5compat
    xorg.libX11
    xorg.libXext
    xorg.libXfixes
    xorg.libXi
    xorg.libXrandr
    xorg.libXrender
    xorg.libxcb
    xorg.xcbutil
    xorg.xcbutilwm
    xorg.xcbutilimage
    xorg.xcbutilkeysyms
    xorg.xcbutilrenderutil
    libxkbcommon
    xcb-util-cursor
    xorg.libXcursor

    # Graphics and font rendering
    mesa
    libdrm
    vulkan-loader
    fontconfig
    freetype
    libglvnd

    # System services and input handling
    dbus
    libevdev
    libpulseaudio
    pipewire
    udev
    libinput
    libinput-gestures
    at-spi2-atk
    at-spi2-core

    # Additional GUI dependencies
    gtk3
    gdk-pixbuf
    cairo
    pango
    harfbuzz
    glib
    gsettings-desktop-schemas
  ];

  wrappedEmulator = pkgs.writeShellScriptBin "run-emulator" ''
    #!/usr/bin/env bash
    echo "Launching emulator with universal Qt/X11/Wayland fix..."

    # Use a temporary directory for runtime AVD data
    export ANDROID_AVD_HOME="''${ANDROID_AVD_HOME:-$(mktemp -d -t android-avd-XXXXXX)}"
    echo "Using AVD directory: $ANDROID_AVD_HOME"

    # Copy the prebuilt AVD if it doesn't exist
    if [ ! -d "$ANDROID_AVD_HOME/android_emulator.avd" ]; then
      cp -r ${prebuiltAvd}/avd/* "$ANDROID_AVD_HOME/"
      chmod -R u+w "$ANDROID_AVD_HOME"
    fi

    # ----------------------------
    # Detect display server
    # ----------------------------
    if [ -n "$WAYLAND_DISPLAY" ]; then
      echo "🌿 Wayland detected: $WAYLAND_DISPLAY"
      USE_WAYLAND=true
      export DISPLAY=:0  # Force XWayland
    else
      echo "🖥️  X11 detected: ''${DISPLAY:-:0}"
      USE_WAYLAND=false
    fi

    # ----------------------------
    # Qt environment
    # ----------------------------
    export QT_QPA_PLATFORM=xcb
    export QT_QPA_PLATFORM_PLUGIN_PATH="${pkgs.qt6.qtbase}/lib/qt-6/plugins"
    export QT_PLUGIN_PATH="${pkgs.qt6.qtbase}/lib/qt-6/plugins"
    export QML2_IMPORT_PATH="${pkgs.qt6.qtbase}/lib/qt-6/qml"
    export QTWEBENGINE_DISABLE_SANDBOX=1
    export QT_OPENGL=desktop
    export QT_QPA_PLATFORMTHEME=gtk3

    # ----------------------------
    # Graphics / OpenGL driver
    # ----------------------------
    if [ -d "/run/opengl-driver" ]; then
        echo "✅ NVIDIA/OpenGL driver detected"
        export LD_LIBRARY_PATH="/run/opengl-driver/lib:$LD_LIBRARY_PATH"
        export LIBGL_DRIVERS_PATH="/run/opengl-driver/lib/dri"
        export MESA_LOADER_DRIVER_OVERRIDE=""
    else
        echo "⚠️ NVIDIA driver not found, using Mesa fallback"
        export LD_LIBRARY_PATH="${pkgs.mesa}/lib:${pkgs.libdrm}/lib:${pkgs.vulkan-loader}/lib:$LD_LIBRARY_PATH"
        export LIBGL_DRIVERS_PATH="${pkgs.mesa}/lib/dri"
        export MESA_LOADER_DRIVER_OVERRIDE=i965
    fi

    # ----------------------------
    # Run the emulator
    # ----------------------------
    exec emulator -avd android_emulator -gpu host -no-snapshot -no-snapshot-load -no-snapshot-save "$@"
  '';

  # Patched Flutter derivation
  patchedFlutter = pkgs.flutter.overrideAttrs (oldAttrs: {
    patchPhase = ''
      runHook prePatch

      # Patch FlutterTask.kt - this handles the main cmake/ninja paths
      substituteInPlace $FLUTTER_ROOT/packages/flutter_tools/gradle/src/main/kotlin/FlutterTask.kt \
        --replace 'val cmakeExecutable = project.file(cmakePath).absolutePath' 'val cmakeExecutable = "cmake"' \
        --replace 'val ninjaExecutable = project.file(ninjaPath).absolutePath' 'val ninjaExecutable = "ninja"'

      # Also patch any Gradle build scripts that reference cmake directly
      find $FLUTTER_ROOT -name "*.gradle" -o -name "*.gradle.kts" | xargs -I {} \
        sed -i 's|cmake/[^/]*/bin/cmake|cmake|g' {} 2>/dev/null || true

      # Patch any other cmake references in Flutter tools
      find $FLUTTER_ROOT/packages/flutter_tools -name "*.dart" | xargs -I {} \
        sed -i 's|/cmake/[^/]*/bin/cmake|cmake|g' {} 2>/dev/null || true

      runHook postPatch
    '';
  });

  # Version pins
  minSdkVersion = "21";
  ndkVersion = "27.0.12077973";
  kotlinVersion = "2.0.21";
  agpVersion = "8.12.3";

  coreShell = pkgs.callPackage (import ../../core/shell.nix) { };

in
pkgs.mkShell {
  name = "flutter-android-dev-env";

  buildInputs =
    coreShell.nativeBuildInputs
    ++ (with pkgs; [
      # Basic development tools
      bashInteractive
      git
      cmake
      ninja
      python3
      jdk17
      gradle
      patchedFlutter
      wrappedEmulator

      # Android SDK components
      wrappedAndroidEnv

      # nix-ld for dynamic linking
      nix-ld

      # X11 utilities
      xorg.setxkbmap
      xorg.xauth
      xorg.xhost
      xorg.xset

      # Mesa demos for testing
      mesa-demos
    ])
    ++ runtimeLibs;

  NIX_LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath runtimeLibs;
  NIX_LD = "${pkgs.glibc}/lib/ld-linux-x86-64.so.2";

  # Set Android environment variables
  ANDROID_HOME = "${wrappedAndroidEnv}/share/android-sdk";
  ANDROID_SDK_ROOT = "${wrappedAndroidEnv}/share/android-sdk";
  # XXX: REMOVED
  # ANDROID_USER_HOME = "/tmp/android-''${USER}";
  # ANDROID_PREFS_ROOT = "/tmp/android-''${USER}";
  JAVA_HOME = "${pkgs.jdk17}";

  # HACK: very weird that this makes any difference
  # but it solves this error: "Fatal Error: Unable to find package java.lang in classpath or bootclasspath"
  GRADLE_OPTS = "-Dorg.gradle.jvmargs='-Xmx4096m'";

  shellHook = ''
    echo "mkShell is active. Setting up Flutter+Android environment..."

    # Set up library paths for dynamic linking
    export LD_LIBRARY_PATH="$NIX_LD_LIBRARY_PATH:$LD_LIBRARY_PATH"

    # Add Android SDK tools to PATH
    export PATH="$ANDROID_HOME/platform-tools:$ANDROID_HOME/cmdline-tools/latest/bin:$ANDROID_HOME/emulator:$ANDROID_HOME/build-tools/36.0.0:${pkgs.cmake}/bin:${pkgs.ninja}/bin:$PATH"

    # Create temporary Android user directory for any runtime state
    # mkdir -p "$ANDROID_USER_HOME"

    echo "Stopping any existing ADB server..."
    adb kill-server &> /dev/null || true

    # Configure Flutter
    flutter config --android-sdk "$ANDROID_HOME" --no-analytics

    # Create flutter project in root directory if one doesnt exist
    if [ ! -f pubspec.yaml ]; then
      echo "No Flutter project found. Creating a new one..."
      flutter create .
    fi

    if ! git rev-parse --is-inside-work-tree >/dev/null 2>&1; then
        echo "No Git repository detected. Initializing..."
        git init
        git add .
        git commit -m "Initial Commit done by Flake for Flutter Android dev shell"
        echo "✅ Git repository initialized and initial commit created."
    else
        echo "Git repository already exists. Skipping initialization."
    fi

    mkdir -p android/app/src/main/{kotlin,java}
    mkdir -p android/app/src/debug/{kotlin,java}
    mkdir -p android/app/src/profile/{kotlin,java}
    mkdir -p android/app/src/release/{kotlin,java}

    # Ensure android directories exist (avoid sed failures)
    mkdir -p android app

    if [ -d android ]; then
      # Create gradle.properties if it doesn't exist
      touch android/gradle.properties
      
      # Use sed to remove existing properties to avoid duplicates  
      sed -i '/^android\.cmake\.path=/d' android/gradle.properties
      sed -i '/^android\.ninja\.path=/d' android/gradle.properties
      sed -i '/^android\.cmake\.version=/d' android/gradle.properties
      
      # Append new properties (preserves any other existing config)
      echo "android.cmake.path=$ANDROID_HOME/cmake/3.22.1/bin" >> android/gradle.properties
      echo "android.ninja.path=$ANDROID_HOME/cmake/3.22.1/bin" >> android/gradle.properties
      echo "android.cmake.version=3.22.1" >> android/gradle.properties

      # ALSO ADD CMAKE_MAKE_PROGRAM override
      echo "android.cmake.makeProgram=$ANDROID_HOME/cmake/3.22.1/bin/ninja" >> android/gradle.properties
    fi

    # Only patch if gradle.kts files exist
    if [ -f "android/build.gradle.kts" ]; then
      echo "⚙️ Pinning Android build tool versions in Kotlin DSL..."

      sed -i -e "s/id(\"com.android.application\") version \"[0-9.]*\"/id(\"com.android.application\") version \"${agpVersion}\"/g" android/build.gradle.kts
      sed -i -e "s/id(\"org.jetbrains.kotlin.android\") version \"[0-9.]*\"/id(\"org.jetbrains.kotlin.android\") version \"${kotlinVersion}\"/g" android/build.gradle.kts
    fi

    if [ -f "android/app/build.gradle.kts" ]; then
      sed -i -e "s/minSdk = [0-9a-zA-Z._]*/minSdk = ${minSdkVersion}/g" android/app/build.gradle.kts
    fi

    # Support for traditional Groovy build files 
    if [ -f "android/build.gradle" ]; then
      echo "⚙️ Pinning Android build tool versions in Groovy DSL..."
      sed -i -e "s/com.android.application.*version.*'[0-9.]*'/com.android.application' version '${agpVersion}'/g" android/build.gradle
      sed -i -e "s/org.jetbrains.kotlin.android.*version.*'[0-9.]*'/org.jetbrains.kotlin.android' version '${kotlinVersion}'/g" android/build.gradle
    fi

    if [ -f "android/app/build.gradle" ]; then
      sed -i -e "s/minSdkVersion [0-9]*/minSdkVersion ${minSdkVersion}/g" android/app/build.gradle
    fi

    # Verify our tools are accessible
    echo "🔧 Using CMake: $(which cmake) ($(cmake --version | head -1))"
    echo "🔧 Using Ninja: $(which ninja) ($(ninja --version))"
    echo "🔧 Android SDK: $ANDROID_HOME"

    flutter doctor --quiet
    echo "✅ Flutter + Android dev shell ready."

    echo "👉 To launch the emulator, run: run-emulator"
    echo "   (AVD will be created in a temporary directory)"

    # XXX: needed to build nobodywho
    export LIBCLANG_PATH="${coreShell.LIBCLANG_PATH}"
    export ANDROID_NDK="$ANDROID_HOME/ndk/${ndkVersion}"
  '';
}

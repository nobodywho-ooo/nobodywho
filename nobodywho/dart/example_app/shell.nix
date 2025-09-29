{
  pkgs,
  android-nixpkgs,
  system ? "x86_64-linux",
}:
let
  androidEnv =
    android-nixpkgs.sdk.${system} (
      sdkPkgs: with sdkPkgs; [
        cmdline-tools-latest
        build-tools-36-0-0
        platform-tools
        platforms-android-36
        emulator
        ndk-26-1-10909125
        # include system image inside SDK instead of relying on sdkmanager. This ensures emulator functionality.
        system-images-android-36-google-apis-playstore-x86-64
      ]
    )
    // {
      # Override the emulator to include the missing Qt/X11 dependencies
      buildInputs =
        (androidEnv.buildInputs or [ ])
        ++ (with pkgs; [
          xcb-util-cursor
          xorg.libXcursor
          xorg.libX11
          xorg.libxcb
          qt6.qtbase
          qt6.qtsvg
        ]);
    };

  wrappedEmulator = pkgs.writeShellScriptBin "run-emulator" ''
    	  #!/usr/bin/env bash
    	  echo "Launching emulator with universal Qt/X11/Wayland fix..."

    	  # ----------------------------
    	  # Detect display server
    	  # ----------------------------
    	  if [ -n "$WAYLAND_DISPLAY" ]; then
    	    echo "🌿 Wayland detected: $WAYLAND_DISPLAY"
    	    USE_WAYLAND=true
    	    export DISPLAY=:0  # Force XWayland
    	  else
    	    echo "🖥️  X11 detected: ${"DISPLAY:-:0"}"
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
    	      export LIBGL_DRIVERS_PATH="${pkgs.mesa}/lib/dri" w
    	      export MESA_LOADER_DRIVER_OVERRIDE=i965
    	  fi

    	  # ----------------------------
    	  # Run the emulator
    	  # ----------------------------
    	  exec emulator -avd android_emulator -gpu host -no-snapshot -no-snapshot-load -no-snapshot-save "$@"
    	'';

  # Patched Flutter derivation.
  patchedFlutter = pkgs.flutter.overrideAttrs (oldAttrs: {
    # This patchPhase runs during the package's build time.
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

  # >>> PIN FOR COMPATIBILITY >>>
  minSdkVersion = "21";
  ndkVersion = "27.0.12077973";
  kotlinVersion = "2.0.21";
  agpVersion = "8.12.3"; # Android Gradle Plugin

  #########################################################################################################

  coreShell = pkgs.callPackage (import ../../core/shell.nix) { };
in
(pkgs.buildFHSEnv {
  name = "FHS flutter-android-dev-env";

  targetPkgs =
    pkgs:
    coreShell.nativeBuildInputs
    ++ (with pkgs; [
      # Basic development tools for the shell
      bashInteractive
      git
      cmake
      ninja
      python3
      jdk17
      nix-ld
      gradle
      patchedFlutter
      wrappedEmulator

      # Android SDK components and environment
      androidEnv

      # Core runtime libraries
      glibc
      zlib
      ncurses5
      stdenv.cc.cc.lib

      # Qt 5 & 6 and X11 GUI dependencies
      libsForQt5.qt5.qtbase
      libsForQt5.qt5.qtsvg
      libsForQt5.qt5.qtwayland
      libsForQt5.qt5.qttools
      libsForQt5.qt5.qtdeclarative
      qt6.qt5compat
      libsForQt5.qt5ct
      qt6.qtbase
      qt6.qtsvg
      qt6.qtwayland
      qt6.qt5compat
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

      # Graphics and font rendering
      mesa
      libdrm
      vulkan-loader
      fontconfig
      freetype
      mesa-demos
      # XXX: nonfree linuxPackages.nvidia_x11
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

      # Additional GUI/Qt dependencies for emulator button functionality
      gtk3
      gdk-pixbuf
      cairo
      pango
      harfbuzz
      glib
      gsettings-desktop-schemas

      # Critical X11 and cursor dependencies for Qt xcb platform
      xcb-util-cursor
      xorg.libXcursor
      xorg.setxkbmap
      xorg.xauth
      xorg.xhost
      xorg.xset
    ]);

  multiPkgs =
    pkgs: with pkgs; [
      zlib
      ncurses5
      mesa
    ];

  profile = ''
    	  echo "FHS shell is active. Setting up Flutter+Android environment..."
    	  #  Critical nix-ld environment variables for dynamic linking compatibility
    	  export NIX_LD_LIBRARY_PATH="${
         pkgs.lib.makeLibraryPath [
           # Core runtime libraries
           pkgs.glibc
           pkgs.zlib
           pkgs.ncurses5
           pkgs.stdenv.cc.cc.lib

           # Qt 5/6 and X11 GUI dependencies
           pkgs.libsForQt5.qt5.qtbase
           pkgs.libsForQt5.qt5.qtsvg
           pkgs.libsForQt5.qt5.qtwayland
           pkgs.libsForQt5.qt5.qttools
           pkgs.libsForQt5.qt5.qtdeclarative
           pkgs.qt6.qt5compat
           pkgs.libsForQt5.qt5ct
           pkgs.qt6.qtbase
           pkgs.qt6.qtsvg
           pkgs.qt6.qtwayland
           pkgs.qt6.qt5compat
           pkgs.xorg.libX11
           pkgs.xorg.libXext
           pkgs.xorg.libXfixes
           pkgs.xorg.libXi
           pkgs.xorg.libXrandr
           pkgs.xorg.libXrender
           pkgs.xorg.libxcb
           pkgs.xorg.xcbutil
           pkgs.xorg.xcbutilwm
           pkgs.xorg.xcbutilimage
           pkgs.xorg.xcbutilkeysyms
           pkgs.xorg.xcbutilrenderutil
           pkgs.libxkbcommon

           # Graphics and font rendering
           pkgs.mesa
           pkgs.libdrm
           pkgs.vulkan-loader
           pkgs.libglvnd
           # XXX: nonfree pkgs.linuxPackages.nvidia_x11
           pkgs.fontconfig
           pkgs.freetype

           # System services and input handling
           pkgs.dbus
           pkgs.libpulseaudio
           pkgs.pipewire
           pkgs.udev
           pkgs.libinput
           pkgs.libevdev
           pkgs.libinput-gestures
           pkgs.at-spi2-atk
           pkgs.at-spi2-core

           # Additional GUI dependencies
           pkgs.gtk3
           pkgs.gdk-pixbuf
           pkgs.cairo
           pkgs.pango
           pkgs.harfbuzz
           pkgs.glib
           pkgs.gsettings-desktop-schemas

           # Critical X11 and cursor dependencies for Qt xcb platform
           pkgs.xcb-util-cursor
           pkgs.xorg.libXcursor
           pkgs.xorg.setxkbmap
           pkgs.xorg.xauth
           pkgs.xorg.xhost
           pkgs.xorg.xset
         ]
       }"

    	  export LD_LIBRARY_PATH="$NIX_LD_LIBRARY_PATH:$LD_LIBRARY_PATH"

    	  echo "✅ FHS /usr/lib graphics mock initialized at $FHS_LIB/usr/lib"

    	  # Fast path for subsequent shell entries
    	  if [ -f "$PWD/.flutter_env_ready" ] && [ -d "$PWD/.android/sdk" ]; then
    	    export ANDROID_HOME="$PWD/.android/sdk"
    	    export ANDROID_SDK_ROOT="$ANDROID_HOME"
    	    export JAVA_HOME="${pkgs.jdk17}"
    	    export PATH="${pkgs.cmake}/bin:${pkgs.ninja}/bin:$PATH"
    	    # Prepend to LD_LIBRARY_PATH so emulator sees these first
    	    export LD_LIBRARY_PATH="$FHS_LIB/usr/lib:$LD_LIBRARY_PATH"
                echo "✅ FHS graphics symlinks initialized at $FHS_LIB"
    	    echo "⚡ Fast shell entry - Flutter environment ready!"
    	    echo "👉 To launch the emulator:"
    	    echo "          run-emulator"
    	    echo "👉 To launch emulator with debug output:"
    	    echo "          run-emulator-debug"
    	    echo "👉 To launch emulator headless (no GUI):"
    	    echo "          run-emulator-headless"
    	    echo "👉 To build your app, run: flutter build apk --release"
    	  else
    	    # Full setup (first time or missing setup)
    	    echo "Performing full environment setup..."

    	    echo "Stopping any existing ADB server..."
    	    "${androidEnv}/share/android-sdk/platform-tools/adb" kill-server &> /dev/null || true

    	    mkdir -p "$PWD/.android/sdk"
    	    export ANDROID_HOME="$PWD/.android/sdk"
    	    export ANDROID_SDK_ROOT="$ANDROID_HOME"
    	    export JAVA_HOME="${pkgs.jdk17}"

    	    # Verify Gradle + Java setup
    	    gradle --version

    	    echo "🔧 Using Java:"
    	    "$JAVA_HOME/bin/java" -version

    	    # Ensure all SDK directories exist
    	    mkdir -p "$ANDROID_HOME/licenses" "$ANDROID_HOME/avd" "$ANDROID_HOME/bin"

    	    # Copy over SDK parts including system-images now in androidEnv
    	    cp -LR ${androidEnv}/share/android-sdk/* "$ANDROID_HOME/" || true

    	    # Copy essential binaries
    	    for bin in adb avdmanager emulator sdkmanager; do
    	      cp -LR ${androidEnv}/bin/$bin "$ANDROID_HOME/bin/" || true
    	    done
    	    rm -rf "$ANDROID_HOME/cmake"

    	    # Create the cmake directory structure that Gradle expects
    	    mkdir -p "$ANDROID_HOME/cmake/3.22.1/bin"

    	    # Create symlinks to our Nix cmake and ninja
    	    ln -sf "$(which cmake)" "$ANDROID_HOME/cmake/3.22.1/bin/cmake"
    	    ln -sf "$(which ninja)" "$ANDROID_HOME/cmake/3.22.1/bin/ninja"

    	    echo "Created cmake symlink: $ANDROID_HOME/cmake/3.22.1/bin/cmake -> $(which cmake)"

    	    # # ---- FHS-style /usr/lib mock for emulator ----
    	    # FHS_LIB="$HOME/.fhs-emulator-libs"
    	    # mkdir -p "$FHS_LIB/usr/lib/dri"
    	    #
    	    # # Core Mesa / OpenGL
    	    # ln -sf ${pkgs.mesa}/lib/libGL.so.1         "$FHS_LIB/usr/lib/libGL.so.1"
    	    # ln -sf ${pkgs.mesa}/lib/libEGL.so.1        "$FHS_LIB/usr/lib/libEGL.so.1"
    	    #    ln -sf ${pkgs.mesa}/lib/libGLESv2.so.2     "$FHS_LIB/usr/lib/libGLESv2.so.2"
    	    # ln -sf ${pkgs.mesa}/lib/libGLX.so.0        "$FHS_LIB/usr/lib/libGLX.so.0"
    	    # ln -sf ${pkgs.mesa}/lib/libOSMesa.so.8     "$FHS_LIB/usr/lib/libOSMesa.so.8"
    	    #
    	    # # GLX / X11 and EGL drivers
    	    # ln -sf ${pkgs.mesa}/lib/dri/*              "$FHS_LIB/usr/lib/dri/"
    	    # ln -sf ${pkgs.libglvnd}/lib/libGLdispatch.so "$FHS_LIB/usr/lib/libGLdispatch.so"
    	    # ln -sf ${pkgs.libglvnd}/lib/libGLX.so.0      "$FHS_LIB/usr/lib/libGLX.so.0"
    	    # ln -sf ${pkgs.libglvnd}/lib/libEGL.so.1      "$FHS_LIB/usr/lib/libEGL.so.1"
    	    # ln -sf ${pkgs.libglvnd}/lib/libGL.so.1       "$FHS_LIB/usr/lib/libGL.so.1"
    	    #
    	    # # Vulkan loader
    	    # ln -sf ${pkgs.vulkan-loader}/lib/libvulkan.so "$FHS_LIB/usr/lib/libvulkan.so"
    	    #
    	    # # Optional: drivers Mesa expects
    	    # ln -sf ${pkgs.mesa}/lib/dri/*              "$FHS_LIB/usr/lib/"
    	    #
    	    # ln -sf ${pkgs.qt6.qtbase}/lib/libQt6Gui.so* "$FHS_LIB/usr/lib/"
    	    # ln -sf ${pkgs.qt6.qtsvg}/lib/libQt6Svg.so* "$FHS_LIB/usr/lib/"
    	    # ln -sf ${pkgs.qt6.qtwayland}/lib/libQt6WaylandClient.so* "$FHS_LIB/usr/lib/"
    	    # ln -sf ${pkgs.qt6.qt5compat}/lib/libQt6CompatWidgets.so* "$FHS_LIB/usr/lib/"
    	    #
    	    #
    	    # # Prepend to LD_LIBRARY_PATH so emulator sees these first
    	    # export LD_LIBRARY_PATH="$FHS_LIB/usr/lib:$LD_LIBRARY_PATH"
    	    #        echo "✅ FHS graphics symlinks initialized at $FHS_LIB"

    	    chmod -R u+w "$ANDROID_HOME"
    	    find "$ANDROID_HOME/bin" "$ANDROID_HOME/platform-tools" "$ANDROID_HOME/emulator" \
    		 "$ANDROID_HOME/cmdline-tools/latest/bin" "$ANDROID_HOME/build-tools" \
    		 "$ANDROID_HOME/platforms" "$ANDROID_HOME/ndk" -type f -exec chmod +x {} \;

    	    # Accept licenses
    	    for license in android-sdk-license android-sdk-preview-license googletv-license; do
    	      touch "$ANDROID_HOME/licenses/$license"
    	    done
    	    yes | flutter doctor --android-licenses || true

    	    flutter config --android-sdk "$ANDROID_HOME"

    	    # Create flutter project in root directory if one doesnt exist.
    	    if [ ! -f pubspec.yaml ]; then
    	      echo "No Flutter project found. Creating a new one..."
    	      flutter create .
    	      echo ".android/sdk" >> .gitignore
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
    	      echo "android.cmake.path=${pkgs.cmake}/bin" >> android/gradle.properties
    	      echo "android.ninja.path=${pkgs.ninja}/bin" >> android/gradle.properties
    	      echo "android.cmake.version=" >> android/gradle.properties

    	      # ALSO ADD CMAKE_MAKE_PROGRAM override
    	      echo "android.cmake.makeProgram=${pkgs.ninja}/bin/ninja" >> android/gradle.properties
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

    	    #Support for traditional Groovy build files 
    	    if [ -f "android/build.gradle" ]; then
    	      echo "⚙️ Pinning Android build tool versions in Groovy DSL..."
    	      sed -i -e "s/com.android.application.*version.*'[0-9.]*'/com.android.application' version '${agpVersion}'/g" android/build.gradle
    	      sed -i -e "s/org.jetbrains.kotlin.android.*version.*'[0-9.]*'/org.jetbrains.kotlin.android' version '${kotlinVersion}'/g" android/build.gradle
    	    fi

    	    if [ -f "android/app/build.gradle" ]; then
    	      sed -i -e "s/minSdkVersion [0-9]*/minSdkVersion ${minSdkVersion}/g" android/app/build.gradle
    	    fi

    	    # Create AVD if missing
    	    if ! avdmanager list avd | grep -q 'android_emulator'; then
    	      echo "Creating default AVD: android_emulator"
    	      yes | avdmanager create avd \
    		--name "android_emulator" \
    		--package "system-images;android-36;google_apis_playstore;x86_64" \
    		--device "pixel" \
    		--abi "x86_64" \
    		--tag "google_apis_playstore" \
    		--force
    	    fi

    	    # PATH and tool verification
    	    export PATH="${pkgs.cmake}/bin:${pkgs.ninja}/bin:$PATH"

    	    # Verify our tools are accessible
    	    echo "🔧 Using CMake: $(which cmake) ($(cmake --version | head -1))"
    	    echo "🔧 Using Ninja: $(which ninja) ($(ninja --version))"

    	    flutter doctor --quiet
    	    echo "✅ Flutter + Android dev shell ready."

    	    # Mark environment as ready for fast path next time
    	    touch "$PWD/.flutter_env_ready"
    	    echo ".flutter_env_ready" >> .gitignore

    	    echo "👉 To launch the emulator, run:"
    	    echo "    run-emulator"
    	    echo "👉 To launch emulator with debug output:"
    	    echo "    run-emulator-debug"
    	    echo "👉 To launch emulator headless (no GUI):"
    	    echo "    run-emulator-headless"
    	    
    	    echo ""
    	    echo "👉 To build your app, run:"
    	    echo "   flutter build apk --release"
    	  fi

        # XXX: needed to build nobodywho
        export LIBCLANG_PATH="${coreShell.LIBCLANG_PATH}"
        export ANDROID_NDK="$ANDROID_HOME/ndk/${ndkVersion}"
    	'';
  runScript = "bash";
}).env

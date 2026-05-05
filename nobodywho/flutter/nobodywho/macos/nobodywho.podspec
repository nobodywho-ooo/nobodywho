framework_name = "nobodywho_flutter.xcframework"
stt_framework_name = "nobodywho_stt.xcframework"

# Resolve xcframework using Dart script
# This supports multiple resolution strategies:
# 1. Environment variable override (NOBODYWHO_FLUTTER_XCFRAMEWORK_PATH)
# 2. Local cargo build detection
# 3. Cached download
# 4. Download from GitHub releases

script_path = File.join(__dir__, '..', 'tool', 'resolve_binary.dart')
cache_dir = File.join(__dir__, '..', '.dart_tool', 'nobodywho_cache')

# Run the Dart script to resolve the xcframework path
resolve_output = `dart run "#{script_path}" --platform=macos --build-type=release --cache-dir="#{cache_dir}" 2>&1`
resolve_status = $?.exitstatus

if resolve_status != 0
  raise "Error: Failed to resolve NobodyWho xcframework for macOS:\n#{resolve_output}\n" \
        "You can manually set NOBODYWHO_FLUTTER_XCFRAMEWORK_PATH to point to your xcframework."
end

# The script outputs the path to stdout (last line), with status messages to stderr
xcframework_path = resolve_output.strip.split("\n").last

unless File.exist?(xcframework_path)
  raise "Error: Resolved xcframework path does not exist: #{xcframework_path}"
end

# Copy the framework to local Frameworks directory
frameworks_dir = File.join(__dir__, 'Frameworks')
`
mkdir -p "#{frameworks_dir}"
cd "#{frameworks_dir}"
if [ -d "#{framework_name}" ]
then
  echo "Found existing framework. Removing..."
  rm -rf "#{framework_name}"
fi
echo "Copying framework from #{xcframework_path}..."
# IMPORTANT: Use -R not -r. On macOS, cp -r follows/dereferences symlinks,
# which breaks the versioned framework bundle structure required for code signing.
cp -R "#{xcframework_path}" "./#{framework_name}"
`

# Resolve the optional STT xcframework. STT is a separately-loaded module: it ships
# whisper.cpp + its own bundled ggml inside an embedded framework that the host app
# dlopens at runtime. Two-level namespacing keeps whisper's ggml from colliding with
# llama's ggml inside the main nobodywho_flutter framework.
#
# If the user hasn't built / downloaded the STT framework, the pod still installs —
# nobodywho returns SpeechToTextError::ModuleLoad at runtime when STT is invoked.
# Same Dart resolver as the main framework, just with --component=stt --optional. The
# --optional flag turns "asset not in this release" into a clean exit-0-with-empty-stdout
# so the pod still installs against older releases.
stt_resolve_output = `dart run "#{script_path}" --platform=macos --build-type=release --cache-dir="#{cache_dir}" --component=stt --optional 2>/dev/null`
stt_xcframework_path = stt_resolve_output.strip.split("\n").last

stt_vendored = nil
if stt_xcframework_path && !stt_xcframework_path.empty? && File.exist?(stt_xcframework_path)
  `
  cd "#{frameworks_dir}"
  if [ -d "#{stt_framework_name}" ]; then rm -rf "#{stt_framework_name}"; fi
  echo "Copying STT framework from #{stt_xcframework_path}..."
  cp -R "#{stt_xcframework_path}" "./#{stt_framework_name}"
  `
  stt_vendored = "Frameworks/#{stt_framework_name}"
else
  Pod::UI.warn "NobodyWho: stt xcframework not found, STT will be unavailable at runtime. " \
    "Build via flutter/scripts/build_stt_apple.sh or set NOBODYWHO_STT_XCFRAMEWORK_PATH."
end

Pod::Spec.new do |s|
  s.name             = 'nobodywho'
  s.version          = '0.1.0'
  s.summary          = 'Flutter FFI plugin for NobodyWho - local LLM inference'
  s.description      = <<-DESC
Flutter FFI plugin for NobodyWho - local LLM inference with tool calling, embeddings, and cross-encoding
                       DESC
  s.homepage         = 'https://nobodywho.ooo'
  s.license          = { :file => '../LICENSE' }
  s.author           = { 'Your Company' => 'email@example.com' }

  s.source           = { :path => '.' }
  s.libraries = 'c++'
  s.frameworks = 'Accelerate'

  # If your plugin requires a privacy manifest, for example if it collects user
  # data, update the PrivacyInfo.xcprivacy file to describe your plugin's
  # privacy impact, and then uncomment this line. For more information,
  # see https://developer.apple.com/documentation/bundleresources/privacy_manifest_files
  # s.resource_bundles = {'nobodywho_privacy' => ['Resources/PrivacyInfo.xcprivacy']}

  s.dependency 'FlutterMacOS'

  s.platform = :osx
  s.pod_target_xcconfig = { 'DEFINES_MODULE' => 'YES' }

  # this is where we include the pre-compiled nobodywho code
  if stt_vendored
    s.vendored_frameworks = ["Frameworks/#{framework_name}", stt_vendored]
  else
    s.vendored_frameworks = "Frameworks/#{framework_name}"
  end
end

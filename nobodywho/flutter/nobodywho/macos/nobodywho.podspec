framework_name = "NobodyWhoFlutter.xcframework"

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
cp -r "#{xcframework_path}" "./#{framework_name}"
`

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

  # This will ensure the source files in Classes/ are included in the native
  # builds of apps using this FFI plugin. Podspec does not support relative
  # paths, so Classes contains a forwarder C file that relatively imports
  # `../src/*` so that the C sources can be shared among all target platforms.
  s.source           = { :path => '.' }
  s.source_files = 'Classes/**/*'
  s.public_header_files = 'Classes/**/*.h'
  s.libraries = 'c++'
  s.frameworks = 'Accelerate'

  # If your plugin requires a privacy manifest, for example if it collects user
  # data, update the PrivacyInfo.xcprivacy file to describe your plugin's
  # privacy impact, and then uncomment this line. For more information,
  # see https://developer.apple.com/documentation/bundleresources/privacy_manifest_files
  # s.resource_bundles = {'nobodywho_privacy' => ['Resources/PrivacyInfo.xcprivacy']}

  s.dependency 'FlutterMacOS'

  s.platform = :osx, '15.5'
  s.pod_target_xcconfig = { 'DEFINES_MODULE' => 'YES' }
  s.swift_version = '5.0'

  # this is where we include the pre-compiled nobodywho code
  s.vendored_frameworks = "Frameworks/#{framework_name}"
end



release_tag_name = "nobodywho_flutter-v0.0.0"
framework_name="NobodyWhoFlutter.xcframework"


# Get the framework path from environment variable
xcframework_path = ENV['NOBODYWHO_FLUTTER_XCFRAMEWORK']

# Validate environment variable is set
if xcframework_path.nil? || xcframework_path.empty?
  raise "Error: NOBODYWHO_FLUTTER_XCFRAMEWORK environment variable is not set. " \
        "Please set it to the path of your xcframework file."
end

# Validate the framework exists
unless File.exist?(xcframework_path)
  raise "Error: Framework not found at path: #{xcframework_path}. " \
        "Please ensure NOBODYWHO_FLUTTER_XCFRAMEWORK points to a valid xcframework file."
end

# Copy the framework to local Frameworks directory
`
cd Frameworks
if [ -d #{framework_name} ]
then
  echo "Found existing framework. Removing..."
  rm -rf #{framework_name}
fi
echo "Copying framework from #{xcframework_path}..."
cp -r #{xcframework_path} ./#{framework_name}
`

Pod::Spec.new do |s|
  s.name             = 'nobodywho_flutter'
  s.version          = '0.0.1'
  s.summary          = 'A new Flutter FFI plugin project.'
  s.description      = <<-DESC
A new Flutter FFI plugin project.
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
  # s.resource_bundles = {'nobodywho_flutter_privacy' => ['Resources/PrivacyInfo.xcprivacy']}

  s.dependency 'Flutter'

  s.platform = :ios, '13.0'
  s.pod_target_xcconfig = {
    'DEFINES_MODULE' => 'YES',
    'EXCLUDED_ARCHS[sdk=iphonesimulator*]' => 'i386',
    'SWIFT_INCLUDE_PATHS' => '$(PODS_TARGET_SRCROOT)/Classes'
  }
  s.swift_version = '5.0'

  # Ensure the header is available to Swift
  s.preserve_paths = 'Classes/binding.h'

  # this is where we include the pre-compiled nobodywho code
  s.vendored_frameworks = "Frameworks/#{framework_name}"
end

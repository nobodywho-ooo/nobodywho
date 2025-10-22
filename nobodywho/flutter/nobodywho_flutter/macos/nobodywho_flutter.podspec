

release_tag_name = "nobodywho_flutter-v0.0.0"
framework_name="NobodyWhoFlutter.xcframework"

`
cd Frameworks
if [ -f #{framework_name} ]
then
  echo "Found existing framework. Removing..."
  rm -rf #{framework_name}
fi
cp -r #{ENV['NOBODYWHO_FLUTTER_XCFRAMEWORK']} ./#{framework_name}
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

  # If your plugin requires a privacy manifest, for example if it collects user
  # data, update the PrivacyInfo.xcprivacy file to describe your plugin's
  # privacy impact, and then uncomment this line. For more information,
  # see https://developer.apple.com/documentation/bundleresources/privacy_manifest_files
  # s.resource_bundles = {'nobodywho_flutter_privacy' => ['Resources/PrivacyInfo.xcprivacy']}

  s.dependency 'FlutterMacOS'

  s.platform = :osx, '10.11'
  s.pod_target_xcconfig = { 'DEFINES_MODULE' => 'YES' }
  s.swift_version = '5.0'

  # this is where we include the pre-compiled nobodywho code
  s.vendored_frameworks = "Frameworks/#{framework_name}"
end

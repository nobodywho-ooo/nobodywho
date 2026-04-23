# NobodyWho React Native - iOS podspec
# Downloads the prebuilt xcframework from GitHub Releases at pod install time.
require "json"

package = JSON.parse(File.read(File.join(__dir__, "package.json")))
version = package["version"]
folly_compiler_flags = '-DFOLLY_NO_CONFIG -DFOLLY_MOBILE=1 -DFOLLY_USE_LIBCPP=1 -Wno-comma -Wno-shorten-64-to-32'

framework_name = "NobodywhoFramework.xcframework"
framework_dir = File.join(__dir__, framework_name)

# Download the xcframework from GitHub Releases if not already present
unless File.exist?(framework_dir)
  zip_name = "#{framework_name}.zip"
  zip_path = File.join(__dir__, zip_name)
  url = "https://github.com/nobodywho-ooo/nobodywho/releases/download/nobodywho-react-native-#{version}/#{zip_name}"

  puts "[NobodyWho] Downloading xcframework from #{url}"

  # Download using curl (available on all macOS systems)
  system("curl", "-L", "-f", "-o", zip_path, url) or
    raise "Failed to download NobodyWho xcframework.\n" \
          "URL: #{url}\n" \
          "Check that the release exists: https://github.com/nobodywho-ooo/nobodywho/releases/tag/nobodywho-react-native-#{version}\n" \
          "For local development, manually place the xcframework at: #{framework_dir}"

  puts "[NobodyWho] Extracting xcframework..."
  system("unzip", "-o", "-q", zip_path, "-d", __dir__) or
    raise "Failed to extract #{zip_name}"

  File.delete(zip_path) if File.exist?(zip_path)

  unless File.exist?(framework_dir)
    raise "xcframework not found after extraction: #{framework_dir}"
  end

  puts "[NobodyWho] xcframework ready at #{framework_dir}"
end

Pod::Spec.new do |s|
  s.name         = "Nobodywho"
  s.version      = package["version"]
  s.summary      = package["description"]
  s.homepage     = package["homepage"]
  s.license      = package["license"]
  s.authors      = package["author"] || { "NobodyWho" => "info@nobodywho.ooo" }

  s.platforms    = { :ios => min_ios_version_supported }
  s.source       = { :git => "https://github.com/nobodywho-ooo/nobodywho.git", :tag => "#{s.version}" }

  s.source_files = "ios/**/*.{h,m,mm,swift}", "ios/generated/**/*.{h,m,mm}", "cpp/**/*.{hpp,cpp,c,h}", "generated/cpp/**/*.{hpp,cpp,c,h}"
  s.vendored_frameworks = framework_name
  s.libraries = 'c++'
  s.frameworks = 'Accelerate'
  s.dependency    "uniffi-bindgen-react-native", "0.30.0-1"

  # Use install_modules_dependencies helper to install the dependencies if React Native version >=0.71.0.
  # See https://github.com/facebook/react-native/blob/febf6b7f33fdb4904669f99d795eba4c0f95d7bf/scripts/cocoapods/new_architecture.rb#L79.
  if respond_to?(:install_modules_dependencies, true)
    install_modules_dependencies(s)
  else
    s.dependency "React-Core"

    # Don't install the dependencies when we run `pod install` in the old architecture.
    if ENV['RCT_NEW_ARCH_ENABLED'] == '1' then
      s.compiler_flags = folly_compiler_flags + " -DRCT_NEW_ARCH_ENABLED=1"
      s.dependency "React-Codegen"
      s.dependency "RCT-Folly"
      s.dependency "RCTRequired"
      s.dependency "RCTTypeSafety"
      s.dependency "ReactCommon/turbomodule/core"
    end
  end

  # Only arm64 simulator is supported (no x86_64 simulator build)
  s.pod_target_xcconfig = {
    'EXCLUDED_ARCHS[sdk=iphonesimulator*]' => 'i386 x86_64'
  }
  s.user_target_xcconfig = {
    'EXCLUDED_ARCHS[sdk=iphonesimulator*]' => 'i386 x86_64'
  }
end

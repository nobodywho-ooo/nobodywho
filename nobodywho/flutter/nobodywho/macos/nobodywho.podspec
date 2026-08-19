require 'fileutils'

framework_name = "nobodywho_flutter.xcframework"
script_path = File.join(__dir__, '..', 'tool', 'resolve_binary.dart')
cache_dir = File.join(__dir__, '..', '.dart_tool', 'nobodywho_cache')
command = ['dart', 'run', script_path, '--platform=macos', '--build-type=release', "--cache-dir=#{cache_dir}"]
xcframework_path = IO.popen(command, &:read).strip
raise 'Failed to resolve NobodyWho xcframework' unless $?.success? && File.directory?(xcframework_path)

frameworks_dir = File.join(__dir__, 'Frameworks')
destination = File.join(frameworks_dir, framework_name)
FileUtils.rm_rf(destination)
FileUtils.mkdir_p(frameworks_dir)
FileUtils.cp_r(xcframework_path, destination, preserve: true)

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

  s.dependency 'FlutterMacOS'

  s.platform = :osx
  s.pod_target_xcconfig = { 'DEFINES_MODULE' => 'YES' }

  s.vendored_frameworks = "Frameworks/#{framework_name}"
end

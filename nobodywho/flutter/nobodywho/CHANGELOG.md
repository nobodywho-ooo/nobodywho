## 0.3.2-rc2

* Add config to exclude x86_64 and i386 ios simulators to the ios podspec

## 0.3.2-rc1

* Change MacOS and iOS podspec files to copy .xcframework with -R, to preserve symlinks

## 0.3.1

* Change MacOS and iOS releases to use dynamic linking

## 0.3.0

* Add support for tool parameters with composite types (e.g. List<List<int>>)
* Fix CI/CD for targets that depend on the XCFramework files (MacOS + iOS)

## 0.2.0

* Add option to provide descriptions for individual parameters in Tool constructor.
* Remove slow trigger_word grammar triggers, significantly speeding up generation of long messages when tools are present
* Default to add_bos=true if GGUF file does not specify

## 0.1.1

* Set up automated publishing from CI

## 0.1.0

* Initial release!

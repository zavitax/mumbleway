#
# To learn more about a Podspec see http://guides.cocoapods.org/syntax/podspec.html.
# Run `pod lib lint tflite_flutter.podspec` to validate before publishing.
#
Pod::Spec.new do |s|
  s.name             = 'tflite_flutter'
  s.version          = '0.0.1'
  s.summary          = 'A new Flutter plugin project.'
  s.description      = <<-DESC
A new Flutter plugin project.
                       DESC
  s.homepage         = 'http://example.com'
  s.license          = { :file => '../LICENSE' }
  s.author           = { 'Your Company' => 'email@example.com' }

  s.source           = { :path => '.' }
  #s.source           = { :http => 'https://github.com/CaptainDario/DaKanji-Dependencies/releases/download/v3.0.0/libtensorflowlite_c-mac.dylib.zip' }
  s.source_files     = 'Classes/**/*'
  s.dependency 'FlutterMacOS'

  s.platform = :osx, '10.11'
  s.pod_target_xcconfig = { 'DEFINES_MODULE' => 'YES' }
  s.swift_version = '5.0'

  # MumbleWay: uncommented. This is what puts the dylib in
  # Contents/Frameworks and has Xcode sign it with the app's identity, which
  # is both what Apple requires and where the patched loader looks.
  #
  # **The filename must match the dylib's own install name.** Vendoring it also
  # links it, so the app binary records `LC_LOAD_DYLIB @rpath/
  # libtensorflowlite_c.dylib` -- taken from this file's `LC_ID_DYLIB` -- while
  # CocoaPods copies the file into Frameworks under whatever it is called here.
  # Shipped as `libtensorflowlite_c-mac.dylib`, those two disagree, and every
  # macOS build died in dyld before `main`:
  #
  #     Library not loaded: @rpath/libtensorflowlite_c.dylib
  #     tried: '/Applications/mumbleway.app/Contents/Frameworks/
  #            libtensorflowlite_c.dylib' (no such file)
  #
  # Renaming the file is the fix rather than `install_name_tool`, because then
  # one name is true everywhere: here, in the loader, and in `bindings.dart`.
  s.vendored_libraries = 'libtensorflowlite_c.dylib'
end

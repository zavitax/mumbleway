#
# To learn more about a Podspec see http://guides.cocoapods.org/syntax/podspec.html.
# Run `pod lib lint rust_lib_mumbleway.podspec` to validate before publishing.
#
Pod::Spec.new do |s|
  s.name             = 'rust_lib_mumbleway'
  s.version          = '0.0.1'
  s.summary          = 'A new Flutter FFI plugin project.'
  s.description      = <<-DESC
A new Flutter FFI plugin project.
                       DESC
  s.homepage         = 'http://example.com'
  s.license          = { :file => '../LICENSE' }
  s.author           = { 'Your Company' => 'email@example.com' }

  # This will ensure the source files in Classes/ are included in the native
  # builds of apps using this FFI plugin. Podspec does not support relative
  # paths, so Classes contains a forwarder C file that relatively imports
  # `../src/*` so that the C sources can be shared among all target platforms.
  s.source           = { :path => '.' }
  s.source_files     = 'Classes/**/*'
  s.dependency 'FlutterMacOS'

  # MumbleWay: cpal's CoreAudio backend emits `cargo:rustc-link-lib=framework=`
  # directives, but those do not survive into a *static* library -- Xcode links
  # the .a and knows nothing about them, so the app failed with undefined
  # _AudioUnitRender / _AudioUnitSetProperty / _AudioUnitUninitialize. The
  # consuming target has to link the frameworks itself.
  s.frameworks = 'AudioToolbox', 'AudioUnit', 'CoreAudio', 'CoreFoundation'

  # Matches MACOSX_DEPLOYMENT_TARGET in Runner.xcodeproj.
  s.platform = :osx, '10.15'
  s.pod_target_xcconfig = { 'DEFINES_MODULE' => 'YES' }
  s.swift_version = '5.0'

  s.script_phase = {
    :name => 'Build Rust library',
    # First argument is relative path to the `rust` folder, second is name of rust library
    # See the iOS podspec for why this cannot be left to /.cargo/config.toml:
    # cargo reads config relative to the working directory, and Xcode's script
    # phase does not run inside the repository. macOS happens to build without
    # it today, which makes it the more dangerous of the two -- it would break
    # on the first machine where pkg-config stops finding a system Opus.
    :script => 'export CMAKE_POLICY_VERSION_MINIMUM=3.5; sh "$PODS_TARGET_SRCROOT/../cargokit/build_pod.sh" ../../rust rust_lib_mumbleway',
    :execution_position => :before_compile,
    :input_files => ['${BUILT_PRODUCTS_DIR}/cargokit_phony'],
    # Let XCode know that the static library referenced in -force_load below is
    # created by this build step.
    :output_files => ["${BUILT_PRODUCTS_DIR}/librust_lib_mumbleway.a"],
  }
  s.pod_target_xcconfig = {
    'DEFINES_MODULE' => 'YES',
    # Flutter.framework does not contain a i386 slice.
    'EXCLUDED_ARCHS[sdk=iphonesimulator*]' => 'i386',
    'OTHER_LDFLAGS' => '-force_load ${BUILT_PRODUCTS_DIR}/librust_lib_mumbleway.a',
  }
end
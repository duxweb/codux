#
# To learn more about a Podspec see http://guides.cocoapods.org/syntax/podspec.html.
# Run `pod lib lint codux_protocol_ffi.podspec` to validate before publishing.
#
Pod::Spec.new do |s|
  s.name             = 'codux_protocol_ffi'
  s.version          = '1.8.0'
  s.summary          = 'Rust-backed Codux remote protocol bindings.'
  s.description      = <<-DESC
Codux remote protocol FFI bridge backed by the shared Rust protocol crate.
                       DESC
  s.homepage         = 'https://codux.dev'
  s.license          = { :file => '../LICENSE' }
  s.author           = { 'Codux' => 'support@codux.dev' }

  s.source           = { :path => '.' }
  s.source_files = 'Classes/**/*'
  s.preserve_paths = '../scripts/**/*', 'Frameworks/**/*'
  s.dependency 'Flutter'
  s.platform = :ios, '13.0'
  s.script_phase = {
    :name => 'Build Codux Protocol Rust FFI',
    :script => 'bash "$PODS_TARGET_SRCROOT/../scripts/build-apple.sh"',
    :execution_position => :before_compile
  }

  # Flutter.framework does not contain a i386 slice.
  s.pod_target_xcconfig = {
    'DEFINES_MODULE' => 'YES',
    'EXCLUDED_ARCHS[sdk=iphonesimulator*]' => 'i386',
    'LIBRARY_SEARCH_PATHS' => '$(inherited) "${PODS_TARGET_SRCROOT}/Frameworks"'
  }
  s.user_target_xcconfig = {
    # Weak-link Network.framework so the iOS 26-only nw_path_is_ultra_constrained
    # resolves to NULL on iOS 16 (guarded in netdev) instead of a launch failure.
    'OTHER_LDFLAGS' => '$(inherited) -weak_framework Network -force_load "${PODS_ROOT}/../.symlinks/plugins/codux_protocol_ffi/ios/Frameworks/libcodux_protocol_ffi.a"'
  }
  s.swift_version = '5.0'
end

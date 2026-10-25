Pod::Spec.new do |s|
  s.name           = 'CelmicAudio'
  s.version        = '0.1.0'
  s.summary        = 'Low-latency microphone capture and UDP streaming for Celmic'
  s.description    = 'Captures the microphone with AVAudioEngine and streams PCM to the Celmic desktop app.'
  s.author         = 'Celmic contributors'
  s.homepage       = 'https://github.com/blackfoxsoftware/celmic'
  s.license        = 'MIT'
  s.platforms      = {
    :ios => '16.4'
  }
  s.source         = { git: '' }
  s.swift_version  = '5.9'
  s.static_framework = true

  s.dependency 'ExpoModulesCore'

  # Swift/Objective-C compatibility
  s.pod_target_xcconfig = {
    'DEFINES_MODULE' => 'YES',
  }

  s.source_files = "**/*.{h,m,mm,swift,hpp,cpp}"
end

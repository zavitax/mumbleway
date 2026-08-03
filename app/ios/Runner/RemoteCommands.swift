import AVFoundation
import Flutter
import MediaPlayer

/// Bluetooth remote buttons on iOS.
///
/// A handlebar remote presents as a Bluetooth keyboard, and the app already
/// listens for keys — but the buttons a rider actually uses are not keys. Play,
/// pause and track-skip are HID *consumer* controls, and iOS does not deliver
/// those to applications at all: they go to whichever app owns Now Playing, via
/// the remote command centre. Volume never arrives either; the system takes it.
///
/// So on iOS the learning screen sat waiting for a key event that was never
/// going to come, however many times the button was pressed. Android has no
/// such split — its media session reports the same buttons as key codes — which
/// is why this went unnoticed until a remote met a phone.
///
/// The key codes sent here are Android's on purpose. Dart maps them into its
/// own binding space, so a button learned on one platform means the same thing
/// on the other, and a rider swapping phones keeps their bindings.
final class RemoteCommands {
  private let channel: FlutterMethodChannel
  private var capturing = false

  /// Android `KeyEvent` constants, so both platforms name a button the same.
  private enum Code {
    static let playPause = 85
    static let stop = 86
    static let next = 87
    static let previous = 88
    static let play = 126
    static let pause = 127
  }

  init(messenger: FlutterBinaryMessenger) {
    channel = FlutterMethodChannel(name: "mumbleway/buttons", binaryMessenger: messenger)
    channel.setMethodCallHandler { [weak self] call, result in
      guard let self else {
        result(nil)
        return
      }
      switch call.method {
      case "captureMediaButtons":
        let want = call.arguments as? Bool ?? false
        self.setCapturing(want)
        // Echoed back so the settings screen can say whether the remote is
        // being listened for at all. A silent failure here is indistinguishable
        // from a remote that is not sending anything.
        result(want ? "listening" : "idle")
      default:
        result(FlutterMethodNotImplemented)
      }
    }
  }

  deinit {
    setCapturing(false)
  }

  /// Takes the remote's buttons, or gives them back.
  ///
  /// Not on all the time. Claiming these means the rider's music app stops
  /// answering its own remote, which is the right trade while a button is
  /// bound to push-to-talk and plain rude otherwise. Dart turns it on when
  /// there is a media binding or one is being learned, and off again after.
  private func setCapturing(_ want: Bool) {
    guard want != capturing else { return }
    capturing = want

    let centre = MPRemoteCommandCenter.shared()
    let commands: [(MPRemoteCommand, Int)] = [
      (centre.togglePlayPauseCommand, Code.playPause),
      (centre.playCommand, Code.play),
      (centre.pauseCommand, Code.pause),
      (centre.stopCommand, Code.stop),
      (centre.nextTrackCommand, Code.next),
      (centre.previousTrackCommand, Code.previous),
    ]

    guard want else {
      for (command, _) in commands {
        command.removeTarget(self)
        command.isEnabled = false
      }
      MPNowPlayingInfoCenter.default().nowPlayingInfo = nil
      return
    }

    for (command, code) in commands {
      command.isEnabled = true
      command.addTarget { [weak self] _ in
        self?.send(code)
        return .success
      }
    }

    // Without an entry here the app is not the Now Playing app, and the remote
    // commands above are registered but never called. The content is nominal —
    // nothing is playing in the sense the system means — but it is what makes
    // the buttons arrive.
    MPNowPlayingInfoCenter.default().nowPlayingInfo = [
      MPMediaItemPropertyTitle: "MumbleWay",
      MPNowPlayingInfoPropertyIsLiveStream: true,
      MPNowPlayingInfoPropertyPlaybackRate: 1.0,
    ]
  }

  /// Reports a button as a press followed at once by a release.
  ///
  /// A remote command is a single event: the system says the button was used,
  /// never that it is still down. So hold-to-talk cannot work from one of these
  /// — the release would arrive in the same breath as the press — and the
  /// settings screen says so rather than letting a rider bind it and find out
  /// mid-ride. The pair is still sent, because every binding downstream is
  /// written in terms of press and release.
  private func send(_ code: Int) {
    // A remote command handler is not guaranteed to run on the main thread,
    // and a channel may only be spoken to from there. Called off it, the call
    // does not fail loudly — it simply never arrives, which is exactly what a
    // button that refuses to be learned looks like.
    DispatchQueue.main.async { [weak self] in
      guard let self else { return }
      for pressed in [true, false] {
        self.channel.invokeMethod(
          "mediaButton", arguments: ["keyCode": code, "pressed": pressed])
      }
    }
  }
}

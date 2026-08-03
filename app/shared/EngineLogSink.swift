import Foundation
import os

// The Flutter module is named differently on each platform, and this file is
// built into both targets.
#if canImport(Flutter)
  import Flutter
#elseif canImport(FlutterMacOS)
  import FlutterMacOS
#endif

/// Repeats the engine's log into the system log.
///
/// So that a device plugged into a Mac can be watched live in Console rather
/// than only questioned afterwards through the app's own panel. The two are the
/// same lines; this one is reachable while the app is misbehaving, which is
/// exactly when the panel is the last thing anyone wants to be navigating.
///
/// `os_log` rather than `NSLog`: `NSLog` writes to the same place but is
/// synchronous and timestamps at the point of writing, so a burst of lines both
/// stalls the caller and arrives with the wrong times on it. `os_log` is also
/// the one Console can filter by subsystem, which matters when the alternative
/// is reading every line the whole device emits.
final class EngineLogSink {
  private let channel: FlutterMethodChannel

  /// Categories are made once and kept: each one allocates, and the log is the
  /// wrong place to be allocating per line.
  private static let subsystem = "com.mumbleway.mumbleway"
  private static let logs: [OSLog] = [
    OSLog(subsystem: subsystem, category: "trace"),
    OSLog(subsystem: subsystem, category: "debug"),
    OSLog(subsystem: subsystem, category: "info"),
    OSLog(subsystem: subsystem, category: "warn"),
    OSLog(subsystem: subsystem, category: "error"),
  ]

  init(messenger: FlutterBinaryMessenger) {
    channel = FlutterMethodChannel(name: "mumbleway/log", binaryMessenger: messenger)
    channel.setMethodCallHandler { call, result in
      switch call.method {
      case "write":
        guard let args = call.arguments as? [String: Any],
          let lines = args["lines"] as? [[String: Any]]
        else {
          result(FlutterError(code: "args", message: "write wants a list of lines.", details: nil))
          return
        }
        for line in lines {
          let level = line["level"] as? Int ?? 2
          let message = line["message"] as? String ?? ""
          EngineLogSink.write(level: level, message: message)
        }
        result(nil)
      default:
        result(FlutterMethodNotImplemented)
      }
    }
  }

  private static func write(level: Int, message: String) {
    let log = logs[min(max(level, 0), logs.count - 1)]
    let type: OSLogType
    switch level {
    case 0, 1: type = .debug
    case 3: type = .default  // warnings; .error would colour them as faults
    case 4: type = .error
    default: type = .info
    }
    // `%{public}@` deliberately: without it the system redacts every
    // interpolated string to <private> in release builds, which leaves a log
    // that faithfully records that something happened and refuses to say what.
    // Nothing here carries a password — the engine keeps them out at source.
    os_log("%{public}@", log: log, type: type, message)
  }
}

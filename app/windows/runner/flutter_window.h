#ifndef RUNNER_FLUTTER_WINDOW_H_
#define RUNNER_FLUTTER_WINDOW_H_

#include <flutter/dart_project.h>
#include <flutter/encodable_value.h>
#include <flutter/flutter_view_controller.h>
#include <flutter/method_channel.h>

#include <cstdint>
#include <memory>
#include <unordered_map>
#include <vector>

#include "win32_window.h"

// A window that does nothing but host a Flutter view.
class FlutterWindow : public Win32Window {
 public:
  // Creates a new FlutterWindow hosting a Flutter view running |project|.
  explicit FlutterWindow(const flutter::DartProject& project);
  virtual ~FlutterWindow();

 protected:
  // Win32Window:
  bool OnCreate() override;
  void OnDestroy() override;
  LRESULT MessageHandler(HWND window, UINT const message, WPARAM const wparam,
                         LPARAM const lparam) noexcept override;

 private:
  // Takes or drops the system's promise not to sleep, as calls come and go.
  void SetCallActive(bool active);

  // Re-applies the efficiency request after anything that could change the
  // answer: whether a call is up, and whether anybody is looking at us.
  void ApplyPowerThrottling();

  struct HotkeySpec {
    int64_t key_id;
    int modifiers;
  };

  struct RegisteredHotkey {
    int id;
    UINT vk;
    UINT win_mods;
    int64_t key_id;
    int modifiers;
  };

  // Registers or replaces system-wide hotkeys for the opted-in bindings.
  void SetGlobalHotkeys(const std::vector<HotkeySpec>& hotkeys);
  void UnregisterGlobalHotkeys();
  void InvokeGlobalHotkey(int64_t key_id, int modifiers, bool pressed);
  void PollHotkeyReleases();

  // The project to run.
  flutter::DartProject project_;

  // The Flutter instance hosted by this window.
  std::unique_ptr<flutter::FlutterViewController> flutter_controller_;

  // Whether there is a conversation in progress. Reported from Dart.
  bool call_active_ = false;

  // Whether this app is the one the user is working in. Tracked because an
  // unattended window is the only time it is safe to ask to be run slowly.
  bool foreground_ = true;

  std::unique_ptr<flutter::MethodChannel<flutter::EncodableValue>>
      power_channel_;
  std::unique_ptr<flutter::MethodChannel<flutter::EncodableValue>>
      buttons_channel_;
  std::unordered_map<int, RegisteredHotkey> registered_hotkeys_;
  std::vector<RegisteredHotkey> held_hotkeys_;
  bool hotkey_timer_active_ = false;
};

#endif  // RUNNER_FLUTTER_WINDOW_H_

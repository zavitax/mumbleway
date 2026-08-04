#include "flutter_window.h"

#include <flutter/standard_method_codec.h>
#include <windows.h>

#include <optional>

#include "flutter/generated_plugin_registrant.h"

FlutterWindow::FlutterWindow(const flutter::DartProject& project)
    : project_(project) {}

FlutterWindow::~FlutterWindow() {}

bool FlutterWindow::OnCreate() {
  if (!Win32Window::OnCreate()) {
    return false;
  }

  RECT frame = GetClientArea();

  // The size here must match the window dimensions to avoid unnecessary surface
  // creation / destruction in the startup path.
  flutter_controller_ = std::make_unique<flutter::FlutterViewController>(
      frame.right - frame.left, frame.bottom - frame.top, project_);
  // Ensure that basic setup of the controller was successful.
  if (!flutter_controller_->engine() || !flutter_controller_->view()) {
    return false;
  }
  RegisterPlugins(flutter_controller_->engine());
  SetChildContent(flutter_controller_->view()->GetNativeWindow());

  // Whether there is a conversation worth keeping the machine awake for.
  //
  // Its own channel rather than the overlay's: this platform has no floating
  // window and never will — on a desktop the app is a window among windows,
  // already a click away — but it sleeps like every other machine, and a call
  // that dies because a laptop lid was left open is the same failure a rider
  // gets on a phone in a pocket.
  power_channel_ =
      std::make_unique<flutter::MethodChannel<flutter::EncodableValue>>(
          flutter_controller_->engine()->messenger(), "mumbleway/power",
          &flutter::StandardMethodCodec::GetInstance());
  power_channel_->SetMethodCallHandler(
      [this](const flutter::MethodCall<flutter::EncodableValue>& call,
             std::unique_ptr<flutter::MethodResult<flutter::EncodableValue>>
                 result) {
        if (call.method_name() == "callActive") {
          const auto* active = std::get_if<bool>(call.arguments());
          SetCallActive(active != nullptr && *active);
          result->Success(flutter::EncodableValue(true));
        } else {
          result->NotImplemented();
        }
      });

  // Establishes the idle state rather than waiting for the first call to end.
  // The app starts with nothing connected, which is exactly the state that
  // should cost nothing.
  ApplyPowerThrottling();

  flutter_controller_->engine()->SetNextFrameCallback([&]() {
    this->Show();
  });

  // Flutter can complete the first frame before the "show window" callback is
  // registered. The following call ensures a frame is pending to ensure the
  // window is shown. It is a no-op if the first frame hasn't completed yet.
  flutter_controller_->ForceRedraw();

  return true;
}

void FlutterWindow::OnDestroy() {
  // Given back explicitly. A process that exits holding ES_SYSTEM_REQUIRED
  // has its request dropped by the system anyway, but relying on that means
  // the one path where it is not — a window closed while the process lives on
  // — leaves a machine that will not sleep and nothing on screen to explain
  // why.
  SetCallActive(false);

  if (flutter_controller_) {
    flutter_controller_ = nullptr;
  }

  Win32Window::OnDestroy();
}

void FlutterWindow::SetCallActive(bool active) {
  call_active_ = active;

  // ES_SYSTEM_REQUIRED holds off the idle sleep timer for as long as it keeps
  // being asserted; ES_CONTINUOUS is what makes it stick rather than count as
  // a single nudge. Clearing it is the same call without the flag.
  //
  // Deliberately no ES_DISPLAY_REQUIRED. Keeping a machine awake is not the
  // same as keeping a screen lit, and a voice call has nothing anybody needs
  // to look at — a monitor held on for the length of a conversation would
  // cost more than everything else in this pass put together.
  //
  // This is per-thread state, and the method channel handler runs on the
  // platform thread, which is the one that lives as long as the window does.
  SetThreadExecutionState(active ? (ES_CONTINUOUS | ES_SYSTEM_REQUIRED)
                                 : ES_CONTINUOUS);

  ApplyPowerThrottling();
}

void FlutterWindow::ApplyPowerThrottling() {
  // EcoQoS: asks Windows to schedule this process for efficiency rather than
  // speed, which on a hybrid CPU means the small cores and a lower clock.
  //
  // Only while there is no call *and* nobody is looking. Either condition
  // alone would be wrong. Throttling during a call would put the audio thread
  // on an efficiency core, and a DSP chain that misses its deadline is heard
  // rather than measured. Throttling a window somebody is working in trades
  // battery for a sluggish interface, which is not the bargain this pass is
  // making anywhere else.
  //
  // An unattended window with nothing connected is neither of those. It is
  // also the state the app spends most of its life in.
  const bool efficient = !call_active_ && !foreground_;

  PROCESS_POWER_THROTTLING_STATE state = {};
  state.Version = PROCESS_POWER_THROTTLING_CURRENT_VERSION;
  state.ControlMask = PROCESS_POWER_THROTTLING_EXECUTION_SPEED;
  state.StateMask = efficient ? PROCESS_POWER_THROTTLING_EXECUTION_SPEED : 0;

  // Unchecked on purpose: this is a request, and Windows releases before
  // 1709 simply decline it. Nothing downstream depends on the answer, and a
  // machine that will not run us slowly is not a fault worth reporting.
  SetProcessInformation(GetCurrentProcess(), ProcessPowerThrottling, &state,
                        sizeof(state));
}

LRESULT
FlutterWindow::MessageHandler(HWND hwnd, UINT const message,
                              WPARAM const wparam,
                              LPARAM const lparam) noexcept {
  // Give Flutter, including plugins, an opportunity to handle window messages.
  if (flutter_controller_) {
    std::optional<LRESULT> result =
        flutter_controller_->HandleTopLevelWindowProc(hwnd, message, wparam,
                                                      lparam);
    if (result) {
      return *result;
    }
  }

  switch (message) {
    case WM_FONTCHANGE:
      flutter_controller_->engine()->ReloadSystemFonts();
      break;

    // WM_ACTIVATEAPP rather than WM_ACTIVATE: the question is whether this
    // application is the one being used, not which of its windows has the
    // caret. Clicking between our own windows must not toggle the process
    // between efficiency and performance.
    case WM_ACTIVATEAPP:
      foreground_ = wparam != FALSE;
      ApplyPowerThrottling();
      break;
  }

  return Win32Window::MessageHandler(hwnd, message, wparam, lparam);
}

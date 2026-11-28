#include "flutter_window.h"

#include <flutter/standard_message_codec.h>
#include <flutter/standard_method_codec.h>
#include <windows.h>
#include <shlobj.h>
#include <wincrypt.h>

#include <algorithm>
#include <cstdint>
#include <limits>
#include <optional>
#include <string>
#include <utility>
#include <variant>
#include <vector>

#include "flutter/generated_plugin_registrant.h"

namespace {

constexpr size_t kMaxSecretsFileBytes = 16 * 1024 * 1024;

std::wstring SecretsFilePath(std::wstring* out_dir) {
  std::wstring base;
  PWSTR raw_path = nullptr;
  if (SUCCEEDED(SHGetKnownFolderPath(FOLDERID_LocalAppData, 0, nullptr,
                                     &raw_path)) &&
      raw_path != nullptr) {
    base = raw_path;
    CoTaskMemFree(raw_path);
  } else {
    if (raw_path != nullptr) {
      CoTaskMemFree(raw_path);
    }
    const DWORD needed = GetEnvironmentVariableW(L"LOCALAPPDATA", nullptr, 0);
    if (needed > 1) {
      std::wstring buf(needed, L'\0');
      const DWORD len =
          GetEnvironmentVariableW(L"LOCALAPPDATA", buf.data(), needed);
      if (len > 0 && len < needed) {
        buf.resize(len);
        base = std::move(buf);
      }
    }
  }
  if (base.empty()) {
    return std::wstring();
  }
  std::wstring dir = base + L"\\MumbleWay";
  if (out_dir != nullptr) {
    *out_dir = dir;
  }
  return dir + L"\\secrets.bin";
}

struct LocalSecretsReadResult {
  bool ok = false;
  bool unreadable = false;
  flutter::EncodableMap map;
  std::string error;
};

LocalSecretsReadResult UnreadableSecrets(std::string error) {
  LocalSecretsReadResult res;
  res.ok = false;
  res.unreadable = true;
  res.error = std::move(error);
  return res;
}

LocalSecretsReadResult ReadLocalSecrets() {
  const std::wstring path = SecretsFilePath(nullptr);
  if (path.empty()) {
    return UnreadableSecrets("Could not resolve local secrets path.");
  }

  HANDLE file = CreateFileW(path.c_str(), GENERIC_READ, FILE_SHARE_READ,
                            nullptr, OPEN_EXISTING, FILE_ATTRIBUTE_NORMAL,
                            nullptr);
  if (file == INVALID_HANDLE_VALUE) {
    const DWORD err = GetLastError();
    if (err == ERROR_FILE_NOT_FOUND || err == ERROR_PATH_NOT_FOUND) {
      LocalSecretsReadResult res;
      res.ok = true;
      res.unreadable = false;
      return res;
    }
    return UnreadableSecrets("Failed to open local secrets file (error " +
                             std::to_string(err) + ").");
  }

  LARGE_INTEGER file_size{};
  if (!GetFileSizeEx(file, &file_size)) {
    const DWORD err = GetLastError();
    CloseHandle(file);
    return UnreadableSecrets(
        "Failed to query local secrets file size (error " +
        std::to_string(err) + ").");
  }
  if (file_size.QuadPart <= 0 ||
      static_cast<uint64_t>(file_size.QuadPart) > kMaxSecretsFileBytes) {
    CloseHandle(file);
    return UnreadableSecrets(
        "Local secrets file is empty or exceeds maximum size.");
  }

  std::vector<uint8_t> ciphertext(static_cast<size_t>(file_size.QuadPart));
  size_t offset = 0;
  while (offset < ciphertext.size()) {
    const DWORD chunk = static_cast<DWORD>(
        (std::min)(ciphertext.size() - offset,
                   static_cast<size_t>((std::numeric_limits<DWORD>::max)())));
    DWORD read = 0;
    if (!ReadFile(file, ciphertext.data() + offset, chunk, &read, nullptr)) {
      const DWORD err = GetLastError();
      CloseHandle(file);
      return UnreadableSecrets("Failed to read local secrets file (error " +
                               std::to_string(err) + ").");
    }
    if (read == 0) {
      break;
    }
    offset += read;
  }
  CloseHandle(file);
  if (offset != ciphertext.size()) {
    return UnreadableSecrets("Unexpected EOF while reading local secrets.");
  }

  DATA_BLOB encrypted{};
  encrypted.pbData = ciphertext.data();
  encrypted.cbData = static_cast<DWORD>(ciphertext.size());
  DATA_BLOB plaintext{};
  if (!CryptUnprotectData(&encrypted, nullptr, nullptr, nullptr, nullptr,
                          CRYPTPROTECT_UI_FORBIDDEN, &plaintext)) {
    const DWORD err = GetLastError();
    return UnreadableSecrets(
        "Failed to decrypt local secrets with DPAPI (error " +
        std::to_string(err) + ").");
  }

  if (plaintext.pbData == nullptr || plaintext.cbData == 0) {
    if (plaintext.pbData != nullptr) {
      LocalFree(plaintext.pbData);
    }
    return UnreadableSecrets("Decrypted local secrets payload is empty.");
  }

  std::vector<uint8_t> plain_bytes(plaintext.pbData,
                                   plaintext.pbData + plaintext.cbData);
  SecureZeroMemory(plaintext.pbData, plaintext.cbData);
  LocalFree(plaintext.pbData);

  std::unique_ptr<flutter::EncodableValue> decoded =
      flutter::StandardMessageCodec::GetInstance().DecodeMessage(
          plain_bytes.data(), plain_bytes.size());
  SecureZeroMemory(plain_bytes.data(), plain_bytes.size());
  if (decoded == nullptr) {
    return UnreadableSecrets("Failed to decode local secrets payload.");
  }

  const auto* raw_map = std::get_if<flutter::EncodableMap>(decoded.get());
  if (raw_map == nullptr) {
    return UnreadableSecrets("Local secrets payload is not a map.");
  }

  flutter::EncodableMap out;
  for (const auto& entry : *raw_map) {
    const auto* k = std::get_if<std::string>(&entry.first);
    const auto* v = std::get_if<std::string>(&entry.second);
    if (k != nullptr && v != nullptr) {
      out.emplace(flutter::EncodableValue(*k), flutter::EncodableValue(*v));
    }
  }
  LocalSecretsReadResult res;
  res.ok = true;
  res.unreadable = false;
  res.map = std::move(out);
  return res;
}

bool WriteLocalSecrets(const flutter::EncodableMap& secrets) {
  std::wstring dir;
  const std::wstring path = SecretsFilePath(&dir);
  if (path.empty() || dir.empty()) {
    return false;
  }

  if (secrets.empty()) {
    if (!DeleteFileW(path.c_str())) {
      const DWORD err = GetLastError();
      return err == ERROR_FILE_NOT_FOUND || err == ERROR_PATH_NOT_FOUND;
    }
    return true;
  }

  std::unique_ptr<std::vector<uint8_t>> encoded =
      flutter::StandardMessageCodec::GetInstance().EncodeMessage(
          flutter::EncodableValue(secrets));
  if (encoded == nullptr || encoded->empty() ||
      encoded->size() > (std::numeric_limits<DWORD>::max)()) {
    return false;
  }

  DATA_BLOB input{};
  input.pbData = encoded->data();
  input.cbData = static_cast<DWORD>(encoded->size());
  DATA_BLOB protected_blob{};
  const BOOL ok = CryptProtectData(&input, nullptr, nullptr, nullptr, nullptr,
                                   CRYPTPROTECT_UI_FORBIDDEN, &protected_blob);
  SecureZeroMemory(encoded->data(), encoded->size());
  if (!ok) {
    return false;
  }

  const int dir_status = SHCreateDirectoryExW(nullptr, dir.c_str(), nullptr);
  if (dir_status != ERROR_SUCCESS && dir_status != ERROR_ALREADY_EXISTS &&
      dir_status != ERROR_FILE_EXISTS) {
    if (protected_blob.pbData != nullptr) {
      LocalFree(protected_blob.pbData);
    }
    return false;
  }

  const std::wstring tmp_path = path + L".tmp";
  HANDLE file = CreateFileW(tmp_path.c_str(), GENERIC_WRITE, 0, nullptr,
                            CREATE_ALWAYS, FILE_ATTRIBUTE_NORMAL, nullptr);
  if (file == INVALID_HANDLE_VALUE) {
    if (protected_blob.pbData != nullptr) {
      LocalFree(protected_blob.pbData);
    }
    return false;
  }

  size_t offset = 0;
  while (offset < protected_blob.cbData) {
    const DWORD chunk = static_cast<DWORD>(
        static_cast<size_t>(protected_blob.cbData) - offset);
    DWORD written = 0;
    if (!WriteFile(file, protected_blob.pbData + offset, chunk, &written,
                   nullptr) ||
        written == 0) {
      CloseHandle(file);
      DeleteFileW(tmp_path.c_str());
      LocalFree(protected_blob.pbData);
      return false;
    }
    offset += written;
  }
  FlushFileBuffers(file);
  CloseHandle(file);
  LocalFree(protected_blob.pbData);

  if (!MoveFileExW(tmp_path.c_str(), path.c_str(),
                   MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH)) {
    DeleteFileW(tmp_path.c_str());
    return false;
  }
  return true;
}

void HandleSecretsMethodCall(
    const flutter::MethodCall<flutter::EncodableValue>& call,
    std::unique_ptr<flutter::MethodResult<flutter::EncodableValue>> result) {
  if (call.method_name() == "readAll") {
    LocalSecretsReadResult read_res = ReadLocalSecrets();
    if (!read_res.ok) {
      result->Error("dpapi-unreadable", read_res.error);
      return;
    }
    result->Success(flutter::EncodableValue(read_res.map));
    return;
  }
  if (call.method_name() == "writeAll") {
    const auto* incoming =
        std::get_if<flutter::EncodableMap>(call.arguments());
    if (incoming == nullptr) {
      result->Error("bad-args", "writeAll expects a string map.");
      return;
    }
    LocalSecretsReadResult read_res = ReadLocalSecrets();
    if (!read_res.ok) {
      result->Error("dpapi-unreadable", read_res.error);
      return;
    }
    flutter::EncodableMap merged = std::move(read_res.map);
    bool updated = false;
    for (const auto& entry : *incoming) {
      const auto* k = std::get_if<std::string>(&entry.first);
      const auto* v = std::get_if<std::string>(&entry.second);
      if (k != nullptr && v != nullptr) {
        merged[flutter::EncodableValue(*k)] = flutter::EncodableValue(*v);
        updated = true;
      }
    }
    if (updated && !WriteLocalSecrets(merged)) {
      result->Error("dpapi", "Failed to store encrypted secrets.");
      return;
    }
    result->Success();
    return;
  }
  if (call.method_name() == "deleteKeys") {
    const auto* keys = std::get_if<flutter::EncodableList>(call.arguments());
    if (keys == nullptr) {
      result->Error("bad-args", "deleteKeys expects a key list.");
      return;
    }
    LocalSecretsReadResult read_res = ReadLocalSecrets();
    if (!read_res.ok) {
      result->Error("dpapi-unreadable", read_res.error);
      return;
    }
    flutter::EncodableMap merged = std::move(read_res.map);
    bool changed = false;
    for (const auto& item : *keys) {
      const auto* k = std::get_if<std::string>(&item);
      if (k != nullptr && merged.erase(flutter::EncodableValue(*k)) > 0) {
        changed = true;
      }
    }
    if (changed && !WriteLocalSecrets(merged)) {
      result->Error("dpapi", "Failed to update encrypted secrets.");
      return;
    }
    result->Success();
    return;
  }
  result->NotImplemented();
}

}  // namespace

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

  // Device-local DPAPI-encrypted secret storage under %LOCALAPPDATA%\MumbleWay.
  secrets_channel_ =
      std::make_unique<flutter::MethodChannel<flutter::EncodableValue>>(
          flutter_controller_->engine()->messenger(), "mumbleway/secrets",
          &flutter::StandardMethodCodec::GetInstance());
  secrets_channel_->SetMethodCallHandler(
      [](const flutter::MethodCall<flutter::EncodableValue>& call,
         std::unique_ptr<flutter::MethodResult<flutter::EncodableValue>>
             result) {
        HandleSecretsMethodCall(call, std::move(result));
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

  power_channel_.reset();
  secrets_channel_.reset();

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

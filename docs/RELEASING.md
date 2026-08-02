# Signing and releasing

Everything here is driven by GitHub Actions secrets. **Nothing in this document
requires giving anyone access to your developer accounts** — you generate the
credentials yourself and paste them in as encrypted secrets. That is the point:
a signing identity is the one thing that proves a build came from you, so it
should stay under your control.

Where a step must happen on a Mac, that is called out. Where it can be done
without one, the alternative is given.

---

## 1. Windows — SignPath

Unsigned Windows binaries trip SmartScreen, which tells the user the app is
"unrecognised". That is a poor first impression for a voice client that
immediately asks for microphone access.

[SignPath Foundation](https://signpath.io/solutions/open-source-community)
signs open-source projects for free. The private key never leaves their
infrastructure: CI uploads the build, SignPath signs it server-side and returns
it. There is no certificate file or hardware token to manage.

### One-time setup

1. Apply at <https://signpath.io/solutions/open-source-community> with the
   repository URL. Approval is manual and takes a few days.
2. In SignPath, install the **SignPath GitHub App** and grant it access to this
   repository.
3. Add the predefined **`GitHub.com`** trusted build system to your organisation
   and link it to the project.
4. Create a project (suggested slug `mumbleway`), a signing policy
   (`test-signing` for CI builds, `release-signing` for tagged releases) and an
   artifact configuration that signs the `.exe` and `.dll` inside a zip.
5. Create an API token for a user with submitter rights on that policy.

### Repository configuration

| Kind | Name | Value |
|---|---|---|
| Secret | `SIGNPATH_API_TOKEN` | the API token |
| Variable | `SIGNPATH_ORGANIZATION_ID` | your SignPath organisation id |
| Variable | `SIGNPATH_PROJECT_SLUG` | defaults to `mumbleway` |
| Variable | `SIGNPATH_SIGNING_POLICY_SLUG` | defaults to `test-signing` |
| Variable | `SIGNPATH_ARTIFACT_CONFIGURATION_SLUG` | your artifact configuration |

Secrets go in *Settings → Secrets and variables → Actions → Secrets*;
variables in the *Variables* tab beside it.

**Until `SIGNPATH_API_TOKEN` exists the build still succeeds and produces an
unsigned package**, and the job log says so with a notice. Forks and pull
requests cannot read secrets, so they are unsigned by design rather than broken.

### Signing locally

With your own certificate:

```powershell
& "${env:ProgramFiles(x86)}\Windows Kits\10\bin\10.0.22621.0\x64\signtool.exe" `
    sign /fd SHA256 /td SHA256 /tr http://timestamp.digicert.com `
    /f cert.pfx /p "$env:CERT_PASSWORD `
    build\windows\x64\runner\Release\mumbleway.exe
```

Sign `rust_lib_mumbleway.dll` too — the engine is the part that touches the
network and the microphone.

---

## 2. Apple — certificates and profiles

You need the **Apple Developer Program** (99 USD/year). Everything below is done
in your own account; only the exported files reach CI, as encrypted secrets.

### 2a. Create the signing request

**On a Mac:** Keychain Access → *Certificate Assistant* → *Request a Certificate
From a Certificate Authority*, save to disk.

**Without a Mac,** openssl produces an equivalent request:

```bash
openssl genrsa -out apple_distribution.key 2048
openssl req -new -key apple_distribution.key -out apple_distribution.csr \
    -subj "/emailAddress=you@example.com/CN=Your Name/C=US"
```

Keep the `.key` file — you cannot rebuild the `.p12` without it.

### 2b. Create the certificate

1. <https://developer.apple.com/account/resources/certificates> → **+**
2. Choose **Apple Distribution** (covers App Store and Ad Hoc).
3. Upload the `.csr`, download the resulting `.cer`.

Convert it to a `.p12` bundle, which is what CI needs:

```bash
openssl x509 -in distribution.cer -inform DER -out distribution.pem -outform PEM
openssl pkcs12 -export \
    -inkey apple_distribution.key -in distribution.pem \
    -out distribution.p12 -passout pass:CHOOSE_A_PASSWORD
```

On a Mac you can instead double-click the `.cer` and export the `.p12` from
Keychain Access.

### 2c. Register the app and a provisioning profile

1. **Identifiers** → **+** → App ID, bundle id `com.mumbleway.mumbleway`.
   Enable the **Push to Talk** and **Background Modes** capabilities if you
   later want them; the app works without.
2. **Profiles** → **+** → *App Store Connect* distribution, select the App ID
   and the certificate, download the `.mobileprovision`.

### 2d. Create the App Store Connect app record

Separate from the App ID in step 2c, and easy to skip because signing does not
need it. Uploading does: without a record, a perfectly signed build is rejected
with

```
Cannot determine the Apple ID from Bundle ID 'com.mumbleway.mumbleway'
and platform 'IOS'. (19)
```

<https://appstoreconnect.apple.com/apps> → **+** → **New App**

* **Platform** iOS
* **Bundle ID** `com.mumbleway.mumbleway`, offered in the dropdown once the App
  ID from 2c exists
* **Name** — the public store name, and unique across the whole store, so it
  may already be taken
* **Primary language**, and an **SKU**, which is any private string
  (`mumbleway-001` will do)

### 2e. Create an App Store Connect API key

This is what lets CI upload builds without your Apple ID password.

1. <https://appstoreconnect.apple.com/access/integrations/api> → **+**
2. Role **App Manager** or higher. A *Developer* key authenticates but cannot
   resolve the app, and fails with the same "Cannot determine the Apple ID"
   message as a missing record — worth knowing before hunting the wrong fault.
   Download the `.p8` — *it is downloadable exactly once.*
3. Note the **Issuer ID** and **Key ID** shown on that page.

### 2f. Add the secrets

Base64 the binary files so they survive as text — **except the `.p8`**, which
goes in as its raw contents, `-----BEGIN PRIVATE KEY-----` line and all.
Base64-encoding that one is the natural thing to do after encoding the `.p12`
directly above it, and produces `Failed to load AuthKey file. (-39)`:

```bash
gh secret set APP_STORE_CONNECT_API_KEY < AuthKey_XXXXXXXXXX.p8
```

```bash
base64 -i distribution.p12            | pbcopy   # macOS
base64 -w0 distribution.p12                      # Linux
[Convert]::ToBase64String([IO.File]::ReadAllBytes("distribution.p12")) | Set-Clipboard  # Windows
```

| Secret | Contents |
|---|---|
| `APPLE_CERTIFICATE_P12` | base64 of `distribution.p12` |
| `APPLE_CERTIFICATE_PASSWORD` | the password chosen in 2b |
| `APPLE_PROVISIONING_PROFILE` | base64 of the `.mobileprovision` |
| `APPLE_TEAM_ID` | 10-character team id, top-right of the developer portal |
| `APP_STORE_CONNECT_ISSUER_ID` | from 2d |
| `APP_STORE_CONNECT_KEY_ID` | from 2d |
| `APP_STORE_CONNECT_API_KEY` | base64 of the `.p8` |

**Do not paste any of these into a chat, an issue, or a pull request.** A `.p12`
plus its password is your signing identity; anyone holding both can ship
software as you. If one leaks, revoke the certificate in the developer portal
immediately — revocation is instant and free.

---

## 3. Android — upload key and Play access

### 3a. Generate an upload keystore

```bash
keytool -genkey -v -keystore upload-keystore.jks \
    -keyalg RSA -keysize 2048 -validity 10000 -alias upload
```

Back this file up somewhere durable. With Play App Signing, Google holds the
*app* signing key and this is only the *upload* key, so a lost upload key can be
reset by support — but that is a support ticket you would rather not file.

### 3b. Create a service account for the Play Developer API

1. Play Console → *Setup* → **API access** → link a Google Cloud project.
2. Create a service account, grant it **Release manager** on your app.
3. Create a JSON key and download it.

### 3c. Add the secrets

| Secret | Contents |
|---|---|
| `ANDROID_KEYSTORE` | base64 of `upload-keystore.jks` |
| `ANDROID_KEYSTORE_PASSWORD` | store password |
| `ANDROID_KEY_ALIAS` | `upload` |
| `ANDROID_KEY_PASSWORD` | key password |
| `GOOGLE_PLAY_SERVICE_ACCOUNT_JSON` | the whole JSON key file |

---

## 4. What each store expects

| Store | Artifact | Notes |
|---|---|---|
| Microsoft Store | `.msix` | Partner Center signs it; the SignPath signature covers direct downloads |
| Direct download | signed `.zip` | what CI publishes today |
| App Store | `.ipa` | uploaded to TestFlight first; review takes days |
| Google Play | `.aab` | APKs are for direct install only; Play requires a bundle |

---

## 5. Releasing

Tag a version and push it:

```bash
git tag v1.0.0
git push origin v1.0.0
```

That runs the full matrix, signs the Windows build, and attaches every artifact
to a GitHub release. Store uploads run only when the corresponding secrets are
present, so this is safe to try before any store account exists.

A tag is not a store submission: TestFlight and Play both need a human to
promote the build afterwards, which is deliberate.

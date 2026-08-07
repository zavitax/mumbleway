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

An App ID is scoped to a platform, so iOS and macOS need **two separate
records** — both carrying the bundle id `com.mumbleway.mumbleway`, which the
two targets share. In **Identifiers** they show up as two rows reading
identically, told apart only by the platform column. Nothing done to one
affects the other, and the commonest way to lose an afternoon here is to fix a
capability on the iOS record and expect the Mac build to notice.

Do the steps below for the iOS record now. Do them again for the macOS record
whenever you first build for the Mac — step 2 in particular, since the build
fails without it.

1. **Identifiers** → **+** → App ID, bundle id `com.mumbleway.mumbleway`.
2. Enable **iCloud**, and under it **Key-value storage**. This one is not
   optional: the app declares
   `com.apple.developer.ubiquity-kvstore-identifier` so it can sync the server
   list between the user's devices, and signing fails outright against an App
   ID that does not grant it —

   ```
   Provisioning profile "..." doesn't include the
   com.apple.developer.ubiquity-kvstore-identifier entitlement.
   ```

   Do the same for the macOS App ID. Nothing else about iCloud needs setting
   up: key-value storage has no container and no CloudKit schema.
3. Enable the **Push to Talk** and **Background Modes** capabilities if you
   later want them; the app works without.
4. **Profiles** → **+** → *App Store Connect* distribution, select the App ID
   and the certificate, download the `.mobileprovision`. Regenerate the
   profile after changing capabilities — an existing one does not pick them
   up, and the error above is what you get if you forget.

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

### 2g. macOS — the Mac App Store

A fourth job, `mac-app-store`. It gates on `MACOS_CERTIFICATE_P12` like the
others, so it skips cleanly until the secrets below exist.

**It cannot reuse anything from 2b.** macOS signs twice with two different
certificate types: the app with *Mac App Distribution*, and the `.pkg` wrapped
around it with *Mac Installer Distribution*. Neither is the iOS certificate,
and the mismatch is not caught locally — the package builds, uploads, and is
rejected some hours later.

In **Certificates** → **+**, create both, using the same CSR from 2a:

* **Mac App Distribution** → export as `mac_app.p12`
* **Mac Installer Distribution** → export as `mac_installer.p12`

Then **Profiles** → **+** → *Mac App Store* distribution, against the macOS App
ID from 2c — the separate record, with iCloud enabled. It downloads as
`.provisionprofile`, not `.mobileprovision`.

`APPLE_PROVISIONING_PROFILE` is **not** where that goes. It feeds TestFlight and
holds the iOS profile; a macOS profile placed there breaks the iOS build while
saying nothing about macOS. The macOS one has its own secret:

| Secret | Contents |
|---|---|
| `MACOS_CERTIFICATE_P12` | base64 of `mac_app.p12` |
| `MACOS_CERTIFICATE_PASSWORD` | its password |
| `MACOS_INSTALLER_CERTIFICATE_P12` | base64 of `mac_installer.p12` |
| `MACOS_INSTALLER_CERTIFICATE_PASSWORD` | its password |
| `MACOS_PROVISIONING_PROFILE` | base64 of the `.provisionprofile` |

The three `APP_STORE_CONNECT_*` secrets and `APPLE_TEAM_ID` carry over unchanged
— an API key is per-account, not per-platform.

One more record to create, and it is not a second app: in App Store Connect,
open the existing MumbleWay app and use **Add Platform** → **macOS**. A separate
app record with the same bundle id is the wrong shape, and the upload fails the
same way a missing record does.

**For a local Mac build**, none of the above is needed — nothing has to reach
GitHub at all. But iCloud does have to be enabled on the macOS App ID per 2c,
or the build stops before it compiles anything:

```
error: "Runner" has entitlements that require signing with a development
certificate. Enable development signing in the Signing & Capabilities editor.
```

---

### 2h. Export compliance

Both Info.plists declare `ITSAppUsesNonExemptEncryption` as **false**, and the
first thing to understand is what that key actually asks. It is not "does this
app use encryption". It is "does this app use **non-exempt** encryption".
Apple's own UI blurs the two — it offers to let you "specify that you don't use
encryption" — and a reader who takes that at face value will conclude the
repository is lying about itself.

**What the app does cryptographically** is unchanged by the setting, and is
worth stating plainly, because this is the input to the determination below:

- The voice path encrypts with AES-128 under OCB2, implemented in this
  repository (`core/src/crypto/ocb2.rs`) rather than taken from the system.
  **No operating system provides OCB2** — Mumble specified it and every client
  implements it, so no amount of rewriting onto CryptoKit reaches the
  "encryption provided by the OS" exemption.
- The control channel runs TLS 1.2/1.3 from rustls/ring, compiled into the
  binary.
- The client identity is a self-signed ECDSA P-256 certificate; server
  certificates are pinned by SHA-256 fingerprint.

**Where the value comes from.** App Store Connect's export questionnaire was
answered for this app and returned *"Based on your answers, you don't need to
upload any documents. You can specify that you don't use encryption in the
Info.plist"*. That result — under **App Information** in App Store Connect — is
the record. The plist mirrors it. When the two disagree, the upload is rejected
with **error 90592**, `Invalid Export Compliance Code`, which is exactly how the
mismatch was found.

So the direction of authority runs one way: the questionnaire decides, the plist
follows. Do not change this key to match a reading of the code. If the
cryptography changes, answer the questionnaire again first.

**The part that is still yours.** Apple not wanting documents is not the same as
BIS not wanting a filing. The algorithms here are standard and published rather
than proprietary, which is the ordinary position for a voice app, and US export
rules generally place such software under ECCN 5D992.c — reached by filing a
self-classification report with BIS and the NSA, renewed annually. Whether that
obligation applies to your distribution is a legal determination about your
product, not a technical one, and it is worth twenty minutes of somebody who
does export compliance rather than a guess from a developer.

If a compliance code is ever issued, it goes in as
`ITSEncryptionExportComplianceCode` alongside this key.

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

## 4. Microsoft Store — Partner Center

Separate from section 1, and the two do not overlap. SignPath signs the build
people download from GitHub. **Partner Center strips that and re-signs with its
own certificate**, so a Store package is submitted unsigned and a SignPath
signature would be thrown away. That is why the publish workflow builds two
different Windows artifacts and only one of them can be installed locally.

Registration is now **free** for both individual and company accounts — the 19
USD and 99 USD fees were dropped, company accounts in May 2026. A government-ID
identity check replaces the card, and you can sign up with a work account
through Entra ID.

### 4a. Reserve the app in Partner Center

Do this first. **The app has to exist before anything can be uploaded to it**,
and no API can create one — that is true of the manual route and the automated
one alike.

Partner Center → *Apps and games* → **New product** → *App*, then reserve the
name. The reserved name is what the Store lists it under.

### 4b. Copy the identity values

Partner Center → your app → *Product management* → **Product identity**. Three
values, all account-specific and none of them guessable:

| Partner Center field | Secret | Shape |
|---|---|---|
| `Package/Identity/Name` | `MSIX_IDENTITY_NAME` | `12345Publisher.MumbleWay` |
| `Package/Identity/Publisher` | `MSIX_PUBLISHER` | `CN=A1B2C3D4-1234-...` |
| `Package/Identity/PublisherDisplayName` | `MSIX_PUBLISHER_DISPLAY_NAME` | your publisher name |

They must match **exactly** or the upload is rejected. `Publisher` is the one
that catches people out: it is a `CN=` followed by a GUID, not a company name,
and it is not the same string as `PublisherDisplayName`.

**The identity cannot be changed after the app is created.** Choose the reserved
name deliberately.

`app/pubspec.yaml` carries `identity_name: com.mumbleway.mumbleway`, which is
correct only for a sideload package. The workflow overrides all three from these
secrets for the Store build, which is why they are not committed.

### 4c. Add the secrets

| Secret | Contents |
|---|---|
| `MSIX_PUBLISHER` | `Package/Identity/Publisher` |
| `MSIX_IDENTITY_NAME` | `Package/Identity/Name` |
| `MSIX_PUBLISHER_DISPLAY_NAME` | `Package/Identity/PublisherDisplayName` |

`MSIX_PUBLISHER` is the gate. Until it is set the job builds a **test-signed
sideload package** and says so with a notice; once it is set the same job builds
a Store package instead. Both are attached to the run as
`mumbleway-windows-msix`, and only the sideload one will install on your own
machine — a Store package has no signature until Partner Center gives it one, so
double-clicking it fails, correctly.

### 4d. The first submission is by hand

Download `mumbleway-windows-msix` from the publish run and upload the `.msix` in
Partner Center → *Packages*. Then the listing: description, screenshots, age
rating.

**A privacy policy URL is required, and this app cannot skip it.** Store Policy
10.5.1 requires one from any product that accesses or transmits personal
information; a voice client that records a microphone and sends the audio to a
server is squarely inside that. Submission is blocked without the URL, and it is
the sort of thing discovered at the end of a long form.

Expect to justify the `microphone` capability in the listing. It is declared in
`msix_config` because the app cannot work without it.

### 4e. The version has to be raised by hand

Unlike iOS and Android, which take their build number from `github.run_number`
and therefore always rise, the Windows package version is **pinned in
`app/pubspec.yaml`**:

```yaml
msix_config:
  msix_version: 1.0.0.0
```

`msix:create` reads it from there and the workflow passes no override, so every
Store build carries the same version. Partner Center rejects a submission whose
version is not higher than the last one, so **the second submission fails unless
this is edited first**. Two rules when editing it:

- four components, and **the fourth must be `0`** — the revision field is
  reserved for the Store and a non-zero value is rejected;
- it must be strictly greater than the previous accepted submission.

This is worth fixing rather than remembering: passing `--version` to
`msix:create` from `github.run_number`, the way the other three platforms
already do, would make it automatic. Left as it is for now so that the version
in the Store cannot move without somebody deciding it should.

### 4f. Automating submissions (not set up)

Uploading is a drag-and-drop today, deliberately — the workflow produces the
package and stops. Automating it needs more setup than the other stores:

1. Partner Center → *Account settings* → **Tenants** → associate an Azure AD
   (Entra ID) tenant. Needs global administrator on that directory.
2. Register an application in it and generate a client secret.
3. Give that application the **Manager** role in Partner Center.
4. Collect the tenant id, client id, client secret, seller id and product id.

Two caveats before spending an evening on it. The official
[`microsoft/store-submission`](https://github.com/microsoft/store-submission)
action documents the `win32` flow, for `.msi` and `.exe` installers — MSIX
packages go through the older Store submission API, which is a different set of
calls. And once a submission has been created through the API, **editing it in
Partner Center stops it being manageable by the API afterwards**; pick one and
stay with it.

None of this is implemented in `publish.yml` and none of it has been tried here.

---

## 5. What each store expects

| Store | Artifact | Notes |
|---|---|---|
| Microsoft Store | `.msix` | Partner Center re-signs it, so it is submitted unsigned; version raised by hand — see 4 |
| Direct download | signed `.zip` | what CI publishes today; this is the one SignPath signs |
| App Store | `.ipa` | uploaded to TestFlight first; review takes days |
| Google Play | `.aab` | APKs are for direct install only; Play requires a bundle |
| Mac App Store | `.pkg` | two certificates, one for the app and one for the installer; see 2g |

---

## 6. Releasing

Two routes, and they are not the same thing.

**To get a build to testers**, run the publish workflow:

```bash
gh workflow run publish.yml --ref main -f track=internal
```

**To cut a release**, tag a version and push it:

```bash
git tag v1.0.0
git push origin v1.0.0
```

A tag does everything the dispatch does *and* attaches every artifact to a
GitHub release, so it is the bigger, permanent statement. Reach for the dispatch
when the goal is simply a build somebody can ride with.

Either way, store uploads run only where the matching secrets are present, and
every job **skips cleanly without them** — so a green run can mean nothing was
uploaded at all. `gh secret list` shows which are set; it prints names and dates
and never values.

Note that pushing to `main` publishes nothing. That runs `build.yml`, which
compiles the matrix and uploads to no store.

### What still waits for a person

| Platform | After the workflow |
|---|---|
| Google Play | live on the internal track at once; wider tracks upload as `draft` and must be promoted |
| TestFlight | build available to testers; older builds are expired automatically |
| App Store / Mac App Store | uploaded to App Store Connect, **not** submitted for review |
| Microsoft Store | nothing is uploaded — the `.msix` is an artifact to submit by hand, see 4 |

That a wider Play track and an App Store review both stop for a human is
deliberate. Anything reaching people who did not opt into a test build should
not go live because a workflow ran.

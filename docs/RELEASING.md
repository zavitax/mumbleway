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

### 4d. The first submission is by hand — and only the first

Download `mumbleway-windows-msix` from the publish run and upload the `.msix` in
Partner Center → *Packages*. Then the listing: description, screenshots, age
rating.

**Upload the file from the run, not one off your own machine.** A local
`dart run msix:create` produces a package with the *pubspec* identity in it, and
Partner Center answers with:

> The PublisherDisplayName element in the app manifest of mumbleway.msix is
> MumbleWay, which doesn't match your publisher display name: …

which reads like a broken workflow and is really the wrong file. Both used to be
called `mumbleway.msix`; the CI ones are now named `mumbleway-store-<version>`
and `mumbleway-sideload-<version>` so they cannot be mixed up, and the job
prints the identity it built and fails if it is still the default.

#### `runFullTrust` needs approval, and cannot be removed

Partner Center will also warn:

> The following restricted capabilities require approval before you can use them
> in your app: runFullTrust

That is expected and not a fault. `msix:create` adds `runFullTrust` to every
packaged desktop app because a Win32 binary cannot run inside the sandbox a UWP
app runs in — removing it would produce a package that installs and cannot
start. It is a *warning* rather than an error, so it does not block the upload.

What it does need is a justification during submission, under *Submission
options* → restricted capabilities: this is a Flutter desktop application, not a
UWP one, and it needs full trust to open audio devices through the operating
system's own APIs. If the account is not authorised for it at all — a different
message, saying the account "isn't authorized to submit apps that use the
runFullTrust capability" — that is granted through Partner Center's developer
support rather than from the submission form.

**A privacy policy URL is required, and this app cannot skip it.** Store Policy
10.5.1 requires one from any product that accesses or transmits personal
information; a voice client that records a microphone and sends the audio to a
server is squarely inside that. Submission is blocked without the URL, and it is
the sort of thing discovered at the end of a long form.

Expect to justify the `microphone` capability in the listing. It is declared in
`msix_config` because the app cannot work without it.

### 4e. The version moves on its own

Nothing to do here, but worth knowing what the Store will show.

Partner Center rejects a submission whose version is not higher than the last
accepted one, and rejects it *after* the upload. MSIX also has no separate
build-number field — the four-part version is the whole of it — and **the Store
reserves the fourth part, which must be `0`**. That leaves three usable
components, so the workflow builds:

```
<major>.<minor>.<github.run_number>.0        e.g. 1.0.59.0
```

Major and minor come from `version:` in `app/pubspec.yaml`, so a deliberate
`1.0` → `1.1` still reads as one. The run number only ever goes up, so the
package version only ever goes up, as long as major and minor never go *down*.

Two consequences:

- **The patch component is not carried across.** `1.0.7` and `1.0.9` both
  package as `1.0.<run>.0`. With three usable fields and one of them owed to a
  monotonic counter, something had to give, and the counter is the part the
  Store actually requires.
- **`msix_version` in `pubspec.yaml` no longer decides anything in CI.** It is
  the default for a local `dart run msix:create` and nothing else; `--version`
  overrides it. Raising it by hand will not affect a release.

The workflow refuses to build rather than let Partner Center reject it later, in
two cases it can see coming:

- **a major version of `0`** — the first component of an MSIX version cannot be
  zero, so any pre-1.0 `version:` in `pubspec.yaml` is a hard stop;
- **any component above 65535**, which is the range the format allows.

### 4e-bis. Changing listing text and the package in one certification run

**Every Microsoft submission costs a certification run of hours to days, and one
in flight blocks the next.** So a listing fix and a new package must go in the
*same* submission or the second waits for the first. This is the route, and it
was worked out the expensive way on 4 September 2026, when the description had
been wrong in the Store for three weeks.

```bash
# 1. stage the package without committing — this is what makes it one run
msstore publish path/to/mumbleway-store-<version>.msix -id <productId> --noCommit

# 2. read the draft it just created
msstore submission get <productId> > draft.json

# 3. edit draft.json  (see the warnings below), then put the whole thing back
msstore submission update <productId> -p draft.json

# 4. read it back and check, then commit
msstore submission get <productId>
msstore submission publish <productId>
```

Four things fail in ways that do not name themselves:

- **`msstore publish` wants the package *file*, not the folder it is in.** Given
  a directory it looks for a *project* to infer a publisher from, and answers
  `We could not find a project publisher for the project at ...` — which reads
  as a credentials or identity problem and is a wrong argument.
- **`msstore submission updateMetadata` will not take a partial payload.**
  Sending `{"Listings": ...}` alone returns HTTP 400 `InvalidParameterValue`
  complaining about `'NotSet'` — a value that appears nowhere in what was sent,
  because the CLI inflates the fragment into a whole product and everything left
  out becomes that. Use `update` with the *complete* product from `get`.
- **Send back every field, not the ones being changed.** A listing object with
  `Features` or the search-term `Keywords` omitted is a listing with those
  fields cleared, and they are not recoverable from the published submission
  once committed.
- **The first call may simply fail.** `💥 Error while creating submission` on an
  otherwise clean account was transient here and succeeded on the retry. Check
  `msstore submission status` before assuming it did nothing — the failed call
  may still have created the draft.

Verify before committing rather than after. `get` returns the draft, and the
package list should show the new version as `PendingUpload` beside the old one
as `PendingDelete`; anything else means the package did not attach.

#### If you realise too late, a submission you made can be cancelled

`msstore submission delete <productId> --no-confirm` removes a submission that
is already in certification, and the app drops straight back to its last
published state. Then re-stage and rebuild as above.

**`--no-confirm` is not optional in a non-interactive shell.** Without it the
command asks Yes/No, finds no stdin, and dies inside `YesNoConfirmationAsync`
with a stack trace — which reads like a failure to delete and is a failure to
*ask*. Nothing is changed when that happens; check the status and try again.

**And the warning under 4f about a wedged cancelled submission does not apply
to this.** That one is about a submission cancelled by a person in Partner
Center, which the Ingestion API then refuses to delete because it did not
create it. A submission the API created, the API can remove — verified on
4 September 2026, when the search terms and Features were left out of a
submission that had already reached certification and the whole thing had to be
withdrawn and redone. It came back clean.

Which is the real lesson: **put everything in the first submission.** The
listing is locked the moment one is in flight, so a field left out is either a
second certification run or a cancellation. Description, search terms, Features
and release notes all live in `BaseListing` and all go in together.

### 4f. Automating the submission

**The build was always automated; this is about the upload.** Every publish run
already produces `mumbleway-store-<version>.msix` with the Partner Center
identity in it and the version incremented from the run number. What follows
hands that file to Partner Center instead of a person.

**It updates; it cannot create.** No API can reserve a product or make the first
submission, so 4a and 4d happen once by hand and everything after them can be
automated. Microsoft supports this route for **free products only** — if
MumbleWay ever has a paid tier, this goes back to the form.

#### The account setup, once

1. Partner Center → *Account settings* → **Tenants**: associate a Microsoft
   Entra tenant, or create one. Needs global administrator on that directory.
2. Register an application in that tenant
   (Entra admin centre → *Identity* → *Applications* → **App registrations**),
   then *Certificates & secrets* → **New client secret**. Copy the value at once
   — it is shown only on the screen that creates it.
3. Partner Center → *Account settings* → *User management* → **Microsoft Entra
   applications** → add that application, and give it the **Manager** role.
   Without the role, authentication succeeds and every call is refused, which
   reads like a wrong secret.

#### The secrets

| Secret | Where it comes from |
|---|---|
| `AZURE_AD_TENANT_ID` | Entra admin centre → *Identity* → *Overview* → Tenant ID |
| `AZURE_AD_APPLICATION_CLIENT_ID` | the app registration's *Application (client) ID* |
| `AZURE_AD_APPLICATION_SECRET` | the client secret from step 2 |
| `MSIX_SELLER_ID` | Partner Center → *Account settings* → **Legal info** → **Seller ID**. Digits only — see below |
| `MSIX_STORE_PRODUCT_ID` | the product's Store ID, also in its Store listing URL |

`MSIX_STORE_PRODUCT_ID` is the gate: until it is set, the job builds and
attaches the package exactly as before and submits nothing.

**The Seller ID is under *Legal info*, not *Identifiers*.** Microsoft's own
setup page sends you to "Account settings → Developer settings or Identifiers",
and it is on neither — *Identifiers* holds the Publisher ID, which looks like a
plausible answer and is the wrong one. Only the Seller ID is numeric. Give the
CLI the other and it dies with

```
System.FormatException: The input string '***' was not in a correct format.
   at MSStore.CLI.Services.CLIConfigurator.RetrieveSellerId
```

which names neither the setting nor the problem, because the value is masked in
the log. The workflow now checks it is all digits before calling the CLI and
says so plainly instead.

#### How to release

The submission does **not** run on every publish. It runs on a tag push, or when
the workflow is dispatched with **Also submit the MSIX to the Microsoft Store**
ticked:

```bash
gh workflow run publish.yml --ref main -f track=internal -f microsoft_store=true
```

That default is the important part. Certification takes hours to days, each
submission supersedes the one before it, and **a submission in flight blocks the
next one** — so a Store upload on every internal Android build would leave the
Store permanently trailing and never finishing a review.

A green job means *submitted*, not *published*. The Store keeps showing the old
version until certification passes.

#### It will not submit over one in certification

Before uploading, the job asks the Store what state the last submission is in
and declines if anything is still moving — `Certification`, `Publishing`,
`CommitStarted` and the rest. Pushing anyway is not harmless: at best the call
is refused, at worst it replaces a package part way through certification and
the clock starts again.

Two choices in how it does that, both deliberate:

- **It declines rather than fails.** A submission in certification is the
  ordinary state for hours to days after every release, and failing the run for
  it would paint every Android internal release in that window red — which
  teaches people to stop reading red. The other three stores are unaffected and
  do their job; the MSIX job warns and skips.
- **It allows on a list of finished states, not on a list of busy ones.** An
  unrecognised status stops the submission rather than passing it, because a
  status nobody has seen before is far more likely to be a new state than a
  finished one. If the Store adds one, the warning says so and the list in
  `publish.yml` needs the addition.

There is no override input. The escape hatch is the one that already exists —
the package is attached to the run, and if the Store says something is in
flight, Partner Center is where somebody should be looking before adding to it.

#### One thing to decide once and not revisit

Once a submission has been created through the API, **editing that submission by
hand in Partner Center can leave it unmanageable by the API**. Pick one route
and stay with it: the listing text, screenshots and age rating are fine to edit
in Partner Center between releases, but do not hand-edit a submission the
workflow created while it is open.

---

## 5. What each store expects

| Store | Artifact | Notes |
|---|---|---|
| Microsoft Store | `.msix` | Partner Center re-signs it, so it is submitted unsigned; uploaded on a tag or on request — see 4f |
| Direct download | signed `.zip` | runs from wherever it is unpacked and installs nothing; this is the one SignPath signs |
| Direct download | `.msi` | a real install: Start Menu entry, uninstall entry, upgrade in place — see 5a |
| App Store | `.ipa` | uploaded to TestFlight first; review takes days |
| Google Play | `.aab` | APKs are for direct install only; Play requires a bundle |
| Mac App Store | `.pkg` | two certificates, one for the app and one for the installer; see 2g |

### 5a. The Windows installer

Three Windows artifacts, and they answer three different questions. The `.msix`
goes to the Store and **cannot be installed by hand** — Partner Center re-signs
it, so the one CI produces is unsigned and Windows refuses it. The `.zip` runs
in place and installs nothing, which is what you want for "try this build".
The `.msi` is the one for somebody who wants to keep the app.

It installs per-machine into `Program Files\MumbleWay`, adds a Start Menu entry
and an uninstall entry, and upgrades in place: installing a newer version
replaces the older one rather than sitting beside it.

Built by `app/windows/installer/build.ps1`, which CI runs and you can too:

```powershell
cd app; flutter build windows --release; cd ..
.\app\windows\installer\build.ps1 -Version 1.0.0
```

That is deliberate — an installer that only exists inside a workflow cannot be
changed without pushing to find out whether the change worked.

Two things about it are worth knowing before editing:

* **The version is three fields, not four.** Windows Installer compares only
  `major.minor.build`, and its limits are tighter than the Store's: 0–255,
  0–255, 0–65535. A fourth field is accepted and then ignored, so two builds
  differing only there are indistinguishable and the newer one refuses to
  install over the older. The script rejects anything else by hand.
* **It runs before `msix:create`.** The installer harvests the build folder
  whole, so a `.msix` written there first would be packaged inside the `.msi`.
  The script fails rather than doing it.

Signing is not wired up: SignPath signs the `.zip` (section 1), and the `.msi`
is unsigned today. An unsigned installer trips SmartScreen exactly as an
unsigned `.exe` does, so a signed one is the obvious next step — the artifact
configuration in SignPath would need to cover the `.msi`.

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

<#
.SYNOPSIS
  Builds the Windows MSI from an already-built Flutter bundle.

.DESCRIPTION
  A script rather than steps in publish.yml, so that the installer can be built
  and opened by hand exactly as CI builds it. An installer that only exists
  inside a workflow is one nobody can try a change to without pushing.

  It does not build the app. `flutter build windows --release` first; this
  packages what that produced.

.EXAMPLE
  # From the repository root, after flutter build windows --release:
  .\app\windows\installer\build.ps1 -Version 1.0.0

.EXAMPLE
  # What CI runs, give or take the paths:
  .\app\windows\installer\build.ps1 -Version 1.0.$env:GITHUB_RUN_NUMBER -Output dist
#>
[CmdletBinding()]
param(
    # major.minor.build. See the validation below for why three and not four.
    [Parameter(Mandatory = $true)]
    [string] $Version,

    # The Flutter bundle to package. Defaults to where `flutter build windows`
    # leaves it, resolved relative to the repository root rather than the
    # caller's working directory.
    [string] $Stage,

    # Where the .msi lands.
    [string] $Output,

    # Pinned so a new WiX release cannot change the output underneath a build.
    [string] $WixVersion = '5.0.2'
)

$ErrorActionPreference = 'Stop'

# Paths are all derived from this file's location, so the script works from any
# working directory.
$here = Split-Path -Parent $MyInvocation.MyCommand.Path
$repo = (Resolve-Path (Join-Path $here '..\..\..')).Path

if (-not $Stage)  { $Stage  = Join-Path $repo 'app\build\windows\x64\runner\Release' }
if (-not $Output) { $Output = Join-Path $repo 'app\build\windows\installer' }

# ---------------------------------------------------------------------------
# The version, checked here rather than by Windows Installer twenty minutes on
# ---------------------------------------------------------------------------
# ProductVersion is major.minor.build with **tighter limits than the MSIX
# version beside it**: 0-255, 0-255, 0-65535. And only those three fields are
# compared for upgrades — a fourth is accepted and then ignored, so two builds
# differing only there are indistinguishable to the installer and the newer one
# will not replace the older.
if ($Version -notmatch '^(\d+)\.(\d+)\.(\d+)$') {
    throw "Version must be major.minor.build, three fields and no more; got '$Version'. A fourth field is legal in an MSI and is ignored when comparing versions, which makes upgrades silently stop working."
}
$major, $minor, $build = [int]$Matches[1], [int]$Matches[2], [int]$Matches[3]
if ($major -gt 255)   { throw "MSI major version must be 0-255; got $major." }
if ($minor -gt 255)   { throw "MSI minor version must be 0-255; got $minor." }
if ($build -gt 65535) { throw "MSI build version must be 0-65535; got $build." }

# ---------------------------------------------------------------------------
# The bundle
# ---------------------------------------------------------------------------
if (-not (Test-Path (Join-Path $Stage 'mumbleway.exe'))) {
    throw "No mumbleway.exe in $Stage. Run 'flutter build windows --release' in app\ first."
}

# The runtime the app links against but Windows does not ship. Checked because
# its absence is invisible until somebody installs on a clean machine: every
# development box has the redistributable already, so a build missing these
# starts perfectly well here and fails for a stranger. See the comment in
# app/windows/CMakeLists.txt.
$runtime = @('msvcp140.dll', 'vcruntime140.dll', 'vcruntime140_1.dll')
$absent = @($runtime | Where-Object { -not (Test-Path (Join-Path $Stage $_)) })
if ($absent.Count -gt 0) {
    throw "The bundle is missing the Visual C++ runtime: $($absent -join ', '). InstallRequiredSystemLibraries in app/windows/CMakeLists.txt should have put it beside the executable. An installer built without it works on this machine and fails on a clean Windows install."
}

# `<Files Include="$(Stage)\**">` sweeps whatever is in the bundle folder, so a
# package left there by another step ends up *inside* this one. Both are real:
# `dart run msix:create` writes its .msix here, and an earlier run of this
# script with -Output pointed at the bundle would leave a .msi. The portable-zip
# step in publish.yml guards the same folder for the same reason, and that guard
# is what this one is modelled on.
$stray = @(Get-ChildItem $Stage -Include *.msi, *.msix -Recurse -ErrorAction SilentlyContinue)
if ($stray.Count -gt 0) {
    throw "A package is in the bundle folder ($($stray[0].Name)). It would be harvested into the installer. This script has to run before msix:create, and -Output must stay outside the bundle."
}

# ---------------------------------------------------------------------------
# The licence page
# ---------------------------------------------------------------------------
# Generated from the repository's LICENSE every build. Committing an .rtf
# beside it would be a second copy of the licence, free to disagree with the
# first, and an installer is the wrong place to show a stale one.
#
# The GPL is laid out with hard line breaks and is pure ASCII, so the
# conversion is a font, an escape for RTF's three special characters, and a
# \par per line. Nothing here handles non-ASCII, because there is none to
# handle; the check below fails loudly if that ever stops being true.
$licenseSource = Join-Path $repo 'LICENSE'
$text = Get-Content $licenseSource -Raw
$nonAscii = [regex]::Match($text, '[^\x00-\x7F]')
if ($nonAscii.Success) {
    throw "LICENSE contains a non-ASCII character (U+$([int][char]$nonAscii.Value | ForEach-Object { $_.ToString('X4') })) at offset $($nonAscii.Index). The RTF conversion here only handles ASCII and would mangle it."
}

New-Item -ItemType Directory -Force -Path $Output | Out-Null
$licenseRtf = Join-Path $Output 'license.rtf'

$body = ($text -replace '\\', '\\\\' -replace '\{', '\{' -replace '\}', '\}') `
        -split "`r?`n" -join "\par`r`n"
$rtf = "{\rtf1\ansi\ansicpg1252\deff0{\fonttbl{\f0\fnil\fcharset0 Consolas;}}`r`n\fs16 $body`r`n}"
Set-Content -Path $licenseRtf -Value $rtf -Encoding ascii
Write-Host "licence: $([math]::Round((Get-Item $licenseRtf).Length / 1KB, 1)) KB of RTF from LICENSE"

# ---------------------------------------------------------------------------
# WiX
# ---------------------------------------------------------------------------
# Installed as a local tool rather than assumed present. The runners do carry
# WiX 3.14, and depending on that was the tempting shortcut: this workflow has
# already been broken once by the runner image moving underneath it, and a
# pinned version cannot do that. WiX 5 also has <Files Include="**">, which is
# what keeps the 38-file bundle from becoming a hand-maintained list.
if (-not (Get-Command wix -ErrorAction SilentlyContinue)) {
    Write-Host "installing WiX $WixVersion"
    dotnet tool install --global wix --version $WixVersion | Out-Null
    $env:PATH = "$env:USERPROFILE\.dotnet\tools;$env:PATH"
}

# The extension version has to match the toolset's, and the toolset is not
# necessarily the one just requested: a machine with its own `wix` already on
# PATH keeps it, and pinning the extension to $WixVersion against a different
# toolset fails in a way that reads as a broken .wxs rather than a version
# skew. So ask the tool what it is instead of assuming.
$wixActual = (wix --version) -replace '\+.*$', ''
if ($wixActual -ne $WixVersion) {
    Write-Host "note: wix $wixActual is on PATH, not the pinned $WixVersion; matching the extension to it"
}

# The UI extension carries WixUI_InstallDir. `wix extension add` is idempotent.
wix extension add --global WixToolset.UI.wixext/$wixActual | Out-Null
if ($LASTEXITCODE -ne 0) { throw "could not add WixToolset.UI.wixext/$wixActual." }

$msi = Join-Path $Output "mumbleway-$Version-x64.msi"

wix build (Join-Path $here 'mumbleway.wxs') `
    -arch x64 `
    -define "Version=$Version" `
    -define "Stage=$Stage" `
    -define "License=$licenseRtf" `
    -ext WixToolset.UI.wixext `
    -out $msi
if ($LASTEXITCODE -ne 0) { throw "wix build failed with exit code $LASTEXITCODE." }

$size = [math]::Round((Get-Item $msi).Length / 1MB, 1)
Write-Host "built $([System.IO.Path]::GetFileName($msi)) ($size MB)"

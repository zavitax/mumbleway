# Store graphics

Generated. **Do not edit these by hand** — the next run overwrites them.

```bash
python tool/make_store_assets.py
```

Everything here comes from `app/assets/icon/mumbleway.svg` and
`app/assets/logo/mumbleway-logo-on-dark.svg`, so the store page cannot drift
away from the app. The script checks each file against the spec it has to meet
and refuses to report success if one would be rejected.

Words — description, tagline, every field cut to its limit — are in
[`docs/STORE_DESCRIPTION.md`](../../docs/STORE_DESCRIPTION.md) and
[`docs/STORE_LISTING.md`](../../docs/STORE_LISTING.md).

## What to upload where

| Store | File | Field |
|---|---|---|
| App Store | `app-store/icon-1024.png` | App icon, in App Store Connect |
| Mac App Store | `mac-app-store/icon-1024.png` | App icon |
| Google Play | `google-play/icon-512.png` | App icon |
| Google Play | `google-play/feature-graphic-1024x500.png` | Feature graphic — **required**, the listing will not publish without it |
| Microsoft Store | `microsoft-store/store-logo-300.png` | Store logo |
| Microsoft Store | `microsoft-store/hero-1920x1080.png` | Optional promotional art |
| Anywhere else | `shared/` | README, release page, sticker |

The Microsoft Store's *tile* images are not here: `msix:create` builds them from
`logo_path` in `app/pubspec.yaml` and puts them inside the package.

## Why the corners differ between files

`mumbleway.svg` draws its own rounded corners. That is right when the icon is
shown as-is and wrong when a store applies its own mask, because the upload is
then rounded twice and a thin dark crescent appears inside the mask — invisible
in a file listing, obvious on a phone.

- **iOS and Google Play mask the icon themselves**, so they get a square,
  full-bleed render.
- **The Microsoft Store shows it as-is**, so it keeps its own corners.

Apple additionally refuses any marketing icon with an alpha channel, so those
two are written as RGB. The verify pass checks all of this.

## Screenshots are not here, and cannot be generated

They need the app running on each device size, connected to a real server with
people in the channel. A composed mock-up would be a picture of something that
does not exist, which is both a poor listing and against every one of these
stores' rules.

What has to be captured, per store:

| Store | Needed |
|---|---|
| App Store | iPhone 6.9" (1320×2868) and 6.5" (1242×2688); iPad 12.9" (2048×2732) if the iPad build is listed |
| Mac App Store | 1280×800, 1440×900, 2560×1600 or 2880×1800 |
| Google Play | at least 2 phone shots, 16:9 or 9:16, shortest side ≥ 320 px |
| Microsoft Store | at least 1, 1366×768 or larger |

Worth showing, in roughly this order — they are the things nothing else
communicates:

1. **The main call screen with someone talking**, the meter live. This is the
   app doing its job.
2. **The floating window over a navigation app.** The single most persuasive
   image here, because it answers "how do I use this while riding" without a
   word of copy.
3. **Noise profiles**, with Auto showing what it settled on.
4. **The diagnostics panel**, spectrum moving. Sells the seriousness of the
   noise work to the people who care about it.
5. **Channel tree and roster**, so it is obvious this is group voice.

Take them in both English and Russian if the listings are localised; a Russian
page with English screenshots reads as a half-finished translation.

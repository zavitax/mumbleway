/**
 * Read the App Store and Mac App Store listings back out of App Store Connect.
 *
 *     node tool/read_app_store_listing.mjs
 *
 * The counterpart to `tool/read_play_listing.mjs`, and it exists for the same
 * reason: `tool/check_listing.py` measures the copy in this repository against
 * each store's limits and cannot know whether that copy was ever pasted into a
 * store. **The keyword fields in particular are visible nowhere else** — Apple
 * never publishes them, so without this they are only what STORE_LISTING.md
 * remembers.
 *
 * **Read-only.** Every request here is a GET. Nothing creates a version, and
 * nothing submits anything; adding either would make this a script nobody dares
 * run. Keep it that way.
 *
 * ## Why this is not the MCP server
 *
 * The `app-store-connect` MCP is refused a collection read on version
 * localizations — `The resource 'appStoreVersionLocalizations' does not allow
 * 'GET_COLLECTION'`. That is not a permissions problem, it is the wrong URL:
 * Apple offers no top-level collection there, only the relationship under a
 * version. This walks the relationships instead, and gets everything.
 *
 * ## Credentials
 *
 * `tool/asc.mjs` holds them, the JWT and the three verbs. This file imports
 * `get` and nothing else, which is what keeps the guarantee above true.
 */

import { client } from './asc.mjs';

const APP_ID = process.argv[2] ?? '6797305046';

// `get` alone. `tool/asc.mjs` also exports `post` and `patch`, and pulling
// either of them in here is the first step to a script nobody dares run.
const { get } = client();

const LIMITS = { name: 30, subtitle: 30, keywords: 100, promotionalText: 170, description: 4000, whatsNew: 4000 };
const measure = (field, value) => {
  const n = (value ?? '').length;
  const limit = LIMITS[field];
  return limit ? `${n}/${limit}${n > limit ? '  OVER' : ''}` : `${n}`;
};

// Name and subtitle live on appInfos, not on a version: they are the same
// across platforms and change without a release.
const infos = await get(`/apps/${APP_ID}/appInfos`);
for (const info of infos.data) {
  const locs = await get(`/appInfos/${info.id}/appInfoLocalizations`);
  console.log(`\n===== APP INFO (${info.attributes.state}) =====`);
  for (const l of locs.data) {
    const a = l.attributes;
    console.log(`  --- ${a.locale} ---`);
    console.log(`  name       ${measure('name', a.name)}   ${a.name ?? ''}`);
    console.log(`  subtitle   ${measure('subtitle', a.subtitle)}   ${a.subtitle ?? ''}`);
  }
}

const versions = await get(`/apps/${APP_ID}/appStoreVersions?limit=10`);
for (const v of versions.data) {
  const a = v.attributes;
  console.log(`\n===== ${a.platform}  ${a.versionString}  ${a.appStoreState} =====`);
  const locs = await get(`/appStoreVersions/${v.id}/appStoreVersionLocalizations`);
  for (const l of locs.data) {
    const t = l.attributes;
    console.log(`\n  --- ${t.locale} ---`);
    console.log(`  keywords         ${measure('keywords', t.keywords)}`);
    console.log(`    ${t.keywords ?? '(empty)'}`);
    console.log(`  promotionalText  ${measure('promotionalText', t.promotionalText)}`);
    console.log(`  description      ${measure('description', t.description)}`);
    console.log(`  whatsNew         ${measure('whatsNew', t.whatsNew)}`);
    console.log(`    ${(t.whatsNew ?? '(empty)').split('\n')[0]}`);
    // The one number this project has already had wrong in six places.
    const stale = /paid back to 60 ms|снижается до 60 мс/.test(t.description ?? '');
    if (stale) console.log('  *** description still says 60 ms; FLOOR_MS is 200 ***');
  }
}

/**
 * Which production releases carry a given app version, for the listing guard
 * in `push_play_listing.mjs`. Pure, so it can be tested without Play.
 */

/** Whether a release `name` names `version` as a whole token: `1.2.1` must not
 *  match `1.2.10` or `11.2.1`. */
export function namesVersion(name, version) {
  if (!name || !version) return false;
  const escaped = version.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
  return new RegExp(`(^|[^\\d.])${escaped}([^\\d.]|$)`).test(name);
}

/**
 * Splits the production track's releases for `version`.
 *
 * Only a `completed` release is what every user can install, so only that
 * lets `--commit` publish listing copy (which goes live for everyone at once).
 * An `inProgress` staged rollout is live for some users and is reported, but
 * does not satisfy the guard. `draft` and `halted` releases install for nobody.
 */
export function productionStatus(releases, version) {
  const withCodes = (releases ?? []).filter((r) => r.versionCodes?.length);
  const named = (r) => r.name ?? '(unnamed)';
  const completed = withCodes.filter((r) => r.status === 'completed').map(named);
  const staged = withCodes.filter((r) => r.status === 'inProgress').map(named);
  const other = withCodes
    .filter((r) => r.status !== 'completed' && r.status !== 'inProgress')
    .map((r) => `${named(r)} [${r.status ?? 'unknown'}]`);
  return {
    completed,
    staged,
    other,
    released: completed.some((n) => namesVersion(n, version)),
    staging: staged.some((n) => namesVersion(n, version)),
  };
}

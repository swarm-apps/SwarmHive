# Integration gotchas

Every one of these has bitten a real integration. Skim before you start; revisit
when something "should work" but doesn't.

## Transport: a plain-`http://` server is rejected by default

The biggest silent failure. The server endpoint serving `200/204` over `curl`
does **not** mean the client can reach it:

- **Tauri**: release builds reject non-https with `InsecureTransportProtocol`
  (dev/debug only warns — which is why it passes local testing and breaks the
  shipped build). Add `dangerousInsecureTransportProtocol: true` under
  `plugins.updater`.
- **Android**: API 28+ blocks cleartext in release. Add `usesCleartextTraffic: true`
  via `expo-build-properties`.

Both flags are **dogfood-only escape hatches**. Production servers must be https;
then remove the flags.

## RN: `PortalHost` must be mounted, or dialogs silently no-render

RNR `Dialog`/`AlertDialog` render through a Portal. No `PortalHost` at the root →
the dialog "opens" (state flips) but nothing is drawn, no warning. First thing to
check when RN update dialogs don't appear.

## RN: `versionCode` must increment per release

The gate compares installed `versionCode` to the published one with strict `>`.
`app.json` with no `android.versionCode` → Expo defaults every build to `1` →
installed `1` is never `< published 1` → "no update" forever. Bump it per release
(or add a CI step that does). This defeats the whole point of the integration if
missed, and it looks like the server is wrong when it isn't.

## Missing copied files (e.g. RN `ports.ts`) = registry manifest bug

If a copied component imports a sibling that `shadcn add` didn't create (the RN
`rn-adapter` importing `./ports`, say), the registry item's `files` manifest is
missing that path, so it ships broken to **every** consumer. Symptom: `tsc` fails
`Cannot find module './ports'` right after a clean add. Stopgap: copy the canonical
file from `packages/registry-{rn,web}/registry/.../<file>` in the swarmhive repo.
Real fix (registry maintainer): add the path to `registry.json` and rebuild
`public/r`. Report it — don't just patch your copy and move on.

## `shadcn add --yes` does not answer overwrite prompts

`--yes` skips the "proceed?" prompt but **not** per-file "overwrite existing?".
In a non-TTY/agent run it hangs or clobbers. Pipe `yes n |` so existing shared
`ui/*` (dialog/button/progress/utils — already yours) are kept; only the new
SwarmHive files are written.

## i18n: dialogs default to English

`update-texts` defaults to the `en` preset; `UpdateProvider`/dialogs take a
`locale` (or `texts`) prop but if you don't pass it, a localized app shows English
update dialogs — a regression if you replaced localized bespoke UI. Pass your
app's current locale (the texts ship `zh-CN` too), or inject `texts`.

## Channel default is `stable`

The update endpoint serves the **stable** channel when none is specified (Tauri's
URL passes no channel; RN likewise). A `swarmhive publish --channel beta` lands on
beta and the client sees **nothing** until `swarmhive channels promote --name stable`.
Not a bug — release-train safety. (Publishing/promoting is the `swarmhive-cli` skill.)

## Error state has no dialog — handle it

The registry ships prompt/force/progress dialogs but **no error dialog**. A failed
`check()`/`download()` leaves `status === "error"` with no UI. Worse, a settings
"status" badge written as `hasUpdate ? … : isChecking ? … : "up to date"` will
render a green "up to date" on error — actively misreporting a failed check as
success. Give `"error"` its own branch (and ideally a retry via `useUpdate().check`
or `retry`).

## Environment: pnpm store drift + the shadcn deps install

`shadcn add` runs `pnpm add` for its npm deps. If the repo's `node_modules` was
linked from a now-moved pnpm store you'll see `ERR_PNPM_UNEXPECTED_STORE` (and a
non-TTY refusal to delete `node_modules`). Fix with `CI=true pnpm install` to
relink first, then re-run the add. Not a SwarmHive problem, but it stops the add
cold.

## Workspace Tauri apps: bundle path (cross-skill)

When publishing the build you just wired up: if `src-tauri` is a *member* of a
Cargo workspace, the bundle is at the **repo-root `target/`**, not
`src-tauri/target/`. That's a publish-side detail — see the `swarmhive-cli` skill.

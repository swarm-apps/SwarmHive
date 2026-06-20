# registry-rn — Expo React Native (Android)

The `@swarmhive-rn` registry. UI is **NativeWind + React Native Reusables (RNR)**
over `createRnAdapter`, which calls the SDK's `checkUpdateAndroid` and installs the
APK via Expo (`expo-file-system` download → `getContentUriAsync` → IntentLauncher
`ACTION_VIEW` → system PackageInstaller, fire-and-forget). Android-only; iOS goes
through TestFlight / App Store.

## components.json

```jsonc
"registries": {
  "@swarmhive-rn": "https://raw.githubusercontent.com/swarm-apps/swarmhive/main/packages/registry-rn/public/r/{name}.json",
  "@react-native-reusables": "https://reactnativereusables.com/r/nativewind/{name}.json"
}
```

The RN dialogs reference `@react-native-reusables/{dialog,alert-dialog,button,progress,text}`
— that namespace must be registered or `shadcn add` can't resolve the primitives.

## shadcn add

```bash
yes n | pnpm dlx shadcn@latest add \
  @swarmhive-rn/update-provider \
  @swarmhive-rn/prompt-update-dialog \
  @swarmhive-rn/force-update-dialog \
  @swarmhive-rn/update-progress-dialog \
  @swarmhive-rn/update-settings-section --yes
```

Cascade brings in `rn-adapter` + its lib files **`ports.ts`, `expo-installer.ts`,
`expo-downloader.ts`, `rn-storage.ts`**, plus `use-update`, `release-notes-view`,
`update-texts`, and the RNR primitives. npm deps: `@swarm-hive/sdk`,
`expo-file-system`, `expo-intent-launcher`, `@react-native-async-storage/async-storage`,
`lucide-react-native`. **Confirm `src/lib/ports.ts` exists after the add** — `rn-adapter`
imports `./ports`; if it's missing, the registry manifest dropped it (see gotchas).

## Wiring (`app/_layout.tsx`)

`UpdateProvider` **requires `baseUrl` + `appSlug`** — RN has no `tauri.conf.json`
to read the endpoint from. `createRnAdapter` defaults downloader/installer/storage
to the Expo implementations, so those are optional. Mount a `PortalHost` (RNR
dialogs render through a Portal — without it they don't appear, no error).

```tsx
<UpdateProvider baseUrl="https://updates.example.com" appSlug="swarmnote-rn">
  {/* ...app... */}
  <UpdateHost />     {/* prompt(open on "available") + force + progress */}
  <PortalHost />     {/* RNR Portal target — required */}
</UpdateProvider>
```

`UpdateHost` composes the three dialogs and opens the prompt when `status` becomes
`"available"`, handing off to the progress dialog once `downloading`. The provider
does `checkOnMount` and re-checks when `AppState` returns to `active` — that
AppState recheck is the **fallback for "user cancelled the system installer"**
(Android gives no reliable callback), so keep it.

`UpdateProvider` props: `baseUrl`*, `appSlug`*, `locale`/`texts` (i18n, default
English), `abi`, `channel`, `currentVersion`, and injectable `downloader`/`installer`/`storage`.

## `app.json` — version gate + transport + install permission

```jsonc
"android": {
  "package": "com.example.app",
  "versionCode": 2          // MUST increase every release, else the gate never fires
},
"plugins": [
  ["expo-build-properties", { "android": { "usesCleartextTraffic": true } }],
  // ^ ONLY for a plain-http dogfood server (Android 9+ blocks cleartext in release).
  //   Production must be https.
  "./plugins/with-android-install-permission"   // REQUEST_INSTALL_PACKAGES
]
```

The update gate is keyed on **`versionCode`** (the SDK compares
`Application.nativeBuildVersion` to the published `versionCode` with a strict `>`).
If `versionCode` doesn't increment per published build, installed == published and
`checkUpdateAndroid` always reports "no update" — the gate can never trigger.

## Verify

The Android endpoint returns **200 with a `has_update` boolean** (not Tauri's 204):

```bash
EP="https://updates.example.com/api/v1/updates/android/<slug>"
curl -s "$EP?current_version_code=0&current_version_name=0.0.0&abi=arm64-v8a"   # {has_update:true, download_url, sha256, size_bytes}
curl -s "$EP?current_version_code=<current>&current_version_name=<v>&abi=arm64-v8a" # {has_update:false}
```

`download_url` is a SwarmHive download redirect; `sha256` lets the installer verify
the APK. Same stable-channel default as Tauri — promote from `beta` to be served.

## NativeWind / RNR specifics

The vendored RNR primitives carry these (don't fight them):

- **prompt → `Dialog`** (closable, Close X); **force / progress → `AlertDialog`**
  (not closable, no X). Progress uses AlertDialog so there's no stray Close button
  during a download.
- Any dialog dismiss (back / scrim / Close / "Later") should call `postpone()` so
  the next AppState-active recheck doesn't immediately re-pop; `busy` (downloading/
  ready) dismiss just hides.
- Colors are semantic tokens (`bg-background`, `text-foreground`, …) resolved from
  the consumer's `global.css` — the registry never hardcodes hex, so the dialogs
  inherit your app's theme + dark mode automatically.

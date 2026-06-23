# registry-web — Tauri desktop / Electron / web

The `@swarmhive` registry. UI is shadcn (Radix) over `createTauriAdapter`, which
wraps `@tauri-apps/plugin-updater` (so minisign signature verification + the
download/install handles come from the plugin, not the SDK).

## components.json

```jsonc
"registries": {
  "@swarmhive": "https://raw.githubusercontent.com/swarm-apps/swarmhive/main/packages/registry-web/public/r/{name}.json"
}
```

## shadcn add

```bash
# yes n keeps your existing ui/* (dialog/button/progress/utils) instead of overwriting
yes n | pnpm dlx shadcn@latest add \
  @swarmhive/update-provider \
  @swarmhive/prompt-update-dialog \
  @swarmhive/force-update-dialog \
  @swarmhive/update-progress-dialog \
  @swarmhive/update-settings-section --yes
```

Cascade brings in: `tauri-adapter`, `use-update`, `release-notes-view`,
`update-texts`, and `@shadcn` `dialog`/`button`/`progress`. npm deps added:
`@swarm-hive/sdk`, `@tauri-apps/plugin-updater`, `@tauri-apps/plugin-process`,
`@tauri-apps/plugin-store`, `lucide-react`.

Files land at your aliases, e.g. `src/lib/tauri-adapter.ts`,
`src/hooks/use-update.ts`, `src/components/{update-provider,prompt-update-dialog,…}.tsx`.

## `tauri.conf.json` — point the updater at SwarmHive

```jsonc
"plugins": {
  "updater": {
    "pubkey": "<your minisign pubkey>",
    "endpoints": [
      "https://updates.example.com/api/v1/updates/tauri/<app-slug>?current_version={{current_version}}&target={{target}}&arch={{arch}}"
    ],
    // ONLY if the server is plain http (dogfood). Release builds otherwise reject
    // non-https with InsecureTransportProtocol. Production must be https.
    "dangerousInsecureTransportProtocol": true,
    "windows": { "installMode": "passive" }
  }
}
```

- `{{target}}` is the OS (`darwin`/`windows`/`linux`), `{{arch}}` is separate
  (`aarch64`/`x86_64`) — the server expects them split, not a merged triple.
- The endpoint serves the **stable** channel by default; nothing else is passed.
- `createUpdaterArtifacts: true` in `bundle` so the build emits the `.app.tar.gz`
  + `.sig` the updater verifies (signed by the same key as `pubkey`).
- **Remove stale capability scopes**: if you migrated off another updater, delete
  its `http:default` allow-URL from `capabilities/*.json`. The updater plugin uses
  its own transport; a leftover scope is dead config that contradicts the migration.

## Wiring (TanStack Router example)

`UpdateProvider` takes **no props** — `createTauriAdapter` reads the endpoint from
`tauri.conf.json` via `plugin-updater`'s `check()`. The prompt dialog is controlled
(`open`/`onOpenChange`); force + progress self-manage by status. Open the prompt
when status transitions into `"available"`.

```tsx
function RootLayout() {
  return (
    <UpdateProvider>      {/* checkOnMount + recheckOnFocus built in */}
      <UpdateGate />
    </UpdateProvider>
  );
}

function UpdateGate() {
  const { status } = useUpdate();          // must be inside UpdateProvider
  const [promptOpen, setPromptOpen] = useState(false);
  const prev = useRef(status);
  useEffect(() => {
    if (prev.current !== "available" && status === "available") setPromptOpen(true);
    prev.current = status;
  }, [status]);
  return (
    <>
      <Outlet />
      <ForceUpdateDialog />                  {/* shows on "force-required" */}
      <PromptUpdateDialog open={promptOpen} onOpenChange={setPromptOpen} />
    </>
  );
}
```

`UpdateProvider` props worth knowing: `locale` / `texts` (i18n — default is English,
pass your app's locale or the dialogs ship English), `checkOnMount`,
`recheckOnFocus`, `currentVersion` (override), `engine.{dismissTtlMs,recheckIntervalMs}`.

A settings "check for updates" surface uses the same `useUpdate()` (`status`,
`release`, `progress`, `check`, `download`). `progress.percent` is **0–1** — multiply
by 100 for a percent display.

## Verify

```bash
EP="https://updates.example.com/api/v1/updates/tauri/<slug>"
curl -s -o /dev/null -w "%{http_code}\n" "$EP?current_version=0.4.0&target=darwin&arch=aarch64"  # 200 (update)
curl -s -o /dev/null -w "%{http_code}\n" "$EP?current_version=<current>&target=darwin&arch=aarch64" # 204 (none)
```

200 body is a flat Tauri-updater JSON: `version` / `url` (SwarmHive download
redirect) / `signature` (minisign) / `notes` + a `swarmhive` object
(`upgrade_type`, `rollout_percent`, `channel`). `upgrade_type` is computed by the
server from release policy (`min_version` / rollout): `force` drives `ForceUpdateDialog`,
`prompt` drives `PromptUpdateDialog`. If you published to `beta`, the default
(stable) endpoint returns 204 until `swarmhive channels promote --name stable`.

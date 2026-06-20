# SwarmHive Agent Skills

Reusable [agent skills](https://github.com/obra/skills) for working with SwarmHive. Install one into
your own project (Claude Code / compatible agents) with `npx skills`:

## swarmhive-cli

Drive the `swarmhive` CLI to publish and manage app update releases (Tauri desktop + React Native
Android): `init` / `verify` / `publish` / channel `promote` / `rollback`, plus storage & mail admin.
Encodes the CLI's non-interactive AI contract so an agent can run releases safely and correctly.

```bash
npx skills add https://github.com/swarm-apps/swarmhive/tree/main/skills/swarmhive-cli
```

Best installed in the **consumer app repo** you publish from (e.g. SwarmDrop, SwarmNote, SwarmNote-RN),
where you run `swarmhive publish`. Set `SWARMHIVE_TOKEN` (and optionally `SWARMHIVE_SERVER`) in the
environment first.

## swarmhive-update-ui

Integrate the SwarmHive **in-app update UI** into a host app — the client-side counterpart to
publishing. Covers `shadcn add` of the update components (`@swarmhive` registry-web for Tauri/web,
`@swarmhive-rn` registry-rn for Expo Android), wiring `<UpdateProvider>` + the prompt/force/progress
dialogs over the headless `@swarm-hive/sdk` engine, configuring the endpoint + transport, and the
integration gotchas (PortalHost, Android `versionCode`, http transport flags, stable-channel default,
i18n locale).

```bash
npx skills add https://github.com/swarm-apps/swarmhive/tree/main/skills/swarmhive-update-ui
```

Best installed in the **consumer app repo** whose update UI you're wiring (e.g. SwarmDrop, SwarmNote,
SwarmNote-RN). Pairs with `swarmhive-cli`: that ships releases from the server side, this wires the
update UI on the client side.

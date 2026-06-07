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

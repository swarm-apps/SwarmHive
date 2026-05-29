export default async function globalTeardown(): Promise<void> {
  const ctx = globalThis.__SWARMHIVE_E2E__;
  if (!ctx) return;

  if (ctx.serverPid > 0) {
    try {
      process.kill(ctx.serverPid);
    } catch {
      // already exited
    }
  }

  if (ctx.container) {
    try {
      await ctx.container.stop({ remove: true, removeVolumes: true });
    } catch {
      // container already stopped
    }
  }
}

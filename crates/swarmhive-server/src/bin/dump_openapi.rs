//! Dumps the live OpenAPI document to stdout without booting a database.
//!
//! Used by `pnpm --filter @swarmhive/admin openapi`-style codegen so the
//! SPA's TypeScript types stay in lockstep with the server's `#[utoipa::path]`
//! annotations even when no dev server is running.

fn main() {
    let doc = swarmhive_server::openapi_doc();
    let json = serde_json::to_string_pretty(&doc).expect("serialize openapi");
    println!("{json}");
}

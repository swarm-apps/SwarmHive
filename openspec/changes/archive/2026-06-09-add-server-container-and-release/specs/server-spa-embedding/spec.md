## ADDED Requirements

### Requirement: Server SHALL serve the embedded admin SPA under a build feature

The server SHALL, when built with the `embed-spa` cargo feature, embed the `apps/admin/dist` bundle into the binary via `rust-embed` and register a router `fallback` that serves the SPA: a request path matching an embedded asset SHALL return `200` with a `Content-Type` derived from the asset's extension, and any other path that is not an already-registered route SHALL return the SPA `index.html` with `200` so the client-side router can handle it. When the `embed-spa` feature is absent the server SHALL NOT register the fallback and unmatched routes SHALL keep their default `404` behavior, so default `cargo build` / `cargo test` need no built SPA.

#### Scenario: Root path serves the SPA shell

- **GIVEN** the server is built with `--features embed-spa` after `pnpm admin:build`
- **WHEN** a browser GETs `/`
- **THEN** the response is `200` with `Content-Type: text/html` and the body is the admin SPA `index.html`

#### Scenario: Embedded static asset is served with its mime type

- **GIVEN** `apps/admin/dist/assets/<hashed>.js` was embedded
- **WHEN** a client GETs that exact asset path
- **THEN** the response is `200` with `Content-Type: text/javascript` (or `application/javascript`) and the embedded bytes

#### Scenario: Unknown client-side route falls back to index.html

- **WHEN** a client GETs a non-API path that is not an embedded asset (e.g. `/apps/swarmdrop`)
- **THEN** the response is `200` serving `index.html` (the SPA router renders the route), not `404`

#### Scenario: API and health routes are never shadowed by the fallback

- **GIVEN** the server is built with `embed-spa`
- **WHEN** a client GETs `/api/v1/version`, `/api/openapi.json`, `/api/docs`, or `/healthz`
- **THEN** each returns its own handler's response, because `fallback` only catches routes the router did not already match

#### Scenario: Default build keeps 404 for unmatched routes

- **GIVEN** the server is built WITHOUT the `embed-spa` feature (the default)
- **WHEN** a client GETs an unregistered path such as `/some-spa-route`
- **THEN** the response is `404` (no SPA fallback is registered) and the build required no `apps/admin/dist`

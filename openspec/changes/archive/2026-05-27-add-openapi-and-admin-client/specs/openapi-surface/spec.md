# openapi-surface

## ADDED Requirements

### Requirement: Server SHALL expose a complete OpenAPI 3.1 document

The server SHALL serve a complete, machine-readable OpenAPI 3.1 document at `GET /api/openapi.json` that enumerates every HTTP endpoint registered on the application router, including their request bodies, response bodies, status codes, and tags.

The document SHALL be served without authentication.

#### Scenario: All registered endpoints are listed

- **WHEN** a client issues `GET /api/openapi.json`
- **THEN** the response is `200 OK` with `Content-Type: application/json`
- **AND** the JSON body's `paths` object contains keys for every endpoint registered on the application router (e.g. `/healthz`, `/api/v1/version`, `/api/v1/auth/login`, `/api/v1/auth/logout`, `/api/v1/auth/me`, `/api/v1/setup/info`, `/api/v1/setup`, `/api/v1/_demo/release-publish`)
- **AND** the document's `info.version` field equals the running server binary's package version

#### Scenario: No authentication required

- **WHEN** an unauthenticated client (no cookie, no bearer token) issues `GET /api/openapi.json`
- **THEN** the response is `200 OK`
- **AND** the body is the full OpenAPI document (not a redirect, not a `401` problem+json)

### Requirement: Every endpoint SHALL document its standard error responses

Every endpoint registered on the application router SHALL include, in its OpenAPI `responses` map, the standard error status codes that the server's `ApiError` type can produce: `401`, `403`, `404`, `409`, `410`, `422`, and `500`. Each error response SHALL reference the shared `Problem` schema (RFC 9457 `application/problem+json` body).

#### Scenario: Endpoint inherits the full error response set

- **WHEN** a client inspects any endpoint's `responses` map in the OpenAPI document
- **THEN** the map contains entries for `401`, `403`, `404`, `409`, `410`, `422`, and `500`
- **AND** each entry's `content."application/problem+json".schema.$ref` resolves to the `Problem` component schema

#### Scenario: Problem schema matches the RFC 9457 wire format

- **WHEN** a client resolves the `Problem` component schema in the OpenAPI document
- **THEN** the schema has properties `type`, `title`, `status`, `detail` (all required), and optional `required_permission`
- **AND** the `type` property is a JSON string (not `type_uri` — the serde rename is reflected)

### Requirement: Server SHALL expose a human-readable Redoc UI

The server SHALL serve a Redoc UI at `GET /api/docs` that renders the OpenAPI document for human consumption. Access SHALL NOT require authentication. The UI SHALL render endpoints grouped by the OpenAPI `tag` value.

#### Scenario: Redoc UI loads in browser

- **WHEN** a user opens `http://<host>/api/docs` in a browser
- **THEN** the response is `200 OK` with `Content-Type: text/html`
- **AND** the rendered page is a Redoc-based API explorer that references `/api/openapi.json`

### Requirement: Endpoints SHALL be tagged for grouping

Every endpoint registered on the application router SHALL carry exactly one OpenAPI `tag` drawn from a fixed set: `health`, `version`, `auth`, `setup`, `internal`. Tags drive Redoc's left-sidebar grouping. Endpoints intended to be removed by a later proposal SHALL use the `internal` tag with a description that names the proposal scheduled to remove them.

#### Scenario: Tags partition the endpoints

- **WHEN** a client inspects the OpenAPI document's `tags` array and each operation's `tags` field
- **THEN** every operation has exactly one tag value
- **AND** every tag value is one of `health`, `version`, `auth`, `setup`, `internal`
- **AND** the `/api/v1/_demo/release-publish` operation has tag `internal` with a description mentioning `add-app-release-artifact`

### Requirement: OpenAPI endpoints SHALL NOT be rate-limited

The endpoints `GET /api/openapi.json` and `GET /api/docs` SHALL be exempt from the per-IP rate-limit layer that protects `/api/v1/auth/*` and `/api/v1/setup*`. Documentation traffic (dev refreshes, client codegen) is high-frequency by nature and SHALL NOT trigger throttling.

#### Scenario: Repeated documentation fetches do not trigger 429

- **WHEN** a client issues 100 consecutive `GET /api/openapi.json` requests from the same IP within one second
- **THEN** no response returns `429 Too Many Requests`
- **AND** every response is `200 OK` with the full OpenAPI body

### Requirement: All DTOs touched by current endpoints SHALL appear as component schemas

Request and response body types referenced by any documented endpoint SHALL appear in the OpenAPI document's `components.schemas` map, referenced via `$ref` from endpoint operations. For the surface area shipped by this proposal, that means at minimum: `User`, `UserStatus`, `PermissionName`, `Problem`, `HealthResponse`, `VersionResponse`, `LoginReq`, `MeResponse`, `SetupReq`, `SetupInfo`.

`Permission` and `Role` from `swarmhive-api-types` are *not* required in this proposal — no current endpoint takes or returns them. They will land naturally when `add-custom-roles` / role-management endpoints ship.

#### Scenario: Component schemas are populated and referenced

- **WHEN** a client inspects `components.schemas` and any operation's `requestBody` / `responses[*].content`
- **THEN** `components.schemas` contains at minimum `User`, `UserStatus`, `PermissionName`, `Problem`, `HealthResponse`, `VersionResponse`, `LoginReq`, `MeResponse`, `SetupReq`, `SetupInfo`
- **AND** operations reference these schemas via `$ref: "#/components/schemas/<Name>"` rather than inlining the schema body

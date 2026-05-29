# storage-wizard-page

## ADDED Requirements

### Requirement: Admin SHALL expose a reachable storage settings page

The settings menu item for storage SHALL be an enabled link to `/settings/storage` (no longer a disabled placeholder), and the settings parent menu SHALL be visible to users holding `storage:manage` (in addition to `mail:manage`). The page SHALL list configured S3 backends from `GET /api/v1/storage/backends`, showing name, endpoint, bucket, whether active, URL mode, sha256-checksum support, and connectivity status.

#### Scenario: Storage menu is reachable for a storage manager

- **GIVEN** an authenticated user holding `storage:manage`
- **WHEN** the user opens the settings area
- **THEN** a "Storage" menu item is shown as an enabled link to `/settings/storage`

#### Scenario: Backends list renders

- **GIVEN** one configured backend
- **WHEN** the user opens `/settings/storage`
- **THEN** the backend's name, endpoint, bucket, and active state are shown

### Requirement: Admin SHALL create a backend with optional presets

The page SHALL provide a create form covering name, endpoint, bucket, region, access key id, access key secret, force-path-style, optional prefix, optional public base URL, URL mode (public / signed), and signed-URL TTL. It SHALL POST `/api/v1/storage/backends`, gated on `storage:manage`. Selecting a preset (RustFS / Aliyun OSS / custom) SHALL prefill `force_path_style` and `url_mode` defaults; the preset value itself is not submitted.

#### Scenario: RustFS preset prefills path-style

- **GIVEN** the create form is open
- **WHEN** the user selects the RustFS preset
- **THEN** force-path-style is enabled in the form

#### Scenario: Creating a backend adds it to the list

- **GIVEN** a user holding `storage:manage`
- **WHEN** the user submits a valid backend form
- **THEN** the request POSTs `/api/v1/storage/backends` and the new backend appears after refetch

### Requirement: Admin SHALL edit a backend without resubmitting the secret

The edit form SHALL PATCH `/api/v1/storage/backends/:id`. When the secret field is left empty the request SHALL omit `access_key_secret` so the stored secret is preserved. The form SHALL pre-fill the selected backend's non-secret fields on each open (re-mounting per row).

#### Scenario: Empty secret preserves the stored secret

- **GIVEN** a backend with a stored secret (`secret_set` is true)
- **WHEN** the user edits it and saves with the secret field empty
- **THEN** the PATCH body omits `access_key_secret` and `secret_set` remains true

### Requirement: Admin SHALL test backend connectivity

The page SHALL offer a test action calling `POST /api/v1/storage/backends/:id/test`, surfacing the result: on success a message indicating connectivity and whether sha256 checksums are supported; on failure the returned detail. After a test the backends list SHALL be refetched so updated checksum/connectivity status is reflected.

#### Scenario: Successful test reports checksum support

- **GIVEN** a reachable backend
- **WHEN** the user runs the test
- **THEN** a success message is shown indicating whether sha256 checksums are supported
- **AND** the list refetches

#### Scenario: Failed test shows the detail

- **GIVEN** a misconfigured backend
- **WHEN** the user runs the test
- **THEN** an error message shows the returned failure detail

### Requirement: Admin SHALL activate a backend

The page SHALL offer an activate action (with confirmation) calling `POST /api/v1/storage/backends/:id/activate`, gated on `storage:manage`. After activation the activated backend SHALL be shown active and the previously active one inactive.

#### Scenario: Activating switches the active backend

- **GIVEN** two backends, one active
- **WHEN** the user activates the other and confirms
- **THEN** after refetch the newly activated backend is active and the other is not

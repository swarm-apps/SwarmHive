# notification-delivery-attempts Specification

## Purpose
TBD - created by archiving change add-notification-delivery-attempts. Update Purpose after archive.
## Requirements
### Requirement: Per-attempt delivery history

The worker SHALL append a delivery-attempt row for each delivery attempt, recording that attempt's status, response code, request/response snapshot, and error, so the full retry timeline is retained rather than overwritten.

#### Scenario: Each attempt appends a history row

- **WHEN** a delivery is attempted, fails, retried, and eventually marked dead
- **THEN** one attempt row exists per attempt, with increasing attempt numbers
- **AND** each row carries that attempt's status and response code

#### Scenario: Redelivery appends rather than clears

- **WHEN** an operator redelivers a delivery
- **THEN** the next attempt appends a new history row without removing earlier ones

### Requirement: Delivery detail includes the attempt timeline

The delivery detail endpoint SHALL include the delivery's attempts ordered by attempt number.

#### Scenario: Detail returns the timeline

- **WHEN** an authorized caller requests a delivery by id
- **THEN** the response includes an attempts array ordered by attempt number
- **AND** each attempt carries its status, response code, and error


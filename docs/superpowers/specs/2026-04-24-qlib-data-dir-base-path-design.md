# QLIB_DATA_DIR Base Path Design

## Summary

This change redefines `QLIB_DATA_DIR` in the Python model service as a base directory instead of a direct Qlib provider path. The service will resolve the actual Qlib US daily provider path as `<QLIB_DATA_DIR>/qlib/us_data`.

The change is intentionally narrow:

- keep existing `/data/update` behavior
- keep existing `/train` and `/features` Qlib provider usage
- change only path resolution semantics and related validation

## Goals

- make `QLIB_DATA_DIR` mean one stable thing: a base directory
- remove repeated hard-coded assumptions that `QLIB_DATA_DIR` points directly to `.../us_data`
- centralize provider path resolution in one shared helper
- fail fast when the environment variable still uses the old direct-provider form

## Non-Goals

- rewriting `/data/update`
- replacing `qlib_scripts`
- changing training data format away from Qlib provider files
- changing Rust crates or runtime configuration outside `services/model`

## Current State

Three model-service modules read `QLIB_DATA_DIR` directly and currently default to `~/.qlib/qlib_data/us_data`:

- `services/model/workflow/data_update.py`
- `services/model/workflow/features.py`
- `services/model/workflow/train.py`

This spreads path semantics across multiple files and keeps the old assumption that the environment variable is already the final provider path.

## Proposed Design

### Environment Variable Semantics

- `QLIB_DATA_DIR` is the base data directory
- default base directory is `~/.qlib`
- resolved provider directory is `<base>/qlib/us_data`
- the system does not support the old form where `QLIB_DATA_DIR` itself points to `.../us_data`

Examples:

- unset `QLIB_DATA_DIR` -> provider path resolves to `~/.qlib/qlib/us_data`
- `QLIB_DATA_DIR=C:\data\market` -> provider path resolves to `C:\data\market\qlib\us_data`

Invalid example:

- `QLIB_DATA_DIR=C:\Users\Hi\.qlib\qlib_data\us_data`

### Shared Path Helper

Add one shared helper in the model service for Qlib path resolution. The helper is responsible for:

- reading `QLIB_DATA_DIR`
- applying the default base directory
- resolving the provider path as `<base>/qlib/us_data`
- rejecting values that already end in `us_data`

The helper should return a `Path` so callers can keep their existing usage patterns.

## Affected Code

All direct reads of `QLIB_DATA_DIR` in the Python model service will be replaced by the shared helper:

- `services/model/workflow/data_update.py`
- `services/model/workflow/features.py`
- `services/model/workflow/train.py`

`/data/update` continues to use the resolved provider path for update output. `/train` and `/features` continue to initialize Qlib against the resolved provider path.

## Error Handling

The new helper validates configuration syntax, not dataset readiness.

### Invalid Environment Variable

If `QLIB_DATA_DIR` is set to a path that already ends with `us_data`, the helper raises a clear error that explains:

- `QLIB_DATA_DIR` must now be a base directory
- the provider path is derived as `<base>/qlib/us_data`
- an example of a valid value

This is an intentional breaking change for old local configuration.

### Missing Provider Directory

The helper does not fail just because `<base>/qlib/us_data` does not exist yet.

Reason:

- `/data/update` may need to create or populate that location
- `/train` and `/features` should keep surfacing their existing runtime errors when data is unavailable

This keeps responsibilities clear:

- path helper: validate path meaning
- feature/train/update flows: handle actual data availability

## Testing

Add tests for the shared path resolution:

- default behavior resolves to `~/.qlib/qlib/us_data`
- configured base directory resolves to `<base>/qlib/us_data`
- old direct-provider values ending in `us_data` are rejected with a clear error

Update affected model-service tests so they assert the new path contract where needed.

## Documentation

Update operator-facing docs to match the new contract:

- `docs/runbook.md`
- any relevant `services/model` readme or usage notes that mention `QLIB_DATA_DIR`

Required documentation wording:

- `QLIB_DATA_DIR` is the base directory
- actual provider location is `<QLIB_DATA_DIR>/qlib/us_data`
- old `.../us_data` values are no longer valid

## Risks

### Main Risk

Existing local environments that still set `QLIB_DATA_DIR` to a direct provider path will fail immediately.

This is acceptable because the change is explicit and the failure mode is easy to diagnose.

### Low-Risk Areas

- no change to training algorithm behavior
- no change to data update algorithm behavior
- no change to Qlib data format

## Rollback

Rollback is straightforward:

- restore the old direct environment variable reads
- remove the strict validation
- restore documentation examples

## Implementation Notes

- keep the change scoped to `services/model`
- avoid introducing a larger configuration abstraction in this iteration
- prefer one small shared module or helper function over repeated path logic

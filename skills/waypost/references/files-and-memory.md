# Files and source memory

## Verified files

Use `waypost_blob_prepare` with the actual file's content type, exact nonnegative byte length, SHA-256 lowercase hexadecimal and optional filename. A zero-byte file is valid. The service returns `upload_id`, `blob_id`, an authenticated HTTP PUT URL and required headers. Upload exactly those bytes using the host's available HTTP/file capability, then call `waypost_blob_commit` with a durable request ID.

Do not expose bearer tokens to the model's ordinary message text, reports, memory metadata or export package. Do not substitute a bucket path or arbitrary external download URL. When downloading, use `waypost_blob_get`, authenticated HTTP transport and verify the actual byte length/SHA-256.

A missing file transfer capability is a host limitation; do not claim an attachment was uploaded. Preserve the requested delivery: sending a text-only partial result requires user authorization when the user asked for the attachment too. Sealed files are immutable. Retry unsealed uploads against their original reservation; sealed PUTs are rejected. Read permission alone does not grant permission to re-share a Blob.

## Publish memory

Use an already created, currently writable Collection and its actual revision. Collection source/scope are fixed; external file timestamps do not replace the server revision.

- Full `snapshot`: replace the entire authorized Collection. Omitted active items become source-local Tombstones. An empty snapshot clears that Collection and requires explicit complete-scope authorization.
- Partial `delta`: upsert only observed items and explicitly delete only confirmed deletions. Omission preserves the rest. Never turn an unavailable file into a deletion.

Use unique item IDs and unique relative paths, without absolute paths, `..`, empty path segments or backslashes. Each item must reference a sealed Blob the current Actor can re-reference. `items` and `delete_ids` must not intersect.

Respect the deployment's change/item/byte limits. A sealed JSON manifest (`{"items": [...], "delete_ids": [...]}`) can replace inline changes when advertised limits allow it; do not supply both forms. It cannot bypass schema, permission or capacity checks.

Persist the exact publish request and ID before `waypost_memory_publish`. Check the original idempotent outcome before changing `expected_revision`. On `REVISION_CONFLICT`, read the current authorized revision and compare source changes; do not merely increase the expected revision and overwrite. Preserve independently edited items or report an unresolved conflict.

## Read and export

Use `waypost_memory_read`. Capture the returned actual revision on the first page and preserve `next_cursor`; subsequent pages remain on that revision. Include Tombstones when reproducing the complete state. An explicit revision must match the cursor. Revision 0 is a valid empty collection.

Fetch actual referenced Blob bytes and verify sealed metadata. Produce a portable package containing source/scope, the fixed revision, active items, deletion markers, safe provenance metadata and exact original files. Never include connection credentials, signing keys or object-store credentials. The repository's `examples/consumer.py export` provides a SQLite-backed archive example.

A successful retry may return its original older revision after the head advances. That is the original publication receipt, not proof that the current head still equals that version.

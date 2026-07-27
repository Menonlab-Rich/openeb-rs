# openeb-rs

`openeb-rs` is a Rust workspace for working with OpenEB-style event camera data and device abstractions.

The repository is split into a small set of focused crates:

- `openeb-core` contains the HAL-style abstractions, event types, dispatchers, decoder traits, and device traits.
- `openeb-devices` provides file-backed device support, including raw `.raw` reader logic, header parsing, indexing, and iterator helpers.
- `utilities` contains shared support code, currently centered on pooled buffers.
- `macros` re-exports shared declarative macros and the custom derive helper.
- `local-proc-macros` contains the local procedural macro implementation used by the workspace.

## Workspace layout

```text
.
├── Cargo.toml
├── core/
├── openeb-devices/
├── utilities/
├── macros/
└── local-proc-macros/
```

## Crate overview

### `openeb-core`

`openeb-core` defines the core model used across the workspace.

Key areas:

- `hal/types.rs` defines the primary event types (`EventCD`, `EventExtTrigger`) and shared type aliases.
- `hal/errors.rs` defines decoder, stream, hardware, and processing error types.
- `hal/facilities.rs` defines the facility trait hierarchy and the `FacilityHandle` / `FacilityType` system used to register and retrieve capabilities from devices.
- `hal/dispatcher.rs` contains the event and error dispatchers used to fan out decoded data to subscribers.
- `hal/decoders/` contains decoder implementations and shared decoder traits.
- `hal/device/` contains the device trait and discovery-related types.
- `camera.rs` is the start of a C++/FFI-backed camera abstraction and is currently incomplete.

### `openeb-devices`

`openeb-devices` is the file-backed device layer.

Its current focus is reading raw event files and exposing them through the same facility model used by `openeb-core`.

Important modules:

- `header.rs` parses the metadata header from raw files and derives sensor information from it.
- `types.rs` defines file-level errors, file format helpers, raw buffer helpers, and index types.
- `raw/device.rs` builds a `RawFileHandler` that wires a file, stream, decoder, and facilities together.
- `raw/stream.rs` implements the buffered file stream facility.
- `raw/decoder.rs` wraps the concrete decoder behind a shared `RawFormatDecoder` interface.
- `raw/reader.rs` exposes the public `RawFileReader` API for opening files, loading batches, seeking, and subscribing to decoded events.
- `raw/iterator.rs` provides `EventWindowIterator` for fixed-size or time-window event consumption.
- `raw/index.rs` builds a timestamp index to support seeking.
- `raw/facilities.rs` provides raw-file implementations of geometry and hardware-identification facilities.

### `utilities`

`utilities` currently provides `PooledBuffer<T>`, a small recycling wrapper around `Vec<T>`.

The decoders use it to batch events and return vectors to a pool when the batch drops.

### `macros`

`macros` re-exports:

- `derive_new::new`
- `paste`
- the local `derive_value` attribute macro

It also defines workspace macros used by the facility layer:

- `property!`
- `pack_facility!`

### `local-proc-macros`

This crate contains the `derive_value` procedural macro, which applies a standard `Debug + Clone + Copy + PartialEq + Eq` derive set to value types.

## Runtime flow

The raw-file path currently works like this:

1. `RawFileReader::try_from_file` opens a raw file and parses the header.
2. The reader constructs a `RawFileHandler` and registers the facilities it exposes.
3. The file stream is read in chunks by `RREventStream`.
4. `RREventStreamDecoder` selects the concrete raw decoder for the file format.
5. The decoder batches decoded events and publishes them through `EventDispatcher`.
6. Consumers subscribe to decoded CD or external-trigger event batches through the reader APIs.
7. Optional indexing is built from the file to support `seek`.

## Current status and limitations

This codebase is functional in structure but not complete in implementation.

Notable gaps:

- `openeb-core/src/camera.rs` still contains a `todo!()` and is not usable yet.
- `RREventStreamDecoder` supports `EVT3` today; the `EVT2`, `DAT`, and `HDF5` branches are still `todo!()`.
- `openeb-core/src/hal/decoders/evt2.rs` is present but unfinished.
- Several facility definitions in `openeb-core/src/hal/facilities.rs` are scaffolding or placeholders.
- The code currently assumes raw files with a header format understood by `header.rs`.

## Building

Use Cargo from the workspace root:

```bash
cargo check
```

## Notes for contributors

- The workspace is organized around the facility abstraction in `openeb-core`. When adding new device behavior, prefer modeling it as a facility rather than adding ad hoc methods.
- Shared event batching should continue to use `PooledBuffer<T>` so vector reuse stays centralized.
- If you add a new raw file format, update the format selection in `openeb-devices/src/raw/decoder.rs` and make sure the header parser can detect it.


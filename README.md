# openeb-rs

`openeb-rs` is a single Rust crate for OpenEB-style event camera data and
device abstractions.

The HAL and shared buffer utilities are always available. File-backed raw
device support is optional:

```toml
[dependencies]
openeb-rs = { version = "0.1", features = ["devices"] }
```

The `all` feature enables every optional component and is the default feature.
Use `default-features = false` for a core-only build.

## Layout

```text
.
├── Cargo.toml
└── src/
    ├── hal/       # HAL abstractions, facilities, dispatchers, and decoders
    ├── buffer.rs  # pooled event buffers
    └── devices/   # feature-gated raw-file support
```

The primary modules are:

- `hal` contains event types, decoder and device traits, facilities, and
  dispatchers.
- `buffer` provides `PooledBuffer<T>` for reusable decoded-event batches.
- `devices` contains raw-file header parsing, indexing, stream/decoder wiring,
  and reader/iterator APIs. Its main types are also re-exported at the crate
  root when the `devices` feature is enabled.

## Using `RawFileHandler`

`RawFileHandler` is the low-level, facility-oriented API for a raw event file.
It parses the header and exposes the file's geometry, hardware metadata, ROI,
event stream, and decoder through the HAL device interface. The const generic
sets the stream read-buffer size:

```rust,no_run
use openeb::RawFileHandler;
use openeb::hal::device::device::Device;
use openeb::hal::facilities::FacilityType;

let device = RawFileHandler::<131_072>::new_from_path("events.raw")?;
let geometry = device
    .get_facility(FacilityType::GeometryFacility)
    .expect("raw files provide geometry");
```

For decoded event batches, indexing, seeking, and subscriptions, use
`RawFileReader` instead. EVT3 decoding is currently supported; EVT2, DAT, and
HDF5 decoder paths are not yet implemented.

The `property!` and `pack_facility!` macros, along with the `new` derive
re-export, now live directly in the crate. The previous standalone macro and
procedural-macro packages are no longer needed.

## Runtime flow

1. `RawFileReader::try_from_file` opens a raw file and parses its header.
2. The reader constructs a `RawFileHandler` and registers its facilities.
3. `RREventStream` reads the file in chunks.
4. `RREventStreamDecoder` selects the concrete raw decoder.
5. Decoded events are batched and published through `EventDispatcher`.
6. Consumers subscribe through the reader APIs; optional indexing supports
   `seek`.

## Building

```bash
cargo check                         # default/all features
cargo check --no-default-features    # core only
cargo test --all-features
```

The implementation is incomplete: EVT3 is the currently supported raw decoder,
while EVT2, DAT, and HDF5 paths remain unfinished, and several facilities are
still scaffolding.

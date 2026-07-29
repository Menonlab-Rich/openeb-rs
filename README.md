# openevt

`openevt` is an independently maintained Rust crate for OpenEB-inspired event
camera data and device abstractions. It is related to Prophesee's OpenEB only
in spirit and is not endorsed, sponsored, or maintained by Prophesee.

The HAL and shared buffer utilities are always available. File-backed raw
device support is optional:

```toml
[dependencies]
openevt = { version = "0.1", features = ["devices"] }
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
use openevt::RawFileHandler;
use openevt::hal::device::device::Device;
use openevt::hal::facilities::FacilityType;

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
cargo build --features python       # optional Python/NumPy bindings
```

To build Python wheels locally, install Maturin and run:

```bash
python -m pip install maturin
maturin build --release
```

The resulting wheels are written to `target/wheels/`. Test them in a clean
environment before uploading with `maturin publish` or a PyPI publishing
tool. The published distribution and importable module are named `openevt`:

```bash
python -m pip install openevt
```

## Attribution and independence

This project is a separate rewrite inspired by the architecture and event
camera abstractions in [Prophesee's OpenEB](https://github.com/prophesee-ai/openeb).
We gratefully credit the Prophesee team for that foundational work. `openevt`
is independently maintained and is not affiliated with, endorsed by, or
sponsored by Prophesee.

The implementation is incomplete: EVT3 is the currently supported raw decoder,
while EVT2, DAT, and HDF5 paths remain unfinished, and several facilities are
still scaffolding.

# Raw file device plugin example

`RawFilePlugin` is the reference implementation for adapting an existing
native device to the `abi_stable` plugin boundary. It is intentionally an
adapter, not a second decoder.

## Build and load

Build the library with both device and plugin support:

```sh
cargo build --release --features devices,plugins
```

The resulting `openevt` cdylib exports the `openevt_device_plugin` root module.
Place it in one of the standard plugin directories, or point the host at its
directory with `OPENEVT_PLUGIN_PATH`.

To make raw files discoverable, set `OPENEVT_RAW_FILES` to a platform-separated
list of EVT3 paths. The discovery serial is the path, so the host can pass that
serial to `PluginRegistry::open_device`.

```rust,no_run
use openevt::hal::device::discovery::PluginRegistry;

let mut registry = PluginRegistry::new();
registry.load_default_paths();
for camera in registry.list_devices() {
    println!("{}", camera.plugin_info.serial);
}
```

## Event channel behavior

The native reader has crossbeam receivers for CD and external-trigger batches.
Those receivers never cross the ABI. Instead:

1. `start_events` calls `RawFileReader::cd_receiver` and retains the receiver.
2. `start_external_triggers` does the same for external-trigger events.
3. `load_batch` delegates to `RawFileReader::load_batch`.
4. The plugin drains all batches produced by that decode and calls the shared
   `EventBatchSink` once per batch.

The callback receives an `RSlice`, which is valid only for the duration of the
callback. Consumers that need to queue events must copy them. This is the same
ownership rule as a borrowed native facility buffer, and avoids leaking a Rust
allocator or channel implementation into third-party plugins.

Both event callbacks use the same sink object. A host may implement one method
and ignore the other when it only needs CD events. Calling `load_batch` before
starting a stream preserves the native `NotInitialized`/stream error behavior.

## Facilities

`PluginFacilityType` contains a stable descriptor key for every native HAL
facility: anti-flicker, decoders, camera sync, digital crop and event masks,
ERC, frame decoders, filters, event streams, geometry, software and hardware
information, registers, biases, monitoring, ROI, and trigger I/O. `Other` is
reserved for plugin-specific extensions.

The raw-file adapter reports the six capabilities it implements: geometry,
hardware identification, ROI, event stream, event stream decoder, and event
decoder. `PluginFacility` intentionally identifies a capability without
exposing native trait objects. Operations should be added as separately
versioned ABI traits as plugins need them; native `Any`, crossbeam, `String`,
and borrowed standard slices must not be added to the shared interface.

## Template for future devices

Future adapters should follow the same shape:

- keep the existing implementation as the source of truth;
- retain native channels, locks, and buffers inside the plugin;
- convert errors to `RString` at the boundary;
- use `RVec`, `RSlice`, `ROption`, and `RResult` in ABI methods;
- expose a small root-module constructor and a discovery implementation;
- document callback lifetime and backpressure behavior explicitly.

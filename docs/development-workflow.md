# Build, test, and troubleshoot

## Build the workspace

From the repository root:

```sh
cargo check --workspace
cargo test --workspace --all-features
cargo build --release --workspace
```

The device plugin crate is `openevt-plugins`. Its library is configured as
both `rlib` and `cdylib`; the shared library under `target/release` is the
artifact to place on the plugin search path. The simulator additionally needs
FFmpeg development libraries available to `ffmpeg-next`.

## Load a device plugin explicitly

```sh
export OPENEVT_PLUGIN_PATH="$PWD/target/release"
cargo test -p openevt-plugins --test plugin_discovery -- --nocapture
```

The discovery integration test expects a built plugin directory and verifies
that `openevt_simulator` advertises the `EventSimulator` device.

## Configure a device from the host

The normal sequence is:

1. Create `PluginRegistry`.
2. Call `load_default_paths()` or `load_directory()`.
3. Call `list_devices()`.
4. Request `configuration_schema(serial)`.
5. Create `new_configuration(serial)` and fill values by parameter name.
6. Call `open_device_with_configuration()`.
7. Obtain facilities, subscribe sinks, start the stream, and load batches.

Do not assume parameter ordering; use each schema parameter's stable `name`.
Defaults are suggestions in the schema and are not automatically inserted into
the configuration object.

## Common failures

**No libraries load.** Confirm the directory exists, contains the platform
library extension, and is in `OPENEVT_PLUGIN_PATH`. Check that the host and
plugin were built from compatible sources.

**Schema validation fails.** Check required values, spelling, duplicate
parameters, and textual representations (`true`, integers, finite floats, and
enum choices). The host validates before opening, and the plugin validates
again by design.

**Events never arrive.** Subscribe before starting/loading. For a device
plugin, call the stream's `start` or the legacy `start_events` path before
requesting batches. Empty CD batches are meaningful and should still be
delivered when a source advanced.

**Seeking produces wrong timestamps.** Reset all state that affects output,
discard prefetched data, and seek the decoder and model together. The simulator
replays from the beginning because a codec keyframe seek alone does not restore
stateful event-model alignment.

**A frame generator shows stale pixels.** Clear or fully overwrite the RGBA
buffer in `render`; do not update only coordinates that received events unless
the contract explicitly defines a persistent framebuffer.

**FFmpeg cannot open a video.** Verify the input has a video stream, FFmpeg is
installed, dimensions are nonzero, and the selected output dimensions are
positive. The simulator uses the encoded average FPS unless `fps` overrides it.

## Performance checklist

- Reuse decoder, scaler, event, and output buffers.
- Bound producer queues and define stop/drop behavior.
- Avoid holding a global lock while invoking user callbacks.
- Keep deterministic state transitions if reproducibility matters.
- Benchmark with representative resolution, FPS, event rate, and seek patterns.
- Use the repository's `hotpath` instrumentation only when profiling; do not
  make profiling output part of the plugin ABI.

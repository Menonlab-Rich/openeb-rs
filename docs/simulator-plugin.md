# Video simulator plugin

The simulator plugin exposes one software device with serial
`EventSimulator`. Its creation schema contains:

- `video_file` — required video input;
- `fps` — optional positive FPS override;
- `config_file` — optional TOML file containing the initial `EvsParameters`.

If `fps` is omitted, the plugin uses the video's encoded average frame rate.
If `config_file` is omitted, validated simulator defaults are used.

After the application creates the device through the host layer, it subscribes
to CD events and starts the raw-event stream. Each stream request decodes the
next video frame, converts RGB pixels to luminance photocurrents, assigns a
timestamp in microseconds from the frame index and FPS, and passes that frame
and timestamp to the stateful `EvsSimulator`. Generated events are delivered
to every subscribed CD sink. The plugin owns the FFmpeg input, decoder,
scaler, and simulator state for the device lifetime.

The optional parameter file must contain the fields of `EvsParameters`, for
example:

```toml
a = 1.0
c_c = 1.0
c_lsf = 1.0
zeta = 1.2
v_t = 0.02585
i_d1_t0 = 1.0
i_d2_t0 = 1.0
i_sf = 1.0
dt = 0.01
tau_fe = 1.0
tau_o1 = 1.0
threshold_on = 0.1
threshold_off = 0.1
```

The host layer validates the creation schema before crossing the ABI, and the
plugin validates it again before opening the video or parameter file.

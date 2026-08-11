# Video simulator plugin

The simulator plugin exposes one software device with serial
`EventSimulator`. Its creation schema contains:

- `video_file` — required video input;
- `fps` — optional positive FPS override;
- `config_file` — optional TOML file containing the initial `EvsParameters`.

If `fps` is omitted, the plugin uses the video's encoded average frame rate.
If `config_file` is omitted, validated simulator defaults are used.

After the application creates the device through the host layer, it subscribes
to CD events and starts the raw-event stream. Each stream request decodes a
batch of up to 32 video frames, converts RGB pixels to luminance photocurrents,
assigns timestamps in microseconds from the frame index and FPS, and passes
the frames in order to the stateful `EvsSimulator`. Each frame is held for its
video-frame duration and integrated at the configured Forward-Euler time step,
so the model does not silently change behavior when the video FPS changes.
Generated events from the batch are delivered in one callback to every
subscribed CD sink. The plugin reuses its RGB and photocurrent buffers across
frames and owns the FFmpeg input, decoder, scaler, and simulator state for the
device lifetime.

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

# Optional paper-model extensions; omitted values use a small noise floor and no dark current.
dark_current = 0.0
noise_fe_std = 0.01
noise_o1_std = 0.01
```

`noise_fe_std` and `noise_o1_std` are stationary voltage-noise standard
deviations. The simulator derives the white-noise input variance using Eq. 23
and applies it through the first-order autoregressive model in Eq. 19. Noise
uses a deterministic seed so runs are reproducible.

The host layer validates the creation schema before crossing the ABI, and the
plugin validates it again before opening the video or parameter file.

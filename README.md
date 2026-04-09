<p align="center">
  <img src="assets/icons/linux/icon128.png" width="96" />
</p>

<h3 align="center">voice-typing</h3>

<p align="center">
real time speech to text that types into any focused window or browser input
</p>

---

on device asr using sherpa-onnx running moonshine -- no cloud no api keys no latency

works system wide on windows macos and linux with a tiny always-on-top widget

includes chrome and safari extensions that overlay a mic icon on every text field

### build

```
cargo run
```

### build with browser extensions

```
cargo run --features extensions
```

extensions are written to `target/debug/extensions/chrome/` and `target/debug/extensions/safari/`

load the chrome extension unpacked from `chrome://extensions`

### cli mode

```
cargo run -- --nogui mic
cargo run -- --nogui wav path/to/file.wav
```

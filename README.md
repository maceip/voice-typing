<p align="center">
  <img src="assets/icons/linux/icon128.png" width="96" />
</p>

<h3 align="center">voice-typing</h3>

<p align="center">
real time speech to text that types into any focused window or browser input
</p>

---

|  |  |
| :--- | :--- |
|  **Offline** <br>on device asr using sherpa-onnx running moonshine -- no cloud no api keys no latency |<img width="99"  alt="fffg" src="https://github.com/user-attachments/assets/1eb01e0f-a79a-432d-80cc-caeca7ba6ee0" /> |
| **Widget** <br> works system wide on windows macos and linux with a tiny always-on-top widget | <img width="99" alt="Screenshot 2026-04-09 215345" src="https://github.com/user-attachments/assets/dd36acf3-ba36-49ed-a991-b202cac58257" />|
| **Wired** <br> includes chrome and safari extensions that overlay a mic icon on every text field |<img width="99" height="197" alt="Screenshot 2026-04-09 224943" src="https://github.com/user-attachments/assets/c8db4e4a-c3a4-4486-95c1-6a6e8cc5b0b5" />|






```toml
[MODEL_INFO]
voice_activity_detector = "silero_vad"
ASR = _moonshinev2
```
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


https://github.com/moonshine-ai/moonshine <br>
http://github.com/snakers4/silero-vad

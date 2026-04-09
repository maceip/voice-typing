Place incoming UI icons here.

Mic filenames used by the app:

- any `.svg` with `microphone-1` in the filename
- any `.svg` with `microphone-on` in the filename

Other expected filenames:

- `settings.svg`
- `close.svg`
- `drag-handle.svg`

Behavior:

- If a file exists here, the app loads it at runtime.
- If it does not exist yet, the app falls back to the built-in placeholder SVG.
- Icons should use `currentColor` so the Rust UI can animate color and opacity without editing the SVG.
- The mic button shows the `microphone-1` match by default and while booting.
- After the mic is ready and active, it switches to the `microphone-on` match.

Current animation contract:

- Mic: pulse while active, breathe while booting.
- Settings: breathe while open.
- Close and drag handle: static by default.

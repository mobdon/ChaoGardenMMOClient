# Old hub archive

Code from the first web hub (the Babylon garden page that came before the game's own Chao Lobby).
Nothing here is imported, built, tested or served. It is kept for history only.

- `index.html`, `src/main.js`: the page and its script as they were when the hub and the login
  shared them. The live `index.html` and `src/main.js` are now the login page only.
- `src/`: the hub's scene and the modules only it used.
- `tests/`: the tests for those modules. They are not run.
- `shared/stage-catalog.json`, `scripts/catalog-stages.js`: the hub's stage select list and its generator.
- `scripts/`: the hub's one-off proof and capture scripts.

Everything was moved as it was, so imports of files that stayed (`./chao-locomotion.js`,
`../shared/constants.js`, ...) and the `scripts/...` paths in `docs/` still name the old place.

Second sweep:
- `src/hub-leftover.css`: the hub's rules cut out of `src/style.css` and `src/sonic.css`. The rules
  still in use were joined into `src/shared-ui.css`.
- `docs/`: notes about hub features whose code is gone or archived here.

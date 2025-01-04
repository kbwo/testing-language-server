# testing-language-server

⚠️ **IMPORTANT NOTICE**
This project is under active development and may introduce breaking changes. If you encounter any issues, please make sure to update to the latest version before reporting bugs.

General purpose LSP server that integrate with testing.
The language server is characterized by portability and extensibility.

## Motivation

This LSP server is heavily influenced by the following tools

- [neotest](https://github.com/nvim-neotest/neotest)
- [Wallaby.js](https://wallabyjs.com)

These tools are very useful and powerful. However, they depend on the execution environment, such as VSCode and Neovim, and the portability aspect was inconvenient for me.
So, I designed this testing-language-server and its dedicated adapters for each test tool to be the middle layer to the parts that depend on each editor.

This design makes it easy to view diagnostics from tests in any editor. Environment-dependent features like neotest and VSCode's built-in testing tools can also be achieved with minimal code using testing-language-server.

## Instllation

```sh
cargo install testing-language-server
cargo install testing-ls-adapter
```

## Features

- [x] Realtime testing diagnostics
- [x] [VSCode extension](https://github.com/kbwo/vscode-testing-ls)
- [x] [coc.nvim extension](https://github.com/kbwo/coc-testing-ls)
- [x] For Neovim builtin LSP, see [testing-ls.nvim](https://github.com/kbwo/testing-ls.nvim)
- [ ] More efficient checking of diagnostics
- [ ] Useful commands in each extension

## Configuration

### Required settings for all editors
You need to prepare .testingls.toml. See [this](./demo/.testingls.toml) for an example of the configuration.

```.testingls.toml
enableWorkspaceDiagnostics = true

[adapterCommand.cargo-test]
path = "testing-ls-adapter"
extra_arg = ["--test-kind=cargo-test"]
include = ["/**/src/**/*.rs"]
exclude = ["/**/target/**"]

[adapterCommand.cargo-nextest]
path = "testing-ls-adapter"
extra_arg = ["--test-kind=cargo-nextest"]
include = ["/**/src/**/*.rs"]
exclude = ["/**/target/**"]

[adapterCommand.jest]
path = "testing-ls-adapter"
extra_arg = ["--test-kind=jest"]
include = ["/jest/*.js"]
exclude = ["/jest/**/node_modules/**/*"]

[adapterCommand.vitest]
path = "testing-ls-adapter"
extra_arg = ["--test-kind=vitest"]
include = ["/vitest/*.test.ts", "/vitest/config/**/*.test.ts"]
exclude = ["/vitest/**/node_modules/**/*"]

[adapterCommand.deno]
path = "testing-ls-adapter"
extra_arg = ["--test-kind=deno"]
include = ["/deno/*.ts"]
exclude = []

[adapterCommand.go]
path = "testing-ls-adapter"
extra_arg = ["--test-kind=go-test"]
include = ["/**/*.go"]
exclude = []

[adapterCommand.node-test]
path = "testing-ls-adapter"
extra_arg = ["--test-kind=node-test"]
include = ["/node-test/*.test.js"]
exclude = []

[adapterCommand.phpunit]
path = "testing-ls-adapter"
extra_arg = ["--test-kind=phpunit"]
include = ["/**/*Test.php"]
exclude = ["/phpunit/vendor/**/*.php"]
```

### VSCode

Install from [VSCode Marketplace](https://marketplace.visualstudio.com/items?itemName=kbwo.testing-language-server).
You can see the example in [settings.json](./demo/.vscode/settings.json).

### coc.nvim
Install from `:CocInstall coc-testing-ls`.
You can see the example in [See more example](./.vim/coc-settings.json)

### Neovim (nvim-lspconfig)

See [testing-ls.nvim](https://github.com/kbwo/testing-ls.nvim)

### Helix
See [language.toml](./demo/.helix/language.toml).

The array wrapper has been removed to simplify the configuration structure. Please update your settings accordingly.

## Adapter
- [x] `cargo test`
- [x] `cargo nextest`
- [x] `jest`
- [x] `deno test`
- [x] `go test`
- [x] `phpunit`
- [x] `vitest`
- [x] `node --test` (Node Test Runner)

### Writing custom adapter
⚠ The specification of adapter CLI is not stabilized yet.

See [ADAPTER_SPEC.md](./doc/ADAPTER_SPEC.md) and [spec.rs](./src/spec.rs).
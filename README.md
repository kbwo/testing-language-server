# testing-language-server

General purpose LSP server that integrate with testing.
The language server is characterized by portability and extensibility.

## Motivation

This LSP server is heavily influenced by the following tools

- [neotest](https://github.com/nvim-neotest/neotest)
- [Wallaby.js](https://wallabyjs.com)

These tools are very useful and powerful. However, they depend on the execution environment, such as VSCode and NeoVim, and the portability aspect was inconvenient for me.
So, I designed this testing-language-server and its dedicated adapters for each test tool to be the middle layer to the parts that depend on each editor.

This design makes it easy to view diagnostics from tests in any editor. Environment-dependent features like neotest and VSCode's built-in testing tools can also be achieved with minimal code using testing-language-server.

## Instllation

```sh
cargo install testing-language-server
cargo install testing-ls-adapter
```

## Features

- [x] Realtime testing diagnostics
- [ ] More efficient checking of diagnostics
- [ ] Adapter installation command
- [x] [VSCode extension](https://github.com/kbwo/vscode-testing-ls)
- [x] [Coc.nvim extension](https://github.com/kbwo/coc-testing-ls)
- [ ] NeoVim builtin LSP plugin

## Configuration

language server config:

```
"languageserver": {
  "testing": {
    "command": "<server path>/testing-language-server",
    "trace.server": "verbose",
    "filetypes": [
      "rust",
      "javascript"
    ],
    "initializationOptions": {
      "initializationOptions": {
        "adapterCommand": {
          "cargo test": [
            {
              "path": "<adapter path>/testing-ls-adapter",
              "extra_args": ["--test-kind=cargo-test"],
              "include_pattern": ["**/*.rs"],
              "exclude_pattern": ["**/target/**"]
            }
          ],
          "jest": [
            {
              "path": "<adapter path>/testing-ls-adapter",
              "extra_args": ["--test-kind=jest"],
              "include_patterns": ["/**/*.js"],
              "exclude_patterns": ["/node_modules/**/*"]
            }
          ]
        }
      }
    }
  }
}
```

## Adapter
- [x] `cargo test`
- [x] `cargo nextest`
- [x] `jest`
- [x] `deno test`
- [x] `go test`
- [x] `phpunit`
- [x] `vitest`

### Writing custom adapter
⚠ The specification of adapter CLI is not stabilized yet.

See [SPEC.md](./doc/SPEC.md) and [spec.rs](./src/spec.rs).

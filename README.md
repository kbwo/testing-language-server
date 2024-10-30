# testing-language-server

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
- [x] For Neovim builtin LSP, see [demo/README.md](./demo/README.md)
- [ ] More efficient checking of diagnostics
- [ ] Useful commands in each extension

## Configuration

### VSCode

Install from [VSCode Marketplace](https://marketplace.visualstudio.com/items?itemName=kbwo.testing-language-server).
You should set `adapterCommand` in `initializationOptions` for each project.
You can see the example in [settings.json](./demo/.vscode/settings.json).


### coc.nvim
Install from `:CocInstall coc-testing-ls`.
You should set `adapterCommand` in `initializationOptions` for each project.
You can see the example in [See more example](./demo/.vim/coc-settings.json)

### Neovim (nvim-lspconfig)

```lua
local lspconfig = require('lspconfig')
local configs = require('lspconfig.configs')
local util = require "lspconfig/util"

configs.testing_ls = {
  default_config = {
    cmd = { "testing-language-server" },
    filetypes = { "rust" },
    root_dir = util.root_pattern(".git", "Cargo.toml"),
      init_options = {
        enable = true,
        fileTypes = {"rust"},
        adapterCommand = {
        -- See test execution settings for each project
        -- This configuration assumes a Rust project
          rust = {
            path = "testing-ls-adapter",
            extra_arg = { "--test-kind=cargo-test", "--workspace" },
            include = { "/demo/**/src/**/*.rs"},
            exclude = { "/**/target/**"},
          }
        },
        enableWorkspaceDiagnostics = true,
        trace = {
          server = "verbose"
        }
      }
  },
  docs = {
    description = [[
      https://github.com/kbwo/testing-language-server

      Language Server for real-time testing.
    ]],
  },
}

lspconfig.testing_ls.setup{}
```

### Helix
See [language.toml](./demo/.helix/language.toml).


## ⚠️ Breaking Changes (2024-10-25)

The configuration structure for adapter commands has been changed:

**Before:**
```json
"adapterCommand": {
  "rust": [
    {
      "path": "testing-ls-adapter",
      "extra_arg": ["--test-kind=cargo-test"]
      // ...
    }
  ]
}
```

**After:**
```json
"adapterCommand": {
  "rust": {
    "path": "testing-ls-adapter",
    "extra_arg": ["--test-kind=cargo-test"]
    // ...
  }
}
```

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
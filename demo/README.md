## Using `nvim-lspconfig`

The specification is not stable, so you need to set it yourself. Once the spec is stable, I will send a PR to `nvim-lspconfig`.
```
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
          rust = {
            {
              path = "testing-ls-adapter",
              extra_arg = { "--test-kind=cargo-test", "--workspace" },
              include = { "/demo/**/src/**/*.rs"},
              exclude = { "/**/target/**"},
            }
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

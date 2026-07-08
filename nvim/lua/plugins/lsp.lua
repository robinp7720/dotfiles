local lsp_servers = {
  bashls = {},
  clangd = {},
  hls = {},
  jsonls = {},
  lua_ls = {
    settings = {
      Lua = {
        diagnostics = { globals = { "vim" } },
        workspace = { checkThirdParty = false },
        telemetry = { enable = false },
      },
    },
  },
  marksman = {},
  pyright = {},
  rust_analyzer = {},
  taplo = {},
  texlab = {},
  tinymist = {
    settings = {
      formatterMode = "typstyle",
      formatterPrintWidth = 79,
      formatterProseWrap = true,
    },
  },
  ts_ls = {},
  vala_ls = {},
  vimls = {},
  yamlls = {},
}

local mason_servers = {
  "bashls",
  "clangd",
  "jsonls",
  "lua_ls",
  "marksman",
  "pyright",
  "rust_analyzer",
  "taplo",
  "texlab",
  "tinymist",
  "ts_ls",
  "vimls",
  "yamlls",
}

return {
  {
    "saghen/blink.cmp",
    version = "1.*",
    event = "InsertEnter",
    dependencies = {
      "rafamadriz/friendly-snippets",
    },
    opts = {
      keymap = { preset = "enter" },
      appearance = { nerd_font_variant = "mono" },
      completion = {
        documentation = {
          auto_show = true,
          auto_show_delay_ms = 500,
        },
      },
      sources = {
        default = { "lsp", "path", "snippets", "buffer" },
      },
    },
    opts_extend = { "sources.default" },
  },
  {
    "mason-org/mason.nvim",
    cmd = "Mason",
    opts = {},
  },
  {
    "mason-org/mason-lspconfig.nvim",
    dependencies = {
      "mason-org/mason.nvim",
      "neovim/nvim-lspconfig",
      "saghen/blink.cmp",
    },
    event = { "BufReadPre", "BufNewFile" },
    config = function()
      vim.diagnostic.config({
        virtual_text = false,
        signs = true,
        underline = true,
        update_in_insert = false,
        severity_sort = true,
        float = {
          border = "rounded",
          source = true,
        },
      })

      require("mason").setup()
      require("mason-lspconfig").setup({
        ensure_installed = mason_servers,
        automatic_enable = false,
      })

      pcall(require, "lspconfig")

      local capabilities = vim.lsp.protocol.make_client_capabilities()
      local ok, blink = pcall(require, "blink.cmp")
      if ok then
        capabilities = blink.get_lsp_capabilities(capabilities)
      end

      for server, config in pairs(lsp_servers) do
        local opts = vim.tbl_deep_extend("force", {}, config, {
          capabilities = capabilities,
        })

        if vim.lsp.config then
          vim.lsp.config(server, opts)
          vim.lsp.enable(server)
        else
          require("lspconfig")[server].setup(opts)
        end
      end
    end,
  },
  {
    "stevearc/conform.nvim",
    cmd = { "ConformInfo", "Format" },
    ft = "typst",
    keys = {
      {
        "<leader>f",
        function()
          require("conform").format({ async = true, lsp_format = "fallback" })
        end,
        mode = { "n", "x" },
        desc = "Format",
      },
    },
    opts = {
      format_on_save = function(bufnr)
        if vim.bo[bufnr].filetype == "typst" then
          return {
            lsp_format = "prefer",
            timeout_ms = 2000,
          }
        end
      end,
      formatters_by_ft = {
        javascript = { "prettierd", "prettier", stop_after_first = true },
        javascriptreact = { "prettierd", "prettier", stop_after_first = true },
        json = { "prettierd", "prettier", stop_after_first = true },
        lua = { "stylua" },
        markdown = { "prettierd", "prettier", stop_after_first = true },
        python = { "isort", "black" },
        tex = { "latexindent" },
        typescript = { "prettierd", "prettier", stop_after_first = true },
        typescriptreact = { "prettierd", "prettier", stop_after_first = true },
        yaml = { "prettierd", "prettier", stop_after_first = true },
      },
    },
    config = function(_, opts)
      local conform = require("conform")
      conform.setup(opts)

      vim.api.nvim_create_user_command("Format", function()
        conform.format({ async = true, lsp_format = "fallback" })
      end, { desc = "Format current buffer" })
    end,
  },
  {
    "mfussenegger/nvim-lint",
    event = { "BufReadPost", "BufWritePost", "InsertLeave" },
    config = function()
      local lint = require("lint")
      lint.linters_by_ft = {
        javascript = { "eslint_d" },
        javascriptreact = { "eslint_d" },
        markdown = { "markdownlint" },
        python = { "ruff" },
        tex = { "chktex" },
        typescript = { "eslint_d" },
        typescriptreact = { "eslint_d" },
      }

      vim.api.nvim_create_autocmd({ "BufWritePost", "InsertLeave" }, {
        group = vim.api.nvim_create_augroup("nvim_lint", { clear = true }),
        callback = function()
          lint.try_lint()
        end,
      })
    end,
  },
}

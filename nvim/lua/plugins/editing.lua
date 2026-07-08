return {
  {
    "nvim-treesitter/nvim-treesitter",
    lazy = false,
    opts = {
      parsers = {
        "bash",
        "bibtex",
        "c",
        "cmake",
        "cpp",
        "css",
        "html",
        "javascript",
        "json",
        "latex",
        "lua",
        "markdown",
        "markdown_inline",
        "python",
        "rust",
        "toml",
        "tsx",
        "typescript",
        "typst",
        "vim",
        "vimdoc",
        "yaml",
      },
      filetypes = {
        "bash",
        "bib",
        "c",
        "cmake",
        "cpp",
        "css",
        "html",
        "javascript",
        "javascriptreact",
        "json",
        "lua",
        "markdown",
        "python",
        "rust",
        "tex",
        "toml",
        "typescript",
        "typescriptreact",
        "typst",
        "vim",
        "vimdoc",
        "yaml",
      },
    },
    config = function(_, opts)
      local ts = require("nvim-treesitter")
      ts.setup({
        install_dir = vim.fn.stdpath("data") .. "/site",
      })

      vim.api.nvim_create_autocmd("FileType", {
        group = vim.api.nvim_create_augroup("treesitter_start", { clear = true }),
        pattern = opts.filetypes,
        callback = function()
          pcall(vim.treesitter.start)
          pcall(function()
            vim.bo.indentexpr = "v:lua.require'nvim-treesitter'.indentexpr()"
          end)
        end,
      })

      vim.api.nvim_create_user_command("TSInstallRecommended", function()
        if vim.fn.executable("tree-sitter") == 0 then
          vim.notify("tree-sitter CLI is required to install parsers", vim.log.levels.ERROR)
          return
        end

        ts.install(opts.parsers):wait(300000)
      end, { desc = "Install recommended Treesitter parsers" })
    end,
  },
  {
    "kylechui/nvim-surround",
    version = "*",
    event = "VeryLazy",
    opts = {},
  },
  {
    "numToStr/Comment.nvim",
    keys = { "gc", "gb" },
    opts = {},
  },
  {
    "windwp/nvim-autopairs",
    event = "InsertEnter",
    opts = {},
  },
  {
    "gbprod/yanky.nvim",
    event = "VeryLazy",
    opts = {
      highlight = { timer = 150 },
    },
    keys = {
      { "p", "<Plug>(YankyPutAfter)", mode = { "n", "x" }, desc = "Put after" },
      { "P", "<Plug>(YankyPutBefore)", mode = { "n", "x" }, desc = "Put before" },
      { "[y", "<Plug>(YankyCycleForward)", desc = "Yank history forward" },
      { "]y", "<Plug>(YankyCycleBackward)", desc = "Yank history backward" },
    },
  },
  {
    "NvChad/nvim-colorizer.lua",
    event = { "BufReadPost", "BufNewFile" },
    opts = {
      filetypes = {
        "*",
        dosini = { names = false },
      },
      user_default_options = {
        RGB = true,
        RRGGBB = true,
        names = false,
        mode = "background",
      },
    },
  },
}

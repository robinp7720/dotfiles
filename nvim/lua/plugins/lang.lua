return {
  {
    "lervag/vimtex",
    ft = { "tex", "plaintex", "bib" },
    init = function()
      vim.g.tex_flavor = "latex"
      vim.g.vimtex_compiler_progname = "nvr"
      vim.g.vimtex_complete_enabled = 1
      vim.g.vimtex_complete_close_braces = 1
      vim.g.vimtex_fold_enabled = 1
      vim.g.vimtex_view_method = "mupdf"
      vim.g.vimtex_toc_config = {
        indent_levels = 1,
        show_help = 0,
        mode = 2,
      }
      vim.g.vimtex_grammar_vlty = { lt_command = "languagetool" }
    end,
  },
  {
    "vigoux/LanguageTool.nvim",
    ft = { "tex", "markdown", "text" },
    init = function()
      local jar = "/usr/share/java/languagetool/languagetool-server.jar"
      if vim.fn.filereadable(jar) == 1 then
        vim.g.languagetool_server_jar = jar
      end
    end,
  },
  {
    "MeanderingProgrammer/render-markdown.nvim",
    ft = { "markdown", "Avante" },
    dependencies = {
      "nvim-treesitter/nvim-treesitter",
      "nvim-tree/nvim-web-devicons",
    },
    opts = {},
  },
  {
    "chomosuke/typst-preview.nvim",
    version = "1.*",
    ft = "typst",
    opts = {
      port = vim.env.SSH_CONNECTION and 23635 or 0,
      host = "127.0.0.1",
      open_cmd = vim.env.SSH_CONNECTION and "true %s" or nil,
      follow_cursor = true,
      dependencies_bin = {
        tinymist = "tinymist",
      },
    },
  },
  {
    "mattn/emmet-vim",
    ft = {
      "css",
      "html",
      "javascript",
      "javascriptreact",
      "typescriptreact",
      "vue",
    },
    init = function()
      vim.g.user_emmet_mode = "inv"
      vim.g.user_emmet_leader_key = "<C-d>"
    end,
  },
  {
    "chrisbra/csv.vim",
    ft = "csv",
  },
  {
    "kovetskiy/sxhkd-vim",
    ft = "sxhkd",
  },
  {
    "fladson/vim-kitty",
    ft = "kitty",
  },
}

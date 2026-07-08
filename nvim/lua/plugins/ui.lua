return {
  {
    "nvim-tree/nvim-web-devicons",
    lazy = true,
  },
  {
    "ibhagwan/fzf-lua",
    cmd = "FzfLua",
    dependencies = { "nvim-tree/nvim-web-devicons" },
    opts = {
      "default-title",
      register_ui_select = true,
      files = {
        fd_opts = "--color=never --type f --hidden --follow --exclude .git",
      },
      winopts = {
        height = 0.85,
        width = 0.9,
        preview = {
          layout = "flex",
        },
      },
    },
    keys = {
      { "<leader>ff", "<cmd>FzfLua files<cr>", desc = "Find files" },
      { "<leader>fg", "<cmd>FzfLua live_grep<cr>", desc = "Live grep" },
      { "<leader>fb", "<cmd>FzfLua buffers<cr>", desc = "Buffers" },
      { "<leader>fh", "<cmd>FzfLua helptags<cr>", desc = "Help" },
      { "<space>a", "<cmd>FzfLua diagnostics_workspace<cr>", desc = "Diagnostics" },
      { "<space>o", "<cmd>FzfLua lsp_document_symbols<cr>", desc = "Document symbols" },
      { "<space>s", "<cmd>FzfLua lsp_workspace_symbols<cr>", desc = "Workspace symbols" },
    },
  },
  {
    "stevearc/oil.nvim",
    cmd = "Oil",
    keys = {
      { "<C-n>", "<cmd>Oil<cr>", desc = "File explorer" },
    },
    opts = {
      default_file_explorer = true,
      columns = {
        "icon",
        "permissions",
        "size",
        "mtime",
      },
      view_options = {
        show_hidden = true,
      },
    },
  },
  {
    "nvim-lualine/lualine.nvim",
    event = "VeryLazy",
    dependencies = { "nvim-tree/nvim-web-devicons" },
    opts = {
      options = {
        component_separators = "",
        section_separators = "",
        globalstatus = true,
        theme = "auto",
      },
      sections = {
        lualine_c = {
          { "filename", path = 1 },
        },
      },
    },
  },
  {
    "folke/which-key.nvim",
    event = "VeryLazy",
    opts = {},
  },
  {
    "folke/trouble.nvim",
    cmd = "Trouble",
    keys = {
      { "<leader>xx", "<cmd>Trouble diagnostics toggle<cr>", desc = "Diagnostics" },
      { "<leader>xX", "<cmd>Trouble diagnostics toggle filter.buf=0<cr>", desc = "Buffer diagnostics" },
      { "<leader>cs", "<cmd>Trouble symbols toggle focus=false<cr>", desc = "Symbols" },
      { "<leader>xl", "<cmd>Trouble loclist toggle<cr>", desc = "Location list" },
      { "<leader>xq", "<cmd>Trouble qflist toggle<cr>", desc = "Quickfix list" },
    },
    opts = {},
  },
  {
    "mbbill/undotree",
    cmd = "UndotreeToggle",
    keys = {
      { "<leader>u", "<cmd>UndotreeToggle<cr>", desc = "Undo tree" },
    },
  },
  {
    "junegunn/goyo.vim",
    cmd = "Goyo",
  },
}

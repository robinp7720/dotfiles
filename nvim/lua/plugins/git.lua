return {
  {
    "tpope/vim-fugitive",
    cmd = {
      "Git",
      "G",
      "Gdiffsplit",
      "Gread",
      "Gwrite",
      "Ggrep",
      "GMove",
      "GDelete",
      "GBrowse",
    },
    dependencies = {
      "tpope/vim-rhubarb",
    },
    keys = {
      { "<leader>gs", "<cmd>Git<cr>", desc = "Git status" },
    },
  },
  {
    "lewis6991/gitsigns.nvim",
    event = { "BufReadPre", "BufNewFile" },
    opts = {
      current_line_blame = false,
      on_attach = function(bufnr)
        local gitsigns = require("gitsigns")
        local map = function(lhs, rhs, desc)
          vim.keymap.set("n", lhs, rhs, { buffer = bufnr, desc = desc })
        end

        map("]c", function()
          if vim.wo.diff then
            return "]c"
          end
          vim.schedule(gitsigns.next_hunk)
          return "<Ignore>"
        end, "Next hunk")

        map("[c", function()
          if vim.wo.diff then
            return "[c"
          end
          vim.schedule(gitsigns.prev_hunk)
          return "<Ignore>"
        end, "Previous hunk")

        map("<leader>gb", gitsigns.blame_line, "Blame line")
        map("<leader>ghp", gitsigns.preview_hunk, "Preview hunk")
        map("<leader>ghr", gitsigns.reset_hunk, "Reset hunk")
        map("<leader>ghs", gitsigns.stage_hunk, "Stage hunk")
      end,
    },
  },
}

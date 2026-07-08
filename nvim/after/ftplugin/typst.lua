vim.keymap.set("n", "<localleader>p", "<cmd>TypstPreviewToggle<cr>", {
  buffer = true,
  desc = "Toggle Typst preview",
})

vim.keymap.set("n", "<localleader>s", "<cmd>TypstPreviewSyncCursor<cr>", {
  buffer = true,
  desc = "Sync Typst preview with cursor",
})

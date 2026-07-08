local map = vim.keymap.set
local spelling = require("config.spelling")

map("n", "z=", spelling.fix_word, { desc = "Fix spelling" })
map("n", "<leader>zf", spelling.fix_word, { desc = "Fix spelling" })
map("n", "<leader>zn", spelling.fix_next, { desc = "Fix next spelling mistake" })
map("n", "<leader>za", spelling.add_word, { desc = "Add word to spellfile" })
map("n", "<leader>zr", spelling.undo_add_word, { desc = "Remove word from spellfile" })
map("c", "w!!", "w !sudo tee > /dev/null %", { desc = "Write with sudo" })

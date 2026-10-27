-- Run from the repository root: nvim --headless -u NONE -l tests/nvim_spelling_test.lua
local root = vim.fn.fnamemodify(debug.getinfo(1, "S").source:sub(2), ":p:h:h")
package.path = root .. "/nvim/lua/?.lua;" .. package.path
package.loaded["fzf-lua"] = { register_ui_select = function() end }
local spelling = require("config.spelling")
local pending
vim.ui.select = function(_, _, callback)
  pending = callback
end
vim.notify = function() end

local function open_picker(line, col)
  local buf = vim.api.nvim_create_buf(true, false)
  vim.api.nvim_set_current_buf(buf)
  vim.api.nvim_buf_set_lines(buf, 0, -1, false, { line })
  vim.api.nvim_win_set_cursor(0, { 1, col })
  pending = nil
  spelling.fix_word()
  assert(pending, "Expected spelling suggestions for " .. line)
  return buf
end

local function expect(buf, line)
  assert(vim.api.nvim_buf_get_lines(buf, 0, -1, false)[1] == line,
    "Unexpected buffer text: " .. vim.inspect(vim.api.nvim_buf_get_lines(buf, 0, -1, false)))
end

-- Exercise byte offsets, word edges, punctuation, whitespace, and later words.
for _, case in ipairs({
  { "helo", 0, "hello" },
  { "helo", 3, "hello" },
  { "helo, world!", 2, "hello, world!" },
  { "one helo two", 5, "one hello two" },
  { "one  helo", 3, "one  hello" },
  { "é helo!", 4, "é hello!" },
  { "hélo!", 2, "hello!" },
}) do
  local buf = open_picker(case[1], case[2])
  pending("hello")
  expect(buf, case[3])
end

local buf = open_picker("helo world", 0)
vim.api.nvim_win_set_cursor(0, { 1, 7 })
pending("hello")
expect(buf, "hello world")

buf = open_picker("helo", 0)
local other = vim.api.nvim_create_buf(true, false)
vim.api.nvim_set_current_buf(other)
vim.api.nvim_buf_set_lines(other, 0, -1, false, { "unrelated" })
pending("hello")
expect(buf, "hello")
expect(other, "unrelated")

buf = open_picker("helo!", 1)
pending(nil)
expect(buf, "helo!")

buf = open_picker("helo", 1)
vim.api.nvim_buf_set_lines(buf, 0, -1, false, { "edited" })
pending("hello")
expect(buf, "edited")

buf = open_picker("helo", 1)
vim.bo[buf].modifiable = false
pending("hello")
expect(buf, "helo")

buf = open_picker("helo", 1)
vim.api.nvim_buf_delete(buf, { force = true })
pending("hello")

vim.api.nvim_set_current_buf(other)
vim.api.nvim_buf_set_lines(other, 0, -1, false, { "   " })
pending = nil
spelling.fix_word()
assert(not pending, "Whitespace should not open a picker")
print("Neovim spelling regression checks passed")

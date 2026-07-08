local M = {}

local function use_fzf_select()
  local ok, fzf = pcall(require, "fzf-lua")
  if ok and fzf.register_ui_select then
    pcall(fzf.register_ui_select)
  end
end

local function replace_word(choice)
  if not choice then
    return
  end

  local row = vim.api.nvim_win_get_cursor(0)[1]
  local start_col = vim.fn.searchpos([[\<]], "bcn", row)[2] - 1
  local end_col = vim.fn.searchpos([[\>]], "cn", row)[2]

  if start_col < 0 or end_col <= start_col then
    return
  end

  vim.api.nvim_buf_set_text(0, row - 1, start_col, row - 1, end_col, { choice })
end

function M.fix_word()
  local word = vim.fn.expand("<cword>")
  if word == "" then
    vim.notify("No word under cursor", vim.log.levels.INFO)
    return
  end

  local suggestions = vim.fn.spellsuggest(word, 20)
  if vim.tbl_isempty(suggestions) then
    vim.notify("No spelling suggestions for " .. word, vim.log.levels.INFO)
    return
  end

  use_fzf_select()
  vim.ui.select(suggestions, {
    prompt = "Replace " .. word .. " with",
  }, replace_word)
end

function M.fix_next()
  local before = vim.api.nvim_win_get_cursor(0)
  vim.cmd("normal! ]s")
  local after = vim.api.nvim_win_get_cursor(0)

  if before[1] == after[1] and before[2] == after[2] then
    vim.notify("No next spelling mistake", vim.log.levels.INFO)
    return
  end

  M.fix_word()
end

function M.add_word()
  vim.cmd("normal! zg")
  vim.notify("Added " .. vim.fn.expand("<cword>") .. " to spellfile", vim.log.levels.INFO)
end

function M.undo_add_word()
  vim.cmd("normal! zug")
  vim.notify("Removed " .. vim.fn.expand("<cword>") .. " from spellfile", vim.log.levels.INFO)
end

return M

local M = {}

local function use_fzf_select()
  local ok, fzf = pcall(require, "fzf-lua")
  if ok and fzf.register_ui_select then
    pcall(fzf.register_ui_select, nil, true)
  end
end

function M.fix_word()
  local buf = vim.api.nvim_get_current_buf()
  local cursor = vim.api.nvim_win_get_cursor(0)
  local row, col = cursor[1] - 1, cursor[2]
  local line = vim.api.nvim_get_current_line()
  -- Like <cword>, select the keyword under or after the cursor. matchstrpos
  -- returns zero-based byte offsets with an exclusive end, as set_text needs.
  local word, start_col, end_col
  local offset = 0
  repeat
    word, start_col, end_col = unpack(vim.fn.matchstrpos(line, [[\k\+]], offset))
    offset = end_col
  until start_col < 0 or end_col > col

  if word == "" then
    vim.notify("No word under cursor", vim.log.levels.INFO)
    return
  end

  local suggestions = vim.fn.spellsuggest(word, 20)
  if vim.tbl_isempty(suggestions) then
    vim.notify("No spelling suggestions for " .. word, vim.log.levels.INFO)
    return
  end

  local changedtick = vim.api.nvim_buf_get_changedtick(buf)
  use_fzf_select()
  vim.ui.select(suggestions, {
    prompt = "Replace " .. word .. " with",
  }, function(choice)
    if not choice or not vim.api.nvim_buf_is_loaded(buf) then
      return
    end
    -- Selection is asynchronous: never apply a stale range to an edited buffer
    -- or use the picker/current window as the replacement target.
    if vim.api.nvim_buf_get_changedtick(buf) ~= changedtick or not vim.bo[buf].modifiable then
      vim.notify("Spelling correction skipped: buffer changed or is not editable", vim.log.levels.INFO)
      return
    end
    vim.api.nvim_buf_set_text(buf, row, start_col, row, end_col, { choice })
  end)
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

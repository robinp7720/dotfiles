local augroup = vim.api.nvim_create_augroup
local autocmd = vim.api.nvim_create_autocmd

autocmd({ "BufRead", "BufNewFile" }, {
  group = augroup("filetypes", { clear = true }),
  pattern = "*rtorrent.rc*",
  command = "setfiletype rtorrent",
})

autocmd("ColorScheme", {
  group = augroup("spell_highlights", { clear = true }),
  callback = function()
    vim.api.nvim_set_hl(0, "SpellCap", { fg = "Blue", sp = "Yellow", bold = true })
    vim.api.nvim_set_hl(0, "SpellBad", { sp = "Red", undercurl = true })
    vim.api.nvim_set_hl(0, "SpellRare", { fg = "Blue", sp = "Blue", bold = true })
  end,
})

autocmd("FileType", {
  group = augroup("prose_formatting", { clear = true }),
  pattern = { "markdown", "text", "tex", "gitcommit" },
  callback = function()
    vim.opt_local.formatoptions:append("t")
    vim.opt_local.formatoptions:append("c")
    vim.opt_local.formatoptions:append("q")
    vim.opt_local.formatoptions:append("n")
    vim.opt_local.formatoptions:append("j")
    vim.opt_local.textwidth = 79
  end,
})

autocmd("FileType", {
  group = augroup("compile_single_file", { clear = true }),
  pattern = { "c", "cpp" },
  callback = function(event)
    vim.api.nvim_buf_create_user_command(event.buf, "Runc", function(opts)
      local compiler = vim.bo[event.buf].filetype == "cpp" and "g++" or "gcc"
      local source = vim.fn.shellescape(vim.api.nvim_buf_get_name(event.buf))
      local args = opts.args ~= "" and (" " .. opts.args) or ""

      vim.cmd(("!%s -O3 %s%s && ./a.out; rm -f ./a.out"):format(compiler, source, args))
    end, { nargs = "*", desc = "Compile and run current C/C++ file" })
  end,
})

autocmd("LspAttach", {
  group = augroup("lsp_keymaps", { clear = true }),
  callback = function(event)
    local function map(mode, lhs, rhs, desc)
      vim.keymap.set(mode, lhs, rhs, { buffer = event.buf, desc = desc })
    end

    map("n", "gd", vim.lsp.buf.definition, "Go to definition")
    map("n", "gy", vim.lsp.buf.type_definition, "Go to type definition")
    map("n", "gi", vim.lsp.buf.implementation, "Go to implementation")
    map("n", "gr", vim.lsp.buf.references, "References")
    map("n", "K", vim.lsp.buf.hover, "Hover")
    map("n", "<leader>rn", vim.lsp.buf.rename, "Rename")
    map({ "n", "x" }, "<leader>a", vim.lsp.buf.code_action, "Code action")
    map("n", "<leader>qf", vim.lsp.buf.code_action, "Quick fix")
    map("n", "<leader>cl", vim.lsp.codelens.run, "Code lens")
    map("n", "[g", vim.diagnostic.goto_prev, "Previous diagnostic")
    map("n", "]g", vim.diagnostic.goto_next, "Next diagnostic")
  end,
})

vim.api.nvim_create_user_command("OR", function()
  vim.lsp.buf.code_action({
    apply = true,
    context = {
      only = { "source.organizeImports" },
      diagnostics = {},
    },
  })
end, { desc = "Organize imports with LSP" })

setlocal conceallevel=2

nnoremap <buffer> <localleader>tocs <cmd>VimtexTocToggle<cr>

syntax region Statement start='\\ref{' end='}' transparent contains=myStart,myEnd
syntax match myStart '\\ref{\ze\w\+' contained conceal cchar=[
syntax match myEnd '\(\\ref{\w\+\)\@<=\zs}' contained conceal cchar=]
highlight! link Conceal Statement

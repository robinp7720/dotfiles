" --------------------
" Vim general settings
" --------------------

set ignorecase smartcase
set backspace=indent,eol,start
set ruler
set showcmd
set number
set mouse=a
set completeopt=noinsert,menuone,noselect
set inccommand=nosplit
set termguicolors
set incsearch
set nowrap
set hidden

filetype plugin indent on
syntax on

au BufNewFile,BufRead *rtorrent.rc* set filetype=rtorrent 

let g:python3_host_prog = '/usr/bin/python3'

" --------------------
" Max characters per line column
" --------------------

set cc=80
set tw=79

" --------------------
" Indentation
" --------------------

set autoindent
set smartindent
set tabstop=4    " show existing tab with 4 spaces width
set shiftwidth=4 " when indenting with '>', use 4 spaces width
set expandtab    " On pressing tab, insert 4 spaces


" ------------------------
" Load Plugins
" ------------------------

call plug#begin('~/.local/share/nvim/plugged')

Plug 'roxma/nvim-yarp'
"Plug 'Townk/vim-autoclose'
Plug 'tpope/vim-surround'
Plug 'godlygeek/tabular'

Plug 'junegunn/goyo.vim'
Plug 'svermeulen/vim-yoink'

Plug 'mhinz/vim-startify'
Plug 'neoclide/coc.nvim', {'branch': 'release'}

"Plug 'junegunn/fzf', { 'do': { -> fzf#install() } }
set rtp+=/usr/bin/fzf
Plug 'junegunn/fzf.vim'
Plug 'yuki-ycino/fzf-preview.vim', { 'branch': 'release/remote'}
Plug 'antoinemadec/coc-fzf'

Plug 'glacambre/firenvim', { 'do': { _ -> firenvim#install(0) } }

Plug 'vim-airline/vim-airline'
Plug 'vim-airline/vim-airline-themes'

" NerdTREE file browser
Plug 'scrooloose/nerdtree'
Plug 'Xuyuanp/nerdtree-git-plugin'
Plug 'ryanoasis/vim-devicons'

" Tags
Plug 'ludovicchabant/vim-gutentags'

Plug 'majutsushi/tagbar'
Plug 'liuchengxu/vista.vim'

"Plug 'chomosuke/typst-preview.nvim', {'tag': 'v1.*'}
Plug 'kaarmu/typst.vim'

" Themes
Plug 'ayu-theme/ayu-vim'
Plug 'jdsimcoe/abstract.vim'
Plug 'andreasvc/vim-256noir'
Plug 'nanotech/jellybeans.vim'
Plug 'gilgigilgil/anderson.vim'
Plug 'mhartington/oceanic-next'
Plug 'romainl/Apprentice'
Plug 'sainnhe/forest-night'
Plug 'challenger-deep-theme/vim', { 'as': 'challenger-deep' }
Plug 'ajmwagar/vim-deus'
Plug 'preservim/vim-colors-pencil'
Plug 'FrenzyExists/aquarium-vim'
Plug 'shaunsingh/nord.nvim'

" Plug 'nvim-treesitter/nvim-treesitter', {'do': ':TSUpdate'}
" Plugin to improve syntax highlighting
Plug 'sheerun/vim-polyglot'

" Syntax checking
" Plug 'vim-syntastic/syntastic'

" Use ale for displaying coc errors
" Plug 'dense-analysis/ale'

" Git integration
"Plug 'airblade/vim-gitgutter'
Plug 'jreybert/vimagit'
Plug 'tpope/vim-fugitive'
Plug 'tpope/vim-rhubarb'             " Github integration for fugitive
Plug 'shumphrey/fugitive-gitlab.vim' " Gitlab integration for fugitive
Plug 'RobertAudi/git-blame.vim'

Plug 'Shougo/denite.nvim', { 'do': ':UpdateRemotePlugins' }
Plug 'shougo/neomru.vim'

" Plugins for HTML, JS and CSS
Plug 'mattn/emmet-vim'
Plug 'pangloss/vim-javascript'

" Plugins for working with markdown
Plug 'vim-pandoc/vim-pandoc'
Plug 'vim-pandoc/vim-pandoc-syntax' 

" Plugins for working with latex
Plug 'lervag/vimtex'
Plug 'PietroPate/vim-tex-conceal', {'for': 'tex'}

" Plugins for working with natural language text
Plug 'rhysd/vim-grammarous'     " Grammar checking with LanguageTool
Plug 'vigoux/LanguageTool.nvim' " Also grammar checking with LanguageTool

" Plugin to view undo history
Plug 'mbbill/undotree'

" CMake integration
Plug 'cdelledonne/vim-cmake'
Plug 'puremourning/vimspector'


Plug 'kovetskiy/sxhkd-vim'

Plug 'chrisbra/csv.vim'

Plug 'norcalli/nvim-colorizer.lua'

" Realtime collision
Plug 'jbyuki/instant.nvim'
Plug 'github/copilot.vim'

Plug 'fladson/vim-kitty'

call plug#end()

let g:instant_username = empty($NVIM_INSTANT_USERNAME) ? 'anonymous' : $NVIM_INSTANT_USERNAME

lua require'colorizer'.setup()

" ------------------------
" Languagetool configuration (Grammar checking)
" ------------------------

"let g:languagetool_jar='/usr/share/java/languagetool/languagetool-commandline.jar'

let g:languagetool_server_jar='/usr/share/java/languagetool/languagetool-server.jar'



" ------------------------
" Startify configuration
" ------------------------

let g:startify_change_to_dir = 1
let g:startify_change_to_vcs_root = 1


" ------------------------
" NERDTree configuration
" ------------------------

" Open NERDTree and Startify automatically when no files have been specified
autocmd StdinReadPre * let s:std_in=1
autocmd VimEnter * if argc() == 0 && !exists("s:std_in") | Startify | NERDTree | wincmd w | endif


" Start NERDTree when Vim starts with a directory argument.
autocmd StdinReadPre * let s:std_in=1
autocmd VimEnter * if argc() == 1 && isdirectory(argv()[0]) && !exists('s:std_in') |
    \ execute 'NERDTree' argv()[0] | wincmd p | enew | execute 'cd '.argv()[0] | endif

" Shortcut to open NERDTree
map <C-n> :NERDTreeToggle<CR>

" Close VIM if the only window left open is NERDTree
autocmd bufenter * if (winnr("$") == 1 && exists("b:NERDTree") && b:NERDTree.isTabTree()) | q | endif

let NERDTreeHijackNetrw = 0

" --------------------
" Vimtex configuration
" --------------------

" Use NVR for reverse search capability on neovim
let g:vimtex_compiler_progname = 'nvr'
let g:vimtex_complete_enabled = 1
let g:vimtex_complete_close_braces = 1
let g:vimtex_fold_enabled = 1
let g:vimtex_view_method = 'mupdf'
let g:tex_flavor = 'latex'

let g:coc_filetype_map = {'tex': 'latex'}

let g:vimtex_toc_config = {
      \ 'indent_levels': 1,
      \ 'show_help': 0,
      \ 'mode': 2
      \ }

" \ 'indent_levels': 1,
" Grammar checking through yalafi and language tool!
let g:vimtex_grammar_vlty = {'lt_command': 'languagetool'}


" --------------------
" Latex conceal stuff
" --------------------
" https://stackoverflow.com/questions/55287479/vim-and-latex-how-to-conceal-refname-as-name-in-vim-using-syntax-conce

au VimEnter * syntax region Statement start='\ref{' end='}' transparent contains=myStart,myEnd
au VimEnter * syntax match myStart '\ref{\ze\w\+' contained conceal cchar=[
au VimEnter * syntax match myEnd '\(\ref{\w\+\)\@<=\zs}' contained conceal cchar=]

au VimEnter * hi! link Conceal Statement
au VimEnter * set conceallevel=2


" --------------------
" Github Copilot configuration
" --------------------
imap <silent><script><expr> <C-L> copilot#Accept("\<CR>")
let g:copilot_no_tab_map = v:true

" --------------------
" COC Config
" --------------------

" Use tab for trigger completion with characters ahead and navigate
" NOTE: There's always complete item selected by default, you may want to enable
" no select by `"suggest.noselect": true` in your configuration file
" NOTE: Use command ':verbose imap <tab>' to make sure tab is not mapped by
" other plugin before putting this into your config
inoremap <silent><expr> <TAB>
      \ coc#pum#visible() ? coc#pum#next(1) :
      \ CheckBackspace() ? "\<Tab>" :
      \ coc#refresh()
inoremap <expr><S-TAB> coc#pum#visible() ? coc#pum#prev(1) : "\<C-h>"

" Make <CR> to accept selected completion item or notify coc.nvim to format
" <C-g>u breaks current undo, please make your own choice
inoremap <silent><expr> <CR> coc#pum#visible() ? coc#pum#confirm()
                              \: "\<C-g>u\<CR>\<c-r>=coc#on_enter()\<CR>"

function! CheckBackspace() abort
  let col = col('.') - 1
  return !col || getline('.')[col - 1]  =~# '\s'
endfunction

" Use <c-space> to trigger completion
if has('nvim')
  inoremap <silent><expr> <c-space> coc#refresh()
else
  inoremap <silent><expr> <c-@> coc#refresh()
endif

" Use `[g` and `]g` to navigate diagnostics
" Use `:CocDiagnostics` to get all diagnostics of current buffer in location list
nmap <silent> [g <Plug>(coc-diagnostic-prev)
nmap <silent> ]g <Plug>(coc-diagnostic-next)

" GoTo code navigation
nmap <silent> gd <Plug>(coc-definition)
nmap <silent> gy <Plug>(coc-type-definition)
nmap <silent> gi <Plug>(coc-implementation)
nmap <silent> gr <Plug>(coc-references)

" Use K to show documentation in preview window
nnoremap <silent> K :call ShowDocumentation()<CR>

function! ShowDocumentation()
  if CocAction('hasProvider', 'hover')
    call CocActionAsync('doHover')
  else
    call feedkeys('K', 'in')
  endif
endfunction

" Highlight the symbol and its references when holding the cursor
autocmd CursorHold * silent call CocActionAsync('highlight')

" Symbol renaming
nmap <leader>rn <Plug>(coc-rename)

" Formatting selected code
xmap <leader>f  <Plug>(coc-format-selected)
nmap <leader>f  <Plug>(coc-format-selected)

augroup mygroup
  autocmd!
  " Setup formatexpr specified filetype(s)
  autocmd FileType typescript,json setl formatexpr=CocAction('formatSelected')
  " Update signature help on jump placeholder
  autocmd User CocJumpPlaceholder call CocActionAsync('showSignatureHelp')
augroup end

" Applying code actions to the selected code block
" Example: `<leader>aap` for current paragraph
xmap <leader>a  <Plug>(coc-codeaction-selected)
nmap <leader>a  <Plug>(coc-codeaction-selected)

" Remap keys for applying code actions at the cursor position
nmap <leader>ac  <Plug>(coc-codeaction-cursor)
" Remap keys for apply code actions affect whole buffer
nmap <leader>as  <Plug>(coc-codeaction-source)
" Apply the most preferred quickfix action to fix diagnostic on the current line
nmap <leader>qf  <Plug>(coc-fix-current)

" Remap keys for applying refactor code actions
nmap <silent> <leader>re <Plug>(coc-codeaction-refactor)
xmap <silent> <leader>r  <Plug>(coc-codeaction-refactor-selected)
nmap <silent> <leader>r  <Plug>(coc-codeaction-refactor-selected)

" Run the Code Lens action on the current line
nmap <leader>cl  <Plug>(coc-codelens-action)

" Map function and class text objects
" NOTE: Requires 'textDocument.documentSymbol' support from the language server
xmap if <Plug>(coc-funcobj-i)
omap if <Plug>(coc-funcobj-i)
xmap af <Plug>(coc-funcobj-a)
omap af <Plug>(coc-funcobj-a)
xmap ic <Plug>(coc-classobj-i)
omap ic <Plug>(coc-classobj-i)
xmap ac <Plug>(coc-classobj-a)
omap ac <Plug>(coc-classobj-a)

" Remap <C-f> and <C-b> to scroll float windows/popups
if has('nvim-0.4.0') || has('patch-8.2.0750')
  nnoremap <silent><nowait><expr> <C-f> coc#float#has_scroll() ? coc#float#scroll(1) : "\<C-f>"
  nnoremap <silent><nowait><expr> <C-b> coc#float#has_scroll() ? coc#float#scroll(0) : "\<C-b>"
  inoremap <silent><nowait><expr> <C-f> coc#float#has_scroll() ? "\<c-r>=coc#float#scroll(1)\<cr>" : "\<Right>"
  inoremap <silent><nowait><expr> <C-b> coc#float#has_scroll() ? "\<c-r>=coc#float#scroll(0)\<cr>" : "\<Left>"
  vnoremap <silent><nowait><expr> <C-f> coc#float#has_scroll() ? coc#float#scroll(1) : "\<C-f>"
  vnoremap <silent><nowait><expr> <C-b> coc#float#has_scroll() ? coc#float#scroll(0) : "\<C-b>"
endif

" Use CTRL-S for selections ranges
" Requires 'textDocument/selectionRange' support of language server
nmap <silent> <C-s> <Plug>(coc-range-select)
xmap <silent> <C-s> <Plug>(coc-range-select)

" Add `:Format` command to format current buffer
command! -nargs=0 Format :call CocActionAsync('format')

" Add `:Fold` command to fold current buffer
command! -nargs=? Fold :call     CocAction('fold', <f-args>)

" Add `:OR` command for organize imports of the current buffer
command! -nargs=0 OR   :call     CocActionAsync('runCommand', 'editor.action.organizeImport')

" Add (Neo)Vim's native statusline support
" NOTE: Please see `:h coc-status` for integrations with external plugins that
" provide custom statusline: lightline.vim, vim-airline
set statusline^=%{coc#status()}%{get(b:,'coc_current_function','')}

" Mappings for CoCList
" Show all diagnostics
nnoremap <silent><nowait> <space>a  :<C-u>CocList diagnostics<cr>
" Manage extensions
nnoremap <silent><nowait> <space>e  :<C-u>CocList extensions<cr>
" Show commands
nnoremap <silent><nowait> <space>c  :<C-u>CocList commands<cr>
" Find symbol of current document
nnoremap <silent><nowait> <space>o  :<C-u>CocList outline<cr>
" Search workspace symbols
nnoremap <silent><nowait> <space>s  :<C-u>CocList -I symbols<cr>
" Do default action for next item
nnoremap <silent><nowait> <space>j  :<C-u>CocNext<CR>
" Do default action for previous item
nnoremap <silent><nowait> <space>k  :<C-u>CocPrev<CR>
" Resume latest coc list
nnoremap <silent><nowait> <space>p  :<C-u>CocListResume<CR>

" ------------------------
" Airline configuration
" ------------------------

let g:airline_powerline_fonts = 1

let g:airline#extensions#ale#enabled = 1
let g:airline#extensions#tabline#enabled = 1
let g:airline#extensions#denite#enabled = 1
let g:airline#extensions#fzf#enabled = 1

let g:airline_theme='minimalist'

let g:airline#extensions#default#section_truncate_width = {
      \ 'b': 100,
      \ 'x': 100,
      \ 'y': 100,
      \ 'z': 100,
      \ 'warning': 500,
      \ 'error': 500,
      \ }
let g:airline#extensions#default#layout = [
      \ [ 'a', 'b', 'c' ],
      \ [ 'x', 'y', 'z', 'error', 'warning' ]
      \ ]


" ------------------------
" Theme
" ------------------------

let g:oceanic_next_terminal_bold = 1
let g:oceanic_next_terminal_italic = 1

let ayucolor='light'

colorscheme jellybeans


" ------------------------
" ALE Configuration
" ------------------------


" let g:ale_languagetool_executable='/usr/share/java/languagetool/languagetool-server.jar'

let g:ale_fixers = {
            \   '*': ['remove_trailing_lines', 'trim_whitespace'],
            \   'javascript': ['eslint'],
            \   'python': ['black', 'isort'], 
            \}


let g:ale_linters = {
            \   'tex': ['languagetool', 'writegood']
            \}

let g:ale_hover_cursor = 1


" --------------------
" Spell check settings
" --------------------

set spell
set spelllang=en_us,de_de
highlight SpellCap gui=undercurl guibg=background guifg=NONE
highlight SpellBad guisp=red gui=undercurl guibg=background guifg=NONE
highlight SpellRare gui=bold guifg=NONE

" Use FZF for spell suggestion
function! FzfSpellSink(word)
  exe 'normal! "_ciw'.a:word
endfunction

function! FzfSpell()
  let suggestions = spellsuggest(expand("<cword>"))
  return fzf#run({'source': suggestions, 'sink': function("FzfSpellSink"), 'down': '10'})
endfunction
nnoremap z= :call FzfSpell()<CR>


" --------------------
" Denite settings
" --------------------

" Define mappings
autocmd FileType denite call s:denite_my_settings()
function! s:denite_my_settings() abort
  nnoremap <silent><buffer><expr> <CR>
  \ denite#do_map('do_action')
  nnoremap <silent><buffer><expr> d
  \ denite#do_map('do_action', 'delete')
  nnoremap <silent><buffer><expr> p
  \ denite#do_map('do_action', 'preview')
  nnoremap <silent><buffer><expr> q
  \ denite#do_map('quit')
  nnoremap <silent><buffer><expr> i
  \ denite#do_map('open_filter_buffer')
  nnoremap <silent><buffer><expr> <Space>
  \ denite#do_map('toggle_select').'j'
endfunction

" --------------------
" FZF-Preview configuration
" --------------------



" --------------------
" Emmet configuration
" --------------------

let g:user_emmet_mode='inv'
let g:user_emmet_leader_key='<C-d>'


" --------------------
" Typescript server configuration
" --------------------

let g:nvim_typescript#javascript_support=1
let g:nvim_typescript#vue_support=1
let g:nvim_typescript#server_path='/usr/bin/tsserver'


" --------------------
" Javascript configuration
" --------------------

let g:javascript_plugin_jsdoc = 1

"let g:javascript_conceal_function             = "ƒ"
"let g:javascript_conceal_null                 = "ø"
"let g:javascript_conceal_this                 = "@"
"let g:javascript_conceal_return               = "⇚"
"let g:javascript_conceal_undefined            = "¿"
"let g:javascript_conceal_NaN                  = "ℕ"
"let g:javascript_conceal_prototype            = "¶"
"let g:javascript_conceal_static               = "•"
"let g:javascript_conceal_super                = "Ω"
"let g:javascript_conceal_arrow_function       = "⇒"
"let g:javascript_conceal_noarg_arrow_function = "🞅"
"let g:javascript_conceal_underscore_arrow_function = "🞅"

augroup javascript_folding
  au!
  au FileType javascript setlocal foldmethod=syntax
augroup END

" --------------------
" echodoc configuration
" --------------------


" --------------------
" Fugitive configuration
" --------------------

let g:fugitive_gitlab_domains = ['https://git.rwth-aachen.de']
if !empty($GITLAB_API_TOKEN)
  let g:gitlab_api_keys = {'git.rwth-aachen.de': $GITLAB_API_TOKEN}
endif

" --------------------
" Gutentags configuration
" --------------------

let g:gutentags_project_root = ['Makefile']
let g:gutentags_ctags_tagfile = '.tags'
let g:gutentags_ctags_extra_args = ['--excmd=number']


" --------------------
" Vista configuration
" --------------------
let g:vista_stay_on_open=0
let g:vista_fzf_preview = ['right:50%']


" --------------------
" FZF configuration
" --------------------

" let g:fzf_layout = { 'window': { 'width': 1, 'height': 0.4, 'yoffset': 1 } }

let g:coc_fzf_preview = ''
let g:coc_fzf_opts = []
let g:fzf_preview_use_dev_icons = 1

" --------------------
" Custom commands
" --------------------

" Compile single c file and run it
autocmd Filetype c command! -nargs=* Runc !gcc -O3 %:t <args>; ./a.out; rm ./a.out
autocmd Filetype cpp command! -nargs=* Runc !gcc -O3 %:t <args>; ./a.out; rm ./a.out

" Allow saving of files as sudo when I forgot to start Vim using sudo.
" https://stackoverflow.com/questions/2600783/how-does-the-vim-write-with-sudo-trick-work
cmap w!! w !sudo tee > /dev/null %

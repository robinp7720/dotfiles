# Path to your oh-my-zsh installation.
export ZSH="$HOME/.oh-my-zsh"

AURHELPER="paru"

# Options for oh-my-zsh
# ---------------------
ZSH_THEME="agnoster"
HYPHEN_INSENSITIVE="true"
ENABLE_CORRECTION="true"
COMPLETION_WAITING_DOTS="true"
DISABLE_UNTRACKED_FILES_DIRTY="true"

plugins=(
    git
    fzf
    node
    npm
    colorize
    extract
    magic-enter
    sudo
    fancy-ctrl-z
    git-auto-fetch
    git-extras
    taskwarrior
    zsh-autosuggestions
    command-not-found
    bgnotify
)


# Options for oh-my-zsh plugins
# ---------------------
DISABLE_FZF_AUTO_COMPLETION="false"
ZSH_COLORIZE_STYLE="colorful"

ZSH_AUTOSUGGEST_STRATEGY=(history completion)

# Enable oh-my-zsh
# ---------------------
if [[ -r "$ZSH/oh-my-zsh.sh" ]]; then
  source "$ZSH/oh-my-zsh.sh"
fi

if [[ -r "$HOME/.config/environment" ]]; then
  source "$HOME/.config/environment"
fi

if [[ -r "$HOME/.config/vigil/shell-integration.zsh" ]]; then
  source "$HOME/.config/vigil/shell-integration.zsh"
fi

if [[ -n $VIRTUAL_ENV && -e "${VIRTUAL_ENV}/bin/activate" ]]; then
  source "${VIRTUAL_ENV}/bin/activate"
fi


# Configure aliases
# ---------------------

if [[ $AURHELPER == "paru" ]]; then
    alias update='paru -Syu --noconfirm --sudoloop --devel --pgpfetch'
elif [[ $AURHELPER == "yay" ]]; then
    alias update='yay -Syu --noconfirm --sudoloop --devel --timeupdate --batchinstall=false --pgpfetch'
else
    echo "No valid AUR helper set in .zshrc"
fi


alias yay=$AURHELPER

# Deleting stuff is dangerous
alias rrm="/usr/bin/rm"
alias rm='trash'

# libqcalculate is a far better calculator 
alias calc=qalc

# Hide the terminal when specific applications are run
alias mpv="devour mpv"

# Colorize the output from ip by default
alias ip='ip -c'

alias v='vim'
alias vim='nvim'

alias cat='bat'
alias dig='dog'
alias df='duf'
alias du='dust'

# eza commands
alias ls='eza -lbF --git --icons --header'
alias ll='eza -lbGF --git --icons --header'
alias llm='eza -lbGd --git --sort=modified --icons --header'
alias la='eza -lbhHigUmuSa --time-style=long-iso --git --color-scale --icons'
alias lx='eza -lbhHigUmuSa@ --time-style=long-iso --git --color-scale --icons'

# Setup the path
# ---------------------
typeset -U path PATH
path=(
  "$HOME/.local/bin"
  "$HOME/.cargo/bin"
  "/usr/local/bin"
  "$HOME/.scripts"
  $path
)
if command -v go >/dev/null 2>&1; then
  __go_bin="$(go env GOBIN 2>/dev/null)"
  __go_path="$(go env GOPATH 2>/dev/null)"
  if [[ -n "$__go_bin" ]]; then
    path+=("$__go_bin")
  fi
  if [[ -n "$__go_path" ]]; then
    path+=("$__go_path/bin")
  fi
  unset __go_bin __go_path
fi

# Setup android dev tools
export ANDROID_SDK_ROOT=/opt/android-sdk

path+=("$ANDROID_SDK_ROOT/tools/bin" "$ANDROID_SDK_ROOT/emulator")

# Configs for FZF
# ---------------------

export FZF_CTRL_T_OPTS="--preview 'preview {}'"

# Show a nice greeting when opening a terminal
# ---------------------
greeting() {
    printf "\e[91m"
    fortune stargate science startrek
    echo
}

greeting

# Added by LM Studio CLI (lms)
export PATH="$PATH:/home/robin/.lmstudio/bin"
# End of LM Studio CLI section


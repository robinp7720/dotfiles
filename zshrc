# If you come from bash you might have to change your $PATH.

# Path to your oh-my-zsh installation.
export ZSH="$HOME/.oh-my-zsh"

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
source $ZSH/oh-my-zsh.sh


source ~/.config/environment

#source /opt/esp-idf/export.sh

if [[ -n $VIRTUAL_ENV && -e "${VIRTUAL_ENV}/bin/activate" ]]; then
  source "${VIRTUAL_ENV}/bin/activate"
fi

# Configure aliases
# ---------------------
alias update='yay -Syu --noconfirm --sudoloop --devel --timeupdate --batchinstall=false --pgpfetch'

# Deleting stuff is dangerous
alias rrm="/usr/bin/rm"
alias rm='trash'

# Aliases to navigate to common directories
alias uni="cd $UNIVERSITY"
alias moodle="cd $HOME/Documents/MoodleSync/21ws"

alias malo="cd $UNIVERSITY/22ss/Malo/Hausaufgaben"
alias datkom="cd $UNIVERSITY/Datkom/"
alias dsal="cd $UNIVERSITY/Daten\ Strukturen/4\ Semester/"

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

#alias ssh="kitten ssh"

# Use eza instead of ls on native host only

    # eza commands
    # general use
    alias ls='eza -lbF --git --icons --header'                                                # list, size, type, git
    alias ll='eza -lbGF --git --icons --header'                                             # long list
    alias llm='eza -lbGd --git --sort=modified --icons --header'                            # long list, modified date sort
    alias la='eza -lbhHigUmuSa --time-style=long-iso --git --color-scale --icons'  # all list
    alias lx='eza -lbhHigUmuSa@ --time-style=long-iso --git --color-scale --icons' # all + extended list

# Setup the path
# ---------------------
export PATH=$HOME/.scripts:$PATH
export PATH=/usr/local/bin:$PATH
export PATH=$HOME/.gem/ruby/3.0.0/bin:$PATH
export PATH=$HOME/.cargo/bin:$PATH
export PATH=$HOME/.local/bin:$PATH
export PATH="$PATH:$(go env GOBIN):$(go env GOPATH)/bin"
export PATH="/opt/xpack-arm-none-eabi-gcc-12.2.1-1.2/bin:$PATH"

# Setup android dev tools
export ANDROID_SDK_ROOT=/opt/android-sdk

export PATH=$PATH:$ANDROID_SDK_ROOT/tools/bin
export PATH=$PATH:$ANDROID_SDK_ROOT/emulator

# Configs for FZF
# ---------------------

export FZF_CTRL_T_OPTS="--preview 'preview {}'"

# Because my uni uses extremely outdated microchips for the PSP course
# ---------------------
initavr() {
    export AVR="$HOME/.avr-toolchain"
    export PATH=$AVR/bin:$PATH

    export CPATH=$AVR/avr/include/

    echo -ne "[\e[34mSUC\e[0m]"
    echo ": Initialized avr toolchain."
    avr-gcc --version
}

# Function to connect to the PCPOOL
rdp() {
    bspc config top_padding 0;
    xfreerdp /u:yp302595 /p:$(pass Uni/RWTH | head -n1) /v:$1 /workarea;
    bspc config top_padding 40;
}

ba() {
    . ~/.scripts/start_ba_env.sh
}

# Show a nice greeting when opening a terminal
# ---------------------
greeting() {
    printf "\e[91m"
    fortune stargate science startrek
    echo
    #printf "\e[36m"
    #figlet "Recent Moodle"
    #printf "\e[37m"
    #find ~/Documents/MoodleSync/22ss -type f -exec stat --format '%W %w %n' "{}" \; | sort -nr | cut -d ' ' -f2,6- | head -n 10 | sed 's/  (für Maschinenbauer u. Wirt-Ing. 1. Sem.)//g' | sed 's/ für Materialwissenschaftler und Informatiker//g'

    #printf "\e[36m"
    #figlet "TODO:"
    #printf "\e[37m"
    #task next
}

greeting


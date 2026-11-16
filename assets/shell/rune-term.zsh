# OSC 133 prompt marks for rune-term. Evaluate `rune-term shell-integration zsh`
# from ~/.zshrc, or source this file.

# zle redraws the whole prompt after a resize, so `A` at its first line is the
# only mark the terminal needs there. starship assigns PROMPT once, at init:
# putting `A` back on each prompt makes the order of the two irrelevant.
__rune_term_precmd() {
    [[ $PROMPT == *$'\e]133;A\a'* ]] || PROMPT=$'%{\e]133;A\a%}'$PROMPT
}

__rune_term_preexec() {
    print -n '\e]133;C\a'
}

# Evaluated from ~/.zshrc, a `return` here would end the rc file.
if [[ -o interactive && -z ${__rune_term_marks-} ]]; then
    __rune_term_marks=1
    autoload -Uz add-zsh-hook
    add-zsh-hook precmd __rune_term_precmd
    add-zsh-hook preexec __rune_term_preexec
fi

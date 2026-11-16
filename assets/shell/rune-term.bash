# OSC 133 prompt marks for rune-term. Evaluate `rune-term shell-integration bash`
# from ~/.bashrc, or source this file, e.g. from /etc/profile.d.

# A profile.d file is read by sh too; it stops at this line.
[ -n "${BASH_VERSION-}" ] || return 0

# Readline redraws only the prompt's last line after a resize; the terminal
# needs `P;k=s` to find where that line starts.
__rune_term_mark_prompt() {
    local status=$? sep
    # Sourced before starship, this ran ahead of the PS1 it sets: run last again.
    if [[ ${PROMPT_COMMAND@a} == *a* ]]; then
        [[ ${PROMPT_COMMAND[-1]} == __rune_term_mark_prompt ]] || PROMPT_COMMAND+=(__rune_term_mark_prompt)
    elif [[ $PROMPT_COMMAND != *__rune_term_mark_prompt ]]; then
        PROMPT_COMMAND="${PROMPT_COMMAND:+$PROMPT_COMMAND$'\n'}__rune_term_mark_prompt"
    fi
    [[ $PS1 == *'133;A'* ]] && return $status
    case $PS1 in
        *$'\n'*) sep=$'\n' ;;
        *'\n'*) sep='\n' ;;
        *)
            PS1="\[\e]133;A\a\]$PS1"
            return $status
            ;;
    esac
    PS1="\[\e]133;A\a\]${PS1%"$sep"*}$sep\[\e]133;P;k=s\a\]${PS1##*"$sep"}"
    return $status
}

# PS0 needs bash 4.4. Evaluated from ~/.bashrc, a `return` here would end the rc file.
if [[ $- == *i* && -z ${__rune_term_marks-} ]] && ((BASH_VERSINFO[0] * 100 + BASH_VERSINFO[1] >= 404)); then
    __rune_term_marks=1
    __rune_term_mark_prompt
    PS0="${PS0-}\[\e]133;C\a\]"
fi

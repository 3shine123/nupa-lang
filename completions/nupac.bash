_nupac() {
    local i cur prev opts cmd
    COMPREPLY=()
    if [[ "${BASH_VERSINFO[0]}" -ge 4 ]]; then
        cur="$2"
    else
        cur="${COMP_WORDS[COMP_CWORD]}"
    fi
    prev="$3"
    cmd=""
    opts=""

    for i in "${COMP_WORDS[@]:0:COMP_CWORD}"
    do
        case "${cmd},${i}" in
            ",$1")
                cmd="nupac"
                ;;
            nupac,run)
                cmd="nupac__subcmd__run"
                ;;
            *)
                ;;
        esac
    done

    case "${cmd}" in
        nupac)
            opts="-v -o -I -L -S -rewrite-nupa --verbose --version -fnupa-arc -fno-nupa-arc -fno-checker -fno-libc -asm -arch --gen-completions run"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 1 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                -rewrite-nupa)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --verbose)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -v)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --version)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -fnupa-arc)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -fno-nupa-arc)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -fno-checker)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -fno-libc)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -o)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -I)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -L)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -asm)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -S)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -arch)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --gen-completions)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        nupac__subcmd__run)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 2 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
    esac
}

if [[ "${BASH_VERSINFO[0]}" -eq 4 && "${BASH_VERSINFO[1]}" -ge 4 || "${BASH_VERSINFO[0]}" -gt 4 ]]; then
    complete -F _nupac -o nosort -o bashdefault -o default nupac
else
    complete -F _nupac -o bashdefault -o default nupac
fi

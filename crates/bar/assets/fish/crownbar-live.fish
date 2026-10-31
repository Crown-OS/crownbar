# Installed by `crownbar install`. A command still running after
# $crownbar_live_after shows on the bar until it exits; quicker ones never do.
status is-interactive; or return

set -q crownbar_live_after; or set -g crownbar_live_after 10s
set -q crownbar_live_ignore; or set -g crownbar_live_ignore \
    vi vim nvim nano hx helix micro emacs less more man bat ssh mosh \
    htop btop top tmux zellij screen watch tail journalctl fzf yazi \
    claude codex gemini opencode fish bash zsh python python3 node

function __crownbar_live_preexec --on-event fish_preexec
    set -l line (string trim -- $argv[1])
    test -n "$line"; or return
    set -l program (string split -m1 -f1 ' ' -- $line)
    contains -- $program $crownbar_live_ignore; and return
    set -g __crownbar_live_key fish-$fish_pid-(random)
    "{exe}" push --key $__crownbar_live_key --title $line --glyph terminal \
        --since --reveal-after $crownbar_live_after --pid $fish_pid &>/dev/null &
    disown 2>/dev/null
end

function __crownbar_live_postexec --on-event fish_postexec
    set -l code $status
    set -q __crownbar_live_key; or return
    "{exe}" end $__crownbar_live_key --status $code &>/dev/null &
    disown 2>/dev/null
    set -e __crownbar_live_key
end

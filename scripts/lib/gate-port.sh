#!/usr/bin/env bash
# A runner port belonging to this gate, asked of the OS.
#
# `smix runner up` defaults to 22087, and the device gates took that
# default. So one unrelated runner anywhere on the machine — another
# checkout's, a developer's, one orphaned by a crash — made a gate exit
# at `runner up` before running a single flow. The corpus gate did
# exactly that on 2026-08-09, and the failure read as smix being broken
# when it was the gate colliding with a neighbour. A gate that a
# bystander process can turn red cannot run ten times in a row, and
# cannot run in CI beside anything else.
#
# Source this, do not run it: it exports into the caller's environment.
# `--runner-port` carries `env = "SMIX_RUNNER_PORT"`, so exporting once
# reaches `runner up`, every `smix run`, and the teardown, without a
# flag threaded through each call — and, importantly, without teardown
# being able to disagree with startup about which runner is being torn
# down.
#
# An inherited value wins. A caller who pins a port has a reason: a CI
# lane with a fixed mapping, or a debugging session against a runner
# that is already up.

# A free port, asked of the OS, into the variable named by $1.
#
# Gates that drive both platforms need two ports at once, and writing
# the second as a literal is how six scripts came to pin one: there was
# one way to ask and it only ever answered about `SMIX_RUNNER_PORT`.
#
# It assigns rather than prints because it remembers what it handed out,
# and a `$(…)` call would do that remembering inside a subshell that
# exits immediately. The memory is needed: the socket it probes with is
# closed before the number comes back, so nothing stops the OS offering
# the same port to the next call.
_gate_ports_handed_out="${_gate_ports_handed_out:-}"
gate_free_port() {
    local _into="$1" _port
    while :; do
        _port="$(python3 -c 'import socket
s = socket.socket()
s.bind(("127.0.0.1", 0))
print(s.getsockname()[1])
s.close()')"
        case " $_gate_ports_handed_out " in
            *" $_port "*) continue ;;
        esac
        _gate_ports_handed_out="$_gate_ports_handed_out $_port"
        eval "$_into=\$_port"
        return 0
    done
}

if [[ -z "${SMIX_RUNNER_PORT:-}" ]]; then
    gate_free_port SMIX_RUNNER_PORT
else
    _gate_ports_handed_out="$_gate_ports_handed_out ${SMIX_RUNNER_PORT}"
fi
export SMIX_RUNNER_PORT

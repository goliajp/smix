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

if [[ -z "${SMIX_RUNNER_PORT:-}" ]]; then
    SMIX_RUNNER_PORT="$(python3 -c 'import socket
s = socket.socket()
s.bind(("127.0.0.1", 0))
print(s.getsockname()[1])
s.close()')"
fi
export SMIX_RUNNER_PORT

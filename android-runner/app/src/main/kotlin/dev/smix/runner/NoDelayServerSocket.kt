package dev.smix.runner

import java.net.ServerSocket
import java.net.Socket

/// A server socket whose connections send each write at once.
///
/// NanoHTTPD writes a reply's header and its body separately and never
/// turns Nagle off, so a body smaller than a segment waits for the peer
/// to acknowledge the header — and the peer delays that acknowledgement.
class NoDelayServerSocket : ServerSocket() {
    override fun accept(): Socket = super.accept().also { it.tcpNoDelay = true }
}

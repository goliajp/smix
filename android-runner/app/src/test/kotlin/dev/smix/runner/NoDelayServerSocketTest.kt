// A reply's header and body leave in two writes, and a small body waits
// behind the header's acknowledgement unless the connection sends at once.
// Measured on the fixture before this: a 78-byte `/probe` reply was read
// 100-230 ms after its header on a kept-alive connection.

package dev.smix.runner

import java.net.InetAddress
import java.net.Socket
import org.junit.Assert.assertTrue
import org.junit.Test

class NoDelayServerSocketTest {
    @Test
    fun an_accepted_connection_sends_without_waiting() {
        NoDelayServerSocket().use { server ->
            server.bind(java.net.InetSocketAddress(InetAddress.getLoopbackAddress(), 0))
            Socket(InetAddress.getLoopbackAddress(), server.localPort).use {
                server.accept().use { accepted ->
                    assertTrue("accepted socket has TCP_NODELAY off", accepted.tcpNoDelay)
                }
            }
        }
    }
}

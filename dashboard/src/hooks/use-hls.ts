import Hls from 'hls.js'
import { useEffect } from 'react'

// Attach an HLS source to a <video>. MSE path (hls.js) for Chrome/Firefox;
// Safari plays application/vnd.apple.mpegurl natively, so we skip hls.js there.
// The effect tears the Hls instance down (destroy) on src change / unmount so
// the loader, MSE buffers, and timers don't leak across sim switches.
export function useHls(
  videoRef: React.RefObject<HTMLVideoElement | null>,
  src: string | null,
  enabled: boolean
): void {
  useEffect(() => {
    const video = videoRef.current
    if (video == null || src == null || !enabled) return

    if (Hls.isSupported()) {
      const hls = new Hls({
        liveDurationInfinity: true,
        lowLatencyMode: true,
        // 1s segments server-side: sync 2 segments from the edge (~2s)
        // instead of the 3-target-duration default, and catch up by
        // playing 1.5x rather than seek-jumping when we fall behind.
        liveSyncDurationCount: 2,
        liveMaxLatencyDurationCount: 6,
        maxLiveSyncPlaybackRate: 1.5,
      })
      hls.loadSource(src)
      hls.attachMedia(video)
      hls.on(Hls.Events.MANIFEST_PARSED, () => {
        void video.play().catch(() => {})
      })
      return () => {
        hls.destroy()
      }
    }

    if (video.canPlayType('application/vnd.apple.mpegurl') !== '') {
      video.src = src
      const onLoaded = (): void => {
        void video.play().catch(() => {})
      }
      video.addEventListener('loadedmetadata', onLoaded)
      return () => {
        video.removeEventListener('loadedmetadata', onLoaded)
        video.removeAttribute('src')
        video.load()
      }
    }

    return
  }, [videoRef, src, enabled])
}

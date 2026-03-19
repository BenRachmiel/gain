import { useEffect, useRef } from "react"
import { useAppStore } from "@/stores/app-store"
import { resolveStreamUrl } from "@/lib/api"
import type { ResolveMeta, Track } from "@/types/api"

export function useResolveStream() {
  const esRef = useRef<EventSource | null>(null)
  const resolvingAlbumId = useAppStore((s) => s.resolvingAlbumId)

  useEffect(() => {
    if (resolvingAlbumId === null) {
      esRef.current?.close()
      esRef.current = null
      return
    }

    // Close previous if any
    esRef.current?.close()

    const es = new EventSource(resolveStreamUrl(resolvingAlbumId))
    esRef.current = es

    es.addEventListener("meta", (e: MessageEvent) => {
      const meta: ResolveMeta = JSON.parse(e.data)
      useAppStore.getState().setResolveMeta(meta)
    })

    es.addEventListener("track", (e: MessageEvent) => {
      const track: Track = JSON.parse(e.data)
      useAppStore.getState().addResolvedTrack(track)
    })

    es.addEventListener("done", () => {
      es.close()
      esRef.current = null
      useAppStore.getState().finishResolve()
    })

    es.addEventListener("error", (e) => {
      es.close()
      esRef.current = null
      // SSE error event with data means server-sent error
      const me = e as MessageEvent
      if (me.data) {
        try {
          const d = JSON.parse(me.data)
          if (d.error) console.error("Resolve error:", d.error)
        } catch {
          // connection error, not data error
        }
      }
      useAppStore.getState().cancelResolve()
    })

    es.onerror = () => {
      es.close()
      esRef.current = null
      useAppStore.getState().cancelResolve()
    }

    return () => {
      es.close()
      esRef.current = null
    }
  }, [resolvingAlbumId])
}

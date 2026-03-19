import { useEffect, useRef } from "react"
import { useAppStore } from "@/stores/app-store"
import { statusStreamUrl } from "@/lib/api"
import type {
  JobUpdateEvent,
  TrackUpdateEvent,
  TrackProgressEvent,
  LogEvent,
} from "@/types/api"

export function useStatusStream() {
  const esRef = useRef<EventSource | null>(null)
  const reconnectTimer = useRef<ReturnType<typeof setTimeout>>(undefined)

  useEffect(() => {
    function connect() {
      const lastId = useAppStore.getState().lastEventId
      const es = new EventSource(statusStreamUrl(lastId))
      esRef.current = es

      es.addEventListener("job_update", (e: MessageEvent) => {
        const id = parseInt(e.lastEventId)
        if (id) useAppStore.getState().setLastEventId(id)
        const data: JobUpdateEvent = JSON.parse(e.data)
        useAppStore.getState().handleJobUpdate(data)
      })

      es.addEventListener("track_update", (e: MessageEvent) => {
        const id = parseInt(e.lastEventId)
        if (id) useAppStore.getState().setLastEventId(id)
        const data: TrackUpdateEvent = JSON.parse(e.data)
        useAppStore.getState().handleTrackUpdate(data)
      })

      es.addEventListener("track_progress", (e: MessageEvent) => {
        const id = parseInt(e.lastEventId)
        if (id) useAppStore.getState().setLastEventId(id)
        const data: TrackProgressEvent = JSON.parse(e.data)
        useAppStore.getState().handleTrackProgress(data)
      })

      es.addEventListener("log", (e: MessageEvent) => {
        const id = parseInt(e.lastEventId)
        if (id) useAppStore.getState().setLastEventId(id)
        const data: LogEvent = JSON.parse(e.data)
        useAppStore.getState().addLog(data.message)
      })

      es.onerror = () => {
        es.close()
        esRef.current = null
        reconnectTimer.current = setTimeout(connect, 2000)
      }
    }

    connect()

    return () => {
      esRef.current?.close()
      esRef.current = null
      clearTimeout(reconnectTimer.current)
    }
  }, [])
}

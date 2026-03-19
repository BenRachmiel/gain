import { useRef, useEffect } from "react"
import { ScrollArea } from "@/components/ui/scroll-area"
import { useAppStore } from "@/stores/app-store"

export function EventLog() {
  const logs = useAppStore((s) => s.logs)
  const endRef = useRef<HTMLDivElement>(null)

  useEffect(() => {
    endRef.current?.scrollIntoView({ behavior: "smooth" })
  }, [logs.length])

  if (logs.length === 0) return null

  return (
    <ScrollArea className="h-[120px] rounded-md border bg-card/50 p-2 font-mono text-[11px] text-muted-foreground">
      {logs.map((msg, i) => (
        <div key={i} className="py-px">
          {msg}
        </div>
      ))}
      <div ref={endRef} />
    </ScrollArea>
  )
}

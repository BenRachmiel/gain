import { Checkbox } from "@/components/ui/checkbox"
import { useAppStore } from "@/stores/app-store"
import type { Track } from "@/types/api"

interface TrackRowProps {
  track: Track
}

export function TrackRow({ track }: TrackRowProps) {
  const selected = useAppStore((s) => s.selectedTrackIndices.has(track.index))
  const toggleTrack = useAppStore((s) => s.toggleTrack)

  return (
    <div className="flex items-center gap-2 py-1 border-b border-border/30 last:border-b-0 text-sm">
      <Checkbox
        checked={selected}
        onCheckedChange={() => toggleTrack(track.index)}
        className="h-3.5 w-3.5"
      />
      <span className="w-6 text-right text-xs text-muted-foreground/60 font-mono shrink-0">
        {track.index}
      </span>
      <span className="flex-1 min-w-0 truncate" title={track.title}>
        {track.title}
      </span>
      <span className="text-xs text-muted-foreground/60 font-mono shrink-0">
        {track.duration}
      </span>
    </div>
  )
}

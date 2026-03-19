import { useState, useEffect } from "react"
import { X } from "lucide-react"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { ScrollArea } from "@/components/ui/scroll-area"
import { useAppStore } from "@/stores/app-store"
import { TrackRow } from "./track-row"

export function PreviewPanel() {
  const resolveMeta = useAppStore((s) => s.resolveMeta)
  const resolvedTracks = useAppStore((s) => s.resolvedTracks)
  const selectedTrackIndices = useAppStore((s) => s.selectedTrackIndices)
  const resolvingAlbumId = useAppStore((s) => s.resolvingAlbumId)
  const totalTracks = useAppStore((s) => s.totalTracks)
  const pendingJobId = useAppStore((s) => s.pendingJobId)
  const cancelResolve = useAppStore((s) => s.cancelResolve)
  const selectAllTracks = useAppStore((s) => s.selectAllTracks)
  const deselectAllTracks = useAppStore((s) => s.deselectAllTracks)
  const queueJob = useAppStore((s) => s.queueJob)

  const [artistOverride, setArtistOverride] = useState("")
  const [albumOverride, setAlbumOverride] = useState("")

  useEffect(() => {
    if (resolveMeta) {
      setArtistOverride(resolveMeta.matched_artist ?? resolveMeta.artist)
      setAlbumOverride(resolveMeta.album)
    }
  }, [resolveMeta])

  if (!resolveMeta) return null

  const isResolving = resolvingAlbumId !== null
  const selectedCount = selectedTrackIndices.size
  const done = !isResolving

  let btnText: string
  let btnDisabled: boolean
  if (pendingJobId) {
    btnText = "Queued (resolving...)"
    btnDisabled = true
  } else if (resolvedTracks.length === 0) {
    btnText = "Resolving..."
    btnDisabled = true
  } else if (done) {
    btnText = `Add Selected: ${selectedCount} track${selectedCount !== 1 ? "s" : ""}`
    btnDisabled = selectedCount === 0
  } else {
    btnText = `Queue now (${resolvedTracks.length} / ${totalTracks || "?"})`
    btnDisabled = false
  }

  const handleQueue = () => {
    queueJob(artistOverride, albumOverride)
  }

  const handleDismiss = () => {
    cancelResolve()
  }

  return (
    <div className="flex flex-col min-h-0 h-full">
      <div className="shrink-0">
        <div className="flex items-center justify-between mb-2">
          <h3 className="text-xs font-medium uppercase tracking-wider text-muted-foreground">
            Add to queue
          </h3>
          <Button
            variant="ghost"
            size="icon"
            className="h-6 w-6"
            onClick={handleDismiss}
          >
            <X className="h-3.5 w-3.5" />
          </Button>
        </div>

        <Button
          className="w-full mb-3"
          size="sm"
          onClick={handleQueue}
          disabled={btnDisabled}
        >
          {btnText}
        </Button>

        <div className="rounded-lg border bg-card p-3 space-y-2">
          <div className="text-xs">
            {resolveMeta.matched_artist ? (
              <span>
                <span className="text-green-500">Matched:</span>{" "}
                <strong>{resolveMeta.matched_artist}</strong>
              </span>
            ) : (
              <span>
                <span className="text-amber-500">New artist:</span>{" "}
                <strong>{resolveMeta.artist}</strong>
              </span>
            )}
          </div>
          <div className="flex items-center gap-2">
            <label className="text-xs text-muted-foreground w-12 shrink-0">
              Artist
            </label>
            <Input
              value={artistOverride}
              onChange={(e) => setArtistOverride(e.target.value)}
              className="h-7 text-xs"
            />
          </div>
          <div className="flex items-center gap-2">
            <label className="text-xs text-muted-foreground w-12 shrink-0">
              Album
            </label>
            <Input
              value={albumOverride}
              onChange={(e) => setAlbumOverride(e.target.value)}
              className="h-7 text-xs"
            />
          </div>
          <div className="text-[11px] text-muted-foreground/60 font-mono truncate">
            /music/{artistOverride}/{albumOverride}/
          </div>
        </div>

        {resolvedTracks.length > 0 && (
          <div className="flex justify-between mt-2 gap-2">
            <Button
              variant="secondary"
              size="sm"
              className="text-xs h-6 px-2"
              onClick={selectAllTracks}
            >
              Select all
            </Button>
            <Button
              variant="secondary"
              size="sm"
              className="text-xs h-6 px-2"
              onClick={deselectAllTracks}
            >
              Deselect all
            </Button>
          </div>
        )}
      </div>

      {resolvedTracks.length > 0 && (
        <ScrollArea className="flex-1 min-h-0 mt-2">
          {resolvedTracks.map((track) => (
            <TrackRow key={track.index} track={track} />
          ))}
        </ScrollArea>
      )}
    </div>
  )
}

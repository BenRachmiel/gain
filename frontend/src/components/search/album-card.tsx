import { useState } from "react"
import { useAppStore } from "@/stores/app-store"
import type { Album } from "@/types/api"

const RING_CIRC = 87.96 // 2 * PI * 14

interface AlbumCardProps {
  album: Album
}

export function AlbumCard({ album }: AlbumCardProps) {
  const [imgError, setImgError] = useState(false)
  const resolvingAlbumId = useAppStore((s) => s.resolvingAlbumId)
  const resolvedCount = useAppStore((s) => s.resolvedCount)
  const totalTracks = useAppStore((s) => s.totalTracks)
  const startResolve = useAppStore((s) => s.startResolve)
  const setDockOpen = useAppStore((s) => s.setDockOpen)
  const setDockTab = useAppStore((s) => s.setDockTab)

  const isResolving = resolvingAlbumId === album.id
  const ringOffset =
    totalTracks > 0 ? RING_CIRC * (1 - resolvedCount / totalTracks) : RING_CIRC

  const handleClick = () => {
    if (isResolving) return
    startResolve(album.id)
    setDockTab("preview")
    if (window.innerWidth <= 860) setDockOpen(true)
  }

  return (
    <div
      className={`group relative overflow-hidden rounded-lg border bg-card cursor-pointer transition-colors hover:border-muted-foreground/30 ${isResolving ? "pointer-events-none" : ""}`}
      onClick={handleClick}
    >
      {!imgError && album.cover ? (
        <img
          src={album.cover}
          alt=""
          loading="lazy"
          onError={() => setImgError(true)}
          className="w-full aspect-square object-cover bg-muted"
        />
      ) : (
        <div className="w-full aspect-square bg-muted flex items-center justify-center text-muted-foreground text-3xl">
          {"\u266B"}
        </div>
      )}
      <div className="p-2">
        <div className="text-sm truncate" title={album.title}>
          {album.title}
        </div>
        <div className="text-xs text-muted-foreground truncate">
          {album.artist}
        </div>
        <div className="text-xs text-muted-foreground/60">
          {album.year} &middot; {album.tracks} tracks
        </div>
      </div>

      {/* Resolve progress overlay */}
      {isResolving && (
        <div className="absolute inset-0 bg-background/85 flex flex-col items-center justify-center gap-2">
          <svg
            className="-rotate-90 overflow-visible"
            viewBox="0 0 36 36"
            width={44}
            height={44}
          >
            <circle
              cx={18}
              cy={18}
              r={14}
              fill="none"
              stroke="currentColor"
              strokeWidth={2.5}
              className="text-muted"
            />
            <circle
              cx={18}
              cy={18}
              r={14}
              fill="none"
              stroke="currentColor"
              strokeWidth={2.5}
              strokeLinecap="round"
              strokeDasharray={RING_CIRC}
              strokeDashoffset={ringOffset}
              className="text-muted-foreground transition-all duration-300"
            />
          </svg>
          <span className="text-xs text-muted-foreground">
            {resolvedCount} / {totalTracks || "?"}
          </span>
        </div>
      )}
    </div>
  )
}

import { useAppStore } from "@/stores/app-store"
import { AlbumCard } from "./album-card"

export function AlbumGrid() {
  const albums = useAppStore((s) => s.albums)
  const searchLoading = useAppStore((s) => s.searchLoading)
  const searchError = useAppStore((s) => s.searchError)

  if (searchLoading) {
    return <div className="text-sm text-muted-foreground py-4">Searching...</div>
  }

  if (searchError) {
    return <div className="text-sm text-destructive py-4">{searchError}</div>
  }

  if (albums.length === 0) return null

  return (
    <div className="grid grid-cols-[repeat(auto-fill,minmax(140px,1fr))] gap-3 mt-4">
      {albums.map((album) => (
        <AlbumCard key={album.id} album={album} />
      ))}
    </div>
  )
}

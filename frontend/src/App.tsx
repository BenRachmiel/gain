import { useEffect, useRef } from "react"
import { Toaster, toast } from "sonner"
import { useAppStore } from "@/stores/app-store"
import { useStatusStream } from "@/hooks/use-status-stream"
import { useResolveStream } from "@/hooks/use-resolve-stream"
import { NavBar } from "@/components/layout/nav-bar"
import { SearchBar } from "@/components/search/search-bar"
import { AlbumGrid } from "@/components/search/album-grid"
import { Dock } from "@/components/dock/dock"

function JobToastWatcher() {
  const jobs = useAppStore((s) => s.jobs)
  const toasted = useRef(new Set<string>())

  useEffect(() => {
    for (const job of jobs.values()) {
      if (job.status === "done" && !toasted.current.has(job.id)) {
        toasted.current.add(job.id)
        toast.success(`${job.album} — ${job.artist}`, {
          description: "Download complete",
        })
      } else if (job.status === "error" && !toasted.current.has(job.id)) {
        toasted.current.add(job.id)
        toast.error(`${job.album} — ${job.artist}`, {
          description: "Download failed",
        })
      }
    }
  }, [jobs])

  return null
}

export default function App() {
  const theme = useAppStore((s) => s.theme)
  const loadExistingJobs = useAppStore((s) => s.loadExistingJobs)

  useStatusStream()
  useResolveStream()

  useEffect(() => {
    loadExistingJobs()
  }, [loadExistingJobs])

  useEffect(() => {
    document.documentElement.classList.toggle("dark", theme === "dark")
  }, [theme])

  return (
    <div className="flex h-screen overflow-hidden bg-background text-foreground">
      <div className="flex-1 overflow-y-auto">
        <div className="max-w-5xl mx-auto px-4">
          <NavBar />
          <SearchBar />
          <AlbumGrid />
        </div>
      </div>

      <Dock />

      <Toaster theme={theme} position="bottom-left" richColors closeButton />
      <JobToastWatcher />
    </div>
  )
}

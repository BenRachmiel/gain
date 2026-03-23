import { create } from "zustand"
import type {
  Album,
  Track,
  ResolveMeta,
  JobState,
  JobUpdateEvent,
  TrackUpdateEvent,
  TrackProgressEvent,
} from "@/types/api"
import * as api from "@/lib/api"

type Theme = "dark" | "light"

interface AppState {
  // Search
  albums: Album[]
  searchLoading: boolean
  searchError: string | null

  // Resolution
  resolvingAlbumId: number | null
  resolveMeta: ResolveMeta | null
  resolvedTracks: Track[]
  selectedTrackIndices: Set<number>
  resolvedCount: number
  totalTracks: number

  // Jobs
  jobs: Map<string, JobState>
  pendingJobId: string | null
  sentTrackCount: number

  // SSE
  lastEventId: number
  logs: string[]

  // UI
  dockOpen: boolean
  dockTab: "preview" | "jobs" | "log"
  theme: Theme

  // Actions
  search: (q: string) => Promise<void>
  setAlbums: (albums: Album[]) => void

  // Resolution actions
  startResolve: (albumId: number) => void
  setResolveMeta: (meta: ResolveMeta) => void
  addResolvedTrack: (track: Track) => void
  finishResolve: () => void
  cancelResolve: () => void
  toggleTrack: (index: number) => void
  selectAllTracks: () => void
  deselectAllTracks: () => void

  // Job actions
  queueJob: (artist: string, album: string) => Promise<void>
  handleJobUpdate: (evt: JobUpdateEvent) => void
  handleTrackUpdate: (evt: TrackUpdateEvent) => void
  handleTrackProgress: (evt: TrackProgressEvent) => void
  clearCompletedJobs: () => Promise<void>
  loadExistingJobs: () => Promise<void>

  // SSE
  setLastEventId: (id: number) => void
  addLog: (msg: string) => void

  // UI
  setDockOpen: (open: boolean) => void
  setDockTab: (tab: "preview" | "jobs" | "log") => void
  toggleTheme: () => void
  setTheme: (theme: Theme) => void
}

export const useAppStore = create<AppState>((set, get) => ({
  // Initial state
  albums: [],
  searchLoading: false,
  searchError: null,

  resolvingAlbumId: null,
  resolveMeta: null,
  resolvedTracks: [],
  selectedTrackIndices: new Set(),
  resolvedCount: 0,
  totalTracks: 0,

  jobs: new Map(),
  pendingJobId: null,
  sentTrackCount: 0,

  lastEventId: 0,
  logs: [],

  dockOpen: false,
  dockTab: "preview",
  theme: (localStorage.getItem("theme") as Theme) ?? "dark",

  // Search
  search: async (q) => {
    set({ searchLoading: true, searchError: null })
    try {
      const albums = await api.searchAlbums(q)
      set({ albums, searchLoading: false })
    } catch (e) {
      set({
        searchError: e instanceof Error ? e.message : String(e),
        searchLoading: false,
      })
    }
  },
  setAlbums: (albums) => set({ albums }),

  // Resolution
  startResolve: (albumId) => {
    set({
      resolvingAlbumId: albumId,
      resolveMeta: null,
      resolvedTracks: [],
      selectedTrackIndices: new Set(),
      resolvedCount: 0,
      totalTracks: 0,
      pendingJobId: null,
      sentTrackCount: 0,
    })
  },

  setResolveMeta: (meta) => {
    set({ resolveMeta: meta, totalTracks: meta.total })
  },

  addResolvedTrack: (track) => {
    const { pendingJobId, sentTrackCount } = get()
    if (pendingJobId) {
      // Already queued — send directly to job
      api.appendTracks(pendingJobId, [track])
      set({ sentTrackCount: sentTrackCount + 1, resolvedCount: get().resolvedCount + 1 })
    } else {
      set((s) => {
        const newSelected = new Set(s.selectedTrackIndices)
        newSelected.add(track.index)
        return {
          resolvedTracks: [...s.resolvedTracks, track],
          selectedTrackIndices: newSelected,
          resolvedCount: s.resolvedCount + 1,
        }
      })
    }
  },

  finishResolve: () => {
    const { pendingJobId } = get()
    if (pendingJobId) {
      api.resolveJob(pendingJobId)
    }
    set({ resolvingAlbumId: null })
  },

  cancelResolve: () => {
    set({
      resolvingAlbumId: null,
      resolveMeta: null,
      resolvedTracks: [],
      selectedTrackIndices: new Set(),
      resolvedCount: 0,
      totalTracks: 0,
    })
  },

  toggleTrack: (index) =>
    set((s) => {
      const next = new Set(s.selectedTrackIndices)
      if (next.has(index)) next.delete(index)
      else next.add(index)
      return { selectedTrackIndices: next }
    }),

  selectAllTracks: () =>
    set((s) => ({
      selectedTrackIndices: new Set(s.resolvedTracks.map((t) => t.index)),
    })),

  deselectAllTracks: () => set({ selectedTrackIndices: new Set() }),

  // Jobs
  queueJob: async (artist, album) => {
    const { resolvedTracks, selectedTrackIndices, totalTracks, resolvingAlbumId, resolveMeta } = get()
    const resolved = resolvingAlbumId === null
    const selectedTracks = resolvedTracks.filter((t) =>
      selectedTrackIndices.has(t.index),
    )

    const jobId = await api.startJob({
      artist,
      album,
      tracks: selectedTracks,
      resolved,
      total_tracks: totalTracks || selectedTracks.length,
      cover_url: resolveMeta?.cover_url,
    })

    const newJob: JobState = {
      id: jobId,
      artist,
      album,
      status: "queued",
      current_track: null,
      track_count: totalTracks || selectedTracks.length,
      tracks_done: 0,
    }

    set((s) => {
      const jobs = new Map(s.jobs)
      jobs.set(jobId, newJob)
      return {
        jobs,
        pendingJobId: resolved ? null : jobId,
        sentTrackCount: selectedTracks.length,
        dockTab: "jobs",
      }
    })
  },

  handleJobUpdate: (evt) =>
    set((s) => {
      const jobs = new Map(s.jobs)
      const existing = jobs.get(evt.job_id)
      if (existing) {
        const updated = { ...existing, status: evt.status as JobState["status"] }
        if (evt.track_count != null) updated.track_count = evt.track_count
        if (evt.status === "done" || evt.status === "error") {
          updated.tracks_done = updated.track_count
          updated.trackPhase = undefined
          updated.trackPct = undefined
          updated.trackIndex = undefined
        }
        jobs.set(evt.job_id, updated)
      } else if (evt.artist) {
        jobs.set(evt.job_id, {
          id: evt.job_id,
          artist: evt.artist,
          album: evt.album ?? "",
          status: evt.status as JobState["status"],
          current_track: null,
          track_count: evt.track_count ?? 0,
          tracks_done: 0,
        })
      }
      return { jobs }
    }),

  handleTrackUpdate: (evt) =>
    set((s) => {
      const jobs = new Map(s.jobs)
      const job = jobs.get(evt.job_id)
      if (job && (evt.status === "done" || evt.status === "error")) {
        jobs.set(evt.job_id, {
          ...job,
          tracks_done: job.tracks_done + 1,
          trackPhase: undefined,
          trackPct: undefined,
          trackIndex: undefined,
        })
      }
      return { jobs }
    }),

  handleTrackProgress: (evt) =>
    set((s) => {
      const jobs = new Map(s.jobs)
      const job = jobs.get(evt.job_id)
      if (job) {
        jobs.set(evt.job_id, {
          ...job,
          trackPhase: evt.phase,
          trackPct: evt.pct,
          trackIndex: evt.index,
        })
      }
      return { jobs }
    }),

  clearCompletedJobs: async () => {
    await api.clearJobs()
    set((s) => {
      const jobs = new Map(s.jobs)
      for (const [id, job] of jobs) {
        if (job.status === "done" || job.status === "error") jobs.delete(id)
      }
      return { jobs }
    })
  },

  loadExistingJobs: async () => {
    const existing = await api.getJobs()
    set(() => {
      const jobs = new Map<string, JobState>()
      for (const j of existing) {
        jobs.set(j.id, { ...j })
      }
      return { jobs }
    })
  },

  // SSE
  setLastEventId: (id) => set({ lastEventId: id }),
  addLog: (msg) =>
    set((s) => ({
      logs: [...s.logs.slice(-499), msg],
    })),

  // UI
  setDockOpen: (open) => set({ dockOpen: open }),
  setDockTab: (tab) => set({ dockTab: tab }),
  toggleTheme: () =>
    set((s) => {
      const next = s.theme === "dark" ? "light" : "dark"
      localStorage.setItem("theme", next)
      return { theme: next }
    }),
  setTheme: (theme) => {
    localStorage.setItem("theme", theme)
    set({ theme })
  },
}))

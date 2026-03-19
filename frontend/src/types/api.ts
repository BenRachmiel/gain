export interface Album {
  id: number
  title: string
  artist: string
  cover: string
  tracks: number
  year: string
}

export interface Track {
  index: number
  title: string
  artist: string
  album: string
  duration: string
  url: string
}

export interface ResolveMeta {
  artist: string
  album: string
  matched_artist: string | null
  existing_artists: string[]
  total: number
}

export interface Job {
  id: string
  artist: string
  album: string
  status: "queued" | "active" | "done" | "error"
  current_track: number | null
  track_count: number
  tracks_done: number
}

export interface JobState extends Job {
  trackPhase?: string
  trackPct?: number
  trackIndex?: number
}

export interface JobUpdateEvent {
  job_id: string
  status: string
  artist?: string
  album?: string
  track_count?: number
}

export interface TrackUpdateEvent {
  job_id: string
  index: number
  title: string
  status: "downloading" | "transcoding" | "done" | "error"
  error?: string
}

export interface TrackProgressEvent {
  job_id: string
  index: number
  phase: "download" | "transcode"
  pct: number
}

export interface LogEvent {
  message: string
  job_id?: string
}

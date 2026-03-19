import type { Album, Job, Track } from "@/types/api"

const API_BASE = import.meta.env.VITE_API_URL ?? "/api"

async function json<T>(path: string, init?: RequestInit): Promise<T> {
  const res = await fetch(`${API_BASE}${path}`, init)
  if (!res.ok) throw new Error(`${res.status} ${res.statusText}`)
  return res.json() as Promise<T>
}

export async function searchAlbums(q: string): Promise<Album[]> {
  const data = await json<{ albums?: Album[]; error?: string }>(
    `/search?q=${encodeURIComponent(q)}`,
  )
  if (data.error) throw new Error(data.error)
  return data.albums ?? []
}

export async function startJob(payload: {
  artist: string
  album: string
  tracks: Track[]
  resolved: boolean
  total_tracks: number
}): Promise<string> {
  const data = await json<{ job_id: string }>("/start", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(payload),
  })
  return data.job_id
}

export async function appendTracks(
  jobId: string,
  tracks: Track[],
): Promise<void> {
  await json(`/jobs/${jobId}/tracks`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ tracks }),
  })
}

export async function resolveJob(jobId: string): Promise<void> {
  await json(`/jobs/${jobId}/resolve`, { method: "POST" })
}

export async function getJobs(): Promise<Job[]> {
  return json<Job[]>("/jobs")
}

export async function clearJobs(): Promise<void> {
  await json("/jobs/clear", { method: "POST" })
}

export function statusStreamUrl(lastId: number): string {
  return `${API_BASE}/status?last_id=${lastId}`
}

export function resolveStreamUrl(albumId: number): string {
  return `${API_BASE}/resolve/${albumId}`
}

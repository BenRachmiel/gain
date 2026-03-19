import { Button } from "@/components/ui/button"
import { useAppStore } from "@/stores/app-store"
import { JobCard } from "./job-card"

export function JobList() {
  const jobs = useAppStore((s) => s.jobs)
  const clearCompletedJobs = useAppStore((s) => s.clearCompletedJobs)

  const jobArray = Array.from(jobs.values())
  const hasCompleted = jobArray.some(
    (j) => j.status === "done" || j.status === "error",
  )

  if (jobArray.length === 0) {
    return (
      <div className="text-xs text-muted-foreground/60 py-4 text-center">
        No downloads yet
      </div>
    )
  }

  return (
    <div>
      {hasCompleted && (
        <div className="flex justify-end mb-2">
          <Button
            variant="secondary"
            size="sm"
            className="text-xs h-6 px-2"
            onClick={clearCompletedJobs}
          >
            Clear done
          </Button>
        </div>
      )}
      {jobArray.map((job) => (
        <JobCard key={job.id} job={job} />
      ))}
    </div>
  )
}

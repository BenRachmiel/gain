import { useState } from "react"
import { Search } from "lucide-react"
import { Input } from "@/components/ui/input"
import { Button } from "@/components/ui/button"
import { useAppStore } from "@/stores/app-store"

export function SearchBar() {
  const [query, setQuery] = useState("")
  const searchLoading = useAppStore((s) => s.searchLoading)
  const search = useAppStore((s) => s.search)

  const handleSearch = () => {
    const q = query.trim()
    if (!q) return
    search(q)
  }

  return (
    <div className="flex gap-2">
      <Input
        value={query}
        onChange={(e) => setQuery(e.target.value)}
        onKeyDown={(e) => e.key === "Enter" && handleSearch()}
        placeholder="Search albums..."
        className="flex-1"
      />
      <Button onClick={handleSearch} disabled={searchLoading}>
        <Search className="h-4 w-4 mr-1.5" />
        Search
      </Button>
    </div>
  )
}

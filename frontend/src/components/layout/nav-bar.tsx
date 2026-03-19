import { ThemeToggle } from "./theme-toggle"

export function NavBar() {
  return (
    <nav className="flex items-center justify-between py-4">
      <div className="flex items-center gap-2">
        <svg
          xmlns="http://www.w3.org/2000/svg"
          viewBox="0 0 24 24"
          className="h-[18px] w-[18px] text-muted-foreground"
          fill="none"
          stroke="currentColor"
          strokeWidth="2"
          strokeLinecap="round"
          strokeLinejoin="round"
        >
          <circle cx="12" cy="12" r="10" />
          <circle cx="12" cy="12" r="3" />
          <line x1="12" y1="2" x2="12" y2="5" />
        </svg>
        <h1 className="text-base font-semibold tracking-tight">Gain</h1>
      </div>
      <div className="flex items-center gap-3">
        <ThemeToggle />
      </div>
    </nav>
  )
}

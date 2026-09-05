/** The same lightweight stroke vocabulary as the application's icon set. */
export function QuickFilterIcon({ id }: { id: string }) {
  const shape = id === "dark" ? <path d="M20 15A9 9 0 0 1 9 4a9 9 0 1 0 11 11Z" />
    : id === "bright" ? <><circle cx="12" cy="12" r="4" /><path d="M12 2v2m0 16v2M2 12h2m16 0h2M5 5l2 2m10 10 2 2M5 19l2-2M17 7l2-2" /></>
    : id === "landscape" ? <path d="m3 19 6-13 4 8 3-5 5 10ZM8 9l3 2" />
    : id === "portrait" || id === "faces" ? <><circle cx="12" cy="7" r="4" /><path d="M4 21v-2a8 8 0 0 1 16 0v2Z" /></>
    : id === "monochrome" || id === "color" ? <><circle cx="12" cy="12" r="9" /><path d="M12 3a9 9 0 0 1 0 18Z" fill="currentColor" /></>
    : id.includes("eye") || id.includes("blink") ? <><path d="M2 12s4-7 10-7 10 7 10 7-4 7-10 7S2 12 2 12Z" /><circle cx="12" cy="12" r="3" /></>
    : id.includes("clipping") ? <><path d="M3 20h18M5 16V9m5 7V4m5 12V7m5 9V3" /></>
    : <circle cx="12" cy="12" r="9" strokeDasharray="1 4" />;
  return <svg className="quick-filter-icon" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor"
    strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">{shape}</svg>;
}

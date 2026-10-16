/** The small line icons of the navigation. */
export default function NavIcon({ name, className = 'nav-icon' }: { name: string; className?: string }) {
  return (
    <svg className={className} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.6" aria-hidden="true">
      {name === 'dashboard' ? <path d="M4 4h6v6H4zM14 4h6v6h-6zM4 14h6v6H4zM14 14h6v6h-6z" /> : null}
      {name === 'transactions' ? <path d="M4 7h16M4 12h16M4 17h16" /> : null}
      {name === 'categories' ? <path d="M4 8h7l2-3h7v14H4z" /> : null}
      {name === 'budgets' ? <path d="M5 19V9M12 19V5M19 19v-7" /> : null}
      {name === 'recurring' ? <path d="M5 12a7 7 0 0 1 12-5l2 2M19 4v5h-5M19 12a7 7 0 0 1-12 5l-2-2M5 20v-5h5" /> : null}
      {name === 'export' ? <path d="M12 15V4M8 8l4-4 4 4M5 14v5h14v-5" /> : null}
      {name === 'documents' ? <path d="M7 3h7l5 5v13H7zM14 3v5h5M10 13h6M10 17h6" /> : null}
      {name === 'clip' ? <path d="M21.4 11.1l-9.2 9.2a6 6 0 0 1-8.5-8.5l9.2-9.2a4 4 0 0 1 5.7 5.7l-9.2 9.2a2 2 0 0 1-2.8-2.8l8.5-8.5" /> : null}
      {name === 'help' ? <path d="M12 3a9 9 0 1 0 0 18 9 9 0 0 0 0-18zM9.6 9.3a2.4 2.4 0 1 1 3.4 2.2c-.7.4-1 .9-1 1.7M12 16.8h.01" /> : null}
      {name === 'settings' ? <path d="M4 7h10M18 7h2M4 17h2M10 17h10M14 4v6M6 14v6" /> : null}
      {name === 'sync' ? <path d="M4 8h14M14 4l4 4-4 4M20 16H6M10 12l-4 4 4 4" /> : null}
      {name === 'more' ? <path d="M5 12h.01M12 12h.01M19 12h.01" strokeWidth="2.4" strokeLinecap="round" /> : null}
      {name === 'add' ? <path d="M12 5v14M5 12h14" /> : null}
      {name === 'account' ? <path d="M12 12a3.5 3.5 0 1 0 0-7 3.5 3.5 0 0 0 0 7zM6.5 19a5.5 5.5 0 0 1 11 0" /> : null}
      {name === 'exit' ? <path d="M10 7V5h9v14h-9v-2M4 12h11M12 9l3 3-3 3" /> : null}
      {name === 'collapse' ? <path d="M5 5h14v14H5zM10 5v14" /> : null}
      {name === 'expand' ? <path d="M5 5h14v14H5zM9 5v14M13 9l3 3-3 3" /> : null}
    </svg>
  )
}

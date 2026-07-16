import { Link } from "@tanstack/react-router";

const linkClass =
	"rounded-lg px-3 py-1.5 text-ink-muted no-underline transition-colors hover:bg-accent-soft hover:text-ink";

/** Preferences + About — fixed top-right on every page. */
export function SiteNav() {
	return (
		<nav className="fixed top-5 right-5 z-40 flex items-center gap-1 text-sm sm:top-6 sm:right-6">
			<Link to="/preferences" className={linkClass}>
				Preferences
			</Link>
			<Link to="/about" className={linkClass}>
				About
			</Link>
		</nav>
	);
}

import { useQuery } from "@tanstack/react-query";
import { Link } from "@tanstack/react-router";
import { preferencesGet } from "#/lib/api";
import { stringsFor } from "#/lib/i18n";

const linkClass =
	"rounded-lg px-3 py-1.5 text-ink-muted no-underline transition-colors hover:bg-accent-soft hover:text-ink";

/** Preferences / Stats / About — fixed top-right on every page. */
export function SiteNav() {
	const prefs = useQuery({
		queryKey: ["preferences"],
		queryFn: preferencesGet,
	});
	const t = stringsFor(prefs.data?.locale);

	return (
		<nav className="fixed top-5 right-5 z-40 flex items-center gap-1 text-sm sm:top-6 sm:right-6">
			<Link to="/preferences" className={linkClass}>
				{t.preferences}
			</Link>
			<Link to="/stats" className={linkClass}>
				{t.stats}
			</Link>
			<Link to="/about" className={linkClass}>
				{t.about}
			</Link>
		</nav>
	);
}

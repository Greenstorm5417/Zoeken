import { createFileRoute, Link } from "@tanstack/react-router";
import { SiteNav } from "#/components/SiteNav";
import { useConfig } from "./__root";

export const Route = createFileRoute("/about")({ component: AboutPage });

function AboutPage() {
	const config = useConfig();
	const brand = config?.brand;
	const links = [
		["Documentation", brand?.DOCS_URL],
		["Privacy policy", brand?.PRIVACYPOLICY_URL],
		["Contact", brand?.CONTACT_URL],
		["Source code", brand?.GIT_URL],
	].filter(([, href]) => Boolean(href));

	return (
		<main className="relative mx-auto min-h-dvh w-full max-w-3xl px-6 py-10 pt-20">
			<SiteNav />
			<Link
				to="/"
				className="mb-8 inline-flex items-center gap-2 text-sm text-ink-muted no-underline hover:text-accent"
			>
				<img src="/zoeken-logo.svg" alt="" width={20} height={20} />
				Zoeken
			</Link>
			<h1 className="text-3xl font-bold tracking-tight">
				{config?.instance_name ?? "Zoeken"}
			</h1>
			<p className="mt-3 max-w-xl text-lg leading-relaxed text-ink-muted">
				A clean, private metasearch experience that brings results together
				without tracking your searches.
			</p>
			{links.length ? (
				<ul className="mt-8 flex flex-col gap-3">
					{links.map(([label, href]) => (
						<li key={label}>
							<a
								href={href as string}
								target="_blank"
								rel="noopener noreferrer"
								className="font-medium text-accent hover:underline"
							>
								{label} →
							</a>
						</li>
					))}
				</ul>
			) : null}
			{config?.version ? (
				<p className="mt-10 text-sm text-ink-subtle">
					Version {config.version}
				</p>
			) : null}
		</main>
	);
}

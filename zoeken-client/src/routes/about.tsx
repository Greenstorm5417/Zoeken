import { useQuery } from "@tanstack/react-query";
import { createFileRoute, Link } from "@tanstack/react-router";
import { SiteNav } from "#/components/SiteNav";
import { preferencesGet } from "#/lib/api";
import { stringsFor } from "#/lib/i18n";
import { useConfig } from "./__root";

export const Route = createFileRoute("/about")({ component: AboutPage });

function AboutPage() {
	const config = useConfig();
	const prefs = useQuery({
		queryKey: ["preferences"],
		queryFn: preferencesGet,
	});
	const t = stringsFor(prefs.data?.locale);
	const brand = config?.brand;
	const links = [
		[t.aboutDocs, brand?.DOCS_URL],
		[t.aboutPrivacy, brand?.PRIVACYPOLICY_URL],
		[t.aboutContact, brand?.CONTACT_URL],
		[t.aboutSource, brand?.GIT_URL],
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
				{t.aboutBlurb}
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
			) : (
				<p className="mt-8 max-w-xl text-sm text-ink-subtle">
					Operators: set{" "}
					<code className="font-mono text-xs">general.privacypolicy_url</code>,{" "}
					<code className="font-mono text-xs">general.contact_url</code>, and{" "}
					<code className="font-mono text-xs">brand.docs_url</code> /{" "}
					<code className="font-mono text-xs">brand.issue_url</code> in
					settings.yml so these links appear.
				</p>
			)}
			{config?.version ? (
				<p className="mt-10 text-sm text-ink-subtle">
					{t.aboutVersion} {config.version}
				</p>
			) : null}
		</main>
	);
}

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { createFileRoute, Link } from "@tanstack/react-router";
import { Check, Copy } from "lucide-react";
import { useState } from "react";
import { SelectMenu } from "#/components/SelectMenu";
import { SiteNav } from "#/components/SiteNav";
import {
	clearCookies,
	type Preferences,
	preferencesGet,
	preferencesPost,
} from "#/lib/api";
import { featureCatalog } from "#/lib/clientFeatures";
import { stringsFor } from "#/lib/i18n";
import {
	clearRecentSearches,
	recentSearchesEnabled,
	setRecentSearchesEnabled,
} from "#/lib/recentSearches";
import { getStoredTheme, setTheme, type Theme } from "#/lib/theme";
import { useConfig } from "./__root";

export const Route = createFileRoute("/preferences")({
	component: PreferencesPage,
});

/** Encode preferences into a shareable `#prefs=` URL fragment (base64 JSON). */
function preferencesUrl(prefs: Preferences): string {
	const json = JSON.stringify(prefs);
	const encoded = btoa(unescape(encodeURIComponent(json)));
	return `${window.location.origin}/preferences#prefs=${encoded}`;
}

function PreferencesPage() {
	const config = useConfig();
	const queryClient = useQueryClient();
	const [theme, setThemeState] = useState<Theme>(() => getStoredTheme());
	const [copied, setCopied] = useState(false);
	const [recentOn, setRecentOn] = useState(() => recentSearchesEnabled());
	const preferences = useQuery({
		queryKey: ["preferences"],
		queryFn: preferencesGet,
	});
	const save = useMutation({
		mutationFn: preferencesPost,
		onSuccess: (data) => queryClient.setQueryData(["preferences"], data),
	});
	const clear = useMutation({
		mutationFn: clearCookies,
		onSuccess: () =>
			void queryClient.invalidateQueries({ queryKey: ["preferences"] }),
	});

	// Import settings shared via a `#prefs=` fragment, then clean the URL.
	useState(() => {
		if (typeof window === "undefined") return;
		const match = /#prefs=([^&]+)/.exec(window.location.hash);
		if (!match) return;
		try {
			const json = decodeURIComponent(escape(atob(match[1])));
			const imported = JSON.parse(json) as Preferences;
			save.mutate(imported);
			window.history.replaceState(null, "", window.location.pathname);
		} catch {
			// ignore a malformed fragment
		}
	});

	if (preferences.isLoading)
		return (
			<Page>
				<p>{stringsFor(undefined).prefsLoading}</p>
			</Page>
		);
	if (!preferences.data)
		return (
			<Page>
				<p>{stringsFor(undefined).prefsUnavailable}</p>
			</Page>
		);
	const current = preferences.data;
	const update = (changes: Partial<Preferences>) =>
		save.mutate({ ...current, ...changes });

	const autocompleteBackends = config?.autocomplete_backends?.length
		? config.autocomplete_backends
		: ["duckduckgo", "google", "brave", "bing", "wikipedia"];
	const categoryOptions = [
		...new Set(["general", ...(config?.categories ?? [])]),
	];
	const engines = config?.engines ?? [];
	const selectedEngines = new Set(current.engines);
	const t = stringsFor(current.locale);
	const features = featureCatalog(config?.plugins);

	return (
		<Page>
			<h1 className="text-3xl font-bold tracking-tight">{t.preferences}</h1>
			<p className="mt-2 text-ink-muted">{t.prefsSavedLocally}</p>
			<div className="mt-8 grid max-w-2xl gap-8">
				<section className="grid gap-5">
					<h2 className="text-lg font-medium text-ink">{t.prefsSearch}</h2>
					<div>
						<span className="mb-1.5 block text-sm font-medium text-ink">
							{t.prefsInterfaceLanguage}
						</span>
						<SelectMenu
							fullWidth
							label={t.prefsInterfaceLanguage}
							value={current.locale}
							options={[
								{ value: "all", label: t.prefsAuto },
								...Object.entries(config?.locales ?? {}).map(
									([code, name]) => ({
										value: code,
										label: name,
									}),
								),
							]}
							onChange={(locale) => update({ locale })}
						/>
					</div>
					<div>
						<span className="mb-1.5 block text-sm font-medium text-ink">
							{t.prefsSearchLanguage}
						</span>
						<SelectMenu
							fullWidth
							label={t.prefsSearchLanguage}
							value={current.language}
							options={[
								{ value: "all", label: t.anyLanguage },
								...Object.entries(config?.locales ?? {}).map(
									([code, name]) => ({
										value: code,
										label: name,
									}),
								),
							]}
							onChange={(language) => update({ language })}
						/>
					</div>
					<div>
						<span className="mb-1.5 block text-sm font-medium text-ink">
							{t.prefsSearchMethod}
						</span>
						<SelectMenu
							fullWidth
							label={t.prefsSearchMethod}
							value={current.method}
							options={[
								{ value: "POST", label: "POST (query stays out of the URL)" },
								{ value: "GET", label: "GET (query in the URL)" },
							]}
							onChange={(method) =>
								update({ method: method as Preferences["method"] })
							}
						/>
						<p className="mt-1.5 text-xs text-ink-subtle">
							GET is useful for shareable result links; POST keeps the query out
							of browser history and server access logs.
						</p>
					</div>
					<div>
						<span className="mb-1.5 block text-sm font-medium text-ink">
							{t.prefsSafeSearch}
						</span>
						<SelectMenu
							fullWidth
							label={t.prefsSafeSearch}
							value={current.safesearch}
							options={[
								{ value: "Off", label: t.prefsOff },
								{ value: "Moderate", label: t.moderate },
								{ value: "Strict", label: t.strict },
							]}
							onChange={(safesearch) =>
								update({
									safesearch: safesearch as Preferences["safesearch"],
								})
							}
						/>
					</div>
					<div>
						<span className="mb-1.5 block text-sm font-medium text-ink">
							{t.prefsAutocomplete}
						</span>
						<SelectMenu
							fullWidth
							label={t.prefsAutocomplete}
							value={current.autocomplete || ""}
							options={[
								{ value: "", label: t.prefsOff },
								...autocompleteBackends.map((name) => ({
									value: name,
									label: name,
								})),
							]}
							onChange={(autocomplete) => update({ autocomplete })}
						/>
					</div>
					<label className="flex items-center gap-3 text-sm">
						<input
							type="checkbox"
							checked={current.image_proxy}
							onChange={(e) => update({ image_proxy: e.target.checked })}
							className="size-4 rounded border-line accent-[var(--accent)]"
						/>
						Use the image proxy
					</label>
					<label className="flex items-start gap-3 text-sm">
						<input
							type="checkbox"
							checked={recentOn}
							onChange={(e) => {
								const on = e.target.checked;
								setRecentSearchesEnabled(on);
								setRecentOn(on);
							}}
							className="mt-0.5 size-4 rounded border-line accent-[var(--accent)]"
						/>
						<span>
							<span className="font-medium text-ink">
								Remember recent searches
							</span>
							<span className="mt-0.5 block text-xs text-ink-muted">
								Off by default. When on, queries stay in this browser’s
								localStorage only — never sent to the server.
							</span>
						</span>
					</label>
					{recentOn ? (
						<button
							type="button"
							className="w-fit text-sm text-accent hover:underline"
							onClick={() => clearRecentSearches()}
						>
							Clear recent searches
						</button>
					) : null}
				</section>

				<section className="grid gap-4">
					<h2 className="text-lg font-medium text-ink">{t.prefsAppearance}</h2>
					<div>
						<span className="mb-1.5 block text-sm font-medium text-ink">
							{t.prefsTheme}
						</span>
						<div className="inline-flex rounded-xl border border-line bg-surface-raised p-1">
							{(["system", "light", "dark"] as const).map((option) => (
								<button
									key={option}
									type="button"
									onClick={() => {
										setTheme(option);
										setThemeState(option);
									}}
									className={[
										"rounded-lg px-4 py-1.5 text-sm capitalize transition-colors",
										theme === option
											? "bg-accent text-surface"
											: "text-ink-muted hover:text-ink",
									].join(" ")}
								>
									{option}
								</button>
							))}
						</div>
						<p className="mt-1.5 text-xs text-ink-subtle">{t.prefsThemeHint}</p>
					</div>
				</section>

				<section className="grid gap-3">
					<h2 className="text-lg font-medium text-ink">{t.prefsSync}</h2>
					<p className="text-sm text-ink-muted">{t.prefsSyncHint}</p>
					<button
						type="button"
						onClick={() => {
							void navigator.clipboard
								.writeText(preferencesUrl(current))
								.then(() => {
									setCopied(true);
									window.setTimeout(() => setCopied(false), 2000);
								});
						}}
						className="inline-flex w-fit items-center gap-2 rounded-xl border border-line bg-surface-raised px-4 py-2 text-sm text-ink transition-colors hover:border-accent hover:text-accent"
					>
						{copied ? (
							<>
								<Check className="size-4" aria-hidden />
								{t.prefsCopied}
							</>
						) : (
							<>
								<Copy className="size-4" aria-hidden />
								{t.prefsCopyLink}
							</>
						)}
					</button>
				</section>

				<section className="grid gap-3">
					<h2 className="text-lg font-medium text-ink">Search operators</h2>
					<p className="text-sm text-ink-muted">
						Refine any query with these operators (support varies by engine):
					</p>
					<dl className="grid grid-cols-[auto_1fr] gap-x-4 gap-y-2 rounded-xl border border-line bg-surface-raised p-4 text-sm">
						{[
							['"exact phrase"', "Match the words together, in order"],
							["site:example.com", "Limit results to one site"],
							["-word", "Exclude results containing a word"],
							["filetype:pdf", "Match a specific file type"],
							["!g query", "Search a specific engine by its bang shortcut"],
							[":en query", "Search in a specific language"],
						].map(([op, desc]) => (
							<div key={op} className="contents">
								<dt className="font-mono text-accent">{op}</dt>
								<dd className="text-ink-muted">{desc}</dd>
							</div>
						))}
					</dl>
				</section>

				<section className="grid gap-3">
					<h2 className="text-lg font-medium text-ink">Categories</h2>
					<p className="text-sm text-ink-muted">
						Default categories when you don’t pick a tab.
					</p>
					<div className="flex flex-wrap gap-2">
						{categoryOptions.map((category) => {
							const checked = current.categories.includes(category);
							return (
								<label
									key={category}
									className="flex items-center gap-2 rounded-xl border border-line bg-surface-raised px-3 py-2 text-sm capitalize"
								>
									<input
										type="checkbox"
										checked={checked}
										onChange={(e) => {
											const next = e.target.checked
												? [...current.categories, category]
												: current.categories.filter((c) => c !== category);
											update({
												categories: next.length ? next : ["general"],
											});
										}}
										className="size-4 accent-[var(--accent)]"
									/>
									{category}
								</label>
							);
						})}
					</div>
				</section>

				<section className="grid gap-3">
					<h2 className="text-lg font-medium text-ink">Engines</h2>
					<p className="text-sm text-ink-muted">
						Leave empty to use the instance defaults. Checking any engine
						restricts search to that set.
					</p>
					<div className="grid max-h-72 gap-2 overflow-y-auto rounded-xl border border-line p-3 sm:grid-cols-2">
						{engines.map((engine) => {
							const checked =
								selectedEngines.size === 0
									? engine.enabled
									: selectedEngines.has(engine.name);
							return (
								<label
									key={engine.name}
									className="flex items-start gap-2 text-sm"
								>
									<input
										type="checkbox"
										checked={checked}
										onChange={(e) => {
											const base =
												selectedEngines.size === 0
													? engines
															.filter((item) => item.enabled)
															.map((item) => item.name)
													: [...selectedEngines];
											const next = e.target.checked
												? [...new Set([...base, engine.name])]
												: base.filter((name) => name !== engine.name);
											update({ engines: next });
										}}
										className="mt-0.5 size-4 accent-[var(--accent)]"
									/>
									<span>
										<span className="font-medium text-ink">{engine.name}</span>
										<span className="mt-0.5 block text-xs text-ink-subtle capitalize">
											{engine.categories.join(", ") || "general"}
										</span>
									</span>
								</label>
							);
						})}
					</div>
					<button
						type="button"
						className="w-fit text-sm text-accent hover:underline"
						onClick={() => update({ engines: [] })}
					>
						Reset to instance defaults
					</button>
				</section>

				<section className="grid gap-3">
					<h2 className="text-lg font-medium text-ink">Features</h2>
					<p className="text-sm text-ink-muted">
						Client-side helpers and result transforms for this browser. Ahmia
						filtering still runs on the server when enabled.
					</p>
					<div className="grid gap-2">
						{features.map((feature) => {
							const checked =
								current.plugins?.[feature.id] ?? feature.default_enabled;
							return (
								<label
									key={feature.id}
									className="flex items-start gap-3 rounded-xl border border-line bg-surface-raised px-3 py-2 text-sm"
								>
									<input
										type="checkbox"
										checked={checked}
										onChange={(e) =>
											update({
												plugins: {
													...current.plugins,
													[feature.id]: e.target.checked,
												},
											})
										}
										className="mt-0.5 size-4 accent-[var(--accent)]"
									/>
									<span>
										<span className="font-medium text-ink">{feature.name}</span>
										<span className="mt-0.5 block text-xs text-ink-muted">
											{feature.description}
										</span>
									</span>
								</label>
							);
						})}
					</div>
				</section>

				{features.some((p) => p.id === "hostnames") ? (
					<section className="grid gap-3">
						<h2 className="text-lg font-medium text-ink">Hostname rewrite</h2>
						<p className="text-sm text-ink-muted">
							Enable or disable via the Hostnames feature above. Replace /
							remove / priority rules are instance settings (
							<code className="font-mono text-xs">hostnames:</code> in
							settings.yml) — not per-browser cookies.
						</p>
						{(() => {
							const h = config.hostnames;
							const hasRules =
								h &&
								(Object.keys(h.replace ?? {}).length > 0 ||
									(h.remove?.length ?? 0) > 0 ||
									(h.high_priority?.length ?? 0) > 0 ||
									(h.low_priority?.length ?? 0) > 0);
							if (!hasRules) {
								return (
									<p className="text-xs text-ink-subtle">
										No rewrite rules configured on this instance.
									</p>
								);
							}
							return (
								<dl className="grid gap-2 rounded-xl border border-line bg-surface-raised p-4 text-sm">
									{Object.entries(h.replace ?? {}).map(([from, to]) => (
										<div key={from} className="contents">
											<dt className="font-mono text-xs text-accent break-all">
												replace {from}
											</dt>
											<dd className="font-mono text-xs text-ink-muted break-all">
												→ {to}
											</dd>
										</div>
									))}
									{(h.remove ?? []).map((pat) => (
										<div key={`rm-${pat}`} className="contents">
											<dt className="font-mono text-xs text-accent">remove</dt>
											<dd className="font-mono text-xs text-ink-muted break-all">
												{pat}
											</dd>
										</div>
									))}
									{(h.high_priority ?? []).map((pat) => (
										<div key={`hi-${pat}`} className="contents">
											<dt className="font-mono text-xs text-accent">
												high_priority
											</dt>
											<dd className="font-mono text-xs text-ink-muted break-all">
												{pat}
											</dd>
										</div>
									))}
									{(h.low_priority ?? []).map((pat) => (
										<div key={`lo-${pat}`} className="contents">
											<dt className="font-mono text-xs text-accent">
												low_priority
											</dt>
											<dd className="font-mono text-xs text-ink-muted break-all">
												{pat}
											</dd>
										</div>
									))}
								</dl>
							);
						})()}
					</section>
				) : null}

				<button
					type="button"
					onClick={() => clear.mutate()}
					className="w-fit rounded-xl border border-line bg-surface-raised px-4 py-2 text-sm text-ink transition-colors hover:border-accent hover:text-accent"
				>
					Clear saved preferences
				</button>
				{save.isError || clear.isError ? (
					<p className="text-sm text-red-700">Couldn’t save preferences.</p>
				) : null}
			</div>
		</Page>
	);
}

function Page({ children }: { children: React.ReactNode }) {
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
			{children}
		</main>
	);
}

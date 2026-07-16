import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { createFileRoute, Link } from "@tanstack/react-router";
import { SelectMenu } from "#/components/SelectMenu";
import { SiteNav } from "#/components/SiteNav";
import {
	clearCookies,
	type Preferences,
	preferencesGet,
	preferencesPost,
} from "#/lib/api";
import { useConfig } from "./__root";

export const Route = createFileRoute("/preferences")({
	component: PreferencesPage,
});

function PreferencesPage() {
	const config = useConfig();
	const queryClient = useQueryClient();
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

	if (preferences.isLoading)
		return (
			<Page>
				<p>Loading preferences…</p>
			</Page>
		);
	if (!preferences.data)
		return (
			<Page>
				<p>Preferences are unavailable.</p>
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

	return (
		<Page>
			<h1 className="text-3xl font-bold tracking-tight">Preferences</h1>
			<p className="mt-2 text-ink-muted">Changes are saved to this browser.</p>
			<div className="mt-8 grid max-w-2xl gap-8">
				<section className="grid gap-5">
					<h2 className="text-lg font-medium text-ink">Search</h2>
					<div>
						<span className="mb-1.5 block text-sm font-medium text-ink">
							Language
						</span>
						<SelectMenu
							fullWidth
							label="Language"
							value={current.language}
							options={[
								{ value: "all", label: "Any language" },
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
							Safe search
						</span>
						<SelectMenu
							fullWidth
							label="Safe search"
							value={current.safesearch}
							options={[
								{ value: "Off", label: "Off" },
								{ value: "Moderate", label: "Moderate" },
								{ value: "Strict", label: "Strict" },
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
							Autocomplete
						</span>
						<SelectMenu
							fullWidth
							label="Autocomplete"
							value={current.autocomplete || ""}
							options={[
								{ value: "", label: "Off" },
								...autocompleteBackends.map((name) => ({
									value: name,
									label: name,
								})),
							]}
							onChange={(autocomplete) => update({ autocomplete })}
						/>
					</div>
					<div>
						<span className="mb-1.5 block text-sm font-medium text-ink">
							Search method
						</span>
						<SelectMenu
							fullWidth
							label="Search method"
							value={current.method}
							options={[
								{ value: "POST", label: "POST" },
								{ value: "GET", label: "GET" },
							]}
							onChange={(method) =>
								update({ method: method as Preferences["method"] })
							}
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

				{(config?.plugins?.length ?? 0) > 0 ? (
					<section className="grid gap-3">
						<h2 className="text-lg font-medium text-ink">Plugins</h2>
						<div className="grid gap-2">
							{config?.plugins.map((plugin) => {
								const checked =
									current.plugins?.[plugin.id] ?? plugin.default_enabled;
								return (
									<label
										key={plugin.id}
										className="flex items-start gap-3 rounded-xl border border-line bg-surface-raised px-3 py-2 text-sm"
									>
										<input
											type="checkbox"
											checked={checked}
											onChange={(e) =>
												update({
													plugins: {
														...current.plugins,
														[plugin.id]: e.target.checked,
													},
												})
											}
											className="mt-0.5 size-4 accent-[var(--accent)]"
										/>
										<span>
											<span className="font-medium text-ink">
												{plugin.name}
											</span>
											<span className="mt-0.5 block text-xs text-ink-muted">
												{plugin.description}
											</span>
										</span>
									</label>
								);
							})}
						</div>
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

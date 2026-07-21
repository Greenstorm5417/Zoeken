import { useNavigate } from "@tanstack/react-router";
import { Search } from "lucide-react";
import {
	type FormEvent,
	type KeyboardEvent,
	useEffect,
	useId,
	useRef,
	useState,
} from "react";
import {
	autocomplete,
	type BangInfo,
	bangs as fetchBangs,
	type Suggestion,
} from "#/lib/api";
import {
	clearRecentSearches,
	getRecentSearches,
	recentSearchesEnabled,
	rememberRecentSearch,
} from "#/lib/recentSearches";
import type { SearchRouteParams } from "#/lib/searchParams";
import { useConfig } from "#/routes/__root";

/** Session-scoped suggestion cache (query → suggestions), capped. */
const suggestionCache = new Map<string, Suggestion[]>();

function rememberSuggestions(query: string, suggestions: Suggestion[]) {
	if (suggestionCache.size >= 500) {
		suggestionCache.clear();
	}
	suggestionCache.set(query, suggestions);
}

const OPERATORS = [
	{ label: "site:", insert: "site:", tip: "Limit to a site" },
	{ label: "filetype:", insert: "filetype:", tip: "Filter by file type" },
	{ label: "-exclude", insert: "-", tip: "Exclude a word" },
] as const;

type SearchFormProps = {
	initialQuery?: string;
	autoFocus?: boolean;
	compact?: boolean;
	baseSearch?: Partial<SearchRouteParams>;
};

export function SearchForm({
	initialQuery = "",
	autoFocus = false,
	compact = false,
	baseSearch,
}: SearchFormProps) {
	const navigate = useNavigate();
	const config = useConfig();
	const inputId = useId();
	const listboxId = useId();
	const [q, setQ] = useState(initialQuery);
	const [suggestions, setSuggestions] = useState<Suggestion[]>([]);
	const [activeSuggestion, setActiveSuggestion] = useState(-1);
	const [focused, setFocused] = useState(false);
	const [bangHelpOpen, setBangHelpOpen] = useState(false);
	const [bangFilter, setBangFilter] = useState("");
	const [bangHits, setBangHits] = useState<BangInfo[]>([]);
	const [recents, setRecents] = useState<string[]>(() => getRecentSearches());
	const inputRef = useRef<HTMLInputElement>(null);
	const requestSeq = useRef(0);
	const minChars = config?.autocomplete_min ?? 1;
	const autocompleteEnabled = Boolean(config?.autocomplete);
	const recentOn = recentSearchesEnabled();

	useEffect(() => {
		const trimmed = q.trim();
		if (!autocompleteEnabled || !focused || trimmed.length < minChars) {
			setSuggestions([]);
			return;
		}
		const cached = suggestionCache.get(trimmed);
		if (cached) {
			setSuggestions(cached);
			return;
		}
		const seq = ++requestSeq.current;
		const timeout = window.setTimeout(() => {
			void autocomplete(trimmed)
				.then((items) => {
					const next = items ?? [];
					rememberSuggestions(trimmed, next);
					if (
						seq === requestSeq.current &&
						document.activeElement === inputRef.current
					) {
						setSuggestions(next);
					}
				})
				.catch(() => {
					if (seq === requestSeq.current) setSuggestions([]);
				});
		}, 100);
		return () => window.clearTimeout(timeout);
	}, [autocompleteEnabled, focused, minChars, q]);

	useEffect(() => {
		if (!bangHelpOpen) return;
		const filter = bangFilter.trim().replace(/^!/, "");
		if (!filter) {
			setBangHits([]);
			return;
		}
		const timeout = window.setTimeout(() => {
			void fetchBangs(filter)
				.then(setBangHits)
				.catch(() => setBangHits([]));
		}, 120);
		return () => window.clearTimeout(timeout);
	}, [bangHelpOpen, bangFilter]);

	function submit(query = q) {
		query = query.trim();
		if (!query) return;
		rememberRecentSearch(query);
		setRecents(getRecentSearches());
		setSuggestions([]);
		setActiveSuggestion(-1);
		setBangHelpOpen(false);
		void navigate({
			to: "/search",
			search: { ...baseSearch, q: query, pageno: undefined },
		});
	}

	function insertOperator(op: string) {
		const el = inputRef.current;
		const start = el?.selectionStart ?? q.length;
		const end = el?.selectionEnd ?? q.length;
		const before = q.slice(0, start);
		const after = q.slice(end);
		const needsSpace = before.length > 0 && !/\s$/.test(before);
		const next = `${before}${needsSpace ? " " : ""}${op}${after}`;
		setQ(next);
		setSuggestions([]);
		requestAnimationFrame(() => {
			el?.focus();
			const caret = before.length + (needsSpace ? 1 : 0) + op.length;
			el?.setSelectionRange(caret, caret);
		});
	}

	// Bang autocomplete: when the query ends with `!<partial>`, offer matching
	// engine shortcuts from the instance config instead of query suggestions.
	const bangMatch = /(^|\s)!([a-z0-9_.-]*)$/i.exec(q);
	const bangPrefix = bangMatch?.[2]?.toLowerCase() ?? null;
	const engineBangs =
		bangPrefix === null
			? []
			: (config?.engines ?? [])
					.filter(
						(engine) =>
							engine.shortcut &&
							(bangPrefix === "" ||
								engine.shortcut.toLowerCase().startsWith(bangPrefix) ||
								engine.name.toLowerCase().includes(bangPrefix)),
					)
					.slice(0, 8);

	function applyBang(shortcut: string) {
		const next = q.replace(/(^|\s)!([a-z0-9_.-]*)$/i, `$1!${shortcut} `);
		const applied =
			next === q ? `${q.trimEnd()} !${shortcut} `.trimStart() : next;
		setQ(applied);
		setSuggestions([]);
		setActiveSuggestion(-1);
		setBangHelpOpen(false);
		inputRef.current?.focus();
	}

	const showEngineBangs = focused && !bangHelpOpen && engineBangs.length > 0;
	const showSuggestions =
		focused && !bangHelpOpen && !showEngineBangs && suggestions.length > 0;
	const showRecents =
		focused &&
		!bangHelpOpen &&
		!showEngineBangs &&
		!showSuggestions &&
		recentOn &&
		q.trim() === "" &&
		recents.length > 0;
	const listOpen = showEngineBangs || showSuggestions;
	const optionCount = showEngineBangs ? engineBangs.length : suggestions.length;
	const activeOptionId =
		listOpen && activeSuggestion >= 0
			? `${listboxId}-opt-${activeSuggestion}`
			: undefined;

	function onSubmit(e: FormEvent) {
		e.preventDefault();
		if (
			showEngineBangs &&
			activeSuggestion >= 0 &&
			engineBangs[activeSuggestion]
		) {
			applyBang(engineBangs[activeSuggestion].shortcut);
			return;
		}
		submit(activeSuggestion >= 0 ? suggestions[activeSuggestion].text : q);
	}

	function onKeyDown(event: KeyboardEvent<HTMLInputElement>) {
		if (!listOpen) {
			if (event.key === "Escape") {
				setSuggestions([]);
				setActiveSuggestion(-1);
				setBangHelpOpen(false);
			}
			return;
		}
		if (event.key === "ArrowDown" || event.key === "ArrowUp") {
			event.preventDefault();
			setActiveSuggestion((index) => {
				const next = event.key === "ArrowDown" ? index + 1 : index - 1;
				return (next + optionCount) % optionCount;
			});
			return;
		}
		if (event.key === "Home") {
			event.preventDefault();
			setActiveSuggestion(0);
			return;
		}
		if (event.key === "End") {
			event.preventDefault();
			setActiveSuggestion(optionCount - 1);
			return;
		}
		if (event.key === "Escape") {
			event.preventDefault();
			setSuggestions([]);
			setActiveSuggestion(-1);
			return;
		}
		if (event.key === "Enter" && activeSuggestion >= 0) {
			event.preventDefault();
			if (showEngineBangs) {
				applyBang(engineBangs[activeSuggestion].shortcut);
			} else {
				submit(suggestions[activeSuggestion].text);
			}
		}
	}

	return (
		<search className={compact ? "w-full min-w-0 flex-1" : "w-full"}>
			<form onSubmit={onSubmit} className="w-full">
				<label className="sr-only" htmlFor={inputId}>
					Search
				</label>
				<div className="relative">
					<input
						data-search-input
						id={inputId}
						name="q"
						type="search"
						role="combobox"
						aria-autocomplete="list"
						aria-expanded={listOpen}
						aria-controls={listOpen ? listboxId : undefined}
						aria-activedescendant={activeOptionId}
						aria-haspopup="listbox"
						value={q}
						onChange={(e) => {
							setQ(e.target.value);
							setActiveSuggestion(-1);
						}}
						onKeyDown={onKeyDown}
						onFocus={() => setFocused(true)}
						onBlur={() => {
							setFocused(false);
							setSuggestions([]);
							setActiveSuggestion(-1);
						}}
						ref={inputRef}
						// biome-ignore lint/a11y/noAutofocus: home search should be ready to type
						autoFocus={autoFocus}
						autoComplete="off"
						spellCheck={false}
						placeholder="Search…"
						className={[
							"zoeken-search-input w-full border border-line bg-surface-raised text-ink outline-none",
							"placeholder:text-ink-subtle",
							"transition-[border-color,box-shadow] duration-100",
							"focus:border-accent focus:shadow-[0_0_0_3px_var(--accent-soft)]",
							compact
								? "h-11 rounded-xl px-5 pr-12 text-[0.95rem]"
								: "h-14 rounded-2xl px-6 pr-14 text-lg shadow-[0_1px_3px_rgba(20,32,24,0.06),0_8px_24px_rgba(20,32,24,0.06)]",
						].join(" ")}
					/>
					<button
						type="button"
						title="Bang shortcuts (!)"
						aria-label="Browse bang shortcuts"
						aria-expanded={bangHelpOpen}
						onMouseDown={(e) => e.preventDefault()}
						onClick={() => {
							setBangHelpOpen((open) => !open);
							setBangFilter("");
							setBangHits([]);
						}}
						className={[
							"absolute top-1/2 right-2 -translate-y-1/2 rounded-lg px-2 py-1 font-mono text-sm",
							"text-ink-subtle transition-colors hover:bg-accent-soft hover:text-accent",
							bangHelpOpen ? "bg-accent-soft text-accent" : "",
						].join(" ")}
					>
						!
					</button>

					{bangHelpOpen ? (
						<div className="absolute z-20 mt-2 w-full overflow-hidden rounded-2xl border border-line bg-surface-raised shadow-[0_12px_40px_rgba(20,32,24,0.14)] animate-fade">
							<div className="border-b border-line px-3 py-2">
								<input
									type="search"
									value={bangFilter}
									onChange={(e) => setBangFilter(e.target.value)}
									placeholder="Search bangs (e.g. g, w, gh)…"
									className="w-full rounded-lg border border-line bg-surface px-3 py-2 text-sm text-ink outline-none focus:border-accent"
									// biome-ignore lint/a11y/noAutofocus: help panel opens for typing
									autoFocus
								/>
								<p className="mt-1.5 text-[0.7rem] text-ink-subtle">
									External bangs redirect away from this instance. Type a filter
									to search.
								</p>
							</div>
							<ul className="max-h-64 overflow-y-auto py-1.5">
								{bangHits.length === 0 ? (
									<li className="px-4 py-3 text-sm text-ink-muted">
										{bangFilter.trim()
											? "No matching bangs"
											: "Start typing a bang name"}
									</li>
								) : (
									bangHits.map((bang) => (
										<li key={bang.shortcut}>
											<button
												type="button"
												className="flex w-full items-center gap-3 px-4 py-2 text-left transition-colors hover:bg-accent-soft"
												onMouseDown={(event) => event.preventDefault()}
												onClick={() => applyBang(bang.shortcut)}
											>
												<span className="shrink-0 rounded-md bg-accent-soft px-1.5 py-0.5 font-mono text-[0.8rem] text-accent">
													!{bang.shortcut}
												</span>
												<span className="truncate text-[0.8rem] text-ink-subtle">
													{bang.url.replaceAll("\u0002", "{q}")}
												</span>
											</button>
										</li>
									))
								)}
							</ul>
						</div>
					) : null}

					{showEngineBangs ? (
						<ul
							id={listboxId}
							// biome-ignore lint/a11y/noNoninteractiveElementToInteractiveRole: combobox listbox pattern
							role="listbox"
							aria-label="Engine bangs"
							className="absolute z-20 mt-2 w-full overflow-hidden rounded-2xl border border-line bg-surface-raised py-1.5 shadow-[0_12px_40px_rgba(20,32,24,0.14)] animate-fade"
						>
							<li className="px-4 pt-1 pb-1.5 text-[0.7rem] font-semibold tracking-wide text-ink-subtle uppercase">
								Search a specific engine
							</li>
							{engineBangs.map((engine, index) => (
								// biome-ignore lint/a11y/useFocusableInteractive: keyboard nav stays on the input
								<li
									key={engine.name}
									id={`${listboxId}-opt-${index}`}
									// biome-ignore lint/a11y/noNoninteractiveElementToInteractiveRole: option under listbox
									role="option"
									aria-selected={index === activeSuggestion}
								>
									<button
										type="button"
										tabIndex={-1}
										className={`flex w-full items-center gap-3 px-4 py-2 text-left transition-colors hover:bg-accent-soft ${index === activeSuggestion ? "bg-accent-soft" : ""}`}
										onMouseDown={(event) => event.preventDefault()}
										onMouseEnter={() => setActiveSuggestion(index)}
										onClick={() => applyBang(engine.shortcut)}
									>
										<span className="shrink-0 rounded-md bg-accent-soft px-1.5 py-0.5 font-mono text-[0.8rem] text-accent">
											!{engine.shortcut}
										</span>
										<span className="truncate text-[0.95rem] text-ink capitalize">
											{engine.name}
										</span>
										<span className="ml-auto shrink-0 truncate text-[0.7rem] text-ink-subtle capitalize">
											{engine.categories[0] ?? ""}
										</span>
									</button>
								</li>
							))}
						</ul>
					) : null}
					{showSuggestions ? (
						<ul
							id={listboxId}
							// biome-ignore lint/a11y/noNoninteractiveElementToInteractiveRole: combobox listbox pattern
							role="listbox"
							aria-label="Search suggestions"
							className="absolute z-20 mt-2 w-full overflow-hidden rounded-2xl border border-line bg-surface-raised py-1.5 shadow-[0_12px_40px_rgba(20,32,24,0.14)] animate-fade"
						>
							{suggestions.map((suggestion, index) => (
								// biome-ignore lint/a11y/useFocusableInteractive: keyboard nav stays on the input
								<li
									key={suggestion.text}
									id={`${listboxId}-opt-${index}`}
									// biome-ignore lint/a11y/noNoninteractiveElementToInteractiveRole: option under listbox
									role="option"
									aria-selected={index === activeSuggestion}
								>
									<button
										type="button"
										tabIndex={-1}
										className={`flex w-full items-center gap-3 px-4 py-2.5 text-left text-ink transition-colors hover:bg-accent-soft ${index === activeSuggestion ? "bg-accent-soft" : ""}`}
										onMouseDown={(event) => event.preventDefault()}
										onMouseEnter={() => setActiveSuggestion(index)}
										onClick={() => submit(suggestion.text)}
									>
										{suggestion.image ? (
											<img
												src={suggestion.image}
												alt=""
												width={32}
												height={32}
												className="size-8 shrink-0 rounded-md object-cover bg-surface"
												loading="lazy"
												decoding="async"
											/>
										) : (
											<span className="flex size-8 shrink-0 items-center justify-center rounded-md bg-surface text-ink-subtle">
												<Search className="size-4" aria-hidden />
											</span>
										)}
										<span className="min-w-0 flex-1">
											<span className="block truncate text-[0.95rem]">
												{suggestion.text}
											</span>
											{suggestion.subtext ? (
												<span className="mt-0.5 block truncate text-[0.75rem] text-ink-muted">
													{suggestion.subtext}
												</span>
											) : null}
										</span>
									</button>
								</li>
							))}
						</ul>
					) : null}

					{showRecents ? (
						<ul className="absolute z-20 mt-2 w-full overflow-hidden rounded-2xl border border-line bg-surface-raised py-1.5 shadow-[0_12px_40px_rgba(20,32,24,0.14)] animate-fade">
							<li className="flex items-center justify-between px-4 pt-1 pb-1.5">
								<span className="text-[0.7rem] font-semibold tracking-wide text-ink-subtle uppercase">
									Recent
								</span>
								<button
									type="button"
									className="text-[0.7rem] text-ink-subtle hover:text-accent"
									onMouseDown={(e) => e.preventDefault()}
									onClick={() => {
										clearRecentSearches();
										setRecents([]);
									}}
								>
									Clear
								</button>
							</li>
							{recents.map((recent) => (
								<li key={recent}>
									<button
										type="button"
										className="flex w-full items-center gap-3 px-4 py-2.5 text-left text-ink transition-colors hover:bg-accent-soft"
										onMouseDown={(event) => event.preventDefault()}
										onClick={() => submit(recent)}
									>
										<span className="truncate text-[0.95rem]">{recent}</span>
									</button>
								</li>
							))}
						</ul>
					) : null}
				</div>
			</form>
			<div className="mt-2 flex flex-wrap gap-1.5">
				{OPERATORS.map((op) => (
					<button
						key={op.label}
						type="button"
						title={op.tip}
						onClick={() => insertOperator(op.insert)}
						className="rounded-lg border border-line bg-surface-raised px-2 py-0.5 font-mono text-[0.75rem] text-ink-muted transition-colors hover:border-accent hover:text-accent"
					>
						{op.label}
					</button>
				))}
			</div>
		</search>
	);
}

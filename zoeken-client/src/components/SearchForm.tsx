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
import { autocomplete } from "#/lib/api";
import type { SearchRouteParams } from "#/lib/searchParams";
import { useConfig } from "#/routes/__root";

/** Session-scoped suggestion cache (query → suggestions), capped. */
const suggestionCache = new Map<string, string[]>();

function rememberSuggestions(query: string, suggestions: string[]) {
	if (suggestionCache.size >= 500) {
		suggestionCache.clear();
	}
	suggestionCache.set(query, suggestions);
}

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
	const [q, setQ] = useState(initialQuery);
	const [suggestions, setSuggestions] = useState<string[]>([]);
	const [activeSuggestion, setActiveSuggestion] = useState(-1);
	const [focused, setFocused] = useState(false);
	const inputRef = useRef<HTMLInputElement>(null);
	const requestSeq = useRef(0);
	const minChars = config?.autocomplete_min ?? 1;
	const autocompleteEnabled = Boolean(config?.autocomplete);

	useEffect(() => {
		const trimmed = q.trim();
		if (!autocompleteEnabled || !focused || trimmed.length < minChars) {
			setSuggestions([]);
			return;
		}
		// Session cache: backspacing/retyping a prefix renders instantly.
		const cached = suggestionCache.get(trimmed);
		if (cached) {
			setSuggestions(cached);
			return;
		}
		const seq = ++requestSeq.current;
		const timeout = window.setTimeout(() => {
			void autocomplete(trimmed)
				.then(([, items]) => {
					const suggestions = items ?? [];
					rememberSuggestions(trimmed, suggestions);
					// Drop stale responses: a slower earlier request must not
					// overwrite the newest one.
					if (
						seq === requestSeq.current &&
						document.activeElement === inputRef.current
					) {
						setSuggestions(suggestions);
					}
				})
				.catch(() => {
					if (seq === requestSeq.current) setSuggestions([]);
				});
		}, 100);
		return () => window.clearTimeout(timeout);
	}, [autocompleteEnabled, focused, minChars, q]);

	function submit(query = q) {
		query = query.trim();
		if (!query) return;
		setSuggestions([]);
		void navigate({
			to: "/search",
			search: { ...baseSearch, q: query, pageno: undefined },
		});
	}

	function onSubmit(e: FormEvent) {
		e.preventDefault();
		submit(activeSuggestion >= 0 ? suggestions[activeSuggestion] : q);
	}

	function onKeyDown(event: KeyboardEvent<HTMLInputElement>) {
		if (!suggestions.length) return;
		if (event.key === "ArrowDown" || event.key === "ArrowUp") {
			event.preventDefault();
			setActiveSuggestion((index) => {
				const next = event.key === "ArrowDown" ? index + 1 : index - 1;
				return (next + suggestions.length) % suggestions.length;
			});
		}
		if (event.key === "Escape") setSuggestions([]);
	}

	// Bang autocomplete: when the query ends with `!<partial>`, offer matching
	// engine shortcuts from the instance config instead of query suggestions.
	const bangMatch = /(^|\s)!([a-z0-9_.-]*)$/i.exec(q);
	const bangPrefix = bangMatch?.[2]?.toLowerCase() ?? null;
	const bangs =
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
		// Replace the trailing `!<partial>` with the chosen `!<shortcut> `.
		const next = q.replace(/(^|\s)!([a-z0-9_.-]*)$/i, `$1!${shortcut} `);
		setQ(next);
		setSuggestions([]);
		inputRef.current?.focus();
	}

	const showBangs = focused && bangs.length > 0;
	const showSuggestions = focused && !showBangs && suggestions.length > 0;

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
								? "h-11 rounded-xl px-5 text-[0.95rem]"
								: "h-14 rounded-2xl px-6 text-lg shadow-[0_1px_3px_rgba(20,32,24,0.06),0_8px_24px_rgba(20,32,24,0.06)]",
						].join(" ")}
					/>
					{showBangs ? (
						<ul className="absolute z-20 mt-2 w-full overflow-hidden rounded-2xl border border-line bg-surface-raised py-1.5 shadow-[0_12px_40px_rgba(20,32,24,0.14)] animate-fade">
							<li className="px-4 pt-1 pb-1.5 text-[0.7rem] font-semibold tracking-wide text-ink-subtle uppercase">
								Search a specific engine
							</li>
							{bangs.map((engine) => (
								<li key={engine.name}>
									<button
										type="button"
										className="flex w-full items-center gap-3 px-4 py-2 text-left transition-colors hover:bg-accent-soft"
										onMouseDown={(event) => event.preventDefault()}
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
						<ul className="absolute z-20 mt-2 w-full overflow-hidden rounded-2xl border border-line bg-surface-raised py-1.5 shadow-[0_12px_40px_rgba(20,32,24,0.14)] animate-fade">
							{suggestions.map((suggestion, index) => (
								<li key={suggestion}>
									<button
										type="button"
										className={`flex w-full items-center gap-3 px-4 py-2.5 text-left text-ink transition-colors hover:bg-accent-soft ${index === activeSuggestion ? "bg-accent-soft" : ""}`}
										onMouseDown={(event) => event.preventDefault()}
										onClick={() => submit(suggestion)}
									>
										<Search
											className="size-4 shrink-0 text-ink-subtle"
											aria-hidden
										/>
										<span className="truncate text-[0.95rem]">
											{suggestion}
										</span>
									</button>
								</li>
							))}
						</ul>
					) : null}
				</div>
			</form>
		</search>
	);
}

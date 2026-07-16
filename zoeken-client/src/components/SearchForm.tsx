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
	const minChars = config?.autocomplete_min ?? 1;
	const autocompleteEnabled = Boolean(config?.autocomplete);

	useEffect(() => {
		const trimmed = q.trim();
		if (!autocompleteEnabled || !focused || trimmed.length < minChars) {
			setSuggestions([]);
			return;
		}
		const timeout = window.setTimeout(() => {
			void autocomplete(trimmed)
				.then(([, items]) => {
					if (document.activeElement === inputRef.current) {
						setSuggestions(items ?? []);
					}
				})
				.catch(() => setSuggestions([]));
		}, 180);
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

	const showSuggestions = focused && suggestions.length > 0;

	return (
		<search className={compact ? "w-full min-w-0 flex-1" : "w-full"}>
			<form onSubmit={onSubmit} className="w-full">
				<label className="sr-only" htmlFor={inputId}>
					Search
				</label>
				<div className="relative">
					<input
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
							"transition-[border-color,box-shadow] duration-200",
							"focus:border-accent focus:shadow-[0_0_0_3px_var(--accent-soft)]",
							compact
								? "h-11 rounded-xl px-5 text-[0.95rem]"
								: "h-14 rounded-2xl px-6 text-lg shadow-[0_1px_3px_rgba(20,32,24,0.06),0_8px_24px_rgba(20,32,24,0.06)]",
						].join(" ")}
					/>
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

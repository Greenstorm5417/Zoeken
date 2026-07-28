import { BookOpen } from "lucide-react";
import { useEffect, useState } from "react";
import { AnswerShell } from "#/components/answers/AnswerShell";
import type { InteractiveAnswer, SearchAnswer } from "#/lib/api";
import { search } from "#/lib/api";

function pickDictionary(
	answers: SearchAnswer[],
): Extract<InteractiveAnswer, { type: "dictionary" }> | null {
	for (const a of answers) {
		if (a.interactive?.type === "dictionary") return a.interactive;
	}
	return null;
}

export function DictionaryAnswer({
	answer,
	initial,
}: {
	answer: SearchAnswer;
	initial: Extract<InteractiveAnswer, { type: "dictionary" }>;
}) {
	const [term, setTerm] = useState(initial.term);
	const [definitions, setDefinitions] = useState(initial.definitions);
	const [sourceUrl, setSourceUrl] = useState(answer.url);
	const [busy, setBusy] = useState(false);

	useEffect(() => {
		setTerm(initial.term);
		setDefinitions(initial.definitions);
		setSourceUrl(answer.url);
	}, [initial, answer.url]);

	async function lookup(nextTerm: string) {
		const q = nextTerm.trim();
		if (!q || busy) return;
		setBusy(true);
		try {
			const data = await search({
				q: `define ${q}`,
				engines: "dictionary",
			});
			const interactive = pickDictionary(data.answers);
			const matched = data.answers.find(
				(a) => a.interactive?.type === "dictionary",
			);
			if (!interactive) return;
			setTerm(interactive.term);
			setDefinitions(interactive.definitions);
			if (matched?.url) setSourceUrl(matched.url);
		} finally {
			setBusy(false);
		}
	}

	return (
		<AnswerShell
			title="Dictionary"
			icon={BookOpen}
			footer={
				sourceUrl ? (
					<a
						href={sourceUrl}
						target="_blank"
						rel="noopener noreferrer"
						className="inline-block text-sm text-accent hover:underline"
					>
						Wiktionary
					</a>
				) : null
			}
		>
			<div className="flex flex-col gap-2 sm:flex-row sm:gap-2">
				<input
					type="text"
					value={term}
					onChange={(e) => setTerm(e.target.value)}
					onKeyDown={(e) => {
						if (e.key === "Enter") {
							e.preventDefault();
							void lookup(term);
						}
					}}
					disabled={busy}
					className="min-h-11 min-w-0 flex-1 rounded-[0.625rem] border border-line bg-surface px-3 text-[1.2rem] font-semibold text-ink outline-none focus:border-accent focus:shadow-[0_0_0_3px_var(--accent-soft)] disabled:opacity-60"
					aria-label="Word to define"
				/>
				<button
					type="button"
					disabled={busy || !term.trim()}
					onClick={() => void lookup(term)}
					className="min-h-11 shrink-0 rounded-xl border border-line bg-surface px-4 text-sm font-medium text-accent transition-colors hover:bg-accent-soft disabled:opacity-50"
				>
					{busy ? "…" : "Define"}
				</button>
			</div>

			<ol className="mt-4 list-decimal space-y-2.5 pl-5 text-base leading-relaxed text-ink">
				{definitions.map((def) => (
					<li key={def}>{def}</li>
				))}
			</ol>
		</AnswerShell>
	);
}

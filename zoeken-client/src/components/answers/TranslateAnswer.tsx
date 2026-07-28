import { Languages } from "lucide-react";
import { useEffect, useId, useMemo, useState } from "react";
import { AnswerShell } from "#/components/answers/AnswerShell";
import { SelectMenu } from "#/components/SelectMenu";
import type { InteractiveAnswer, SearchAnswer } from "#/lib/api";
import { search } from "#/lib/searchApi";

const LANGS: Array<{ code: string; label: string }> = [
	{ code: "en", label: "English" },
	{ code: "es", label: "Spanish" },
	{ code: "fr", label: "French" },
	{ code: "de", label: "German" },
	{ code: "nl", label: "Dutch" },
	{ code: "it", label: "Italian" },
	{ code: "pt", label: "Portuguese" },
	{ code: "ru", label: "Russian" },
	{ code: "ja", label: "Japanese" },
	{ code: "zh", label: "Chinese" },
	{ code: "ko", label: "Korean" },
	{ code: "ar", label: "Arabic" },
	{ code: "hi", label: "Hindi" },
	{ code: "tr", label: "Turkish" },
	{ code: "pl", label: "Polish" },
	{ code: "sv", label: "Swedish" },
];

function pickTranslate(
	answers: SearchAnswer[],
): Extract<InteractiveAnswer, { type: "translate" }> | null {
	for (const a of answers) {
		if (a.interactive?.type === "translate") return a.interactive;
	}
	return null;
}

export function TranslateAnswer({
	answer,
	initial,
}: {
	answer: SearchAnswer;
	initial: Extract<InteractiveAnswer, { type: "translate" }>;
}) {
	const sourceId = useId();
	const [source, setSource] = useState(initial.source);
	const [target, setTarget] = useState(initial.target_lang);
	const [translated, setTranslated] = useState(initial.translated);
	const [sourceUrl, setSourceUrl] = useState(answer.url);
	const [busy, setBusy] = useState(false);

	useEffect(() => {
		setSource(initial.source);
		setTarget(initial.target_lang);
		setTranslated(initial.translated);
		setSourceUrl(answer.url);
	}, [initial, answer.url]);

	const options = useMemo(() => {
		const codes = new Set(LANGS.map((l) => l.code));
		codes.add(target);
		return [...codes].map((code) => {
			const known = LANGS.find((l) => l.code === code);
			return { value: code, label: known?.label ?? code };
		});
	}, [target]);

	async function refresh(nextSource: string, nextTarget: string) {
		const text = nextSource.trim();
		if (!text || busy) return;
		setBusy(true);
		try {
			const data = await search({
				q: `translate ${text} to ${nextTarget}`,
				engines: "translate",
			});
			const interactive = pickTranslate(data.answers);
			const matched = data.answers.find(
				(a) => a.interactive?.type === "translate",
			);
			if (!interactive) return;
			setSource(interactive.source);
			setTarget(interactive.target_lang);
			setTranslated(interactive.translated);
			if (matched?.url) setSourceUrl(matched.url);
		} finally {
			setBusy(false);
		}
	}

	return (
		<AnswerShell
			title="Translate"
			icon={Languages}
			footer={
				sourceUrl ? (
					<a
						href={sourceUrl}
						target="_blank"
						rel="noopener noreferrer"
						className="inline-block text-sm text-accent hover:underline"
					>
						MyMemory
					</a>
				) : null
			}
		>
			<label
				htmlFor={sourceId}
				className="mb-1 block text-xs font-medium text-ink-muted"
			>
				Text
			</label>
			<textarea
				id={sourceId}
				value={source}
				onChange={(e) => setSource(e.target.value)}
				onBlur={() => {
					if (source.trim() && source !== initial.source) {
						void refresh(source, target);
					}
				}}
				disabled={busy}
				rows={2}
				className="w-full resize-y rounded-[0.625rem] border border-line bg-surface px-3 py-2.5 text-base text-ink outline-none focus:border-accent focus:shadow-[0_0_0_3px_var(--accent-soft)] disabled:opacity-60"
			/>

			<div className="mt-3 flex flex-wrap items-end gap-3">
				<div className="min-w-[10rem] flex-1 sm:flex-none sm:w-[10rem]">
					<SelectMenu
						label="To"
						value={target}
						options={options}
						onChange={(next) => {
							setTarget(next);
							void refresh(source, next);
						}}
						fullWidth
					/>
				</div>
				<button
					type="button"
					disabled={busy || !source.trim()}
					onClick={() => void refresh(source, target)}
					className="min-h-11 rounded-xl border border-line bg-surface px-4 text-sm font-medium text-accent transition-colors hover:bg-accent-soft disabled:opacity-50"
				>
					{busy ? "Translating…" : "Translate"}
				</button>
			</div>

			<p className="mt-4 text-[1.35rem] leading-snug tracking-tight break-words text-ink">
				{translated}
			</p>
		</AnswerShell>
	);
}

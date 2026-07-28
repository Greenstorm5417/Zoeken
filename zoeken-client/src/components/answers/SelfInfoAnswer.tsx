import { Check, Copy, Fingerprint } from "lucide-react";
import { useState } from "react";
import { AnswerShell } from "#/components/answers/AnswerShell";
import type { InteractiveAnswer, SearchAnswer } from "#/lib/api";

export function SelfInfoAnswer({
	initial,
}: {
	answer: SearchAnswer;
	initial: Extract<InteractiveAnswer, { type: "self_info" }>;
}) {
	const [copied, setCopied] = useState(false);
	const label = initial.kind === "user_agent" ? "User agent" : "Your IP";
	const value = initial.value?.trim() || "Unavailable";
	const canCopy = Boolean(initial.value?.trim());

	async function copy() {
		if (!canCopy) return;
		try {
			await navigator.clipboard.writeText(initial.value);
			setCopied(true);
			window.setTimeout(() => setCopied(false), 1500);
		} catch {
			/* ponytail: ignore clipboard failures */
		}
	}

	return (
		<AnswerShell title={label} icon={Fingerprint}>
			<div className="flex flex-col gap-3 sm:flex-row sm:items-start">
				<p className="min-w-0 flex-1 break-all font-mono text-[1.05rem] leading-snug text-ink sm:text-[1.1rem]">
					{value}
				</p>
				{canCopy ? (
					<button
						type="button"
						onClick={() => void copy()}
						className="inline-flex min-h-11 shrink-0 items-center justify-center gap-1.5 self-start rounded-xl border border-line bg-surface px-3 text-sm font-medium text-ink-muted hover:bg-accent-soft hover:text-accent"
						aria-label="Copy value"
					>
						{copied ? (
							<Check className="size-3.5" aria-hidden />
						) : (
							<Copy className="size-3.5" aria-hidden />
						)}
						{copied ? "Copied" : "Copy"}
					</button>
				) : null}
			</div>
		</AnswerShell>
	);
}

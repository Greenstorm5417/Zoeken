import { Check, Copy, Fingerprint } from "lucide-react";
import { useState } from "react";
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
		<section className="mb-6 max-w-[40rem] rounded-2xl border border-line bg-surface-raised px-5 py-4">
			<p className="mb-2 flex items-center gap-2 text-[0.7rem] font-semibold tracking-wide text-ink-subtle uppercase">
				<Fingerprint className="size-4 text-accent" aria-hidden />
				{label}
			</p>
			<div className="flex items-start gap-3">
				<p className="min-w-0 flex-1 break-all font-mono text-[1.1rem] leading-snug text-ink">
					{value}
				</p>
				{canCopy ? (
					<button
						type="button"
						onClick={() => void copy()}
						className="inline-flex shrink-0 items-center gap-1.5 rounded-xl border border-line bg-surface px-2.5 py-1.5 text-xs font-medium text-ink-muted hover:bg-accent-soft hover:text-accent"
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
		</section>
	);
}

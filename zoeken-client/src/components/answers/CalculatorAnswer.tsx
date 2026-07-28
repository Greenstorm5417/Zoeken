import { Sigma } from "lucide-react";
import type { KeyboardEvent } from "react";
import { useEffect, useId, useState } from "react";
import { AnswerShell } from "#/components/answers/AnswerShell";
import type { InteractiveAnswer, SearchAnswer } from "#/lib/api";
import { calcEval, formatCalcNumber } from "#/lib/calcEval";

type KeySpec = {
	label: string;
	insert?: string;
	action?: "clear" | "backspace" | "equals" | "negate";
	span?: 2 | 3;
	tone?: "op" | "accent" | "muted";
};

/** Landscape-friendly 5-column keypad — ops the evaluator already supports. */
const KEYS: KeySpec[] = [
	{ label: "C", action: "clear", tone: "muted" },
	{ label: "(", insert: "(", tone: "muted" },
	{ label: ")", insert: ")", tone: "muted" },
	{ label: "⌫", action: "backspace", tone: "muted" },
	{ label: "÷", insert: "÷", tone: "op" },
	{ label: "7", insert: "7" },
	{ label: "8", insert: "8" },
	{ label: "9", insert: "9" },
	{ label: "%", insert: "%", tone: "op" },
	{ label: "×", insert: "×", tone: "op" },
	{ label: "4", insert: "4" },
	{ label: "5", insert: "5" },
	{ label: "6", insert: "6" },
	{ label: "^", insert: "^", tone: "op" },
	{ label: "−", insert: "−", tone: "op" },
	{ label: "1", insert: "1" },
	{ label: "2", insert: "2" },
	{ label: "3", insert: "3" },
	{ label: "±", action: "negate", tone: "op" },
	{ label: "+", insert: "+", tone: "op" },
	{ label: "0", insert: "0", span: 2 },
	{ label: ".", insert: "." },
	{ label: "=", action: "equals", tone: "accent", span: 2 },
];

function looksIncomplete(expression: string): boolean {
	const t = expression.trim();
	if (!t) return true;
	return /[+\-−×÷*/^%(]$/.test(t) || t.endsWith(".") || /\($/.test(t);
}

export function CalculatorAnswer({
	answer,
	initial,
}: {
	answer: SearchAnswer;
	initial: Extract<InteractiveAnswer, { type: "calculator" }>;
}) {
	const inputId = useId();
	const seedUnsupported = !calcEval(initial.expression).ok;
	const [expression, setExpression] = useState(initial.expression);
	const [justEquals, setJustEquals] = useState(false);
	const [advancedFallback, setAdvancedFallback] = useState(seedUnsupported);

	useEffect(() => {
		setExpression(initial.expression);
		setJustEquals(false);
		setAdvancedFallback(!calcEval(initial.expression).ok);
	}, [initial.expression]);

	const live = calcEval(expression);
	let resultText = "";
	if (advancedFallback && expression.trim() === initial.expression.trim()) {
		resultText = answer.answer || formatCalcNumber(initial.result);
	} else if (live.ok) {
		resultText = formatCalcNumber(live.value);
	} else if (
		!looksIncomplete(expression) &&
		expression.trim() === initial.expression.trim()
	) {
		resultText = answer.answer || formatCalcNumber(initial.result);
	}

	function insert(text: string) {
		if (advancedFallback) return;
		setExpression((prev) => (justEquals ? text : prev + text));
		setJustEquals(false);
	}

	function clear() {
		setExpression("");
		setJustEquals(false);
		setAdvancedFallback(false);
	}

	function backspace() {
		if (advancedFallback) return;
		setExpression((prev) => (justEquals ? "" : prev.slice(0, -1)));
		setJustEquals(false);
	}

	function negate() {
		if (advancedFallback) return;
		setExpression((prev) => {
			const base = justEquals ? prev : prev;
			if (!base.trim()) return "-";
			if (base.startsWith("-")) return base.slice(1);
			return `-${base}`;
		});
		setJustEquals(false);
	}

	function equals() {
		if (advancedFallback) return;
		const result = calcEval(expression);
		if (result.ok) {
			setExpression(formatCalcNumber(result.value));
			setJustEquals(true);
		}
	}

	function onKeyDown(e: KeyboardEvent<HTMLInputElement>) {
		if (advancedFallback && e.key !== "Escape") return;
		if (e.key === "Enter") {
			e.preventDefault();
			equals();
		} else if (e.key === "Escape") {
			e.preventDefault();
			clear();
		}
	}

	return (
		<AnswerShell title="Calculator" icon={Sigma} wide>
			<div className="flex flex-col gap-3 sm:flex-row sm:items-stretch sm:gap-4">
				<div className="flex min-w-0 flex-1 flex-col rounded-xl border border-line bg-surface px-3 py-3 sm:min-h-[14rem]">
					<label htmlFor={inputId} className="sr-only">
						Expression
					</label>
					<input
						id={inputId}
						type="text"
						inputMode="decimal"
						autoComplete="off"
						spellCheck={false}
						value={expression}
						readOnly={advancedFallback}
						onChange={(e) => {
							if (advancedFallback) return;
							setJustEquals(false);
							setExpression(e.target.value);
						}}
						onKeyDown={onKeyDown}
						className="w-full bg-transparent font-mono text-sm text-ink-subtle outline-none"
						aria-describedby={`${inputId}-result`}
					/>
					<p
						id={`${inputId}-result`}
						className="mt-auto pt-3 text-right text-[1.75rem] font-semibold tabular-nums tracking-tight text-ink sm:text-[2rem]"
					>
						{resultText || "\u00a0"}
					</p>
					{advancedFallback ? (
						<p className="mt-2 text-xs text-ink-subtle">
							Showing original answer — press C to use the keypad.
						</p>
					) : null}
				</div>

				<div className="grid w-full grid-cols-5 gap-1.5 sm:max-w-[22rem] sm:shrink-0 sm:gap-2">
					{KEYS.map((key) => {
						const disabled = advancedFallback && key.action !== "clear";
						const tone =
							key.tone === "accent"
								? "bg-accent text-white hover:opacity-90"
								: key.tone === "op"
									? "bg-accent-soft text-accent hover:opacity-90"
									: key.tone === "muted"
										? "bg-surface text-ink-subtle hover:bg-accent-soft"
										: "bg-surface text-ink hover:bg-accent-soft";
						return (
							<button
								key={key.label}
								type="button"
								disabled={disabled}
								aria-label={
									key.action === "clear"
										? "Clear"
										: key.action === "backspace"
											? "Backspace"
											: key.action === "equals"
												? "Equals"
												: key.action === "negate"
													? "Negate"
													: undefined
								}
								onClick={() => {
									if (key.action === "clear") clear();
									else if (key.action === "backspace") backspace();
									else if (key.action === "equals") equals();
									else if (key.action === "negate") negate();
									else if (key.insert) insert(key.insert);
								}}
								className={`min-h-11 rounded-xl border border-line px-0 text-base font-medium transition disabled:cursor-not-allowed disabled:opacity-40 sm:min-h-12 ${tone} ${
									key.span === 2
										? "col-span-2"
										: key.span === 3
											? "col-span-3"
											: ""
								}`}
							>
								{key.label}
							</button>
						);
					})}
				</div>
			</div>
		</AnswerShell>
	);
}

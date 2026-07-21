import { ArrowLeftRight } from "lucide-react";
import { useEffect, useId, useMemo, useState } from "react";
import { SelectMenu } from "#/components/SelectMenu";
import { type InteractiveAnswer, type SearchAnswer, search } from "#/lib/api";

/** Common ECB / major codes — merged with whatever came from the answer. */
const COMMON_CODES = [
	"USD",
	"EUR",
	"GBP",
	"JPY",
	"CHF",
	"CAD",
	"AUD",
	"NZD",
	"CNY",
	"INR",
	"KRW",
	"SEK",
	"NOK",
	"DKK",
	"PLN",
	"CZK",
	"HUF",
	"RON",
	"BGN",
	"TRY",
	"ILS",
	"MXN",
	"BRL",
	"ZAR",
	"SGD",
	"HKD",
	"THB",
	"PHP",
	"MYR",
	"IDR",
	"ISK",
];

function formatAmount(value: number): string {
	if (!Number.isFinite(value)) return "";
	const digits = Math.abs(value) >= 100 ? 2 : 4;
	return value.toFixed(digits).replace(/\.?0+$/, "");
}

function parseAmount(raw: string): number | null {
	const n = Number.parseFloat(raw.replace(/,/g, ""));
	return Number.isFinite(n) && n > 0 ? n : null;
}

function pickCurrencyAnswer(
	answers: SearchAnswer[],
): Extract<InteractiveAnswer, { type: "currency" }> | null {
	for (const a of answers) {
		if (a.interactive?.type === "currency") return a.interactive;
	}
	return null;
}

export function CurrencyAnswer({
	answer,
	initial,
}: {
	answer: SearchAnswer;
	initial: Extract<InteractiveAnswer, { type: "currency" }>;
}) {
	const amountId = useId();
	const resultId = useId();
	const [amountStr, setAmountStr] = useState(() =>
		formatAmount(initial.amount),
	);
	const [resultStr, setResultStr] = useState(() =>
		formatAmount(initial.result),
	);
	const [from, setFrom] = useState(initial.from);
	const [to, setTo] = useState(initial.to);
	const [rate, setRate] = useState(initial.rate);
	const [sourceUrl, setSourceUrl] = useState(answer.url);
	const [busy, setBusy] = useState(false);

	useEffect(() => {
		setAmountStr(formatAmount(initial.amount));
		setResultStr(formatAmount(initial.result));
		setFrom(initial.from);
		setTo(initial.to);
		setRate(initial.rate);
		setSourceUrl(answer.url);
	}, [initial, answer.url]);

	const options = useMemo(() => {
		const codes = new Set([
			...COMMON_CODES,
			initial.from,
			initial.to,
			from,
			to,
		]);
		return [...codes].sort().map((code) => ({ value: code, label: code }));
	}, [initial.from, initial.to, from, to]);

	function applyRate(nextAmount: number, nextRate: number) {
		setRate(nextRate);
		setAmountStr(formatAmount(nextAmount));
		setResultStr(formatAmount(nextAmount * nextRate));
	}

	async function refreshPair(nextFrom: string, nextTo: string, amount: number) {
		if (nextFrom === nextTo || busy) return;
		setBusy(true);
		try {
			const data = await search({
				q: `${amount} ${nextFrom} to ${nextTo}`,
				engines: "currency",
			});
			const interactive = pickCurrencyAnswer(data.answers);
			const matched = data.answers.find(
				(a) => a.interactive?.type === "currency",
			);
			if (!interactive) return;
			setFrom(interactive.from);
			setTo(interactive.to);
			applyRate(interactive.amount, interactive.rate);
			if (matched?.url) setSourceUrl(matched.url);
		} finally {
			setBusy(false);
		}
	}

	function onFromAmountChange(raw: string) {
		setAmountStr(raw);
		const n = parseAmount(raw);
		if (n == null) return;
		setResultStr(formatAmount(n * rate));
	}

	function onToAmountChange(raw: string) {
		setResultStr(raw);
		const n = parseAmount(raw);
		if (n == null || rate === 0) return;
		setAmountStr(formatAmount(n / rate));
	}

	function onFromCurrency(next: string) {
		if (next === from) return;
		const amount = parseAmount(amountStr) ?? 1;
		setFrom(next);
		void refreshPair(next, to, amount);
	}

	function onToCurrency(next: string) {
		if (next === to) return;
		const amount = parseAmount(amountStr) ?? 1;
		setTo(next);
		void refreshPair(from, next, amount);
	}

	function swap() {
		const nextFrom = to;
		const nextTo = from;
		const nextAmount = parseAmount(resultStr) ?? initial.amount;
		const nextRate = rate === 0 ? 0 : 1 / rate;
		setFrom(nextFrom);
		setTo(nextTo);
		applyRate(nextAmount, nextRate);
	}

	return (
		<section className="mb-6 max-w-[40rem] rounded-2xl border border-line bg-surface-raised px-5 py-4">
			<p className="mb-3 flex items-center gap-2 text-[0.7rem] font-semibold tracking-wide text-ink-subtle uppercase">
				<ArrowLeftRight className="size-4 text-accent" aria-hidden />
				Currency
			</p>

			<div className="flex flex-col gap-3 sm:flex-row sm:items-end">
				<div className="min-w-0 flex-1">
					<label
						htmlFor={amountId}
						className="mb-1 block text-xs text-ink-muted"
					>
						Amount
					</label>
					<div className="flex gap-2">
						<input
							id={amountId}
							type="text"
							inputMode="decimal"
							value={amountStr}
							onChange={(e) => onFromAmountChange(e.target.value)}
							disabled={busy}
							className="min-w-0 flex-1 rounded-[0.625rem] border border-line bg-surface px-3 py-2 text-[1.1rem] font-semibold text-ink outline-none focus:border-accent focus:shadow-[0_0_0_3px_var(--accent-soft)]"
							aria-label="From amount"
						/>
						<div className="w-[5.5rem] shrink-0">
							<SelectMenu
								label="From currency"
								value={from}
								options={options}
								onChange={onFromCurrency}
								fullWidth
							/>
						</div>
					</div>
				</div>

				<button
					type="button"
					onClick={swap}
					disabled={busy}
					className="mx-auto flex size-9 shrink-0 items-center justify-center rounded-full border border-line text-ink-muted transition-colors hover:border-accent hover:text-accent sm:mb-1"
					aria-label="Swap currencies"
				>
					<ArrowLeftRight className="size-4" />
				</button>

				<div className="min-w-0 flex-1">
					<label
						htmlFor={resultId}
						className="mb-1 block text-xs text-ink-muted"
					>
						Converted
					</label>
					<div className="flex gap-2">
						<input
							id={resultId}
							type="text"
							inputMode="decimal"
							value={resultStr}
							onChange={(e) => onToAmountChange(e.target.value)}
							disabled={busy}
							className="min-w-0 flex-1 rounded-[0.625rem] border border-line bg-surface px-3 py-2 text-[1.1rem] font-semibold text-ink outline-none focus:border-accent focus:shadow-[0_0_0_3px_var(--accent-soft)]"
							aria-label="To amount"
						/>
						<div className="w-[5.5rem] shrink-0">
							<SelectMenu
								label="To currency"
								value={to}
								options={options}
								onChange={onToCurrency}
								fullWidth
							/>
						</div>
					</div>
				</div>
			</div>

			<p className="mt-3 text-xs text-ink-muted">
				{busy ? "Updating rate…" : `1 ${from} = ${formatAmount(rate)} ${to}`}
			</p>

			{sourceUrl ? (
				<a
					href={sourceUrl}
					target="_blank"
					rel="noopener noreferrer"
					className="mt-1 inline-block text-sm text-accent hover:underline"
				>
					European Central Bank
				</a>
			) : null}
		</section>
	);
}

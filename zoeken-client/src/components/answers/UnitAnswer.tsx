import { ArrowLeftRight } from "lucide-react";
import { useId, useState } from "react";
import { AnswerShell } from "#/components/answers/AnswerShell";
import { SelectMenu } from "#/components/SelectMenu";
import type { InteractiveAnswer, SearchAnswer } from "#/lib/api";
import {
	convertUnits,
	formatUnitNumber,
	unitById,
	unitsForDimension,
} from "#/lib/units";

function parseAmount(raw: string): number | null {
	const cleaned = raw.replace(/,/g, "").trim();
	if (
		cleaned === "" ||
		cleaned === "-" ||
		cleaned === "." ||
		cleaned === "-."
	) {
		return null;
	}
	const n = Number(cleaned);
	return Number.isFinite(n) ? n : null;
}

const fieldClass =
	"min-h-11 w-full rounded-[0.625rem] border border-line bg-surface px-3 text-[1.25rem] font-semibold tabular-nums text-ink outline-none focus:border-accent focus:shadow-[0_0_0_3px_var(--accent-soft)]";

/** Interactive unit converter — converts locally from units.json. */
export function UnitAnswer({
	initial,
}: {
	answer: SearchAnswer; // kept for InstantAnswerCard API parity
	initial: Extract<InteractiveAnswer, { type: "unit" }>;
}) {
	const fromAmountId = useId();
	const toAmountId = useId();
	const dimension =
		initial.dimension || unitById(initial.from)?.dimension || "length";
	const options = unitsForDimension(dimension).map((u) => ({
		value: u.id,
		label: `${u.id} — ${u.name}`,
	}));

	const [fromId, setFromId] = useState(initial.from);
	const [toId, setToId] = useState(initial.to);
	const [fromText, setFromText] = useState(formatUnitNumber(initial.amount));
	const [toText, setToText] = useState(formatUnitNumber(initial.result));
	const [lastEdited, setLastEdited] = useState<"from" | "to">("from");

	function syncFromAmount(
		nextFromText: string,
		nextFromId = fromId,
		nextToId = toId,
	) {
		setFromText(nextFromText);
		setLastEdited("from");
		const amount = parseAmount(nextFromText);
		if (amount == null) return;
		const result = convertUnits(amount, nextFromId, nextToId);
		if (result != null) setToText(formatUnitNumber(result));
	}

	function syncToAmount(
		nextToText: string,
		nextFromId = fromId,
		nextToId = toId,
	) {
		setToText(nextToText);
		setLastEdited("to");
		const amount = parseAmount(nextToText);
		if (amount == null) return;
		const result = convertUnits(amount, nextToId, nextFromId);
		if (result != null) setFromText(formatUnitNumber(result));
	}

	function onFromUnit(nextFromId: string) {
		setFromId(nextFromId);
		if (lastEdited === "from") {
			syncFromAmount(fromText, nextFromId, toId);
		} else {
			syncToAmount(toText, nextFromId, toId);
		}
	}

	function onToUnit(nextToId: string) {
		setToId(nextToId);
		if (lastEdited === "from") {
			syncFromAmount(fromText, fromId, nextToId);
		} else {
			syncToAmount(toText, fromId, nextToId);
		}
	}

	function swap() {
		const nextFromId = toId;
		const nextToId = fromId;
		const nextFromText = toText;
		const nextToText = fromText;
		setFromId(nextFromId);
		setToId(nextToId);
		setFromText(nextFromText);
		setToText(nextToText);
		setLastEdited(lastEdited === "from" ? "to" : "from");
	}

	return (
		<AnswerShell title="Unit converter" icon={ArrowLeftRight}>
			<div className="flex flex-col gap-3">
				<div className="grid gap-2 sm:grid-cols-[minmax(0,1fr)_minmax(10rem,13rem)] sm:items-end sm:gap-3">
					<div className="min-w-0">
						<label
							htmlFor={fromAmountId}
							className="mb-1 block text-xs font-medium text-ink-muted"
						>
							From
						</label>
						<input
							id={fromAmountId}
							type="text"
							inputMode="decimal"
							aria-label="From amount"
							value={fromText}
							onChange={(e) => syncFromAmount(e.target.value)}
							className={fieldClass}
						/>
					</div>
					<SelectMenu
						label="From unit"
						value={fromId}
						options={options}
						onChange={onFromUnit}
						fullWidth
					/>
				</div>

				<div className="flex justify-center sm:justify-start sm:pl-[calc(50%-1.25rem)]">
					<button
						type="button"
						aria-label="Swap units"
						onClick={swap}
						className="inline-flex size-11 items-center justify-center rounded-full border border-line bg-surface text-ink-muted transition hover:border-accent hover:text-accent"
					>
						<ArrowLeftRight className="size-4" aria-hidden />
					</button>
				</div>

				<div className="grid gap-2 sm:grid-cols-[minmax(0,1fr)_minmax(10rem,13rem)] sm:items-end sm:gap-3">
					<div className="min-w-0">
						<label
							htmlFor={toAmountId}
							className="mb-1 block text-xs font-medium text-ink-muted"
						>
							To
						</label>
						<input
							id={toAmountId}
							type="text"
							inputMode="decimal"
							aria-label="To amount"
							value={toText}
							onChange={(e) => syncToAmount(e.target.value)}
							className={fieldClass}
						/>
					</div>
					<SelectMenu
						label="To unit"
						value={toId}
						options={options}
						onChange={onToUnit}
						fullWidth
					/>
				</div>
			</div>
		</AnswerShell>
	);
}

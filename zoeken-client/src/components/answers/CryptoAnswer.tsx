import { Binary } from "lucide-react";
import { useEffect, useId, useState } from "react";
import { AnswerShell } from "#/components/answers/AnswerShell";
import { SelectMenu } from "#/components/SelectMenu";
import type { InteractiveAnswer, SearchAnswer } from "#/lib/api";
import { runCrypto } from "#/lib/clientCrypto";

const HASH_ALGS = ["md5", "sha1", "sha224", "sha256", "sha384", "sha512"];
const CODEC_ALGS = ["base64", "hex", "url"];
const MODES = [
	{ value: "hash", label: "Hash" },
	{ value: "encode", label: "Encode" },
	{ value: "decode", label: "Decode" },
];

export function CryptoAnswer({
	initial,
}: {
	answer: SearchAnswer;
	initial: Extract<InteractiveAnswer, { type: "crypto" }>;
}) {
	const inputId = useId();
	const [mode, setMode] = useState(initial.mode || "hash");
	const [algorithm, setAlgorithm] = useState(initial.algorithm || "sha256");
	const [input, setInput] = useState(initial.input || "");
	const [result, setResult] = useState("");
	const [error, setError] = useState("");

	const algOptions = (mode === "hash" ? HASH_ALGS : CODEC_ALGS).map((a) => ({
		value: a,
		label: a.toUpperCase(),
	}));

	useEffect(() => {
		setMode(initial.mode || "hash");
		setAlgorithm(initial.algorithm || "sha256");
		setInput(initial.input || "");
	}, [initial.mode, initial.algorithm, initial.input]);

	useEffect(() => {
		const allowed = mode === "hash" ? HASH_ALGS : CODEC_ALGS;
		if (!allowed.includes(algorithm)) {
			setAlgorithm(allowed[0] ?? "md5");
		}
	}, [mode, algorithm]);

	useEffect(() => {
		let cancelled = false;
		void (async () => {
			try {
				const value = await runCrypto(mode, algorithm, input);
				if (!cancelled) {
					setResult(value);
					setError("");
				}
			} catch (e) {
				if (!cancelled) {
					setResult("");
					setError(e instanceof Error ? e.message : "failed");
				}
			}
		})();
		return () => {
			cancelled = true;
		};
	}, [mode, algorithm, input]);

	return (
		<AnswerShell title="Hash / encode" icon={Binary}>
			<div className="mb-3 flex flex-col gap-2 sm:flex-row sm:flex-wrap">
				<div className="min-w-0 flex-1 sm:min-w-[8rem] sm:flex-none">
					<SelectMenu
						label="Mode"
						value={mode}
						options={MODES}
						onChange={setMode}
						fullWidth
					/>
				</div>
				<div className="min-w-0 flex-1 sm:min-w-[8rem] sm:flex-none">
					<SelectMenu
						label="Algorithm"
						value={algorithm}
						options={algOptions}
						onChange={setAlgorithm}
						fullWidth
					/>
				</div>
			</div>

			<label htmlFor={inputId} className="sr-only">
				Input
			</label>
			<textarea
				id={inputId}
				value={input}
				onChange={(e) => setInput(e.target.value)}
				rows={2}
				spellCheck={false}
				className="mb-3 w-full resize-y rounded-xl border border-line bg-surface px-3 py-2.5 font-mono text-sm text-ink outline-none focus:border-accent focus:shadow-[0_0_0_3px_var(--accent-soft)]"
				placeholder="Text to hash or encode"
			/>

			<p className="mb-1.5 text-[0.7rem] font-semibold tracking-wide text-ink-subtle uppercase">
				Result
			</p>
			{error ? (
				<p className="break-all font-mono text-sm text-red-600">{error}</p>
			) : (
				<p className="break-all rounded-xl border border-line/60 bg-surface px-3 py-2.5 font-mono text-sm leading-relaxed text-ink">
					{result || "\u00a0"}
				</p>
			)}
		</AnswerShell>
	);
}

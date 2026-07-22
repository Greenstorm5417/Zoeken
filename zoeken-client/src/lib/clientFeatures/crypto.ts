/**
 * Hash/encode/decode intent detection — SPA client-feature (former answerer).
 * Digests/encodings are computed in the browser by CryptoAnswer.tsx, not here.
 */
import type { InteractiveAnswer, SearchAnswer } from "../api";

const HASH_ALGS = ["md5", "sha1", "sha224", "sha256", "sha384", "sha512"];
const CODEC_ALGS = ["base64", "hex", "url"];

type ParsedCrypto = { mode: string; algorithm: string; input: string };

function normalizeAlg(raw: string): string | null {
	const n = raw
		.toLowerCase()
		.replaceAll("-", "")
		.replaceAll("_", "")
		.replaceAll(" ", "");
	switch (n) {
		case "sha1":
		case "sha224":
		case "sha256":
		case "sha384":
		case "sha512":
		case "md5":
			return n;
		case "base64":
		case "b64":
			return "base64";
		case "hex":
		case "hexadecimal":
			return "hex";
		case "url":
		case "uri":
		case "percent":
			return "url";
		default:
			return null;
	}
}

const isHash = (alg: string) => HASH_ALGS.includes(alg);
const isCodec = (alg: string) => CODEC_ALGS.includes(alg);

/** Collapse "base 64" / "base-64" -> "base64" without lowercasing the rest. */
function collapseBase64Token(text: string): string {
	const trimmed = text.trim().replace(/\?+$/, "").trim();
	const lower = trimmed.toLowerCase();
	for (const needle of ["base 64", "base-64"]) {
		const idx = lower.indexOf(needle);
		if (idx !== -1) {
			return `${trimmed.slice(0, idx)}base64${trimmed.slice(idx + needle.length)}`;
		}
	}
	return trimmed;
}

/** Slice of `original` matching the same byte range as `matched` inside `lowered`. */
function preserveCase(
	original: string,
	lowered: string,
	matched: string,
): string {
	const start = lowered.lastIndexOf(matched);
	if (start !== -1) {
		const end = start + matched.length;
		if (end <= original.length) return original.slice(start, end);
	}
	return matched;
}

function parseCrypto(query: string): ParsedCrypto | null {
	const collapsed = collapseBase64Token(query);
	if (collapsed === "") return null;
	const q = collapsed.toLowerCase();

	// "hash <text> with <alg>" / "hash <text> using <alg>"
	if (q.startsWith("hash ")) {
		const rest = q.slice("hash ".length);
		for (const sep of [" with ", " using "]) {
			const idx = rest.lastIndexOf(sep);
			if (idx === -1) continue;
			const text = rest.slice(0, idx);
			const algRaw = rest.slice(idx + sep.length);
			const alg = normalizeAlg(algRaw.trim());
			if (alg && isHash(alg) && text.trim() !== "") {
				return {
					mode: "hash",
					algorithm: alg,
					input: preserveCase(collapsed, q, text.trim()),
				};
			}
		}
	}

	// "decode <alg> <text>" / "encode <alg> <text>"
	for (const [prefix, mode] of [
		["decode ", "decode"],
		["encode ", "encode"],
	] as const) {
		if (!q.startsWith(prefix)) continue;
		const rest = q.slice(prefix.length);
		const spaceIdx = rest.search(/\s/);
		if (spaceIdx === -1) continue;
		const algTok = rest.slice(0, spaceIdx).trim();
		const input = rest.slice(spaceIdx + 1).trim();
		if (input === "") continue;
		const alg = normalizeAlg(algTok);
		if (alg && isCodec(alg)) {
			return { mode, algorithm: alg, input: preserveCase(collapsed, q, input) };
		}
	}

	// "<alg> encode <text>" / "<alg> decode <text>"
	{
		const parts = q.split(/\s+/);
		const a = parts[0] ?? "";
		const b = parts[1] ?? "";
		const rest = parts.slice(2).join(" ").trim();
		const alg = normalizeAlg(a);
		if (
			rest !== "" &&
			alg &&
			isCodec(alg) &&
			(b === "encode" || b === "decode")
		) {
			return {
				mode: b,
				algorithm: alg,
				input: preserveCase(collapsed, q, rest),
			};
		}
	}

	// "what is <text> in <alg>" / "what's <text> in <alg>"
	for (const prefix of ["what is ", "what's ", "whats "]) {
		if (!q.startsWith(prefix)) continue;
		const rest = q.slice(prefix.length);
		const idx = rest.lastIndexOf(" in ");
		if (idx === -1) continue;
		const text = rest.slice(0, idx).trim();
		const algRaw = rest.slice(idx + 4).trim();
		if (text === "") continue;
		const alg = normalizeAlg(algRaw);
		if (!alg) continue;
		if (isHash(alg)) {
			return {
				mode: "hash",
				algorithm: alg,
				input: preserveCase(collapsed, q, text),
			};
		}
		if (isCodec(alg)) {
			return {
				mode: "encode",
				algorithm: alg,
				input: preserveCase(collapsed, q, text),
			};
		}
	}

	// "<alg> <text>" — first token is algorithm
	{
		const spaceIdx = q.search(/\s/);
		if (spaceIdx !== -1) {
			const first = q.slice(0, spaceIdx);
			const rest = q.slice(spaceIdx + 1).trim();
			const alg = normalizeAlg(first);
			if (rest !== "" && alg) {
				const mode = isHash(alg) ? "hash" : isCodec(alg) ? "encode" : "";
				if (mode !== "") {
					return {
						mode,
						algorithm: alg,
						input: preserveCase(collapsed, q, rest),
					};
				}
			}
		}
	}

	// "<text> <alg>" — last token is algorithm (skip "random sha256" etc.)
	{
		const parts = q.split(/\s+/).filter(Boolean);
		if (parts.length >= 2 && parts[0] !== "random") {
			const alg = normalizeAlg(parts[parts.length - 1]);
			if (alg) {
				const inputLower = parts.slice(0, -1).join(" ");
				if (inputLower !== "" && (isHash(alg) || isCodec(alg))) {
					return {
						mode: isHash(alg) ? "hash" : "encode",
						algorithm: alg,
						input: preserveCase(collapsed, q, inputLower),
					};
				}
			}
		}
	}

	return null;
}

export function computeCryptoAnswer(query: string): SearchAnswer | null {
	const parsed = parseCrypto(query);
	if (!parsed) return null;
	const hint =
		parsed.mode === "hash"
			? `${parsed.algorithm} (client)`
			: parsed.mode === "encode"
				? `${parsed.algorithm} encode (client)`
				: parsed.mode === "decode"
					? `${parsed.algorithm} decode (client)`
					: parsed.mode;
	const interactive: InteractiveAnswer = {
		type: "crypto",
		mode: parsed.mode,
		algorithm: parsed.algorithm,
		input: parsed.input,
	};
	return { answer: hint, engine: "hash_plugin", interactive };
}

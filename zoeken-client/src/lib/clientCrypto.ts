import { md5Hex } from "./md5";
import { sha224Hex } from "./sha224";

const WEB_HASH: Record<string, AlgorithmIdentifier> = {
	sha1: "SHA-1",
	sha256: "SHA-256",
	sha384: "SHA-384",
	sha512: "SHA-512",
};

function toHex(bytes: Uint8Array): string {
	return [...bytes].map((b) => b.toString(16).padStart(2, "0")).join("");
}

function fromHex(hex: string): Uint8Array {
	const cleaned = hex.replace(/\s+/g, "");
	if (cleaned.length % 2 !== 0 || /[^0-9a-fA-F]/.test(cleaned)) {
		throw new Error("invalid hex");
	}
	const out = new Uint8Array(cleaned.length / 2);
	for (let i = 0; i < out.length; i++) {
		out[i] = Number.parseInt(cleaned.slice(i * 2, i * 2 + 2), 16);
	}
	return out;
}

function bytesToBase64(bytes: Uint8Array): string {
	let bin = "";
	for (const b of bytes) bin += String.fromCharCode(b);
	return btoa(bin);
}

function base64ToBytes(b64: string): Uint8Array {
	const bin = atob(b64);
	const out = new Uint8Array(bin.length);
	for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i);
	return out;
}

async function digestHex(
	alg: AlgorithmIdentifier,
	text: string,
): Promise<string> {
	const data = new TextEncoder().encode(text);
	const buf = await crypto.subtle.digest(alg, data);
	return toHex(new Uint8Array(buf));
}

export async function runCrypto(
	mode: string,
	algorithm: string,
	input: string,
): Promise<string> {
	const alg = algorithm.toLowerCase();
	if (mode === "hash") {
		if (alg === "md5") return md5Hex(input);
		if (alg === "sha224") return sha224Hex(input);
		const web = WEB_HASH[alg];
		if (!web) throw new Error(`unsupported hash: ${alg}`);
		return digestHex(web, input);
	}

	if (mode === "encode") {
		if (alg === "base64") return bytesToBase64(new TextEncoder().encode(input));
		if (alg === "hex") return toHex(new TextEncoder().encode(input));
		if (alg === "url") return encodeURIComponent(input);
		throw new Error(`unsupported encode: ${alg}`);
	}

	if (mode === "decode") {
		if (alg === "base64")
			return new TextDecoder().decode(base64ToBytes(input.trim()));
		if (alg === "hex") return new TextDecoder().decode(fromHex(input));
		if (alg === "url") return decodeURIComponent(input);
		throw new Error(`unsupported decode: ${alg}`);
	}

	throw new Error(`unsupported mode: ${mode}`);
}

/** "random uuid" / "random color" / ... style random-value answerer. */
import type { SearchAnswer } from "../api";

const KINDS = ["string", "int", "float", "sha256", "uuid", "color"] as const;
type RandomKind = (typeof KINDS)[number];

const STRING_ALPHABET =
	"abcdefghijklmnopqrstuvwxyz0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ";

function randomBytes(length: number): Uint8Array {
	const bytes = new Uint8Array(length);
	if (typeof crypto !== "undefined" && crypto.getRandomValues) {
		crypto.getRandomValues(bytes);
	} else {
		for (let i = 0; i < length; i++) bytes[i] = Math.floor(Math.random() * 256);
	}
	return bytes;
}

function randomInt(min: number, max: number): number {
	// Inclusive [min, max], uniform enough for a novelty answer card.
	return Math.floor(Math.random() * (max - min + 1)) + min;
}

function toHex(bytes: Uint8Array): string {
	return Array.from(bytes, (b) => b.toString(16).padStart(2, "0")).join("");
}

function randomString(): string {
	const length = randomInt(8, 32);
	let out = "";
	for (let i = 0; i < length; i++) {
		out += STRING_ALPHABET[randomInt(0, STRING_ALPHABET.length - 1)];
	}
	return out;
}

function randomUuidV4(): string {
	const bytes = randomBytes(16);
	bytes[6] = (bytes[6] & 0x0f) | 0x40;
	bytes[8] = (bytes[8] & 0x3f) | 0x80;
	const hex = toHex(bytes);
	return `${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20, 32)}`;
}

function generate(kind: RandomKind): string {
	switch (kind) {
		case "string":
			return randomString();
		case "int":
			return String(randomInt(-(2 ** 31), 2 ** 31));
		case "float":
			return String(Math.random());
		case "sha256":
			return toHex(randomBytes(32));
		case "uuid":
			return randomUuidV4();
		case "color": {
			const value = randomInt(0, 0xff_ffff);
			return `#${value.toString(16).toUpperCase().padStart(6, "0")}`;
		}
	}
}

export function computeRandomAnswer(query: string): SearchAnswer | null {
	const parts = query.trim().split(/\s+/).filter(Boolean);
	if (parts.length !== 2 || parts[0] !== "random") return null;
	const kind = KINDS.find((candidate) => candidate === parts[1]);
	if (!kind) return null;
	return { answer: generate(kind), engine: "random" };
}

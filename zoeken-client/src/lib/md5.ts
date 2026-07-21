/** Compact MD5 (RFC 1321). Web Crypto has no MD5. */

function toHex(bytes: Uint8Array): string {
	return [...bytes].map((b) => b.toString(16).padStart(2, "0")).join("");
}

function md5bytes(message: Uint8Array): Uint8Array {
	const originalLen = message.length;
	const bitLen = BigInt(originalLen) * 8n;
	const padLen = (56 - ((originalLen + 1) % 64) + 64) % 64;
	const total = originalLen + 1 + padLen + 8;
	const buf = new Uint8Array(total);
	buf.set(message);
	buf[originalLen] = 0x80;
	const view = new DataView(buf.buffer);
	view.setUint32(total - 8, Number(bitLen & 0xffffffffn), true);
	view.setUint32(total - 4, Number(bitLen >> 32n), true);

	let a0 = 0x67452301;
	let b0 = 0xefcdab89;
	let c0 = 0x98badcfe;
	let d0 = 0x10325476;

	const s = [
		7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 5, 9, 14, 20, 5,
		9, 14, 20, 5, 9, 14, 20, 5, 9, 14, 20, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11,
		16, 23, 4, 11, 16, 23, 6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21, 6, 10,
		15, 21,
	];
	const K = new Uint32Array(64);
	for (let i = 0; i < 64; i++) {
		K[i] = Math.floor(2 ** 32 * Math.abs(Math.sin(i + 1))) >>> 0;
	}

	const rotl = (x: number, n: number) => ((x << n) | (x >>> (32 - n))) >>> 0;

	for (let offset = 0; offset < total; offset += 64) {
		const M = new Uint32Array(16);
		for (let i = 0; i < 16; i++) {
			M[i] = view.getUint32(offset + i * 4, true);
		}
		let A = a0;
		let B = b0;
		let C = c0;
		let D = d0;
		for (let i = 0; i < 64; i++) {
			let F: number;
			let g: number;
			if (i < 16) {
				F = (B & C) | (~B & D);
				g = i;
			} else if (i < 32) {
				F = (D & B) | (~D & C);
				g = (5 * i + 1) % 16;
			} else if (i < 48) {
				F = B ^ C ^ D;
				g = (3 * i + 5) % 16;
			} else {
				F = C ^ (B | ~D);
				g = (7 * i) % 16;
			}
			F = (F + A + (K[i] ?? 0) + (M[g] ?? 0)) >>> 0;
			A = D;
			D = C;
			C = B;
			B = (B + rotl(F, s[i] ?? 0)) >>> 0;
		}
		a0 = (a0 + A) >>> 0;
		b0 = (b0 + B) >>> 0;
		c0 = (c0 + C) >>> 0;
		d0 = (d0 + D) >>> 0;
	}

	const out = new Uint8Array(16);
	const outView = new DataView(out.buffer);
	outView.setUint32(0, a0, true);
	outView.setUint32(4, b0, true);
	outView.setUint32(8, c0, true);
	outView.setUint32(12, d0, true);
	return out;
}

export function md5Hex(text: string): string {
	return toHex(md5bytes(new TextEncoder().encode(text)));
}

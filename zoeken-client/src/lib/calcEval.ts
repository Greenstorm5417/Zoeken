/** Safe arithmetic evaluator — recursive descent, no `eval()`. */

export type CalcEvalOk = { ok: true; value: number };
export type CalcEvalErr = { ok: false; error: string };
export type CalcEvalResult = CalcEvalOk | CalcEvalErr;

const OPS = new Set(["+", "-", "*", "/", "%", "^", "(", ")"]);

type Tok =
	| { kind: "num"; value: number }
	| { kind: "op"; value: string }
	| { kind: "eof" };

function tokenize(input: string): Tok[] | string {
	const src = input
		.replace(/\u00d7/g, "*") // ×
		.replace(/\u00f7/g, "/") // ÷
		.replace(/\u2212/g, "-") // −
		.replace(/\*\*/g, "^")
		.replace(/\s+/g, "");
	const out: Tok[] = [];
	let i = 0;
	while (i < src.length) {
		const ch = src[i] ?? "";
		if ((ch >= "0" && ch <= "9") || ch === ".") {
			let j = i;
			let seenDot = false;
			while (j < src.length) {
				const c = src[j] ?? "";
				if (c >= "0" && c <= "9") {
					j++;
				} else if (c === "." && !seenDot) {
					seenDot = true;
					j++;
				} else {
					break;
				}
			}
			const raw = src.slice(i, j);
			if (raw === "." || raw === "") return "invalid number";
			const value = Number(raw);
			if (!Number.isFinite(value)) return "invalid number";
			out.push({ kind: "num", value });
			i = j;
			continue;
		}
		if (OPS.has(ch)) {
			out.push({ kind: "op", value: ch });
			i++;
			continue;
		}
		return `unexpected character: ${ch}`;
	}
	out.push({ kind: "eof" });
	return out;
}

class Parser {
	private i = 0;
	constructor(private tokens: Tok[]) {}

	private peek(): Tok {
		return this.tokens[this.i] ?? { kind: "eof" };
	}

	private take(): Tok {
		const t = this.peek();
		if (t.kind !== "eof") this.i++;
		return t;
	}

	private expectOp(value: string): string | null {
		const t = this.peek();
		if (t.kind === "op" && t.value === value) {
			this.take();
			return null;
		}
		return `expected '${value}'`;
	}

	parse(): CalcEvalResult {
		try {
			const value = this.parseExpr();
			if (this.peek().kind !== "eof") {
				return { ok: false, error: "unexpected trailing input" };
			}
			if (!Number.isFinite(value)) {
				return { ok: false, error: "non-finite result" };
			}
			return { ok: true, value };
		} catch (e) {
			return {
				ok: false,
				error: e instanceof Error ? e.message : "parse error",
			};
		}
	}

	/** expr = term (('+' | '-') term)* */
	private parseExpr(): number {
		let left = this.parseTerm();
		for (;;) {
			const t = this.peek();
			if (t.kind === "op" && (t.value === "+" || t.value === "-")) {
				this.take();
				const right = this.parseTerm();
				left = t.value === "+" ? left + right : left - right;
			} else {
				break;
			}
		}
		return left;
	}

	/** term = power (('*' | '/' | '%') power)* */
	private parseTerm(): number {
		let left = this.parsePower();
		for (;;) {
			const t = this.peek();
			if (
				t.kind === "op" &&
				(t.value === "*" || t.value === "/" || t.value === "%")
			) {
				this.take();
				const right = this.parsePower();
				if (t.value === "*") left = left * right;
				else if (t.value === "/") {
					if (right === 0) throw new Error("division by zero");
					left = left / right;
				} else {
					if (right === 0) throw new Error("modulo by zero");
					left = left % right;
				}
			} else {
				break;
			}
		}
		return left;
	}

	/** power = unary ('^' power)?  — right-associative */
	private parsePower(): number {
		const base = this.parseUnary();
		const t = this.peek();
		if (t.kind === "op" && t.value === "^") {
			this.take();
			const exp = this.parsePower();
			return base ** exp;
		}
		return base;
	}

	/** unary = ('+' | '-') unary | primary */
	private parseUnary(): number {
		const t = this.peek();
		if (t.kind === "op" && t.value === "+") {
			this.take();
			return this.parseUnary();
		}
		if (t.kind === "op" && t.value === "-") {
			this.take();
			return -this.parseUnary();
		}
		return this.parsePrimary();
	}

	/** primary = number | '(' expr ')' */
	private parsePrimary(): number {
		const t = this.take();
		if (t.kind === "num") return t.value;
		if (t.kind === "op" && t.value === "(") {
			const value = this.parseExpr();
			const err = this.expectOp(")");
			if (err) throw new Error(err);
			return value;
		}
		throw new Error("expected number or '('");
	}
}

/** Evaluate a basic arithmetic expression. Returns null-style result via `{ok}`. */
export function calcEval(expression: string): CalcEvalResult {
	const trimmed = expression.trim();
	if (!trimmed) return { ok: false, error: "empty expression" };
	const tokens = tokenize(trimmed);
	if (typeof tokens === "string") return { ok: false, error: tokens };
	return new Parser(tokens).parse();
}

/** Format a finite number for display (trim trailing zeros). */
export function formatCalcNumber(value: number): string {
	if (!Number.isFinite(value)) return String(value);
	if (Number.isInteger(value) && Math.abs(value) < 1e15) {
		return String(value);
	}
	const s = value.toPrecision(12);
	return s.includes("e") ? s : s.replace(/\.?0+$/, "");
}

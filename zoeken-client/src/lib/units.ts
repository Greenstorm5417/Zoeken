import unitsData from "./units.json" with { type: "json" };

export type UnitDef = {
	id: string;
	name: string;
	dimension: string;
	si_unit: string;
	to_si: number;
	abbreviations: string[];
};

export const UNITS: UnitDef[] = (unitsData as { units: UnitDef[] }).units;

const byId = new Map(UNITS.map((u) => [u.id, u]));

export function unitById(id: string): UnitDef | undefined {
	return byId.get(id);
}

export function unitsForDimension(dimension: string): UnitDef[] {
	return UNITS.filter((u) => u.dimension === dimension);
}

function toKelvin(value: number, id: string): number | null {
	if (id === "°C") return value + 273.15;
	if (id === "°F") return ((value - 32) * 5) / 9 + 273.15;
	if (id === "K") return value;
	return null;
}

function fromKelvin(kelvin: number, id: string): number | null {
	if (id === "°C") return kelvin - 273.15;
	if (id === "°F") return ((kelvin - 273.15) * 9) / 5 + 32;
	if (id === "K") return kelvin;
	return null;
}

export function convertUnits(
	amount: number,
	fromId: string,
	toId: string,
): number | null {
	const from = unitById(fromId);
	const to = unitById(toId);
	if (!from || !to || from.dimension !== to.dimension) return null;
	if (from.dimension === "temperature") {
		const k = toKelvin(amount, from.id);
		if (k == null) return null;
		return fromKelvin(k, to.id);
	}
	return (amount * from.to_si) / to.to_si;
}

export function formatUnitNumber(value: number): string {
	if (Number.isInteger(value) && Math.abs(value) < 1e15) {
		return String(value);
	}
	const rounded = Number(value.toPrecision(8));
	return String(rounded);
}

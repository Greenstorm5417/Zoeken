import { Check, ChevronDown } from "lucide-react";
import { type KeyboardEvent, useEffect, useId, useRef, useState } from "react";

export type SelectOption = {
	value: string;
	label: string;
};

type SelectMenuProps = {
	label: string;
	value: string;
	options: SelectOption[];
	onChange: (value: string) => void;
	/** Stretch to full width (preferences). Default is compact (SERP toolbar). */
	fullWidth?: boolean;
};

export function SelectMenu({
	label,
	value,
	options,
	onChange,
	fullWidth = false,
}: SelectMenuProps) {
	const [open, setOpen] = useState(false);
	const rootRef = useRef<HTMLDivElement>(null);
	const listId = useId();
	const selected = options.find((o) => o.value === value) ?? options[0];

	useEffect(() => {
		if (!open) return;
		function onPointerDown(event: MouseEvent) {
			if (!rootRef.current?.contains(event.target as Node)) {
				setOpen(false);
			}
		}
		function onKey(event: globalThis.KeyboardEvent) {
			if (event.key === "Escape") setOpen(false);
		}
		document.addEventListener("mousedown", onPointerDown);
		document.addEventListener("keydown", onKey);
		return () => {
			document.removeEventListener("mousedown", onPointerDown);
			document.removeEventListener("keydown", onKey);
		};
	}, [open]);

	function onTriggerKeyDown(event: KeyboardEvent<HTMLButtonElement>) {
		if (
			event.key === "ArrowDown" ||
			event.key === "Enter" ||
			event.key === " "
		) {
			event.preventDefault();
			setOpen(true);
		}
	}

	return (
		<div
			ref={rootRef}
			className={["zoeken-menu", fullWidth ? "zoeken-menu--full" : ""].join(
				" ",
			)}
		>
			<span className="sr-only" id={`${listId}-label`}>
				{label}
			</span>
			<button
				type="button"
				className="zoeken-menu-trigger"
				aria-haspopup="listbox"
				aria-expanded={open}
				aria-labelledby={`${listId}-label`}
				aria-controls={listId}
				onClick={() => setOpen((v) => !v)}
				onKeyDown={onTriggerKeyDown}
			>
				<span className="zoeken-menu-value">{selected?.label ?? label}</span>
				<ChevronDown
					className={["zoeken-menu-chevron", open ? "is-open" : ""].join(" ")}
					aria-hidden
				/>
			</button>
			{open ? (
				<div
					id={listId}
					role="listbox"
					aria-labelledby={`${listId}-label`}
					className="zoeken-menu-list"
				>
					{options.map((option) => {
						const isSelected = option.value === value;
						return (
							<button
								key={option.value}
								type="button"
								role="option"
								aria-selected={isSelected}
								className={[
									"zoeken-menu-option",
									isSelected ? "is-selected" : "",
								].join(" ")}
								onClick={() => {
									onChange(option.value);
									setOpen(false);
								}}
							>
								<span className="truncate">{option.label}</span>
								{isSelected ? (
									<Check
										className="size-3.5 shrink-0 text-accent"
										aria-hidden
									/>
								) : null}
							</button>
						);
					})}
				</div>
			) : null}
		</div>
	);
}

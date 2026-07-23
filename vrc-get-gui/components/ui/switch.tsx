import type * as React from "react";

import { cn } from "@/lib/utils";

interface SwitchProps
	extends Omit<React.ComponentProps<"button">, "onChange" | "role"> {
	checked?: boolean;
	onCheckedChange?: (checked: boolean) => void;
}

const Switch = ({
	checked = false,
	className,
	disabled,
	onCheckedChange,
	...props
}: SwitchProps) => (
	<button
		type="button"
		role="switch"
		aria-checked={checked}
		data-state={checked ? "checked" : "unchecked"}
		disabled={disabled}
		className={cn(
			"inline-flex h-6 w-11 shrink-0 cursor-pointer items-center rounded-full border-2 border-transparent bg-muted transition-colors focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 focus-visible:ring-offset-background disabled:cursor-not-allowed disabled:opacity-50 data-[state=checked]:bg-primary",
			className,
		)}
		onClick={() => onCheckedChange?.(!checked)}
		{...props}
	>
		<span
			className={cn(
				"pointer-events-none block size-5 rounded-full bg-background shadow-md transition-transform",
				checked ? "translate-x-5" : "translate-x-0",
			)}
		/>
	</button>
);
Switch.displayName = "Switch";

export { Switch };

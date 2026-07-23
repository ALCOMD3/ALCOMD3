import type * as React from "react";
import { Card } from "@/components/ui/card";
import { cn } from "@/lib/utils";

export function SecondaryToolbarCard({
	className,
	...props
}: React.ComponentProps<typeof Card>) {
	return (
		<Card
			className={cn(
				"flex flex-wrap items-center gap-2 p-2 compact:gap-1 compact:p-1",
				className,
			)}
			{...props}
		/>
	);
}

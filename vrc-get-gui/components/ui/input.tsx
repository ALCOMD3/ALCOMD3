import type * as React from "react";

import { cn } from "@/lib/utils";

export interface InputProps extends React.ComponentProps<"input"> {}

const Input = ({ className, type, ...props }: InputProps) => {
	return (
		<input
			type={type}
			className={cn(
				"flex h-10 rounded-full border-0 bg-secondary px-4 py-2 text-sm ring-offset-background file:border-0 file:bg-transparent file:text-sm file:font-medium placeholder:text-muted-foreground focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 disabled:cursor-not-allowed disabled:opacity-50 compact:h-8",
				className,
			)}
			{...props}
		/>
	);
};
Input.displayName = "Input";

export { Input };

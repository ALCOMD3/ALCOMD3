import { Slot } from "@radix-ui/react-slot";
import { cva, type VariantProps } from "class-variance-authority";
import type * as React from "react";

import { cn } from "@/lib/utils";

const buttonVariants = cva(
	"inline-flex items-center justify-center whitespace-nowrap rounded-full text-sm font-medium ring-offset-background transition-[background-color,box-shadow,color] duration-200 focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 disabled:pointer-events-none disabled:opacity-50",
	{
		variants: {
			variant: {
				default:
					"bg-[var(--md-sys-color-primary-container)] text-[var(--md-sys-color-on-primary-container)] shadow-sm hover:shadow-md hover:bg-[color-mix(in_srgb,var(--md-sys-color-primary-container)_88%,var(--md-sys-color-primary))]",
				emphasis:
					"bg-[color-mix(in_srgb,var(--md-sys-color-primary)_72%,var(--md-sys-color-primary-container))] text-primary-foreground shadow-sm hover:shadow-md hover:bg-[color-mix(in_srgb,var(--md-sys-color-primary)_82%,var(--md-sys-color-primary-container))]",
				destructive:
					"bg-destructive text-destructive-foreground shadow-sm hover:shadow-md hover:bg-destructive/90",
				warning:
					"bg-warning text-warning-foreground shadow-sm hover:shadow-md hover:bg-warning/90",
				"outline-success":
					"border border-success text-success hover:bg-success/10",
				secondary:
					"bg-secondary text-secondary-foreground hover:bg-secondary/80",
				ghost:
					"hover:bg-accent text-accent-foreground hover:text-accent-foreground",
				"ghost-destructive":
					"hover:bg-destructive/10 text-destructive hover:text-destructive",
				link: "text-primary underline-offset-4 hover:underline",
				info: "bg-info text-info-foreground shadow-sm hover:shadow-md hover:bg-info/90",
				success:
					"bg-success text-success-foreground shadow-sm hover:shadow-md hover:bg-success/90",
			},
			size: {
				default: "h-10 px-6 py-2 compact:h-8 compact:px-4",
				sm: "h-9 px-4 compact:h-7 compact:px-3",
				lg: "h-11 px-8 compact:h-9",
				icon: "h-10 w-10 compact:h-8 compact:w-8",
			},
		},
		defaultVariants: {
			variant: "default",
			size: "default",
		},
	},
);

export interface ButtonProps
	extends React.ComponentProps<"button">,
		VariantProps<typeof buttonVariants> {
	asChild?: boolean;
}

const Button = ({
	className,
	variant,
	size,
	asChild = false,
	...props
}: ButtonProps) => {
	const Comp = asChild ? Slot : "button";
	return (
		<Comp
			className={cn(buttonVariants({ variant, size, className }))}
			{...props}
		/>
	);
};
Button.displayName = "Button";

export { Button, buttonVariants };

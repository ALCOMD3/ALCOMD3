import type { RegisteredRouter } from "@tanstack/react-router";
import { AlignLeft, Folder, Package, Settings } from "lucide-react";
import type { ComponentType } from "react";

export interface SidebarExtensionDefinition {
	href: keyof RegisteredRouter["routeTree"]["types"]["fileRouteTypes"]["fileRoutesByTo"];
	labelKey: string;
	icon: ComponentType<{ className?: string }>;
	manageable: boolean;
}

export const SIDEBAR_EXTENSION_DEFINITIONS: Record<
	string,
	SidebarExtensionDefinition
> = {
	projects: {
		href: "/projects",
		labelKey: "projects",
		icon: Folder,
		manageable: false,
	},
	packages: {
		href: "/packages/repositories",
		labelKey: "resources",
		icon: Package,
		manageable: false,
	},
	settings: {
		href: "/settings",
		labelKey: "settings",
		icon: Settings,
		manageable: false,
	},
	mcp: {
		href: "/mcp",
		labelKey: "mcp:title",
		icon: McpIcon,
		manageable: true,
	},
	log: {
		href: "/log",
		labelKey: "logs",
		icon: AlignLeft,
		manageable: true,
	},
};

function McpIcon({ className }: { className?: string }) {
	return (
		<svg
			className={className}
			viewBox="0 0 180 180"
			fill="none"
			stroke="currentColor"
			strokeWidth="12"
			strokeLinecap="round"
			aria-hidden="true"
		>
			<path d="M18 84.8528 85.8822 16.9706c9.3726-9.37262 24.5688-9.37262 33.9408 0 9.373 9.3725 9.373 24.5685 0 33.9411L68.5581 102.177" />
			<path d="m69.2652 101.47 50.5578-50.5583c9.373-9.3726 24.569-9.3726 33.942 0l.353.3535c9.373 9.3726 9.373 24.5686 0 33.9411L92.7248 146.6a8 8 0 0 0 0 11.313l12.6062 12.607" />
			<path d="M102.853 33.9411 52.6482 84.1457c-9.3726 9.3726-9.3726 24.5683 0 33.9413 9.3726 9.372 24.5685 9.372 33.9411 0l50.2047-50.2048" />
		</svg>
	);
}

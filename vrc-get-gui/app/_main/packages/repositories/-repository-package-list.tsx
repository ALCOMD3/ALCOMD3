"use client";

import type { TauriBasePackageInfo } from "@/lib/bindings";
import { cn } from "@/lib/utils";

export function RepositoryPackageList({
	packages,
	className,
}: {
	packages: TauriBasePackageInfo[];
	className?: string;
}) {
	return (
		<ul className={cn("list-disc pl-6", className)}>
			{packages.map((info) => (
				<li key={info.name}>{info.display_name ?? info.name}</li>
			))}
		</ul>
	);
}

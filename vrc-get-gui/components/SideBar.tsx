"use client";

import { useQuery } from "@tanstack/react-query";
import {
	type RegisteredRouter,
	useLocation,
	useNavigate,
} from "@tanstack/react-router";
import {
	BadgeInfo,
	Blocks,
	CircleAlert,
	CircleArrowUp,
	RefreshCw,
	Settings,
	SwatchBook,
} from "lucide-react";
import type React from "react";
import { useMemo, useSyncExternalStore } from "react";
import {
	checkForUpdates,
	getUpdateInstallProgressActiveSnapshot,
	latestUpdateCheckQueryKey,
	subscribeUpdateInstallProgress,
} from "@/components/CheckForUpdateMessage";
import {
	GuiAnimationSwitch,
	GuiCompactSwitch,
} from "@/components/common-setting-parts";
import { MaterialThemeButton } from "@/components/MaterialThemePanel";
import {
	SIDEBAR_EXTENSION_DEFINITIONS,
	type SidebarExtensionDefinition,
} from "@/components/sidebar-extension-definitions";
import { Button } from "@/components/ui/button";
import {
	Dialog,
	DialogClose,
	DialogContent,
	DialogFooter,
	DialogHeader,
	DialogTrigger,
} from "@/components/ui/dialog";
import {
	Popover,
	PopoverContent,
	PopoverTrigger,
} from "@/components/ui/popover";
import {
	Tooltip,
	TooltipContent,
	TooltipTrigger,
} from "@/components/ui/tooltip";
import type { CheckForUpdateResponse, SidebarExtension } from "@/lib/bindings";
import { commands } from "@/lib/bindings";
import globalInfo from "@/lib/global-info";
import { tc } from "@/lib/i18n";

const DEFAULT_SIDEBAR_EXTENSIONS: SidebarExtension[] = [
	{ id: "projects", installed: true, visible: true },
	{ id: "packages", installed: true, visible: true },
	{ id: "settings", installed: true, visible: true },
	{ id: "mcp", installed: true, visible: true },
	{ id: "log", installed: true, visible: true },
];

export function SideBar({ className }: { className?: string }) {
	"use client";

	const isBadHostName = useQuery({
		queryKey: ["util_is_bad_hostname"],
		queryFn: commands.utilIsBadHostname,
		refetchOnMount: false,
		refetchOnReconnect: false,
		refetchOnWindowFocus: false,
		refetchInterval: false,
		initialData: false,
	});
	const sidebarExtensions = useQuery({
		queryKey: ["environmentGetSidebarExtensions"],
		queryFn: commands.environmentGetSidebarExtensions,
		initialData: DEFAULT_SIDEBAR_EXTENSIONS,
	});
	const isDev = import.meta.env.DEV;

	const visibleSidebarExtensions = useMemo(
		() =>
			sidebarExtensions.data
				.filter((extension) => extension.installed && extension.visible)
				.map((extension) => {
					const definition = SIDEBAR_EXTENSION_DEFINITIONS[extension.id];
					if (!definition) return null;
					return {
						id: extension.id,
						...definition,
					};
				})
				.filter(
					(
						extension,
					): extension is SidebarExtensionDefinition & { id: string } =>
						extension != null,
				),
		[sidebarExtensions.data],
	);

	return (
		<aside
			className={`${className} flex w-[260px] max-w-[260px] p-3 ml-0 my-3 shrink-0 overflow-auto text-[var(--md-sys-color-on-surface)] compact:w-auto compact:p-1 compact:my-2`}
		>
			<div className="flex flex-col gap-1 min-w-40 grow compact:min-w-0">
				{visibleSidebarExtensions.map((extension) => (
					<SideBarItem
						key={extension.id}
						href={extension.href}
						text={tc(extension.labelKey)}
						icon={extension.icon}
					/>
				))}
				<MaterialThemeButton className="w-full compact:size-10" />
				{isDev && <DevRestartSetupButton />}
				{isDev && (
					<SideBarItem
						href={"/dev-palette"}
						text={"UI Palette (dev only)"}
						icon={SwatchBook}
					/>
				)}
				{isDev && <StyleQuickAccess />}
				<div className={"grow"} />
				<SideBarItem
					href={"/extensions"}
					text={tc("extensions")}
					icon={Blocks}
				/>
				<VersionCheckForUpdateButton />
				{isBadHostName.data && <BadHostNameDialogButton />}
			</div>
		</aside>
	);
}

function SideBarItem({
	href,
	text,
	icon,
}: {
	href: keyof RegisteredRouter["routeTree"]["types"]["fileRouteTypes"]["fileRoutesByTo"];
	text: React.ReactNode;
	icon: React.ComponentType<{ className?: string }>;
}) {
	const location = useLocation();
	const navigate = useNavigate();
	const getFirstPathSegment = (path: string) => {
		return path.split("/")[1] || "";
	};
	const isActive =
		getFirstPathSegment(location.pathname || "") === getFirstPathSegment(href);
	return (
		<SideBarButton
			icon={icon}
			className={
				isActive
					? "bg-[var(--md-sys-color-surface-container-highest)] text-[var(--md-sys-color-on-surface)]"
					: "bg-transparent text-[var(--md-sys-color-on-surface)]"
			}
			onClick={() => navigate({ to: href })}
		>
			{text}
		</SideBarButton>
	);
}

function VersionCheckForUpdateButton() {
	const version = useQuery({
		queryKey: ["util_get_version"],
		queryFn: commands.utilGetVersion,
		refetchOnMount: false,
		refetchOnReconnect: false,
		refetchOnWindowFocus: false,
		refetchInterval: false,
	});
	const updateCheck = useQuery<CheckForUpdateResponse | null>({
		queryKey: latestUpdateCheckQueryKey,
		queryFn: () => null,
		enabled: false,
		initialData: null,
	});
	const isUpdateInstallInProgress = useSyncExternalStore(
		subscribeUpdateInstallProgress,
		getUpdateInstallProgressActiveSnapshot,
	);
	const versionText = version.data ? `v${version.data}` : "";
	const hasUpdate = globalInfo.checkForUpdates && updateCheck.data != null;
	const Icon = isUpdateInstallInProgress
		? SpinningRefreshIcon
		: hasUpdate
			? CircleArrowUp
			: BadgeInfo;
	return (
		<SideBarButton
			icon={Icon}
			className={
				hasUpdate ? "text-warning hover:bg-warning/10 hover:text-warning" : ""
			}
			onClick={checkForUpdates}
			disabled={!versionText || !globalInfo.checkForUpdates}
		>
			{versionText ? tc("sidebar:version", { version: versionText }) : "..."}
		</SideBarButton>
	);
}

function SpinningRefreshIcon({ className }: { className?: string }) {
	return <RefreshCw className={`${className} animate-spin`} />;
}

function BadHostNameDialogButton() {
	return (
		<Dialog>
			<DialogTrigger asChild>
				<SideBarButton
					icon={CircleAlert}
					className="text-warning hover:bg-card hover:text-warning"
				>
					{tc("sidebar:bad hostname")}
				</SideBarButton>
			</DialogTrigger>
			<DialogContent className={"max-w-[50vw]"}>
				<DialogHeader>
					<h1 className={"text-warning text-center"}>
						{tc("sidebar:dialog:bad hostname")}
					</h1>
				</DialogHeader>
				<div className={"whitespace-normal"}>
					{tc("sidebar:dialog:bad hostname description")}
				</div>
				<DialogFooter>
					<DialogClose asChild>
						<Button>{tc("general:button:close")}</Button>
					</DialogClose>
				</DialogFooter>
			</DialogContent>
		</Dialog>
	);
}

function DevRestartSetupButton() {
	const navigate = useNavigate();
	const onClick = async () => {
		await commands.environmentClearSetupProcess();
		navigate({ to: "/setup/appearance" });
	};
	return (
		<SideBarButton icon={Settings} onClick={onClick}>
			Restart Setup (dev only)
		</SideBarButton>
	);
}

function SideBarButton({
	icon,
	showIconOnlyWhenCompact,
	className,
	children,
	...props
}: {
	icon: React.ComponentType<{ className?: string }>;
	showIconOnlyWhenCompact?: boolean;
	className?: string;
	children: React.ReactNode;
} & React.ComponentProps<typeof Button>) {
	const IconElement = icon;
	return (
		<Tooltip>
			<TooltipTrigger asChild>
				<Button
					variant="ghost"
					className={`justify-start h-12 px-4 rounded-full text-[var(--md-sys-color-on-surface)] hover:bg-[var(--md-sys-color-surface-container-highest)] hover:text-[var(--md-sys-color-on-surface)] ${className} compact:justify-center compact:px-3 compact:size-10`}
					{...props}
				>
					<div
						className={`mr-4 compact:mr-0 ${showIconOnlyWhenCompact ? "hidden compact:block" : ""}`}
					>
						<IconElement className="h-5 w-5" />
					</div>
					<span className="compact:hidden">{children}</span>
				</Button>
			</TooltipTrigger>
			<TooltipContent side="right">{children}</TooltipContent>
		</Tooltip>
	);
}

export function StyleQuickAccess() {
	return (
		<Popover>
			<PopoverTrigger asChild>
				<SideBarButton icon={SwatchBook}>
					Style Settings (dev only)
				</SideBarButton>
			</PopoverTrigger>
			<PopoverContent>
				<GuiAnimationSwitch />
				<GuiCompactSwitch />
			</PopoverContent>
		</Popover>
	);
}

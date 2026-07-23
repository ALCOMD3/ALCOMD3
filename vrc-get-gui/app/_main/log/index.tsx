"use client";

import {
	queryOptions,
	useMutation,
	useQuery,
	useQueryClient,
} from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";
import { ArrowDownFromLine } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import { HNavBar, HNavBarText, VStack } from "@/components/layout";
import { SearchBox } from "@/components/SearchBox";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import {
	DropdownMenu,
	DropdownMenuContent,
	DropdownMenuItem,
	DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { SecondaryToolbarCard } from "@/components/ui/secondary-toolbar-card";
import {
	Select,
	SelectContent,
	SelectGroup,
	SelectItem,
	SelectTrigger,
	SelectValue,
} from "@/components/ui/select";
import {
	Tooltip,
	TooltipContent,
	TooltipTrigger,
} from "@/components/ui/tooltip";
import type {
	ActivityEntry,
	ActivityEntryFilter,
	ActivityKind,
	ActivitySource,
	ActivityStatus,
	LogEntry,
	LogLevel,
} from "@/lib/bindings";
import { commands } from "@/lib/bindings";
import { ALCOMD3_DATA_PATHS } from "@/lib/constants";
import { isFindKey, useDocumentEvent } from "@/lib/events";
import globalInfo from "@/lib/global-info";
import { tc } from "@/lib/i18n";
import { toastThrownError } from "@/lib/toast";
import { useTauriListen } from "@/lib/use-tauri-listen";
import { useSessionStorage } from "@/lib/useSessionStorage";
import { ActivityListCard } from "./-activity-list-card";
import { LogsListCard } from "./-logs-list-card";

export const Route = createFileRoute("/_main/log/")({
	component: Page,
});

const utilGetLogEntries = queryOptions({
	queryKey: ["utilGetLogEntries"],
	queryFn: async () => commands.utilGetLogEntries(),
});

const environmentLogsLevel = queryOptions({
	queryKey: ["environmentLogsLevel"],
	queryFn: async () => commands.environmentLogsLevel(),
});

type LogView = "activity" | "technical";
type ActivitySourceFilter = ActivitySource | "All";
type ActivityKindFilter = ActivityKind | "All";
type ActivityStatusFilter = ActivityStatus | "All";
const ACTIVITY_SEARCH_DEBOUNCE_MS = 250;
const LOGS_AUTO_SCROLL_STORAGE_KEY = "logs_auto_scroll";
const LOGS_SHOW_SECONDARY_ACTIVITY_STORAGE_KEY = "logs_show_secondary_activity";
const LOGS_SHOW_ACTIVITY_DETAILS_STORAGE_KEY = "logs_show_activity_details";

function Page() {
	const [search, setSearch] = useState("");
	const [activitySearch, setActivitySearch] = useState(search);
	const [view, setView] = useState<LogView>("activity");
	const [activitySource, setActivitySource] =
		useState<ActivitySourceFilter>("All");
	const [activityKind, setActivityKind] = useState<ActivityKindFilter>("All");
	const [activityStatus, setActivityStatus] =
		useState<ActivityStatusFilter>("All");
	const showSecondaryActivity = useSessionStorageBoolean({
		key: LOGS_SHOW_SECONDARY_ACTIVITY_STORAGE_KEY,
		fallbackValue: false,
	});
	const showActivityDetails = useSessionStorageBoolean({
		key: LOGS_SHOW_ACTIVITY_DETAILS_STORAGE_KEY,
		fallbackValue: false,
	});

	const queryClient = useQueryClient();
	const logEntriesQuery = useQuery({
		...utilGetLogEntries,
		enabled: view === "technical",
	});
	const logsLevel = useQuery({
		...environmentLogsLevel,
		enabled: view === "technical",
	});
	useEffect(() => {
		const timeout = window.setTimeout(() => {
			setActivitySearch(search);
		}, ACTIVITY_SEARCH_DEBOUNCE_MS);
		return () => window.clearTimeout(timeout);
	}, [search]);
	const activityFilter = useMemo<ActivityEntryFilter>(
		() => ({
			source: activitySource === "All" ? null : activitySource,
			kind: activityKind === "All" ? null : activityKind,
			status: activityStatus === "All" ? null : activityStatus,
			includeSecondary: showSecondaryActivity,
			includeTechnical: false,
			search: activitySearch.trim() || null,
			limit: 500,
		}),
		[
			activitySource,
			activityKind,
			activityStatus,
			showSecondaryActivity,
			activitySearch,
		],
	);
	const activityListKey = useMemo(
		() => JSON.stringify(activityFilter),
		[activityFilter],
	);
	const activityEntriesQuery = useQuery({
		queryKey: ["activityGetEntries", activityFilter],
		queryFn: async () => commands.activityGetEntries(activityFilter),
		enabled: view === "activity",
	});

	const handleLogLevelChange = useMutation({
		mutationFn: async (value: LogLevel[]) =>
			commands.environmentSetLogsLevel(value),
		onMutate: async (value) => {
			await queryClient.cancelQueries(environmentLogsLevel);
			const data = queryClient.getQueryData(environmentLogsLevel.queryKey);
			queryClient.setQueryData(environmentLogsLevel.queryKey, value);
			return data;
		},
		onError: (e, _, data) => {
			console.error(e);
			toastThrownError(e);
			queryClient.setQueryData(environmentLogsLevel.queryKey, data);
		},
		onSettled: async () => {
			await queryClient.invalidateQueries(environmentLogsLevel);
		},
	});

	const autoScroll = useSessionStorage({
		key: LOGS_AUTO_SCROLL_STORAGE_KEY,
		parse: (value) => value === "true",
		fallbackValue: true,
	});

	const handleLogAutoScrollChange = (value: boolean) => {
		setSessionStorageBoolean(LOGS_AUTO_SCROLL_STORAGE_KEY, value);
	};
	const handleShowSecondaryActivityChange = (value: boolean) => {
		setSessionStorageBoolean(LOGS_SHOW_SECONDARY_ACTIVITY_STORAGE_KEY, value);
	};
	const handleShowActivityDetailsChange = (value: boolean) => {
		setSessionStorageBoolean(LOGS_SHOW_ACTIVITY_DETAILS_STORAGE_KEY, value);
	};

	useTauriListen<LogEntry>("log", (event) => {
		const entry = event.payload as LogEntry;
		const entries = queryClient.getQueryData(utilGetLogEntries.queryKey) ?? [];
		queryClient.setQueryData(utilGetLogEntries.queryKey, [...entries, entry]);
	});

	useTauriListen<ActivityEntry>("activity-log-entry", () => {
		queryClient.invalidateQueries({ queryKey: ["activityGetEntries"] });
	});

	const shouldShowLogLevel = logsLevel.data ?? [];

	return (
		<VStack>
			<ManageLogsHeading
				view={view}
				setView={setView}
				search={search}
				setSearch={setSearch}
				shouldShowLogLevel={shouldShowLogLevel}
				handleLogLevelChange={handleLogLevelChange.mutate}
				handleLogAutoScrollChange={handleLogAutoScrollChange}
				autoScroll={autoScroll}
			/>
			{view === "activity" && (
				<ActivityFilters
					source={activitySource}
					setSource={setActivitySource}
					kind={activityKind}
					setKind={setActivityKind}
					status={activityStatus}
					setStatus={setActivityStatus}
					showSecondary={showSecondaryActivity}
					setShowSecondary={handleShowSecondaryActivityChange}
					showDetails={showActivityDetails}
					setShowDetails={handleShowActivityDetailsChange}
				/>
			)}
			<main className="shrink overflow-hidden flex w-full h-full">
				{view === "activity" ? (
					<ActivityListCard
						key={activityListKey}
						entries={activityEntriesQuery.data ?? []}
						showDetails={showActivityDetails}
					/>
				) : (
					<LogsListCard
						logEntry={logEntriesQuery.data ?? []}
						search={search}
						shouldShowLogLevel={shouldShowLogLevel}
						autoScroll={autoScroll}
					/>
				)}
			</main>
		</VStack>
	);
}

function useSessionStorageBoolean({
	key,
	fallbackValue,
}: {
	key: string;
	fallbackValue: boolean;
}) {
	return useSessionStorage({
		key,
		parse: (value) => value === "true",
		fallbackValue,
	});
}

function setSessionStorageBoolean(key: string, value: boolean) {
	sessionStorage.setItem(key, String(value));
	// Manually dispatch storage event to force state synchronization within the same page,
	// as native sessionStorage.setItem doesn't trigger storage event for the current origin
	window.dispatchEvent(
		new StorageEvent("storage", {
			key,
			newValue: String(value),
			storageArea: sessionStorage,
		}),
	);
}

function ManageLogsHeading({
	view,
	setView,
	search,
	setSearch,
	shouldShowLogLevel,
	handleLogLevelChange,
	handleLogAutoScrollChange,
	autoScroll,
}: {
	view: LogView;
	setView: (value: LogView) => void;
	search: string;
	setSearch: (value: string) => void;
	shouldShowLogLevel: LogLevel[];
	handleLogLevelChange: (newLogLevels: LogLevel[]) => void;
	handleLogAutoScrollChange: (newAutoScroll: boolean) => void;
	autoScroll: boolean;
}) {
	const searchRef = useRef<HTMLInputElement>(null);

	useDocumentEvent(
		"keydown",
		(e) => {
			if (isFindKey(e)) {
				searchRef.current?.focus();
			}
		},
		[],
	);

	return (
		<HNavBar
			className="shrink-0"
			leading={
				<>
					<HNavBarText>{tc("logs")}</HNavBarText>

					<div className="flex shrink-0 items-center gap-1 rounded-full bg-secondary p-1">
						<Button
							size="sm"
							variant={view === "activity" ? "default" : "ghost"}
							onClick={() => setView("activity")}
						>
							{tc("logs:activity:title")}
						</Button>
						<Button
							size="sm"
							variant={view === "technical" ? "default" : "ghost"}
							onClick={() => setView("technical")}
						>
							{tc("logs:technical:title")}
						</Button>
					</div>

					<SearchBox
						className={"w-max grow"}
						value={search}
						onChange={(e) => setSearch(e.target.value)}
						ref={searchRef}
					/>
					{view === "activity" && (
						<Button
							className="shrink-0 compact:h-10"
							onClick={() =>
								commands.activityOpenLogFolder().catch(toastThrownError)
							}
						>
							{tc("logs:activity:open folder")}
						</Button>
					)}
				</>
			}
			trailing={
				view === "technical" ? (
					<TechnicalLogFilters
						shouldShowLogLevel={shouldShowLogLevel}
						handleLogLevelChange={handleLogLevelChange}
						handleLogAutoScrollChange={handleLogAutoScrollChange}
						autoScroll={autoScroll}
					/>
				) : undefined
			}
		/>
	);
}

function ActivityFilters({
	source,
	setSource,
	kind,
	setKind,
	status,
	setStatus,
	showSecondary,
	setShowSecondary,
	showDetails,
	setShowDetails,
}: {
	source: ActivitySourceFilter;
	setSource: (value: ActivitySourceFilter) => void;
	kind: ActivityKindFilter;
	setKind: (value: ActivityKindFilter) => void;
	status: ActivityStatusFilter;
	setStatus: (value: ActivityStatusFilter) => void;
	showSecondary: boolean;
	setShowSecondary: (value: boolean) => void;
	showDetails: boolean;
	setShowDetails: (value: boolean) => void;
}) {
	return (
		<SecondaryToolbarCard>
			<ActivitySelect
				value={source}
				onValueChange={(value) => setSource(value as ActivitySourceFilter)}
				items={["All", "Gui", "Mcp", "DeepLink", "System"]}
				labelPrefix="logs:activity:source"
			/>
			<ActivitySelect
				value={status}
				onValueChange={(value) => setStatus(value as ActivityStatusFilter)}
				items={["All", "Failed", "Started", "Succeeded", "Cancelled", "Info"]}
				labelPrefix="logs:activity:status"
			/>
			<ActivitySelect
				value={kind}
				onValueChange={(value) => setKind(value as ActivityKindFilter)}
				items={["All", "Write", "Read", "Passive", "Open", "Maintenance"]}
				labelPrefix="logs:activity:kind"
			/>
			<label className="flex h-10 cursor-pointer items-center gap-2 rounded-full bg-secondary px-3 text-sm compact:h-8">
				<Checkbox
					checked={showSecondary}
					onCheckedChange={(checked) => setShowSecondary(checked === true)}
					className="hover:before:content-none"
				/>
				{tc("logs:activity:show secondary")}
			</label>
			<label className="flex h-10 cursor-pointer items-center gap-2 rounded-full bg-secondary px-3 text-sm compact:h-8">
				<Checkbox
					checked={showDetails}
					onCheckedChange={(checked) => setShowDetails(checked === true)}
					className="hover:before:content-none"
				/>
				{tc("logs:activity:show details")}
			</label>
		</SecondaryToolbarCard>
	);
}

function ActivitySelect({
	value,
	onValueChange,
	items,
	labelPrefix,
}: {
	value: string;
	onValueChange: (value: string) => void;
	items: string[];
	labelPrefix: string;
}) {
	return (
		<Select value={value} onValueChange={onValueChange}>
			<SelectTrigger className="w-40">
				<SelectValue />
			</SelectTrigger>
			<SelectContent>
				<SelectGroup>
					{items.map((item) => (
						<SelectItem key={item} value={item}>
							{tc(`${labelPrefix}:${item}`)}
						</SelectItem>
					))}
				</SelectGroup>
			</SelectContent>
		</Select>
	);
}

function TechnicalLogFilters({
	shouldShowLogLevel,
	handleLogLevelChange,
	handleLogAutoScrollChange,
	autoScroll,
}: {
	shouldShowLogLevel: LogLevel[];
	handleLogLevelChange: (newLogLevels: LogLevel[]) => void;
	handleLogAutoScrollChange: (newAutoScroll: boolean) => void;
	autoScroll: boolean;
}) {
	return (
		<>
			<DropdownMenu>
				<DropdownMenuTrigger asChild>
					<Button className={"shrink-0 p-3 compact:h-10"}>
						{tc("logs:manage:select logs level")}
					</Button>
				</DropdownMenuTrigger>
				<DropdownMenuContent>
					<LogLevelMenuItem
						logLevel="Info"
						shouldShowLogLevel={shouldShowLogLevel}
						handleLogLevelChange={handleLogLevelChange}
					/>
					<LogLevelMenuItem
						logLevel="Warn"
						className="text-warning"
						shouldShowLogLevel={shouldShowLogLevel}
						handleLogLevelChange={handleLogLevelChange}
					/>
					<LogLevelMenuItem
						logLevel="Error"
						className="text-destructive"
						shouldShowLogLevel={shouldShowLogLevel}
						handleLogLevelChange={handleLogLevelChange}
					/>
					<LogLevelMenuItem
						logLevel="Debug"
						className="text-info"
						shouldShowLogLevel={shouldShowLogLevel}
						handleLogLevelChange={handleLogLevelChange}
					/>
				</DropdownMenuContent>
			</DropdownMenu>

			<Button
				className={"compact:h-10"}
				onClick={() =>
					commands.utilOpen(
						`${globalInfo.vpmHomeFolder}/${ALCOMD3_DATA_PATHS.technicalLogs}`,
						"ErrorIfNotExists",
					)
				}
			>
				{tc("settings:button:open logs")}
			</Button>

			<Tooltip>
				<TooltipTrigger asChild>
					<Button
						variant={"ghost"}
						onClick={() => handleLogAutoScrollChange(!autoScroll)}
						className={`compact:h-10 ${
							autoScroll
								? "bg-secondary border border-primary"
								: "bg-transparent border border-transparent"
						}`}
					>
						<ArrowDownFromLine className={"w-5 h-5"} />
					</Button>
				</TooltipTrigger>
				<TooltipContent>{tc("logs:manage:auto scroll")}</TooltipContent>
			</Tooltip>
		</>
	);
}

function LogLevelMenuItem({
	logLevel,
	className,
	shouldShowLogLevel,
	handleLogLevelChange,
}: {
	logLevel: LogLevel;
	className?: string;
	shouldShowLogLevel: LogLevel[];
	handleLogLevelChange: (newLogLevels: LogLevel[]) => void;
}) {
	const selected = shouldShowLogLevel.includes(logLevel);

	const onChange = () => {
		const newLogLevels = selected
			? shouldShowLogLevel.filter(
					(logLevelFilter) => logLevelFilter !== logLevel,
				)
			: [...shouldShowLogLevel, logLevel];

		handleLogLevelChange(newLogLevels);
	};

	return (
		<DropdownMenuItem
			className="p-0"
			onSelect={(e) => {
				e.preventDefault();
			}}
		>
			<label
				className={
					"flex cursor-pointer items-center gap-2 p-2 whitespace-normal"
				}
			>
				<Checkbox
					checked={selected}
					onCheckedChange={onChange}
					className="hover:before:content-none"
				/>
				<p className={className}>{logLevel}</p>
			</label>
		</DropdownMenuItem>
	);
}

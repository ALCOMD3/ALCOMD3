"use client";

import {
	queryOptions,
	useMutation,
	useQuery,
	useQueryClient,
} from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";
import {
	CheckCircle2,
	CircleAlert,
	Copy,
	Power,
	PowerOff,
	RefreshCw,
} from "lucide-react";
import type React from "react";
import { useCallback, useEffect, useRef, useState } from "react";
import { HNavBar, HNavBarText, VStack } from "@/components/layout";
import { ScrollPageContainer } from "@/components/ScrollPageContainer";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import {
	Tooltip,
	TooltipContent,
	TooltipTrigger,
} from "@/components/ui/tooltip";
import type { McpStatus, McpToolCallEvent } from "@/lib/bindings";
import { commands } from "@/lib/bindings";
import { tc } from "@/lib/i18n";
import { toastSuccess, toastThrownError } from "@/lib/toast";
import { useTauriListen } from "@/lib/use-tauri-listen";

export const Route = createFileRoute("/_main/mcp/")({
	component: Page,
});

const mcpStatus = queryOptions({
	queryKey: ["mcpStatus"],
	queryFn: commands.mcpStatus,
});

const MCP_STATUS_REFETCH_INTERVAL_MS = 10_000;
const MCP_TOOL_ACTIVE_MIN_VISIBLE_MS = 800;

type McpTool = {
	name: string;
	labelKey: string;
};

const MCP_TOOL_GROUPS: {
	labelKey: string;
	tools: McpTool[];
}[] = [
	{
		labelKey: "mcp:tools:group:read",
		tools: [
			{
				name: "alcomd3_list_projects",
				labelKey: "mcp:tool:alcomd3_list_projects",
			},
			{
				name: "alcomd3_get_project_details",
				labelKey: "mcp:tool:alcomd3_get_project_details",
			},
			{
				name: "alcomd3_list_repositories",
				labelKey: "mcp:tool:alcomd3_list_repositories",
			},
			{
				name: "alcomd3_get_package_details",
				labelKey: "mcp:tool:alcomd3_get_package_details",
			},
			{
				name: "alcomd3_list_packages",
				labelKey: "mcp:tool:alcomd3_list_packages",
			},
			{
				name: "alcomd3_list_repository_packages",
				labelKey: "mcp:tool:alcomd3_list_repository_packages",
			},
			{
				name: "alcomd3_get_environment_settings",
				labelKey: "mcp:tool:alcomd3_get_environment_settings",
			},
		],
	},
	{
		labelKey: "mcp:tools:group:write",
		tools: [
			{
				name: "alcomd3_create_project",
				labelKey: "mcp:tool:alcomd3_create_project",
			},
			{
				name: "alcomd3_add_existing_project",
				labelKey: "mcp:tool:alcomd3_add_existing_project",
			},
			{
				name: "alcomd3_add_repository",
				labelKey: "mcp:tool:alcomd3_add_repository",
			},
			{
				name: "alcomd3_backup_project",
				labelKey: "mcp:tool:alcomd3_backup_project",
			},
			{
				name: "alcomd3_copy_project",
				labelKey: "mcp:tool:alcomd3_copy_project",
			},
			{
				name: "alcomd3_restore_project_from_backup",
				labelKey: "mcp:tool:alcomd3_restore_project_from_backup",
			},
			{
				name: "alcomd3_install_project_package",
				labelKey: "mcp:tool:alcomd3_install_project_package",
			},
			{
				name: "alcomd3_uninstall_project_package",
				labelKey: "mcp:tool:alcomd3_uninstall_project_package",
			},
			{
				name: "alcomd3_reinstall_project_package",
				labelKey: "mcp:tool:alcomd3_reinstall_project_package",
			},
		],
	},
	{
		labelKey: "mcp:tools:group:logs",
		tools: [
			{
				name: "alcomd3_search_activity_logs",
				labelKey: "mcp:tool:alcomd3_search_activity_logs",
			},
			{
				name: "alcomd3_get_activity_log_entry",
				labelKey: "mcp:tool:alcomd3_get_activity_log_entry",
			},
			{
				name: "alcomd3_summarize_activity_logs",
				labelKey: "mcp:tool:alcomd3_summarize_activity_logs",
			},
			{
				name: "alcomd3_get_activity_log_context",
				labelKey: "mcp:tool:alcomd3_get_activity_log_context",
			},
			{
				name: "alcomd3_search_technical_logs",
				labelKey: "mcp:tool:alcomd3_search_technical_logs",
			},
			{
				name: "alcomd3_get_technical_log_entry",
				labelKey: "mcp:tool:alcomd3_get_technical_log_entry",
			},
			{
				name: "alcomd3_summarize_technical_logs",
				labelKey: "mcp:tool:alcomd3_summarize_technical_logs",
			},
		],
	},
];

type ActiveToolCalls = Record<string, Record<string, number>>;

function addActiveToolCall(
	calls: ActiveToolCalls,
	toolName: string,
	requestId: string,
	startedAt: number,
): ActiveToolCalls {
	return {
		...calls,
		[toolName]: {
			...(calls[toolName] ?? {}),
			[requestId]: startedAt,
		},
	};
}

function removeActiveToolCall(
	calls: ActiveToolCalls,
	toolName: string,
	requestId: string,
): ActiveToolCalls {
	const toolCalls = calls[toolName];
	if (toolCalls?.[requestId] == null) {
		return calls;
	}

	const remainingToolCalls = { ...toolCalls };
	delete remainingToolCalls[requestId];
	if (Object.keys(remainingToolCalls).length > 0) {
		return {
			...calls,
			[toolName]: remainingToolCalls,
		};
	}

	const remainingCalls = { ...calls };
	delete remainingCalls[toolName];
	return remainingCalls;
}

function toolHasActiveCalls(calls: ActiveToolCalls, toolName: string) {
	return Object.keys(calls[toolName] ?? {}).length > 0;
}

function toolCallTimerKey(toolName: string, requestId: string) {
	return `${toolName}:${requestId}`;
}

function Page() {
	const queryClient = useQueryClient();
	const [activeToolCalls, setActiveToolCalls] = useState<ActiveToolCalls>({});
	const activeToolCallsRef = useRef<ActiveToolCalls>({});
	const toolCallClearTimers = useRef(new Map<string, number>());
	const status = useQuery({
		...mcpStatus,
		refetchInterval: MCP_STATUS_REFETCH_INTERVAL_MS,
	});

	const updateActiveToolCalls = useCallback(
		(update: (calls: ActiveToolCalls) => ActiveToolCalls) => {
			const next = update(activeToolCallsRef.current);
			activeToolCallsRef.current = next;
			setActiveToolCalls(next);
		},
		[],
	);

	const clearToolCallTimer = useCallback((timerKey: string) => {
		const timer = toolCallClearTimers.current.get(timerKey);
		if (timer == null) {
			return;
		}

		window.clearTimeout(timer);
		toolCallClearTimers.current.delete(timerKey);
	}, []);

	const removeVisibleToolCall = useCallback(
		(toolName: string, requestId: string) => {
			updateActiveToolCalls((calls) =>
				removeActiveToolCall(calls, toolName, requestId),
			);
		},
		[updateActiveToolCalls],
	);

	const scheduleToolCallClear = useCallback(
		(toolName: string, requestId: string, delayMs: number) => {
			const timerKey = toolCallTimerKey(toolName, requestId);
			clearToolCallTimer(timerKey);
			const timer = window.setTimeout(() => {
				removeVisibleToolCall(toolName, requestId);
				toolCallClearTimers.current.delete(timerKey);
			}, delayMs);
			toolCallClearTimers.current.set(timerKey, timer);
		},
		[clearToolCallTimer, removeVisibleToolCall],
	);

	useEffect(() => {
		return () => {
			for (const timer of toolCallClearTimers.current.values()) {
				window.clearTimeout(timer);
			}
			toolCallClearTimers.current.clear();
		};
	}, []);

	useTauriListen<McpStatus>("mcp-status-changed", (event) => {
		queryClient.setQueryData(mcpStatus.queryKey, event.payload);
	});

	useTauriListen<McpToolCallEvent>("mcp-tool-call", (event) => {
		const { toolName, requestId, phase } = event.payload;
		const timerKey = toolCallTimerKey(toolName, requestId);
		clearToolCallTimer(timerKey);

		const now = Date.now();
		if (phase === "started") {
			updateActiveToolCalls((calls) =>
				addActiveToolCall(calls, toolName, requestId, now),
			);
			return;
		}

		const startedAt = activeToolCallsRef.current[toolName]?.[requestId] ?? now;
		updateActiveToolCalls((calls) =>
			addActiveToolCall(calls, toolName, requestId, startedAt),
		);
		const elapsedMs = now - startedAt;
		const delayMs = Math.max(0, MCP_TOOL_ACTIVE_MIN_VISIBLE_MS - elapsedMs);
		scheduleToolCallClear(toolName, requestId, delayMs);
	});

	const setEnabled = useMutation({
		mutationFn: async (enabled: boolean) =>
			await commands.mcpSetEnabled(enabled),
		onSuccess: (nextStatus) => {
			queryClient.setQueryData(mcpStatus.queryKey, nextStatus);
		},
		onError: (e) => {
			console.error(e);
			toastThrownError(e);
		},
	});

	const refresh = useMutation({
		mutationFn: async () => await queryClient.invalidateQueries(mcpStatus),
	});

	return (
		<VStack>
			<HNavBar
				className="shrink-0"
				leading={<HNavBarText>{tc("mcp:title")}</HNavBarText>}
				trailing={
					<Button
						variant={"ghost"}
						onClick={() => refresh.mutate()}
						disabled={refresh.isPending}
						className={"compact:h-10"}
					>
						<RefreshCw
							className={`size-5 ${refresh.isPending ? "animate-spin" : ""}`}
						/>
					</Button>
				}
			/>
			<ScrollPageContainer viewportClassName={"rounded-xl shadow-xl h-full"}>
				<main className="flex shrink grow flex-col gap-2">
					{status.data == null ? (
						<Card className="p-4">{tc("general:loading...")}</Card>
					) : (
						<>
							<StatusCard
								status={status.data}
								setEnabled={(enabled) => setEnabled.mutate(enabled)}
								disabled={setEnabled.isPending}
							/>
							<EndpointCard status={status.data} />
							<RecentClientsCard status={status.data} />
							<ToolsCard activeToolCalls={activeToolCalls} />
						</>
					)}
				</main>
			</ScrollPageContainer>
		</VStack>
	);
}

function StatusCard({
	status,
	setEnabled,
	disabled,
}: {
	status: McpStatus;
	setEnabled: (enabled: boolean) => void;
	disabled: boolean;
}) {
	return (
		<Card className="shrink-0 p-4 compact:p-3">
			<div className="flex flex-wrap items-center gap-3">
				<StatusPill active={status.enabled} />
				<RunningPill running={status.running} />
				<div className="grow" />
				<Button
					onClick={() => setEnabled(!status.enabled)}
					disabled={disabled}
					className="gap-2"
				>
					{status.enabled ? (
						<PowerOff className="size-5" />
					) : (
						<Power className="size-5" />
					)}
					{status.enabled ? tc("mcp:button:disable") : tc("mcp:button:enable")}
				</Button>
			</div>
		</Card>
	);
}

function EndpointCard({ status }: { status: McpStatus }) {
	return (
		<Card className="shrink-0 p-4 compact:p-3">
			<h2 className="mb-3">{tc("mcp:endpoint")}</h2>
			<div className="grid gap-2 md:grid-cols-[max-content_1fr]">
				<StatusRow label={tc("mcp:bridge command")}>
					<CommandValue value={status.bridgeCommand} />
				</StatusRow>
				<StatusRow label={tc("mcp:endpoint file")}>
					<CodeValue>{status.endpointFile}</CodeValue>
				</StatusRow>
				<StatusRow label={tc("mcp:protocol version")}>
					{status.protocolVersion}
				</StatusRow>
				<StatusRow label={tc("mcp:transport")}>{status.transport}</StatusRow>
				<StatusRow label={tc("mcp:host")}>{status.host ?? "-"}</StatusRow>
				<StatusRow label={tc("mcp:port")}>{status.port ?? "-"}</StatusRow>
				<StatusRow label={tc("mcp:pid")}>{status.pid}</StatusRow>
			</div>
		</Card>
	);
}

function RecentClientsCard({ status }: { status: McpStatus }) {
	return (
		<Card className="shrink-0 p-4 compact:p-3">
			<h2 className="mb-3">{tc("mcp:clients")}</h2>
			{status.recentClients.length === 0 ? (
				<p className="text-muted-foreground">{tc("mcp:no clients")}</p>
			) : (
				<div className="overflow-x-auto">
					<table className="w-full text-left">
						<thead>
							<tr className="border-b border-primary/20">
								<th className="p-2">{tc("mcp:client")}</th>
								<th className="p-2">{tc("general:version")}</th>
								<th className="p-2">{tc("mcp:last seen")}</th>
							</tr>
						</thead>
						<tbody>
							{status.recentClients.map((client) => (
								<tr key={client.sessionId} className="even:bg-secondary/30">
									<td className="p-2">{client.name}</td>
									<td className="p-2">{client.version ?? "-"}</td>
									<td className="p-2">
										{new Date(client.lastSeenUnixMs).toLocaleString()}
									</td>
								</tr>
							))}
						</tbody>
					</table>
				</div>
			)}
		</Card>
	);
}

function ToolsCard({ activeToolCalls }: { activeToolCalls: ActiveToolCalls }) {
	return (
		<Card className="shrink-0 p-4 compact:p-3">
			<h2 className="mb-3">{tc("mcp:tools")}</h2>
			<div className="grid gap-4">
				{MCP_TOOL_GROUPS.map((group) => (
					<section key={group.labelKey} className="grid gap-2">
						<h3 className="text-sm font-medium text-muted-foreground">
							{tc(group.labelKey)}
						</h3>
						<ul className="grid gap-2 md:grid-cols-2">
							{group.tools.map((tool) => {
								const active = toolHasActiveCalls(activeToolCalls, tool.name);
								return (
									<li key={tool.name}>
										<Tooltip>
											<TooltipTrigger asChild>
												<button
													type="button"
													className="block w-full appearance-none rounded-md border-0 bg-transparent p-0 text-left outline-none focus-visible:ring-2 focus-visible:ring-primary/70"
												>
													<CodeValue active={active}>{tool.name}</CodeValue>
												</button>
											</TooltipTrigger>
											<TooltipContent className="max-w-[80dvw]">
												{tc(tool.labelKey)}
											</TooltipContent>
										</Tooltip>
									</li>
								);
							})}
						</ul>
					</section>
				))}
			</div>
		</Card>
	);
}

function StatusPill({ active }: { active: boolean }) {
	return (
		<div
			className={`inline-flex items-center gap-2 rounded-full px-3 py-1 text-sm ${
				active
					? "bg-secondary text-primary"
					: "bg-secondary text-secondary-foreground"
			}`}
		>
			{active ? (
				<CheckCircle2 className="size-4" />
			) : (
				<CircleAlert className="size-4" />
			)}
			{active ? tc("mcp:enabled") : tc("mcp:disabled")}
		</div>
	);
}

function RunningPill({ running }: { running: boolean }) {
	return (
		<div
			className={`inline-flex items-center gap-2 rounded-full px-3 py-1 text-sm ${
				running ? "bg-secondary text-primary" : "bg-secondary text-warning"
			}`}
		>
			{running ? (
				<CheckCircle2 className="size-4" />
			) : (
				<CircleAlert className="size-4" />
			)}
			{running ? tc("mcp:running") : tc("mcp:not running")}
		</div>
	);
}

function StatusRow({
	label,
	children,
}: {
	label: React.ReactNode;
	children: React.ReactNode;
}) {
	return (
		<>
			<div className="text-muted-foreground">{label}</div>
			<div className="min-w-0">{children}</div>
		</>
	);
}

function CommandValue({ value }: { value: string }) {
	const copy = async () => {
		await navigator.clipboard.writeText(value);
		toastSuccess(tc("mcp:toast:command copied"));
	};
	return (
		<div className="flex min-w-0 items-center gap-2">
			<CodeValue>{value}</CodeValue>
			<Button size="icon" variant="ghost" onClick={copy}>
				<Copy className="size-5" />
			</Button>
		</div>
	);
}

function CodeValue({
	children,
	active = false,
}: {
	children: React.ReactNode;
	active?: boolean;
}) {
	return (
		<code
			data-active={active || undefined}
			className={`block max-w-full overflow-x-auto rounded-md px-2 py-1 text-sm transition ${
				active
					? "bg-accent text-accent-foreground ring-2 ring-primary/70"
					: "bg-secondary"
			}`}
		>
			{children}
		</code>
	);
}
